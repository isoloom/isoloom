//! Each machine's instance (its interfaces and Elastic IP on several networks) and its set-up
//! through the VM core; the Windows machines' and the controller's.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, Template, boolean, call, list, number, object, reference, string};
use isoloom_spec::validate::Cidr;
use isoloom_spec::{DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::network::{controller_group, name_tag, resource, security_group};
use super::{auto_stop, image_user, instance_type, linux_user_data, r, vpc_does};
use crate::vm::linux::{self, Hooks};
use crate::vm::{controller, os, res, windows};

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

/// A Linux machine: its security group, its instance, its set-up over SSH.
pub fn linux(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    m.commented(&format!("Machine `{name}`: what may reach it."), security_group(env, name, false));
    let ami = r(&format!("data.aws_ami.{}.id", res(os(mp).unwrap_or_default())));
    let mem = mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB);
    let instance = resource("aws_instance", &id)
        .add_attribute(("ami", ami))
        .add_attribute(("instance_type", string(instance_type(mem))));
    if mp.addresses.len() > 1 {
        m.blocks(several_networks(env, name, instance));
    } else {
        m.block(one_network(env, name, instance));
    }
    let hooks = Hooks {
        at_boot: interfaces(env, &mp.addresses, &|n| format!("aws_network_interface.{id}_{}", res(n))),
        after_reboot: vec![Template::new().with_expr(auto_stop(
            Template::text("sudo shutdown -h +").with_expr(r("var.auto_stop_minutes")).into_expression(),
            string("true"),
        ))],
        before_egress: redirects(mp),
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

/// The instance on its network, at its address.
fn one_network(env: &Environment, name: &str, instance: BlockBuilder) -> Block {
    let mp = &env.machines[name];
    let id = res(name);
    let Some((net, addr)) = mp.addresses.first() else { return instance.build() };
    instance
        .add_attribute(("subnet_id", r(&format!("aws_subnet.{}.id", res(net)))))
        .add_attribute(("private_ip", string(addr.to_string())))
        .add_attribute(("associate_public_ip_address", boolean(true)))
        .add_attribute(("key_name", r("aws_key_pair.env.key_name")))
        .add_attribute(("vpc_security_group_ids", list([r(&format!("aws_security_group.{id}.id"))])))
        .add_attribute(("user_data", linux_user_data()))
        .add_block(disk(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB)))
        .add_attribute(("tags", object([("Name", name_tag(&format!("-{name}")))])))
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
                .add_attribute(("tags", object([("Name", name_tag(&format!("-{name}-{n}")))])))
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
            .add_attribute(("tags", object([("Name", name_tag(&format!("-{name}")))])))
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

/// The commands bringing up the interfaces after the first (found by MAC, `nic(network)` the
/// interface resource) at their addresses: at every boot, as the SSH user.
fn interfaces(env: &Environment, addresses: &indexmap::IndexMap<String, std::net::Ipv4Addr>, nic: &dyn Fn(&str) -> String) -> Vec<Template> {
    addresses
        .iter()
        .skip(1)
        .map(|(n, a)| {
            let len = Cidr::parse(&env.networks[n].cidr).map_or(32, |c| c.len);
            Template::text("IF=$(ip -o link | grep -i \"")
                .with_expr(call("lower", [r(&format!("{}.mac_address", nic(n)))]))
                .with_text(format!(
                    "\" | awk -F': ' '{{print $2}}'); sudo ip link set \"$IF\" up && (ip -4 addr show \"$IF\" | grep -q {a}/ || sudo ip addr add {a}/{len} dev \"$IF\")"
                ))
        })
        .collect()
}

/// A published port that differs from the service's own, redirected to it inside the machine.
fn redirects(mp: &MachinePlan) -> Vec<Template> {
    let moved: Vec<_> = mp.ports.iter().filter(|p| p.publish != p.port).collect();
    if moved.is_empty() {
        return Vec::new();
    }
    let mut out = vec![Template::text(
        "command -v nft >/dev/null || (sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nftables)",
    )];
    out.extend(moved.into_iter().map(|p| {
        Template::text(format!(
            "sudo nft add table ip isoloom-publish && sudo nft 'add chain ip isoloom-publish prerouting {{ type nat hook prerouting priority -100; }}' && sudo nft add rule ip isoloom-publish prerouting {} dport {} redirect to :{}",
            p.protocol.id(),
            p.publish,
            p.port
        ))
    }));
    out
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
        .add_attribute(("ami", r(&format!("data.aws_ami.{}.id", res(os(mp).unwrap_or_default())))))
        .add_attribute(("instance_type", string(instance_type(mem))))
        .add_attribute(("subnet_id", r(&format!("aws_subnet.{}.id", res(net)))))
        .add_attribute(("private_ip", string(addr.to_string())))
        .add_attribute(("associate_public_ip_address", boolean(true)))
        .add_attribute(("vpc_security_group_ids", list([r(&format!("aws_security_group.{id}.id"))])))
        .add_attribute(("user_data", windows_user_data()))
        .add_block(disk(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB).max(50)))
        .add_attribute(("tags", object([("Name", name_tag(&format!("-{name}")))])))
        .build();
    m.commented(
        "No key pair: AWS refuses ED25519 keys on Windows, and WinRM uses the generated password.",
        instance,
    );
    m.blocks(windows::blocks(windows::Machine {
        env,
        name,
        replace_on: r(&format!("aws_instance.{id}.id")),
        host: r(&format!("aws_instance.{id}.public_ip")),
        password: r("random_password.windows.result"),
        skip: vpc_does,
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
        .with_expr(auto_stop(stop.into_expression(), string("")))
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
            .add_attribute(("tags", object([("Name", name_tag("-controller"))])))
            .build(),
    );
    m.block(eip("isoloom_controller", &first));
    m.blocks(attachments("isoloom_controller", env.networks.keys().map(|n| nic(n))));
    let addresses = env
        .networks
        .iter()
        .map(|(n, p)| (n.clone(), Cidr::parse(&p.cidr).map_or(std::net::Ipv4Addr::UNSPECIFIED, Cidr::controller)))
        .collect();
    let inventory = controller::cloud_inventory(env, |os| image_user(os).to_string());
    let ssh = controller::Ssh {
        env,
        replace_on: r("aws_instance.isoloom_controller.id"),
        host: r("aws_eip.isoloom_controller.public_ip"),
        at_boot: interfaces(env, &addresses, &|n| format!("aws_network_interface.{}", nic(n))),
        inventory,
    };
    m.block(controller::provisioner(ssh).block());
}

/// The address to stand at for the checks and a session when no Linux machine can.
pub fn controller_ip() -> Expression {
    reference("aws_eip.isoloom_controller.public_ip")
}
