//! Each machine's instance (its interfaces and Elastic IP on several networks) and its set-up
//! through the VM core; the Windows machines' and the controller's.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, BlockBuilder, Expression, Module, Template, binary, boolean, call, list, null, number, object, reference, string,
};
use isoloom_render::inventory::WinrmAuth;
use isoloom_spec::{DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::network::{controller_group, security_group};
use super::{addresses, image_user, machine_type, nesting};
use crate::cloud::auto_stop;
use crate::cloud::aws::instance_type;
use crate::cloud::instance::name_with;
use crate::cloud::vm::{disks, gateways, hooks, interfaces, own_image};
use crate::machine::{Remote, controller, linux, windows};
use crate::module::{ident, resource};

/// An instance's user data on Linux: the auto-stop, scheduled at first boot.
fn linux_user_data() -> Expression {
    auto_stop::if_set(auto_stop::script(), null())
}

/// The address Terraform reaches a machine at: its instance's own public address on one
/// network, its Elastic IP on several (AWS doesn't auto-assign one then).
pub(super) fn public_ip(env: &Environment, name: &str) -> Expression {
    let id = ident(name);
    if env.machines.get(name).is_some_and(|mp| !mp.is_windows() && addresses::needs_interfaces(mp)) {
        reference(&format!("aws_eip.{id}.public_ip"))
    } else {
        reference(&format!("aws_instance.{id}.public_ip"))
    }
}

/// The machine's AMI: its own (`vm.image.aws`), else the one looked up for its OS.
fn ami(mp: &MachinePlan) -> Expression {
    own_image::of(mp, own_image::aws).map_or_else(
        || reference(&format!("data.aws_ami.{}.id", ident(mp.os.as_deref().unwrap_or_default()))),
        string,
    )
}

/// A Linux machine: its security group, its instance, its set-up over SSH.
pub(super) fn linux(env: &Environment, (name, mp): (&str, &MachinePlan), m: &mut Module) {
    let id = ident(name);
    m.commented(&format!("Machine `{name}`: what may reach it."), security_group(env, (name, mp), false));
    let mem = mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB);
    let mut instance = resource("aws_instance", &id)
        .add_attribute(("ami", ami(mp)))
        .add_attribute(("instance_type", string(machine_type(mp, mem))))
        .add_blocks(disks::aws(&mp.disks));
    // A looked-up AMI says whether it has NitroTPM; an own AMI is the author's to choose.
    if mp.tpm.is_some() && own_image::of(mp, own_image::aws).is_none() {
        instance = instance.add_block(nitro_tpm(&ident(mp.os.as_deref().unwrap_or_default())));
    }
    let instance = nesting(instance, mp);
    if addresses::needs_interfaces(mp) {
        m.blocks(several_networks((name, mp), instance));
    } else {
        m.block(one_network((name, mp), instance));
    }
    let mut at_boot = interfaces::up(env, &mp.addresses, &|n| mac(&format!("aws_network_interface.{id}_{}", ident(n))));
    at_boot.extend(gateways::at_boot(mp));
    let machine = linux::Machine {
        remote: Remote {
            env,
            name,
            plan: mp,
            replace_on: reference(&format!("aws_instance.{id}.id")),
            host: public_ip(env, name),
        },
        user: image_user(mp.os.as_deref().unwrap_or_default()).to_owned(),
        hooks: hooks::of(mp, at_boot),
    };
    m.block(linux::provisioner(machine).block());
}

/// A TPM on AWS is NitroTPM, which the AMI enables (`tpm_support`): the plan stops when the
/// image has none rather than start a VM without.
fn nitro_tpm(ami: &str) -> Block {
    let condition = binary(reference(&format!("data.aws_ami.{ami}.tpm_support")), BinaryOperator::Eq, string("v2.0"));
    let precondition = Block::builder("precondition")
        .add_attribute(("condition", condition))
        .add_attribute((
            "error_message",
            string("The machine asks for a TPM (vm.tpm), and this AMI has no NitroTPM support."),
        ))
        .build();
    Block::builder("lifecycle").add_block(precondition).build()
}

/// The instance on its network, at its address.
fn one_network((name, mp): (&str, &MachinePlan), instance: BlockBuilder) -> Block {
    let id = ident(name);
    let Some((net, addr)) = mp.addresses.first() else { return instance.build() };
    // A gateway forwards traffic that isn't its own.
    let instance = if mp.is_gateway {
        instance.add_attribute(("source_dest_check", boolean(false)))
    } else {
        instance
    };
    let instance = addresses::on_instance(instance, mp, net);
    instance
        .add_attribute(("subnet_id", reference(&format!("aws_subnet.{}.id", ident(net)))))
        .add_attribute(("private_ip", string(addr.to_string())))
        .add_attribute(("associate_public_ip_address", boolean(true)))
        .add_attribute(("key_name", reference("aws_key_pair.env.key_name")))
        .add_attribute(("vpc_security_group_ids", list([reference(&format!("aws_security_group.{id}.id"))])))
        .add_attribute(("user_data", linux_user_data()))
        .add_block(disk(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB)))
        .add_attribute(("tags", object([("Name", name_with(&format!("-{name}")))])))
        .build()
}

/// An interface per network at its address (source/destination check off, so a reply can leave
/// from the interface of the network it's for), the instance on the first, the others attached,
/// and an Elastic IP on the first.
fn several_networks((name, mp): (&str, &MachinePlan), instance: BlockBuilder) -> Vec<Block> {
    let id = ident(name);
    let group = reference(&format!("aws_security_group.{id}.id"));
    let mut out: Vec<Block> = mp
        .addresses
        .iter()
        .map(|(n, a)| {
            interface(&format!("{id}_{}", ident(n)), n, &a.to_string(), group.clone(), Some(mp))
                .add_attribute(("tags", object([("Name", name_with(&format!("-{name}-{n}")))])))
                .build()
        })
        .collect();
    let first = mp.addresses.keys().next().map(|n| format!("{id}_{}", ident(n))).unwrap_or_default();
    out.push(
        instance
            .add_attribute(("key_name", reference("aws_key_pair.env.key_name")))
            .add_attribute(("user_data", linux_user_data()))
            .add_block(primary(&first))
            .add_block(disk(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB)))
            .add_attribute(("tags", object([("Name", name_with(&format!("-{name}")))])))
            .build(),
    );
    out.extend(attachments(&id, mp.addresses.keys().map(|n| format!("{id}_{}", ident(n)))));
    out.push(eip(&id, &first));
    out
}

/// A network interface, started: on its network's subnet, at `address` (then the machine's
/// further addresses there).
fn interface(id: &str, net: &str, address: &str, group: Expression, mp: Option<&MachinePlan>) -> BlockBuilder {
    let nic = resource("aws_network_interface", id).add_attribute(("subnet_id", reference(&format!("aws_subnet.{}.id", ident(net)))));
    addresses::on_interface(nic, mp, net, address)
        .add_attribute(("security_groups", list([group])))
        .add_attribute(("source_dest_check", boolean(false)))
}

fn primary(interface: &str) -> Block {
    Block::builder("primary_network_interface")
        .add_attribute(("network_interface_id", reference(&format!("aws_network_interface.{interface}.id"))))
        .build()
}

fn disk(gb: u32) -> Block {
    Block::builder("root_block_device")
        .add_attribute(("volume_size", number(u64::from(gb))))
        .build()
}

/// The interfaces after the first, attached to instance `id` in order.
fn attachments(id: &str, interfaces: impl Iterator<Item = String>) -> Vec<Block> {
    (0_u64..)
        .zip(interfaces)
        .skip(1)
        .map(|(i, nic)| {
            resource("aws_network_interface_attachment", &nic)
                .add_attribute(("instance_id", reference(&format!("aws_instance.{id}.id"))))
                .add_attribute(("network_interface_id", reference(&format!("aws_network_interface.{nic}.id"))))
                .add_attribute(("device_index", number(i)))
                .build()
        })
        .collect()
}

fn eip(id: &str, interface: &str) -> Block {
    resource("aws_eip", id)
        .add_attribute(("network_interface", reference(&format!("aws_network_interface.{interface}.id"))))
        .add_attribute(("depends_on", list([reference("aws_internet_gateway.env")])))
        .build()
}

/// The MAC address of an AWS interface, as `ip link` writes it.
fn mac(nic: &str) -> Expression {
    call("lower", [reference(&format!("{nic}.mac_address"))])
}

/// A Windows machine: its security group, its instance (the administrator and WinRM at first
/// boot), its set-up over WinRM.
pub(super) fn windows(env: &Environment, (name, mp): (&str, &MachinePlan), m: &mut Module) {
    let id = ident(name);
    m.commented(
        &format!("Machine `{name}` (Windows): what may reach it."),
        security_group(env, (name, mp), true),
    );
    let Some((net, addr)) = mp.addresses.first() else { return };
    // Windows wants 2 GB and more.
    let mem = mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB).max(2048);
    let instance = resource("aws_instance", &id)
        .add_attribute(("ami", ami(mp)))
        .add_attribute(("instance_type", string(machine_type(mp, mem))))
        .add_attribute(("subnet_id", reference(&format!("aws_subnet.{}.id", ident(net)))))
        .add_attribute(("private_ip", string(addr.to_string())))
        .add_attribute(("associate_public_ip_address", boolean(true)))
        .add_attribute(("vpc_security_group_ids", list([reference(&format!("aws_security_group.{id}.id"))])))
        .add_attribute(("user_data", windows_user_data()))
        .add_block(disk(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB).max(50)))
        .add_blocks(disks::aws(&mp.disks))
        .add_attribute(("tags", object([("Name", name_with(&format!("-{name}")))])));
    let instance = nesting(addresses::on_instance(instance, mp, net), mp).build();
    m.commented(
        "No key pair: AWS refuses ED25519 keys on Windows, and WinRM uses the generated password.",
        instance,
    );
    m.blocks(windows::blocks(windows::Machine {
        remote: Remote {
            env,
            name,
            plan: mp,
            replace_on: reference(&format!("aws_instance.{id}.id")),
            host: reference(&format!("aws_instance.{id}.public_ip")),
        },
        password: reference(windows::PASSWORD),
        skip: hooks::network_does,
        platform: windows::Platform::default(),
    }));
}

/// First boot on Windows: the `isoloom` administrator with the generated password, WinRM over
/// HTTP, and the auto-stop.
fn windows_user_data() -> Expression {
    let stop = Template::text("Register-ScheduledTask -TaskName isoloom-auto-stop -Action (New-ScheduledTaskAction -Execute shutdown.exe -Argument '/s /t 0') -Trigger (New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(")
        .with_expr(reference("var.auto_stop_minutes"))
        .with_text(")) -User SYSTEM -RunLevel Highest -Force | Out-Null");
    Template::text("<powershell>\n$p = ConvertTo-SecureString '")
        .with_expr(reference(windows::PASSWORD))
        .with_text("' -AsPlainText -Force\nNew-LocalUser -Name isoloom -Password $p -PasswordNeverExpires -AccountNeverExpires | Out-Null\nAdd-LocalGroupMember -Group Administrators -Member isoloom\nEnable-PSRemoting -Force -SkipNetworkProfileCheck | Out-Null\nSet-Item WSMan:\\localhost\\Service\\AllowUnencrypted $true\nSet-Item WSMan:\\localhost\\Service\\Auth\\Basic $true\nNew-NetFirewallRule -DisplayName 'isoloom WinRM' -Direction Inbound -Protocol TCP -LocalPort 5985 -Action Allow | Out-Null\n")
        .with_expr(auto_stop::if_set(stop.into_expression(), string("")))
        .with_text("\n</powershell>\n")
        .into_expression()
}

/// The controller: its security group, an interface on every network at the controller
/// address, its instance and Elastic IP, its set-up over SSH once every machine is set up.
pub(super) fn controller(env: &Environment, m: &mut Module) {
    m.commented(
        "The controller (Ansible): every network may reach it, and SSH from allowed_cidr.",
        controller_group(env),
    );
    let nic = |n: &str| format!("isoloom_controller_{}", ident(n));
    let addresses = controller::addresses(env);
    for (n, at) in &addresses {
        m.block(interface(&nic(n), n, &at.to_string(), reference("aws_security_group.isoloom_controller.id"), None).build());
    }
    let first = env.networks.keys().next().map(|n| nic(n)).unwrap_or_default();
    let (_, mem, _) = controller::size(env);
    m.block(
        resource("aws_instance", "isoloom_controller")
            .add_attribute(("ami", reference(&format!("data.aws_ami.{}.id", ident(controller::OS)))))
            .add_attribute(("instance_type", string(instance_type(mem))))
            .add_attribute(("key_name", reference("aws_key_pair.env.key_name")))
            .add_attribute(("user_data", linux_user_data()))
            .add_block(primary(&first))
            .add_attribute(("tags", object([("Name", name_with("-controller"))])))
            .build(),
    );
    m.block(eip("isoloom_controller", &first));
    m.blocks(attachments("isoloom_controller", env.networks.keys().map(|n| nic(n))));
    let inventory = controller::cloud_inventory(env, |os| image_user(os).to_owned(), WinrmAuth::Basic);
    let ssh = controller::Ssh {
        env,
        replace_on: reference("aws_instance.isoloom_controller.id"),
        host: reference("aws_eip.isoloom_controller.public_ip"),
        user: "admin".to_owned(),
        at_boot: interfaces::up(env, &addresses, &|n| mac(&format!("aws_network_interface.{}", nic(n)))),
        inventory,
    };
    m.block(controller::provisioner(ssh).block());
}

/// The address to stand at for the checks and a session when no Linux machine can.
pub(super) fn controller_ip() -> Expression {
    reference("aws_eip.isoloom_controller.public_ip")
}
