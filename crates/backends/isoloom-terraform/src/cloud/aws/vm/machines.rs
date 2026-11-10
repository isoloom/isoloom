//! Each machine's instance (its interfaces and Elastic IP on several networks), set up from its
//! own user data through the VM core; the Windows machines' and the controller's.

use crate::cloud::user_data_limit::Limit;
use crate::hcl_builder::{BinaryOperator, Block, BlockBuilder, Expression, Module, Template, binary, boolean, call, list, number, object, reference, string};
use crate::machine::controller::WinrmAuth;
use isoloom_plan::{Environment, MachinePlan, Vm};

use super::network::{controller_group, security_group};
use super::{RULES, addresses, image_user, machine_type, nesting};
use crate::Result;
use crate::cloud::auto_stop;
use crate::cloud::aws::{ec2_instance, root_disk};
use crate::cloud::instance::{self, name_with};
use crate::cloud::vm::controller_setup;
use crate::cloud::vm::{disks, hooks, interfaces, own_image};
use crate::machine::user_data::Capped;
use crate::machine::{Remote, controller, user_data, windows};
use crate::module::{ident, resource};

/// How AWS takes a Linux instance's user data: gzipped ([`user_data()`]), 16 KB of it.
const CAP: Capped = Capped {
    limit: Limit(crate::cloud::Cloud::Aws),
    gzipped: true,
};

/// An instance's user data: `script`, gzipped (AWS takes 16 KB: [`CAP`]).
fn user_data(script: Expression) -> (&'static str, Expression) {
    ("user_data_base64", call("base64gzip", [script]))
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

/// The VM's AMI: its own (`vm.image.aws`), else the one looked up for its OS.
fn ami(vm: Vm<'_>) -> Expression {
    own_image::of(vm.plan, own_image::aws).map_or_else(|| reference(&format!("data.aws_ami.{}.id", ident(vm.os))), string)
}

/// A Linux machine: its security group, its instance, set up from its user data.
pub(super) fn linux(env: &Environment, vm: Vm<'_>, m: &mut Module) -> Result<()> {
    let Vm { name, plan: mp, os } = vm;
    let id = ident(name);
    m.commented(&format!("Machine `{name}`: what may reach it."), security_group(env, (name, mp), false));
    let instance = ec2_instance(&id)
        .add_attribute(("ami", ami(vm)))
        .add_attribute(("instance_type", string(machine_type(mp))))
        .add_blocks(disks::aws(&mp.disks));
    // A looked-up AMI says whether it has NitroTPM; an own AMI is the author's to choose.
    let tpm_ami = (mp.tpm.is_some() && own_image::of(mp, own_image::aws).is_none()).then(|| ident(os));
    let at_boot = interfaces::up(env, &mp.addresses, &|n| mac(&format!("aws_network_interface.{id}_{}", ident(n))));
    let user = image_user(os);
    let instance = nesting(instance, mp)
        .add_attribute(user_data(hooks::user_data(env, (name, mp), user, at_boot, CAP)?))
        .add_block(lifecycle("user_data_base64", tpm_ami.as_deref()));
    if addresses::needs_interfaces(mp) {
        m.blocks(several_networks(vm, instance));
    } else {
        m.block(one_network(vm, instance));
    }
    Ok(())
}

/// A Linux machine's root disk: what its spec asks for (the default otherwise), at least what its
/// image needs (AWS refuses a volume smaller than the image's snapshot).
fn root_disk_gb(Vm { plan: mp, os, .. }: Vm<'_>) -> u32 {
    let wanted = mp.size().disk_gb;
    let image = own_image::of(mp, own_image::aws).map_or_else(|| super::image_min_disk_gb(os), |_| 0);
    wanted.max(image)
}

/// The instance's one `lifecycle` block (Terraform takes no second): set up once from its user
/// data (`attribute`, [`instance::set_up_once`]).
///
/// A TPM on AWS is NitroTPM, which the AMI enables (`tpm_support`): with `tpm_ami`, the looked-up
/// AMI of a machine that asks for one, the plan stops when the image has none rather than start
/// a VM without.
fn lifecycle(attribute: &str, tpm_ami: Option<&str>) -> Block {
    let block = instance::set_up_once(reference(attribute));
    let Some(ami) = tpm_ami else { return block.build() };
    let condition = binary(reference(&format!("data.aws_ami.{ami}.tpm_support")), BinaryOperator::Eq, string("v2.0"));
    let precondition = Block::builder("precondition")
        .add_attribute(("condition", condition))
        .add_attribute((
            "error_message",
            string("The machine asks for a TPM (hardware.tpm), and this AMI has no NitroTPM support."),
        ))
        .build();
    block.add_block(precondition).build()
}

/// The instance on its network, at its address.
fn one_network(vm: Vm<'_>, instance: BlockBuilder) -> Block {
    let Vm { name, plan: mp, .. } = vm;
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
        .add_block(root_disk(root_disk_gb(vm)))
        .add_attribute(("tags", object([("Name", name_with(&format!("-{name}")))])))
        .build()
}

/// An interface per network at its address (source/destination check off, so a reply can leave
/// from the interface of the network it's for), the instance on the first, the others attached,
/// and an Elastic IP on the first.
fn several_networks(vm: Vm<'_>, instance: BlockBuilder) -> Vec<Block> {
    let Vm { name, plan: mp, .. } = vm;
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
            .add_block(primary(&first))
            .add_block(root_disk(root_disk_gb(vm)))
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
pub(super) fn windows(env: &Environment, vm: Vm<'_>, m: &mut Module) {
    let Vm { name, plan: mp, .. } = vm;
    let id = ident(name);
    m.commented(
        &format!("Machine `{name}` (Windows): what may reach it."),
        security_group(env, (name, mp), true),
    );
    let Some((net, addr)) = mp.addresses.first() else { return };
    let instance = ec2_instance(&id)
        .add_attribute(("ami", ami(vm)))
        .add_attribute(("instance_type", string(machine_type(mp))))
        .add_attribute(("subnet_id", reference(&format!("aws_subnet.{}.id", ident(net)))))
        .add_attribute(("private_ip", string(addr.to_string())))
        .add_attribute(("associate_public_ip_address", boolean(true)))
        .add_attribute(("vpc_security_group_ids", list([reference(&format!("aws_security_group.{id}.id"))])))
        .add_attribute(("user_data", windows_user_data()))
        .add_block(lifecycle("user_data", None))
        .add_block(root_disk(mp.size().disk_gb.max(50)))
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
        platform: windows::Platform {
            ntlm: true,
            ..windows::Platform::default()
        },
    }));
}

/// First boot on Windows: the `isoloom` administrator with the generated password, WinRM over
/// HTTP with NTLM (Windows' own; Basic, which sends the password with every request, stays off),
/// and the auto-stop. The service takes unencrypted messages: Terraform's WinRM client (masterzen/
/// winrm at its 2020 version) doesn't seal NTLM messages, so a service refusing them would refuse
/// Terraform's set-up; the controller's Ansible (pywinrm) seals its own.
fn windows_user_data() -> Expression {
    let stop = auto_stop::windows_task();
    Template::text("<powershell>\n$p = ConvertTo-SecureString '")
        .with_expr(reference(windows::PASSWORD))
        .with_text("' -AsPlainText -Force\nNew-LocalUser -Name isoloom -Password $p -PasswordNeverExpires -AccountNeverExpires | Out-Null\nAdd-LocalGroupMember -Group Administrators -Member isoloom\nEnable-PSRemoting -Force -SkipNetworkProfileCheck | Out-Null\nSet-Item WSMan:\\localhost\\Service\\AllowUnencrypted $true\nNew-NetFirewallRule -DisplayName 'isoloom WinRM' -Direction Inbound -Protocol TCP -LocalPort 5985 -Action Allow | Out-Null\n")
        .with_expr(auto_stop::if_expires(stop.into_expression(), string("")))
        .with_text("\n</powershell>\n")
        .into_expression()
}

/// The controller: its security group, an interface on every network at the controller
/// address, its instance and Elastic IP, set up from its user data (it waits for every machine's
/// own set-up before the playbooks).
pub(super) fn controller(env: &Environment, m: &mut Module) -> Result<()> {
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
    let (_, _, disk) = controller::size(env);
    let inventory = controller::cloud_inventory(env, |os| image_user(os).to_owned(), WinrmAuth::Ntlm);
    let at_boot = interfaces::up(env, &addresses, &|n| mac(&format!("aws_network_interface.{}", nic(n))));
    let user = |os: &str| image_user(os).to_owned();
    let setup = controller_setup::set_up(env, &user, inventory, &at_boot);
    m.block(
        ec2_instance("isoloom_controller")
            .add_attribute(("ami", reference(&format!("data.aws_ami.{}.id", ident(controller::OS)))))
            .add_attribute(("instance_type", string(RULES.controller_size(env))))
            .add_attribute(("key_name", reference("aws_key_pair.env.key_name")))
            .add_attribute(user_data(user_data::capped(&setup, "the controller", CAP)?))
            .add_block(lifecycle("user_data_base64", None))
            .add_block(primary(&first))
            .add_block(root_disk(disk))
            .add_attribute(("tags", object([("Name", name_with(&format!("-{}", controller::NAME)))])))
            .build(),
    );
    m.block(eip("isoloom_controller", &first));
    m.blocks(attachments("isoloom_controller", env.networks.keys().map(|n| nic(n))));
    Ok(())
}

/// The address to stand at for the checks and a session when no Linux machine can.
pub(super) fn controller_ip() -> Expression {
    reference("aws_eip.isoloom_controller.public_ip")
}
