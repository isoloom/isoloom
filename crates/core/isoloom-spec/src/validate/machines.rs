//! `machines`: names, addresses, dependencies, size, names they answer to, implementations
//! (`docker:`, `vm:`, `external:`), network appliances, and the start order.

use std::collections::HashMap;

use super::{Blocks, Problems, block, dns_label};
use crate::model::{Appliance, DockerImpl, Machine, Protocol, Spec, VmImpl};

/// One machine being checked: the spec, its name and definition, and its path.
pub(super) struct Subject<'a> {
    pub spec: &'a Spec,
    pub name: &'a str,
    pub m: &'a Machine,
    /// `machines.<name>`.
    pub at: String,
}

/// At least one machine (unless the environment is cloud services), and one access machine at most.
pub(super) fn declared(spec: &Spec, p: &mut Problems) {
    if spec.machines.is_empty() && spec.cloud.is_none() {
        p.add("machines", "declare at least one machine");
    }
    let access = spec.machines.values().filter(|m| m.access).count();
    if access > 1 {
        p.add("machines", format!("only one machine can be the access machine (found {access})"));
    }
}

/// Network appliances (`docker.appliance`): Docker machines Isoloom wires as their image expects.
pub(super) fn appliances(spec: &Spec, p: &mut Problems) {
    for (name, m) in &spec.machines {
        let Some(d) = m.docker.as_ref().filter(|d| d.appliance.is_some()) else {
            continue;
        };
        let at = format!("machines.{name}");
        if m.vm.is_some() {
            p.add(format!("{at}.vm"), "a network appliance runs on the Docker targets only: leave `vm` out");
        }
        if m.access {
            p.add(format!("{at}.access"), "a network appliance can't be the access machine");
        }
        if d.idle || !d.init.is_empty() {
            p.add(format!("{at}.docker"), "a network appliance runs its own OS: no `idle` or `init`");
        }
        if let Some(c) = &d.config
            && (c.starts_with('/') || c.split('/').any(|segment| segment == ".."))
        {
            p.add(format!("{at}.docker.config"), "a path in the project");
        }
    }
}

/// Every machine, in order; addresses and published ports must be unique across machines.
pub(super) fn each(spec: &Spec, blocks: &Blocks, p: &mut Problems) {
    let mut taken: HashMap<(String, u8), String> = HashMap::new();
    let mut published: HashMap<(u16, Protocol), String> = HashMap::new();
    for (name, m) in &spec.machines {
        super::settings::machine(name, m, p);
        super::hardware::machine(name, m, p);
        let s = Subject {
            spec,
            name,
            m,
            at: format!("machines.{name}"),
        };
        name_and_networks(&s, p);
        addresses(&s, blocks, &mut taken, p);
        more_addresses(&s, blocks, &mut taken, p);
        super::volumes::of_machine(&s, p);
        super::ports::of_machine(&s, &mut published, p);
        for (i, input) in m.inputs.iter().enumerate() {
            if !spec.inputs.contains(input) {
                p.unknown(format!("{}.inputs[{i}]", s.at), format!("`{input}` isn't declared in the spec's `inputs`"));
            }
        }
        super::ports::depends_on(&s, p);
        super::os::of_machine(&s, p);
        resources(&s, p);
        aliases(&s, p);
        if m.docker.is_none() && m.vm.is_none() && !m.access {
            p.add(&s.at, "give the machine a form: its `os`, plus an `app:` (or nothing more, the OS alone)");
        }
        if let Some(d) = &m.docker {
            docker(&s.at, d, p);
        }
        if let Some(v) = &m.vm {
            vm(&s, v, p);
        }
    }
}

/// The machine's name (a DNS label short enough for Kubernetes), and a network to be on.
fn name_and_networks(s: &Subject, p: &mut Problems) {
    if !dns_label(s.name) {
        p.add(&s.at, "machine names are DNS names: kebab-case, starting with a letter");
    } else if s.name.len() > 53 {
        // The machine's published entry point is the Kubernetes Service `<name>-published`,
        // a 63-character DNS label; 10 characters are the suffix.
        p.add(&s.at, "at most 53 characters: it becomes the Kubernetes Service `<name>-published`");
    }
    // A LAN's switch takes its ports from the LAN (checked with the networks).
    if s.m.networks.is_empty() && crate::vlans::switched_by(s.spec, s.name).is_none() {
        p.add(format!("{}.networks", s.at), "attach the machine to at least one network");
    }
}

/// The machine's address on each of its networks: usable, the gateway's own when it is one,
/// and not another machine's.
fn addresses(s: &Subject, blocks: &Blocks, taken: &mut HashMap<(String, u8), String>, p: &mut Problems) {
    let name = s.name;
    for (net, octet) in &s.m.networks {
        let nat = format!("{}.networks.{net}", s.at);
        let Some(c) = block(blocks, net) else {
            p.unknown(nat, format!("no network named `{net}`"));
            continue;
        };
        if super::outside::dhcp_address(s.spec, net, *octet, &nat, p) {
            continue;
        }
        let network = s.spec.networks.get(net);
        let is_gateway = network.and_then(|n| n.gateway.as_deref()) == Some(name);
        if is_gateway {
            if *octet == c.gateway_octet() {
                taken.insert((net.clone(), *octet), name.to_owned());
            } else {
                p.add(
                    nat,
                    format!("`{name}` is this network's gateway, so it takes the gateway address: use {}", c.gateway_octet()),
                );
            }
        } else if c.host(*octet).is_none() {
            p.add(nat, unusable(*octet, network.map_or("", |n| n.cidr.as_str())));
        } else if let Some(other) = taken.insert((net.clone(), *octet), name.to_owned()) {
            p.add(nat, format!("address .{octet} is already used by `{other}`"));
        }
    }
}

/// Why `octet` isn't a usable address in the block `cidr`.
fn unusable(octet: u8, cidr: &str) -> String {
    format!(
        "{octet} isn't a usable address in {cidr} (reserved: .0, .1 for the gateway, the two last addresses for the controller and the router, and the broadcast address)"
    )
}

/// The machine's further addresses on a network (`lan: [10, 11]`): each usable in the
/// network's block, written once, and not another machine's. Only on the environment's own
/// networks: an outside network's addresses are its own LAN's or its DHCP server's.
fn more_addresses(s: &Subject, blocks: &Blocks, taken: &mut HashMap<(String, u8), String>, p: &mut Problems) {
    for (net, octets) in &s.m.more_addresses {
        let nat = format!("{}.networks.{net}", s.at);
        let Some(c) = block(blocks, net) else { continue };
        if s.spec.networks.get(net).is_some_and(crate::model::Network::is_outside) {
            p.add(
                nat,
                "several addresses only on the environment's own networks: an outside network's addresses are its LAN's",
            );
            continue;
        }
        let mut seen = vec![s.m.networks.get(net).copied().unwrap_or_default()];
        for octet in octets {
            if seen.contains(octet) {
                p.add(&nat, format!("address .{octet} is written twice"));
            } else if c.host(*octet).is_none() {
                p.add(&nat, unusable(*octet, s.spec.networks.get(net).map_or("", |n| n.cidr.as_str())));
            } else if let Some(other) = taken.insert((net.clone(), *octet), s.name.to_owned()) {
                p.add(&nat, format!("address .{octet} is already used by `{other}`"));
            }
            seen.push(*octet);
        }
    }
}

/// A VM needs room to boot; a container only what its process uses (a Go service capped at
/// 64 MB is common), so the VM floor applies to machines that can be VMs.
fn resources(s: &Subject, p: &mut Problems) {
    let Some(r) = s.m.resources else { return };
    if s.m.vm.is_some() {
        if r.cpus == Some(0) || r.memory_mb.is_some_and(|v| v < 256) || r.disk_gb.is_some_and(|v| v < 5) {
            p.add(format!("{}.resources", s.at), "too small for a VM: at least 1 cpu, 256 MB and 5 GB");
        }
    } else if r.cpus == Some(0) || r.memory_mb.is_some_and(|v| v < 16) {
        p.add(format!("{}.resources", s.at), "too small: at least 1 cpu and 16 MB");
    }
}

/// Whether `a` is a DNS name (lowercase labels, each at most 63 characters).
pub(super) fn dns_name(a: &str) -> bool {
    a.len() <= 253
        && a.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && l.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                && !l.starts_with('-')
                && !l.ends_with('-')
        })
}

/// The other names a machine answers to: DNS names, no other machine's, not on clones.
fn aliases(s: &Subject, p: &mut Problems) {
    let (spec, name, at) = (s.spec, s.name, &s.at);
    for (i, a) in s.m.aliases.iter().enumerate() {
        if !dns_name(a) {
            p.add(format!("{at}.aliases[{i}]"), format!("`{a}` isn't a DNS name like api.example.com (lowercase)"));
        } else if spec.machines.contains_key(a) || spec.machines.iter().any(|(o, om)| o != name && om.aliases.contains(a)) {
            p.add(format!("{at}.aliases[{i}]"), format!("`{a}` already names another machine"));
        }
    }
    if !s.m.aliases.is_empty() && s.m.count.is_some() {
        p.add(format!("{at}.aliases"), "clones (`count`) can't share names: give aliases to single machines");
    }
}

/// The `docker:` implementation: an image or a build (a Dynamips router: its firmware).
fn docker(at: &str, d: &DockerImpl, p: &mut Problems) {
    // A Dynamips router: Isoloom builds the emulator; the IOS image is the firmware.
    let emulated = d.appliance == Some(Appliance::CiscoDynamips);
    if emulated {
        if d.image.is_some() || d.build.is_some() {
            p.add(
                format!("{at}.docker"),
                "a Dynamips router's container is Isoloom's: give its IOS image as `firmware`, not `image` or `build`",
            );
        }
        if d.firmware.is_none() {
            p.add(format!("{at}.docker.firmware"), "the IOS image (.bin) to boot, a file in the project");
        }
    } else if d.firmware.is_some() {
        p.add(
            format!("{at}.docker.firmware"),
            "only a Dynamips router (`appliance: cisco-dynamips`) boots a firmware",
        );
    }
    match (&d.image, &d.build) {
        // A Dynamips router's container, and the image Isoloom generates from steps, are Isoloom's.
        _ if emulated || d.generated.is_some() => {}
        (Some(_), Some(_)) => p.add(format!("{at}.docker"), "use `image` or `build`, not both"),
        (None, None) => p.add(format!("{at}.docker"), "set `image` (a published image) or `build` (a folder in the project)"),
        _ => {}
    }
    if d.build.is_none() && (d.dockerfile.is_some() || d.target.is_some() || !d.args.is_empty()) {
        p.add(format!("{at}.docker"), "`dockerfile`, `target` and `args` go with `build` (the context)");
    }
    if let Some(t) = d.target.as_deref().filter(|t| !stage_name(t)) {
        p.add(
            format!("{at}.docker.target"),
            format!("`{t}` isn't a Dockerfile stage name (`FROM ... AS <name>`: letters, digits, `-`, `_` and `.`, starting with a letter)"),
        );
    }
}

/// A Dockerfile stage name, as `FROM ... AS <name>` takes it.
fn stage_name(t: &str) -> bool {
    t.starts_with(|c: char| c.is_ascii_alphabetic()) && t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The `vm:` implementation: a known OS, Linux-only steps on Linux, and steps to run.
fn vm(s: &Subject, v: &VmImpl, p: &mut Problems) {
    let at = &s.at;
    if v.os.is_empty() {
        p.add(
            format!("{at}.vm.os"),
            "name the machine's OS with `os: <family>@<version>` (`debian@12`), here or on the machine (or share one through `common:` or a group)",
        );
    }
    // `reboot` is a Linux step. A Windows machine restarts from the environment's playbooks
    // (`ansible.windows.win_reboot`), which run from the controller and wait for it to come
    // back; its `.ps1` steps are one script on the cloud targets.
    if crate::images::is_windows(&v.os) {
        for (i, step) in v.provision.iter().enumerate() {
            if crate::model::is_reboot(step) {
                p.add(
                    format!("{at}.vm.provision[{i}]"),
                    "`reboot` is a Linux step: restart a Windows machine from the environment's playbooks (`ansible.windows.win_reboot`)",
                );
            }
        }
    }
}

/// The start order has no cycle (`depends_on`, gateways, shared volumes).
pub(super) fn cycle(spec: &Spec, p: &mut Problems) {
    let Some(cycle) = super::dependency_cycle(spec) else { return };
    let hint = if spec.networks.values().any(|n| n.gateway.is_some()) {
        " (machines on a gateway's network also start after it)"
    } else {
        ""
    };
    p.add("machines", format!("depends_on forms a cycle: {}{hint}", cycle.join(" -> ")));
}

/// External machines: an address, a sane port, a user name.
pub(super) fn external(spec: &Spec, p: &mut Problems) {
    for (name, m) in &spec.machines {
        let Some(e) = &m.external else { continue };
        let at = format!("machines.{name}.external");
        if e.address.trim().is_empty() || e.address.chars().any(|c| c.is_whitespace() || c == '@' || c == '\'' || c == '"') {
            p.add(
                format!("{at}.address"),
                "an address or hostname reachable from here, like 192.168.1.20 or dc01.lab.local",
            );
        }
        if e.port == Some(0) {
            p.add(format!("{at}.port"), "1 to 65535");
        }
        if e.user.as_deref().is_some_and(|u| u.trim().is_empty() || u.contains(['@', ' '])) {
            p.add(format!("{at}.user"), "a user name");
        }
    }
}

#[cfg(test)]
mod tests;
