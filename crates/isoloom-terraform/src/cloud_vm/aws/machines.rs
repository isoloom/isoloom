//! Each machine's instance (its interfaces and Elastic IP on several networks) and its set-up
//! through the VM core; the Windows machines' and the controller's.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, BlockBuilder, Expression, Module, Template, binary, boolean, call, list, number, object, reference, string,
};
use isoloom_spec::validate::Cidr;
use isoloom_spec::{DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::network::{controller_group, name_with, resource, security_group};
use super::{image_user, instance_type, linux_user_data, r, vpc_does};
use crate::vm::linux::{self, Hooks};
use crate::vm::{cloud, controller, gateways, os, own_image, res, windows};

/// The address Terraform reaches a machine at: its instance's own public address on one
/// network, its Elastic IP on several (AWS doesn't auto-assign one then).
pub fn public_ip(env: &Environment, name: &str) -> Expression {
    let id = res(name);
    if env.machines[name].addresses.len() > 1 {
        r(&format!("aws_eip.{id}.public_ip"))
    } else {
        r(&format!("aws_instance.{id}.public_ip"))
    }
}

/// The machine's AMI: its own (`vm.image.aws`), else the one looked up for its OS.
fn ami(mp: &isoloom_plan::MachinePlan) -> Expression {
    own_image::of(mp, own_image::aws).map_or_else(|| r(&format!("data.aws_ami.{}.id", res(os(mp).unwrap_or_default()))), string)
}

/// A Linux machine: its security group, its instance, its set-up over SSH.
pub fn linux(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    m.commented(&format!("Machine `{name}`: what may reach it."), security_group(env, name, false));
    let mem = mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB);
    let mut instance = resource("aws_instance", &id)
        .add_attribute(("ami", ami(mp)))
        .add_attribute(("instance_type", string(super::machine_type(mp, mem))));
    // A looked-up AMI says whether it has NitroTPM; an own AMI is the author's to choose.
    if mp.tpm.is_some() && own_image::of(mp, own_image::aws).is_none() {
        instance = instance.add_block(nitro_tpm(&res(os(mp).unwrap_or_default())));
    }
    let instance = super::nesting(instance, mp);
    if mp.addresses.len() > 1 {
        m.blocks(several_networks(env, name, instance));
    } else {
        m.block(one_network(env, name, instance));
    }
    let mut at_boot = cloud::interfaces(env, &mp.addresses, &|n| mac(&format!("aws_network_interface.{id}_{}", res(n))));
    at_boot.extend(gateways::at_boot(mp));
    let hooks = Hooks {
        at_boot,
        after_reboot: vec![cloud::reschedule_auto_stop()],
        before_egress: cloud::redirects(mp),
        skip: vpc_does,
    };
    let machine = linux::Machine {
        env,
        name,
        replace_on: r(&format!("aws_instance.{id}.id")),
        host: public_ip(env, name),
        user: image_user(os(mp).unwrap_or_default()).to_string(),
        hooks,
    };
    m.block(linux::provisioner(machine).block());
}

/// A TPM on AWS is NitroTPM, which the AMI enables (`tpm_support`): the plan stops when the
/// image has none rather than start a VM without.
fn nitro_tpm(ami: &str) -> Block {
    let condition = binary(r(&format!("data.aws_ami.{ami}.tpm_support")), BinaryOperator::Eq, string("v2.0"));
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
fn one_network(env: &Environment, name: &str, instance: BlockBuilder) -> Block {
    let mp = &env.machines[name];
    let id = res(name);
    let Some((net, addr)) = mp.addresses.first() else { return instance.build() };
    // A gateway forwards traffic that isn't its own.
    let instance = if mp.is_gateway {
        instance.add_attribute(("source_dest_check", boolean(false)))
    } else {
        instance
    };
    instance
        .add_attribute(("subnet_id", r(&format!("aws_subnet.{}.id", res(net)))))
        .add_attribute(("private_ip", string(addr.to_string())))
        .add_attribute(("associate_public_ip_address", boolean(true)))
        .add_attribute(("key_name", r("aws_key_pair.env.key_name")))
        .add_attribute(("vpc_security_group_ids", list([r(&format!("aws_security_group.{id}.id"))])))
        .add_attribute(("user_data", linux_user_data()))
        .add_block(disk(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB)))
        .add_attribute(("tags", object([("Name", name_with(&format!("-{name}")))])))
        .build()
}

/// An interface per network at its address (source/destination check off, so a reply can leave
/// from the interface of the network it's for), the instance on the first, the others attached,
/// and an Elastic IP on the first.
fn several_networks(env: &Environment, name: &str, instance: BlockBuilder) -> Vec<Block> {
    let mp = &env.machines[name];
    let id = res(name);
    let group = r(&format!("aws_security_group.{id}.id"));
    let mut out: Vec<Block> = mp
        .addresses
        .iter()
        .map(|(n, a)| {
            interface(&format!("{id}_{}", res(n)), n, &a.to_string(), group.clone())
                .add_attribute(("tags", object([("Name", name_with(&format!("-{name}-{n}")))])))
                .build()
        })
        .collect();
    let first = mp.addresses.keys().next().map(|n| format!("{id}_{}", res(n))).unwrap_or_default();
    out.push(
        instance
            .add_attribute(("key_name", r("aws_key_pair.env.key_name")))
            .add_attribute(("user_data", linux_user_data()))
            .add_block(primary(&first))
            .add_block(disk(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB)))
            .add_attribute(("tags", object([("Name", name_with(&format!("-{name}")))])))
            .build(),
    );
    out.extend(attachments(&id, mp.addresses.keys().map(|n| format!("{id}_{}", res(n)))));
    out.push(eip(&id, &first));
    out
}

/// A network interface, started: on its network's subnet, at `address`.
fn interface(id: &str, net: &str, address: &str, group: Expression) -> BlockBuilder {
    resource("aws_network_interface", id)
        .add_attribute(("subnet_id", r(&format!("aws_subnet.{}.id", res(net)))))
        .add_attribute(("private_ips", list([string(address)])))
        .add_attribute(("security_groups", list([group])))
        .add_attribute(("source_dest_check", boolean(false)))
}

fn primary(interface: &str) -> Block {
    Block::builder("primary_network_interface")
        .add_attribute(("network_interface_id", r(&format!("aws_network_interface.{interface}.id"))))
        .build()
}

fn disk(gb: u32) -> Block {
    Block::builder("root_block_device")
        .add_attribute(("volume_size", number(u64::from(gb))))
        .build()
}

/// The interfaces after the first, attached to instance `id` in order.
fn attachments(id: &str, interfaces: impl Iterator<Item = String>) -> Vec<Block> {
    interfaces
        .enumerate()
        .skip(1)
        .map(|(i, nic)| {
            resource("aws_network_interface_attachment", &nic)
                .add_attribute(("instance_id", r(&format!("aws_instance.{id}.id"))))
                .add_attribute(("network_interface_id", r(&format!("aws_network_interface.{nic}.id"))))
                .add_attribute(("device_index", number(i as u64)))
                .build()
        })
        .collect()
}

fn eip(id: &str, interface: &str) -> Block {
    resource("aws_eip", id)
        .add_attribute(("network_interface", r(&format!("aws_network_interface.{interface}.id"))))
        .add_attribute(("depends_on", list([r("aws_internet_gateway.env")])))
        .build()
}

/// The MAC address of an AWS interface, as `ip link` writes it.
fn mac(nic: &str) -> Expression {
    call("lower", [r(&format!("{nic}.mac_address"))])
}

/// A Windows machine: its security group, its instance (the administrator and WinRM at first
/// boot), its set-up over WinRM.
pub fn windows(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    m.commented(&format!("Machine `{name}` (Windows): what may reach it."), security_group(env, name, true));
    let Some((net, addr)) = mp.addresses.first() else { return };
    // Windows wants 2 GB and more.
    let mem = mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB).max(2048);
    let instance = resource("aws_instance", &id)
        .add_attribute(("ami", ami(mp)))
        .add_attribute(("instance_type", string(super::machine_type(mp, mem))))
        .add_attribute(("subnet_id", r(&format!("aws_subnet.{}.id", res(net)))))
        .add_attribute(("private_ip", string(addr.to_string())))
        .add_attribute(("associate_public_ip_address", boolean(true)))
        .add_attribute(("vpc_security_group_ids", list([r(&format!("aws_security_group.{id}.id"))])))
        .add_attribute(("user_data", windows_user_data()))
        .add_block(disk(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB).max(50)))
        .add_attribute(("tags", object([("Name", name_with(&format!("-{name}")))])));
    let instance = super::nesting(instance, mp).build();
    m.commented(
        "No key pair: AWS refuses ED25519 keys on Windows, and WinRM uses the generated password.",
        instance,
    );
    m.blocks(windows::blocks(windows::Machine {
        env,
        name,
        replace_on: r(&format!("aws_instance.{id}.id")),
        host: r(&format!("aws_instance.{id}.public_ip")),
        password: r(cloud::WINDOWS_PASSWORD),
        skip: vpc_does,
        platform: windows::Platform::default(),
    }));
}

/// First boot on Windows: the `isoloom` administrator with the generated password, WinRM over
/// HTTP, and the auto-stop.
fn windows_user_data() -> Expression {
    let stop = Template::text("Register-ScheduledTask -TaskName isoloom-auto-stop -Action (New-ScheduledTaskAction -Execute shutdown.exe -Argument '/s /t 0') -Trigger (New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(")
        .with_expr(r("var.auto_stop_minutes"))
        .with_text(")) -User SYSTEM -RunLevel Highest -Force | Out-Null");
    Template::text("<powershell>\n$p = ConvertTo-SecureString '")
        .with_expr(r("random_password.windows.result"))
        .with_text("' -AsPlainText -Force\nNew-LocalUser -Name isoloom -Password $p -PasswordNeverExpires -AccountNeverExpires | Out-Null\nAdd-LocalGroupMember -Group Administrators -Member isoloom\nEnable-PSRemoting -Force -SkipNetworkProfileCheck | Out-Null\nSet-Item WSMan:\\localhost\\Service\\AllowUnencrypted $true\nSet-Item WSMan:\\localhost\\Service\\Auth\\Basic $true\nNew-NetFirewallRule -DisplayName 'isoloom WinRM' -Direction Inbound -Protocol TCP -LocalPort 5985 -Action Allow | Out-Null\n")
        .with_expr(cloud::auto_stop(stop.into_expression(), string("")))
        .with_text("\n</powershell>\n")
        .into_expression()
}

/// The controller: its security group, an interface on every network at the controller
/// address, its instance and Elastic IP, its set-up over SSH once every machine is set up.
pub fn controller(env: &Environment, m: &mut Module) {
    m.commented(
        "The controller (Ansible): every network may reach it, and SSH from allowed_cidr.",
        controller_group(env),
    );
    let nic = |n: &str| format!("isoloom_controller_{}", res(n));
    for (n, p) in &env.networks {
        let at = Cidr::parse(&p.cidr).map(|c| c.controller().to_string()).unwrap_or_default();
        m.block(interface(&nic(n), n, &at, r("aws_security_group.isoloom_controller.id")).build());
    }
    let first = env.networks.keys().next().map(|n| nic(n)).unwrap_or_default();
    let (_, mem, _) = controller::size(env);
    m.block(
        resource("aws_instance", "isoloom_controller")
            .add_attribute(("ami", r(&format!("data.aws_ami.{}.id", res(controller::OS)))))
            .add_attribute(("instance_type", string(instance_type(mem))))
            .add_attribute(("key_name", r("aws_key_pair.env.key_name")))
            .add_attribute(("user_data", linux_user_data()))
            .add_block(primary(&first))
            .add_attribute(("tags", object([("Name", name_with("-controller"))])))
            .build(),
    );
    m.block(eip("isoloom_controller", &first));
    m.blocks(attachments("isoloom_controller", env.networks.keys().map(|n| nic(n))));
    let addresses = env
        .networks
        .iter()
        .map(|(n, p)| (n.clone(), Cidr::parse(&p.cidr).map_or(std::net::Ipv4Addr::UNSPECIFIED, Cidr::controller)))
        .collect();
    let inventory = controller::cloud_inventory(env, |os| image_user(os).to_string(), isoloom_render::inventory::WinrmAuth::Basic);
    let ssh = controller::Ssh {
        env,
        replace_on: r("aws_instance.isoloom_controller.id"),
        host: r("aws_eip.isoloom_controller.public_ip"),
        user: "admin".into(),
        at_boot: cloud::interfaces(env, &addresses, &|n| mac(&format!("aws_network_interface.{}", nic(n)))),
        inventory,
    };
    m.block(controller::provisioner(ssh).block());
}

/// The address to stand at for the checks and a session when no Linux machine can.
pub fn controller_ip() -> Expression {
    reference("aws_eip.isoloom_controller.public_ip")
}
