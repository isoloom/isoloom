//! Each machine's security group, public IP, interfaces and VM, and its set-up through the VM
//! core; the Windows machines' and the controller's.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, Template, boolean, call, list, number, reference, string};
use isoloom_spec::validate::Cidr;
use isoloom_spec::{DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::network::{Nic, Nsg, nic, nic_id, placed, public_ip};
use super::{USER, image, public_ip as address_of, size, vnet_does};
use crate::vm::cloud::{self, name_with};
use crate::vm::linux::{self, Hooks};
use crate::vm::{controller, os, own_image, res, windows};

/// A Linux machine: its security group, public IP, interfaces and VM, its set-up over SSH.
pub fn linux(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    m.commented(&format!("Machine `{name}`: what may reach it."), group(env, name, false));
    m.block(public_ip(&id, name));
    m.blocks(nics(&id, name, mp));
    let memory = mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB);
    let disk = mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB).max(30);
    let ids = mp
        .addresses
        .keys()
        .map(|n| reference(&format!("azurerm_network_interface.{}.id", nic_id(&id, n))));
    let own = own_image::of(mp, own_image::azure);
    m.block(linux_vm(
        &id,
        name,
        (os(mp).unwrap_or_default(), own),
        (memory, disk, mp.tpm.is_some()),
        list(ids),
    ));
    let hooks = Hooks {
        at_boot: cloud::interfaces(env, &mp.addresses, &|n| mac(&nic_id(&id, n))),
        after_reboot: vec![cloud::reschedule_auto_stop()],
        before_egress: cloud::redirects(mp),
        skip: vnet_does,
    };
    let machine = linux::Machine {
        env,
        name,
        replace_on: reference(&format!("azurerm_linux_virtual_machine.{id}.id")),
        host: address_of(&id),
        user: USER.to_string(),
        hooks,
    };
    m.block(linux::provisioner(machine).block());
}

/// The MAC address of an Azure interface (uppercase, dash-separated), as `ip link` writes it.
fn mac(nic: &str) -> Expression {
    let raw = reference(&format!("azurerm_network_interface.{nic}.mac_address"));
    call("lower", [call("replace", [raw, string("-"), string(":")])])
}

/// An interface per network (the first carries the public IP; forwarding on for a machine on
/// several), each with its association to the machine's security group.
fn nics(id: &str, who: &str, mp: &MachinePlan) -> Vec<Block> {
    controller_nics(id, who, &mp.addresses, &mp.addresses6)
}

/// The same from the addresses themselves (each IPv6 one beside its network's IPv4 one).
fn controller_nics(
    id: &str,
    who: &str,
    addresses: &indexmap::IndexMap<String, std::net::Ipv4Addr>,
    addresses6: &indexmap::IndexMap<String, std::net::Ipv6Addr>,
) -> Vec<Block> {
    let forwarding = addresses.len() > 1;
    addresses
        .iter()
        .enumerate()
        .flat_map(|(i, (net, address))| {
            nic(&Nic {
                id,
                who,
                net,
                address: *address,
                address6: addresses6.get(net).copied(),
                primary: i == 0,
                forwarding,
            })
        })
        .collect()
}

/// A Linux VM (memory in MB, disk in GB, and whether it has a virtual TPM), as `isoloom` with
/// the operator's key; the auto-stop scheduled at first boot.
fn linux_vm(id: &str, who: &str, (os, own): (&str, Option<&str>), (memory, disk, tpm): (u32, u32, bool), nics: Expression) -> Block {
    let custom_data = cloud::auto_stop(call("base64encode", [cloud::shutdown_script()]), isoloom_render::hcl_builder::null());
    let vm = trusted_launch(placed("azurerm_linux_virtual_machine", id, name_with(&format!("-{who}"))), tpm)
        .add_attribute(("size", string(size(memory))))
        .add_attribute(("admin_username", string(USER)))
        .add_attribute(("network_interface_ids", nics))
        .add_attribute(("custom_data", custom_data))
        .add_block(
            Block::builder("admin_ssh_key")
                .add_attribute(("username", string(USER)))
                .add_attribute(("public_key", reference("var.ssh_public_key")))
                .build(),
        )
        .add_block(os_disk(disk));
    with_image(vm, (os, own), tpm).build()
}

/// Trusted launch, when the VM has a TPM: a virtual TPM and secure boot (on a generation 2 image).
fn trusted_launch(b: BlockBuilder, tpm: bool) -> BlockBuilder {
    if tpm {
        b.add_attribute(("secure_boot_enabled", boolean(true)))
            .add_attribute(("vtpm_enabled", boolean(true)))
    } else {
        b
    }
}

fn os_disk(gb: u32) -> Block {
    Block::builder("os_disk")
        .add_attribute(("caching", string("ReadWrite")))
        .add_attribute(("storage_account_type", string("StandardSSD_LRS")))
        .add_attribute(("disk_size_gb", number(u64::from(gb))))
        .build()
}

/// The VM's image: its own (`vm.image.azure`, by resource id), else the Marketplace's for its OS.
fn with_image(vm: BlockBuilder, (os, own): (&str, Option<&str>), tpm: bool) -> BlockBuilder {
    match own {
        Some(id) => vm.add_attribute(("source_image_id", string(id))),
        None => vm.add_block(source_image(os, tpm)),
    }
}

/// An OS's Marketplace image; with a TPM, its generation 2 build (trusted launch needs one).
fn source_image(os: &str, tpm: bool) -> Block {
    let (publisher, offer, sku) = image(os).unwrap_or_default();
    let sku = if tpm { sku.strip_suffix("-gen1").unwrap_or(sku) } else { sku };
    Block::builder("source_image_reference")
        .add_attribute(("publisher", string(publisher)))
        .add_attribute(("offer", string(offer)))
        .add_attribute(("sku", string(sku)))
        .add_attribute(("version", string("latest")))
        .build()
}

/// Who may reach a machine: its own networks, its set-up from `allowed_cidr` (SSH, or WinRM on
/// Windows), the networks `reach` opens to it (on their ports, TCP and UDP), and its published
/// ports from `allowed_cidr`.
fn group(env: &Environment, name: &str, windows: bool) -> Block {
    let spec = env.spec();
    let m = &spec.machines[name];
    let allowed = || reference("var.allowed_cidr");
    let mut g = Nsg::new(&res(name), name);
    for n in m.networks.keys() {
        g = g.allow(&format!("net-{n}"), "*", "*", string(env.networks[n].cidr.as_str()));
        if let Some(v6) = &env.networks[n].ipv6 {
            g = g.allow(&format!("net-{n}-ipv6"), "*", "*", string(v6.cidr.as_str()));
        }
    }
    g = if windows {
        g.allow("winrm", "Tcp", "5985", allowed())
    } else {
        g.allow("ssh", "Tcp", "22", allowed())
    };
    let opened = spec
        .reach
        .iter()
        .filter(|x| m.networks.contains_key(&x.to) && !m.networks.contains_key(&x.from));
    for (i, reach) in opened.enumerate() {
        // Each block of the network the rule opens from: its IPv4 one, and its IPv6 one when
        // both ends are IPv6.
        let from = &env.networks[&reach.from];
        let both6 = from.ipv6.is_some() && env.networks[&reach.to].ipv6.is_some();
        let v6 = from.ipv6.as_ref().filter(|_| both6).map(|v| (v.cidr.as_str(), "-ipv6"));
        for (block, suffix) in std::iter::once((from.cidr.as_str(), "")).chain(v6) {
            let rule = format!("reach-{i}-{}{suffix}", reach.from);
            g = reach_rules(g, &rule, &reach.ports, block);
        }
    }
    for p in &env.machines[name].ports {
        let (rule, protocol) = if p.protocol.is_tcp() {
            (format!("published-{}", p.publish), "Tcp")
        } else {
            (format!("published-{}-udp", p.publish), "Udp")
        };
        g = g.allow(&rule, protocol, &p.publish.to_string(), allowed());
    }
    g.build()
}

/// A Windows machine: its security group, public IP, interface and VM (its name, the
/// administrator and a WinRM listener set at creation), its set-up over WinRM.
pub fn windows(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    m.commented(&format!("Machine `{name}` (Windows): what may reach it."), group(env, name, true));
    m.block(public_ip(&id, name));
    m.blocks(nics(&id, name, mp));
    m.block(windows_vm(env, name, mp));
    m.blocks(windows::blocks(windows::Machine {
        env,
        name,
        replace_on: reference(&format!("azurerm_windows_virtual_machine.{id}.id")),
        host: address_of(&id),
        password: reference(cloud::WINDOWS_PASSWORD),
        skip: vnet_does,
        platform: windows::Platform {
            named: true,
            ntlm: true,
            before: vec![windows_auto_stop()],
        },
    }));
}

fn windows_vm(env: &Environment, name: &str, mp: &MachinePlan) -> Block {
    let id = res(name);
    // Windows wants 2 GB and more, and Azure's images a 127 GB disk.
    let memory = mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB).max(2048);
    let disk = mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB).max(127);
    let ids = env.machines[name]
        .addresses
        .keys()
        .map(|n| reference(&format!("azurerm_network_interface.{}.id", nic_id(&id, n))));
    let vm = trusted_launch(placed("azurerm_windows_virtual_machine", &id, name_with(&format!("-{name}"))), mp.tpm.is_some())
        .add_attribute(("computer_name", string(name.chars().take(15).collect::<String>())))
        .add_attribute(("size", string(size(memory))))
        .add_attribute(("admin_username", string(USER)))
        .add_attribute(("admin_password", reference(cloud::WINDOWS_PASSWORD)))
        .add_attribute(("network_interface_ids", list(ids)))
        .add_block(Block::builder("winrm_listener").add_attribute(("protocol", string("Http"))).build())
        .add_block(os_disk(disk));
    with_image(vm, (os(mp).unwrap_or_default(), own_image::of(mp, own_image::azure)), mp.tpm.is_some()).build()
}

/// The auto-stop on Windows, a scheduled task registered over WinRM (Azure runs no user data
/// there).
fn windows_auto_stop() -> Expression {
    let task = Template::text("powershell -NoProfile -Command \"Register-ScheduledTask -TaskName isoloom-auto-stop -Action (New-ScheduledTaskAction -Execute shutdown.exe -Argument '/s /t 0') -Trigger (New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(")
        .with_expr(reference("var.auto_stop_minutes"))
        .with_text(")) -User SYSTEM -RunLevel Highest -Force\"");
    cloud::auto_stop(task.into_expression(), string("cmd /c ver"))
}

/// The rules a `reach` rule opens from `block`: every port, or each port in TCP and UDP.
fn reach_rules(mut g: Nsg, rule: &str, ports: &[u16], block: &str) -> Nsg {
    if ports.is_empty() {
        return g.allow(rule, "*", "*", string(block));
    }
    for p in ports {
        g = g.allow(&format!("{rule}-{p}-tcp"), "Tcp", &p.to_string(), string(block));
        g = g.allow(&format!("{rule}-{p}-udp"), "Udp", &p.to_string(), string(block));
    }
    g
}

/// The controller: its security group (every network may reach it, and SSH from
/// `allowed_cidr`), an interface on every network at the controller address, its VM, its set-up
/// over SSH once every machine is set up.
pub fn controller(env: &Environment, m: &mut Module) {
    let id = "isoloom_controller";
    let mut g = Nsg::new(id, "controller");
    for (n, p) in &env.networks {
        g = g.allow(&format!("net-{n}"), "*", "*", string(p.cidr.as_str()));
    }
    m.commented(
        "The controller (Ansible): every network may reach it, and SSH from allowed_cidr.",
        g.allow("ssh", "Tcp", "22", reference("var.allowed_cidr")).build(),
    );
    m.block(public_ip(id, "controller"));
    let addresses: indexmap::IndexMap<String, std::net::Ipv4Addr> = env
        .networks
        .iter()
        .map(|(n, p)| (n.clone(), Cidr::parse(&p.cidr).map_or(std::net::Ipv4Addr::UNSPECIFIED, Cidr::controller)))
        .collect();
    m.blocks(controller_nics(id, "controller", &addresses, &indexmap::IndexMap::new()));
    let ids = addresses.keys().map(|n| reference(&format!("azurerm_network_interface.{}.id", nic_id(id, n))));
    let (_, memory, disk) = controller::size(env);
    let disk = disk.unwrap_or(30).max(30);
    m.block(linux_vm(id, "controller", (controller::OS, None), (memory, disk, false), list(ids)));
    let ssh = controller::Ssh {
        env,
        replace_on: reference(&format!("azurerm_linux_virtual_machine.{id}.id")),
        host: address_of(id),
        user: USER.to_string(),
        at_boot: cloud::interfaces(env, &addresses, &|n| mac(&nic_id(id, n))),
        inventory: controller::cloud_inventory(env, |_| USER.to_string(), isoloom_render::inventory::WinrmAuth::Ntlm),
    };
    m.block(controller::provisioner(ssh).block());
}
