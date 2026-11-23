//! Each machine's instance (its interfaces and Elastic IP on several networks), set up from its
//! own user data through the VM core; the Windows machines' and the controller's.

use crate::cloud::user_data_limit::Limit;
use crate::hcl_builder::{BinaryOperator, Block, BlockBuilder, Expression, Module, Template, binary, boolean, call, list, number, object, reference, string};
use crate::machine::controller::WinrmAuth;
use isoloom_plan::{Environment, MachinePlan, Vm};

use super::network::{controller_group, security_group};
use super::resources as res;
use super::{RULES, addresses, image_user, machine_type, nesting};
use crate::Result;
use crate::cloud::auto_stop;
use crate::cloud::aws::{ec2_instance, root_disk};
use crate::cloud::instance::{self, name_with};
use crate::cloud::vm::controller_setup;
use crate::cloud::vm::{disks, hooks, interfaces, own_image};
use crate::machine::user_data::Capped;
use crate::machine::{Remote, controller, user_data, windows};
use crate::module::{ResourceRef, ident};

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
    if env.machines.get(name).is_some_and(|mp| !mp.is_windows() && addresses::needs_interfaces(mp)) {
        res::eip(name).attr("public_ip")
    } else {
        res::instance(name).attr("public_ip")
    }
}

/// The VM's AMI: its own (`vm.image.aws`), else the one looked up for its OS.
fn ami(vm: Vm<'_>) -> Expression {
    own_image::of(vm.plan, own_image::aws).map_or_else(|| reference(&format!("data.aws_ami.{}.id", ident(vm.os))), string)
}

/// A Linux machine: its security group, its instance, set up from its user data.
pub(super) fn linux(env: &Environment, vm: Vm<'_>, module: &mut Module) -> Result<()> {
    let Vm { name, plan: mp, os } = vm;
    module.commented(&format!("Machine `{name}`: what may reach it."), security_group(env, (name, mp), false));
    let instance = ec2_instance(res::instance(name).name())
        .add_attribute(("ami", ami(vm)))
        .add_attribute(("instance_type", string(machine_type(mp))))
        .add_blocks(disks::aws(&mp.disks));
    // A looked-up AMI says whether it has NitroTPM; an own AMI is the author's to choose.
    let tpm_ami = (mp.tpm.is_some() && own_image::of(mp, own_image::aws).is_none()).then(|| ident(os));
    let at_boot = interfaces::up(env, &mp.addresses, &|n| mac(&res::interface(name, n)));
    let user = image_user(os);
    let instance = nesting(instance, mp)
        .add_attribute(user_data(hooks::user_data(env, (name, mp), user, at_boot, CAP)?))
        .add_block(lifecycle("user_data_base64", tpm_ami.as_deref()));
    if addresses::needs_interfaces(mp) {
        module.blocks(several_networks(vm, instance));
    } else {
        module.block(one_network(vm, instance));
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
    let Some((net, addr)) = mp.addresses.first() else { return instance.build() };
    // A gateway forwards traffic that isn't its own.
    let instance = if mp.is_gateway {
        instance.add_attribute(("source_dest_check", boolean(false)))
    } else {
        instance
    };
    let instance = addresses::on_instance(instance, mp, net);
    instance
        .add_attribute(("subnet_id", res::subnet(net).attr("id")))
        .add_attribute(("private_ip", string(addr.to_string())))
        .add_attribute(("associate_public_ip_address", boolean(true)))
        .add_attribute(("key_name", reference("aws_key_pair.env.key_name")))
        .add_attribute(("vpc_security_group_ids", list([res::security_group(name).attr("id")])))
        .add_block(root_disk(root_disk_gb(vm)))
        .add_attribute(("tags", object([("Name", name_with(&format!("-{name}")))])))
        .build()
}

/// An interface per network at its address (source/destination check off, so a reply can leave
/// from the interface of the network it's for), the instance on the first, the others attached,
/// and an Elastic IP on the first.
fn several_networks(vm: Vm<'_>, instance: BlockBuilder) -> Vec<Block> {
    let Vm { name, plan: mp, .. } = vm;
    let group = res::security_group(name).attr("id");
    let mut out: Vec<Block> = mp
        .addresses
        .iter()
        .map(|(n, a)| {
            interface(&res::interface(name, n), n, &a.to_string(), group.clone(), Some(mp))
                .add_attribute(("tags", object([("Name", name_with(&format!("-{name}-{n}")))])))
                .build()
        })
        .collect();
    let first = res::interface(name, mp.addresses.keys().next().map_or("", String::as_str));
    out.push(
        instance
            .add_attribute(("key_name", reference("aws_key_pair.env.key_name")))
            .add_block(primary(&first))
            .add_block(root_disk(root_disk_gb(vm)))
            .add_attribute(("tags", object([("Name", name_with(&format!("-{name}")))])))
            .build(),
    );
    out.extend(attachments(name, mp.addresses.keys()));
    out.push(eip(name, &first));
    out
}

/// A network interface, started: on its network's subnet, at `address` (then the machine's
/// further addresses there).
fn interface(nic: &ResourceRef, net: &str, address: &str, group: Expression, mp: Option<&MachinePlan>) -> BlockBuilder {
    let nic = nic.block().add_attribute(("subnet_id", res::subnet(net).attr("id")));
    addresses::on_interface(nic, mp, net, address)
        .add_attribute(("security_groups", list([group])))
        .add_attribute(("source_dest_check", boolean(false)))
}

fn primary(interface: &ResourceRef) -> Block {
    Block::builder("primary_network_interface")
        .add_attribute(("network_interface_id", interface.attr("id")))
        .build()
}

/// Machine `machine`'s interfaces on `nets` after the first, attached to its instance in order.
fn attachments<'a>(machine: &str, nets: impl Iterator<Item = &'a String>) -> Vec<Block> {
    (0_u64..)
        .zip(nets)
        .skip(1)
        .map(|(i, net)| {
            res::interface_attachment(machine, net)
                .block()
                .add_attribute(("instance_id", res::instance(machine).attr("id")))
                .add_attribute(("network_interface_id", res::interface(machine, net).attr("id")))
                .add_attribute(("device_index", number(i)))
                .build()
        })
        .collect()
}

fn eip(machine: &str, interface: &ResourceRef) -> Block {
    res::eip(machine)
        .block()
        .add_attribute(("network_interface", interface.attr("id")))
        .add_attribute(("depends_on", list([res::internet_gateway().whole()])))
        .build()
}

/// The MAC address of an AWS interface, as `ip link` writes it.
fn mac(nic: &ResourceRef) -> Expression {
    call("lower", [nic.attr("mac_address")])
}

/// A Windows machine: its security group, its instance (the administrator and WinRM at first
/// boot), its set-up over WinRM.
pub(super) fn windows(env: &Environment, vm: Vm<'_>, module: &mut Module) {
    let Vm { name, plan: mp, .. } = vm;
    module.commented(
        &format!("Machine `{name}` (Windows): what may reach it."),
        security_group(env, (name, mp), true),
    );
    let Some((net, addr)) = mp.addresses.first() else { return };
    let instance = ec2_instance(res::instance(name).name())
        .add_attribute(("ami", ami(vm)))
        .add_attribute(("instance_type", string(machine_type(mp))))
        .add_attribute(("subnet_id", res::subnet(net).attr("id")))
        .add_attribute(("private_ip", string(addr.to_string())))
        .add_attribute(("associate_public_ip_address", boolean(true)))
        .add_attribute(("vpc_security_group_ids", list([res::security_group(name).attr("id")])))
        .add_attribute(("user_data", windows_user_data()))
        .add_block(lifecycle("user_data", None))
        .add_block(root_disk(mp.size().disk_gb.max(50)))
        .add_blocks(disks::aws(&mp.disks))
        .add_attribute(("tags", object([("Name", name_with(&format!("-{name}")))])));
    let instance = nesting(addresses::on_instance(instance, mp, net), mp).build();
    module.commented(
        "No key pair: AWS refuses ED25519 keys on Windows, and WinRM uses the generated password.",
        instance,
    );
    module.blocks(windows::blocks(windows::Machine {
        remote: Remote {
            env,
            name,
            plan: mp,
            replace_on: res::instance(name).attr("id"),
            host: res::instance(name).attr("public_ip"),
        },
        password: reference(windows::PASSWORD),
        skip: hooks::network_does,
        platform: windows::Platform {
            ntlm: true,
            ..windows::Platform::default()
        },
    }));
}

/// First boot on Windows: the administrator ([`windows::first_boot_admin`]), WinRM with NTLM
/// (Windows' own; Basic stays off), and the auto-stop. The controller's Ansible (pywinrm) seals
/// its own messages.
fn windows_user_data() -> Expression {
    let stop = auto_stop::windows_task();
    let mut t = Template::text("<powershell>\n");
    t.append(windows::first_boot_admin());
    t.push_text(format!("{}\n", windows::winrm(WinrmAuth::Ntlm)));
    t.push_expr(auto_stop::if_expires(stop.into_expression(), string("")));
    t.push_text("\n</powershell>\n");
    t.into_expression()
}

/// The controller: its security group, an interface on every network at the controller
/// address, its instance and Elastic IP, set up from its user data (it waits for every machine's
/// own set-up before the playbooks).
pub(super) fn controller(env: &Environment, module: &mut Module) -> Result<()> {
    module.commented(
        "The controller (Ansible): every network may reach it, and SSH from allowed_cidr.",
        controller_group(env),
    );
    let addresses = controller::addresses(env);
    for (n, at) in &addresses {
        let group = res::security_group(res::CONTROLLER).attr("id");
        module.block(interface(&res::interface(res::CONTROLLER, n), n, &at.to_string(), group, None).build());
    }
    let first = res::interface(res::CONTROLLER, env.networks.keys().next().map_or("", String::as_str));
    let disk = controller::size(env).disk_gb;
    let inventory = controller::cloud_inventory(env, |os| image_user(os).to_owned(), WinrmAuth::Ntlm);
    let at_boot = interfaces::up(env, &addresses, &|n| mac(&res::interface(res::CONTROLLER, n)));
    let user = |os: &str| image_user(os).to_owned();
    let setup = controller_setup::set_up(env, &user, inventory, &at_boot);
    module.block(
        ec2_instance(res::instance(res::CONTROLLER).name())
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
    module.block(eip(res::CONTROLLER, &first));
    module.blocks(attachments(res::CONTROLLER, env.networks.keys()));
    Ok(())
}

/// The address to stand at for the checks and a session when no Linux machine can.
pub(super) fn controller_ip() -> Expression {
    res::eip(res::CONTROLLER).attr("public_ip")
}
