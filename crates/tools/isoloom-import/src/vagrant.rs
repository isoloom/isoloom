//! Drafts an `isoloom.yml` from a Vagrantfile, from what it set when it ran against a stand-in
//! `Vagrant` module ([`Recording`]): every setting and call, with loops and variables already
//! evaluated. What the format expresses goes into the draft; everything else becomes a note.

mod boxes;
mod recording;
mod write;

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_spec::net::Cidr;
use serde_json::Value;

pub use recording::Recording;

use crate::draft::{Draft, NoteKind, Notes};
use crate::naming::{label, unique};
use crate::{Error, Result};
use recording::{Call, number, text};

/// A machine drafted from a `define` (or from the whole Vagrantfile when it defines none).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Machine {
    name: String,
    os: &'static str,
    /// The box and its version, when the draft keeps it as an override.
    image: Option<(String, Option<String>)>,
    /// Each private network's block and the machine's last octet on it.
    networks: Vec<(String, u8)>,
    /// (guest port, host port, UDP).
    publish: Vec<(u16, u16, bool)>,
    /// Folders of the computer shared into it: (from, path, read only).
    host_folders: Vec<(String, String, bool)>,
    cpus: Option<u32>,
    memory_mb: Option<u32>,
    /// Its provisioning steps: scripts, playbooks and reboots, in order.
    steps: Vec<String>,
}

/// The step a shell provisioner's `reboot: true` becomes.
const REBOOT: &str = isoloom_spec::model::REBOOT_STEP;

/// A network the machines are on: a private network, or one bridged to the host's LAN.
#[derive(Debug)]
struct Net {
    /// Its block.
    cidr: Cidr,
    /// The name the Vagrantfile gave it (the VirtualBox internal network's), if any.
    name: String,
    /// How it is bridged, for a public network.
    bridged: Option<Bridge>,
}

/// A public network: bridged to an interface of the host.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Bridge {
    /// The host interface Vagrant bridges to, when the Vagrantfile names it.
    interface: Option<String>,
    /// The machines take their address from the LAN's DHCP server.
    dhcp: bool,
    /// With `dhcp`, the default route is the LAN's.
    default_route: bool,
}

/// The block a DHCP public network is given: the Vagrantfile doesn't say the LAN's.
const LAN_PLACEHOLDER: Cidr = Cidr::enclosing(std::net::Ipv4Addr::new(192, 168, 1, 0), 24);

/// What the import collects across the machines.
#[derive(Debug)]
struct Import<'a> {
    /// The top configuration's `vm`: what every machine inherits.
    global: Option<&'a Recording>,
    notes: Notes,
    /// The networks by key: a private network by its block (`192.168.33.0/24`), a public one by
    /// `bridged ` and its block or interface.
    nets: IndexMap<String, Net>,
    /// Host-side Ansible playbooks: run once for the environment.
    env_playbooks: Vec<String>,
}

/// Drafts a spec from a recorded Vagrantfile. `fallback_name` names the environment (usually
/// its folder); `source` is how the Vagrantfile is named in the draft's header.
///
/// # Errors
///
/// [`Error::NoVagrantMachine`] when the Vagrantfile defines no machine of its own.
pub fn draft(recorded: &Recording, fallback_name: &str, source: &str) -> Result<Draft> {
    let mut import = Import {
        global: recorded.child("vm"),
        notes: Notes::default(),
        nets: IndexMap::new(),
        env_playbooks: Vec::new(),
    };
    let mut machines: Vec<Machine> = Vec::new();
    for (raw_name, own) in defined_machines(recorded) {
        if let Some(mut m) = import.machine(&raw_name, own) {
            m.name = unique(&m.name, |n| machines.iter().any(|other| other.name == n));
            machines.push(m);
        }
    }
    if machines.is_empty() {
        return Err(Error::NoVagrantMachine);
    }
    if !import.env_playbooks.is_empty() {
        import.notes.add(
            "provision ansible",
            NoteKind::Changed,
            "host-side Ansible became environment-level `provision:`, run from a controller VM; check its inventory groups",
        );
    }
    let yaml = write::draft_yaml(&import, &machines, &label(fallback_name), source);
    Ok(import.notes.draft(yaml))
}

/// The machines a recording defines, by name, each with its own configuration (none for the
/// one machine of a Vagrantfile without `define`: the top configuration is all it has).
fn defined_machines(recorded: &Recording) -> Vec<(String, Option<&Recording>)> {
    let defines: Vec<&Call> = recorded
        .child("vm")
        .into_iter()
        .flat_map(Recording::calls)
        .filter(|c| c.name() == "define")
        .collect();
    if defines.is_empty() {
        return vec![("default".to_owned(), None)];
    }
    defines.iter().map(|c| (c.first(), c.block())).collect()
}

impl<'a> Import<'a> {
    /// One defined machine, or none for Isoloom's own (its router and controller).
    fn machine(&mut self, raw_name: &str, own: Option<&Recording>) -> Option<Machine> {
        let at = format!("define {raw_name}");
        if raw_name.starts_with("isoloom-") {
            self.notes.add(at, NoteKind::Equivalent, "Isoloom adds its own router and controller");
            return None;
        }
        let name = label(raw_name);
        if name != raw_name {
            self.notes
                .add(at.clone(), NoteKind::Changed, format!("renamed `{name}` (machine names are DNS names)"));
        }
        let vm = own.and_then(|o| o.child("vm"));
        let (os, image) = self.os_and_image(&at, vm);
        let mut m = Machine {
            name: name.clone(),
            os,
            image,
            networks: Vec::new(),
            publish: Vec::new(),
            host_folders: Vec::new(),
            cpus: None,
            memory_mb: None,
            steps: Vec::new(),
        };
        // Calls: the machine's own, after the global ones (Vagrant runs global provisioners first).
        let global = self.global.into_iter().flat_map(Recording::calls).filter(|c| c.name() != "define");
        for c in global.chain(vm.into_iter().flat_map(Recording::calls)) {
            self.call(&at, &mut m, c);
        }
        if let Some(h) = text(self.pick(vm, "hostname")).filter(|h| h != raw_name && *h != name) {
            self.notes.add(
                format!("{at}.vm.hostname"),
                NoteKind::Equivalent,
                format!("`{h}`: the hostname is the machine's name, `{name}`"),
            );
        }
        if m.networks.is_empty() {
            self.notes.add(at, NoteKind::Changed, "no network: put the machine on one");
        }
        Some(m)
    }

    /// A setting of the machine's `vm`, else the top configuration's.
    fn pick<'v>(&self, vm: Option<&'v Recording>, key: &str) -> Option<&'v Value>
    where
        'a: 'v,
    {
        vm.and_then(|v| v.setting(key)).or_else(|| self.global?.setting(key))
    }

    /// The box: an OS name, plus the box itself when it isn't the one Isoloom would use.
    fn os_and_image(&mut self, at: &str, vm: Option<&Recording>) -> (&'static str, Option<(String, Option<String>)>) {
        let boxname = text(self.pick(vm, "box")).unwrap_or_default();
        let windows_hint = text(self.pick(vm, "communicator")).as_deref() == Some("winrm") || text(self.pick(vm, "guest")).as_deref() == Some("windows");
        let read = boxes::os_of(&boxname);
        let os = if windows_hint && !isoloom_spec::os::Kind::of(read.os).is_windows() {
            "windows-server@2019"
        } else {
            read.os
        };
        let version = text(self.pick(vm, "box_version"));
        let image = if boxname.is_empty() {
            self.notes.add(format!("{at}.vm.box"), NoteKind::Changed, format!("no box; assumed {os}"));
            None
        } else if read.builtin && version.is_none() {
            None
        } else {
            if !read.builtin {
                self.notes.add(
                    format!("{at}.vm.box"),
                    NoteKind::Changed,
                    format!("`{boxname}` kept as the machine's image; its OS read as {os}: check it"),
                );
            }
            Some((boxname, version))
        };
        (os, image)
    }

    /// What one call (`vm.network`, `vm.provider`, `vm.provision`...) adds to the machine.
    fn call(&mut self, at: &str, m: &mut Machine, c: &Call) {
        match (c.name(), c.first().as_str()) {
            ("network", "private_network") => self.private_network(at, m, c),
            ("network", "forwarded_port") => self.forwarded_port(at, m, c),
            ("network", "public_network") => self.public_network(at, m, c),
            ("provider", p) => self.provider(&format!("{at}.vm.provider {p}"), m, c.block()),
            ("provision", _) => self.provision(at, m, c),
            ("synced_folder", _) if c.opt("disabled").and_then(Value::as_bool) != Some(true) => self.synced_folder(at, m, c),
            _ => {}
        }
    }

    /// A public network: a `bridged` network, at the machine's fixed address in its block, else
    /// by DHCP from the LAN (the block a placeholder then, said).
    fn public_network(&mut self, at: &str, m: &mut Machine, c: &Call) {
        let at = format!("{at}.vm.network");
        let interface = match c.opt("bridge") {
            Some(Value::Array(list)) => {
                let first = text(list.first());
                if list.len() > 1 {
                    self.notes.add(
                        &at,
                        NoteKind::Changed,
                        format!(
                            "several interfaces to bridge to: the first, `{}`, is kept",
                            first.as_deref().unwrap_or_default()
                        ),
                    );
                }
                first
            }
            other => text(other),
        };
        let fixed = text(c.opt("ip")).and_then(|ip| ip.parse::<Ipv4Addr>().ok());
        let dhcp = fixed.is_none();
        let bridge = Bridge {
            interface: interface.clone(),
            dhcp,
            default_route: dhcp && c.opt("use_dhcp_assigned_default_route").and_then(Value::as_bool) == Some(true),
        };
        let (cidr, octet) = if let Some(ip) = fixed {
            (block(ip, text(c.opt("netmask")).as_deref()).0, ip.octets()[3])
        } else {
            self.notes.add(
                &at,
                NoteKind::Changed,
                format!("by DHCP from the LAN, whose block the Vagrantfile doesn't say: `cidr` is {LAN_PLACEHOLDER}, set it to the LAN's"),
            );
            (LAN_PLACEHOLDER, 0)
        };
        let key = format!("bridged {}", if dhcp { interface.unwrap_or_default() } else { cidr.to_string() });
        self.nets.entry(key.clone()).or_insert_with(|| Net {
            cidr,
            name: "lan".to_owned(),
            bridged: Some(bridge),
        });
        m.networks.push((key, octet));
    }

    /// A synced folder: a host folder of a Linux machine, read only when mounted `ro`. Vagrant's
    /// own share of the project at `/vagrant` is Isoloom's copy of it. How Vagrant shares it
    /// (`type`, owners) isn't kept, and said.
    fn synced_folder(&mut self, at: &str, m: &mut Machine, c: &Call) {
        let at = format!("{at}.vm.synced_folder");
        let (from, path) = (c.first(), c.arg(1).unwrap_or_default());
        if from.trim_end_matches('/') == "." && path.trim_end_matches('/') == "/vagrant" {
            self.notes.add(
                at,
                NoteKind::Equivalent,
                "the project: Isoloom copies it into each Linux VM (/opt/isoloom); to share it both ways instead, give the machine `host_folders: [{ from: ., path: /vagrant }]`",
            );
            return;
        }
        if isoloom_spec::images::is_windows(m.os) {
            self.notes
                .add(at, NoteKind::Changed, format!("`{from}` left out: a Windows machine has no host folders"));
            return;
        }
        let read_only = c
            .opt("mount_options")
            .and_then(Value::as_array)
            .is_some_and(|o| o.iter().any(|v| text(Some(v)).is_some_and(|t| t.split(',').any(|w| w == "ro"))));
        let ignored: Vec<String> = c
            .options()
            .into_iter()
            .flatten()
            .filter(|(k, _)| !matches!(k.as_str(), "disabled" | "mount_options" | "create" | "id"))
            .map(|(k, v)| format!("{k}: {}", text(Some(v)).unwrap_or_else(|| v.to_string())))
            .collect();
        if !ignored.is_empty() {
            self.notes.add(
                &at,
                NoteKind::Changed,
                format!("`{from}`: each target shares a host folder its own way; not kept: {}", ignored.join(", ")),
            );
        }
        m.host_folders.push((from, path, read_only));
    }

    /// A private network with a fixed address: the machine joins its block (said when it isn't
    /// the address's own: one wider than /24 or narrower than /29). An address that isn't IPv4 is
    /// left out, and said.
    fn private_network(&mut self, at: &str, m: &mut Machine, c: &Call) {
        if text(c.opt("type")).as_deref() == Some("dhcp") || c.opt("ip").is_none() {
            self.notes.add(
                format!("{at}.vm.network"),
                NoteKind::Changed,
                "a DHCP private network: give the machine a fixed address",
            );
            return;
        }
        let given = text(c.opt("ip")).unwrap_or_default();
        let Ok(ip) = given.parse::<Ipv4Addr>() else {
            self.notes.add(
                format!("{at}.vm.network"),
                NoteKind::Changed,
                format!("`{given}` isn't an IPv4 address: the machine joins no network for it; give it an IPv4 one"),
            );
            return;
        };
        let (cidr, asked) = block(ip, text(c.opt("netmask")).as_deref());
        if let Some(len) = asked {
            self.notes.add(
                format!("{at}.vm.network"),
                NoteKind::Changed,
                format!("{ip}/{len} isn't a /24 to /29; re-addressed to {cidr}"),
            );
        }
        let key = cidr.to_string();
        self.nets.entry(key.clone()).or_insert_with(|| Net {
            cidr,
            name: text(c.opt("virtualbox__intnet")).map(|n| label(&n)).unwrap_or_default(),
            bridged: None,
        });
        m.networks.push((key, ip.octets()[3]));
    }

    fn forwarded_port(&mut self, at: &str, m: &mut Machine, c: &Call) {
        if c.opt("disabled").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let port = |key| number(c.opt(key)).and_then(|n| u16::try_from(n).ok());
        match (port("guest"), port("host")) {
            (Some(g), Some(h)) => {
                let udp = text(c.opt("protocol")).is_some_and(|p| p.eq_ignore_ascii_case("udp"));
                m.publish.push((g, h, udp));
            }
            _ => self.notes.add(
                format!("{at}.vm.network"),
                NoteKind::Changed,
                "a forwarded port without both ports: add it by hand",
            ),
        }
    }

    /// A provider block: the CPUs and memory it gives the machine, whichever hypervisor it's for.
    fn provider(&mut self, at: &str, m: &mut Machine, b: Option<&Recording>) {
        let Some(b) = b else { return };
        let cpus = number(b.setting("cpus")).or_else(|| number(b.child("vmx").and_then(|vmx| vmx.setting("numvcpus"))));
        let mem = boxes::memory_mb(b.setting("memory")).or_else(|| boxes::memory_mb(b.setting("memsize")));
        let smp = text(b.setting("smp")).and_then(|s| s.split(',').find_map(|kv| kv.strip_prefix("cpus=").and_then(|n| n.parse().ok())));
        m.cpus = m.cpus.or(cpus).or(smp);
        m.memory_mb = m.memory_mb.or(mem);
        if b.calls().any(|c| c.name() == "customize") {
            self.notes.add(
                at,
                NoteKind::ByDesign,
                "`customize`: raw commands for one hypervisor, which no other target understands",
            );
        }
    }

    fn provision(&mut self, at: &str, m: &mut Machine, c: &Call) {
        let at = format!("{at}.vm.provision");
        match c.first().as_str() {
            "shell" => self.provision_shell(&at, m, c),
            "ansible_local" => match text(c.opt("playbook")) {
                Some(p) => m.steps.push(p.trim_start_matches("./").into()),
                None => self.notes.add(at, NoteKind::Changed, "ansible_local without a playbook"),
            },
            "ansible" => {
                if let Some(p) = text(c.opt("playbook"))
                    && !self.env_playbooks.contains(&p)
                {
                    self.env_playbooks.push(p);
                }
            }
            "file" => self
                .notes
                .add(at, NoteKind::Equivalent, "Isoloom copies the project into each Linux VM (/opt/isoloom)"),
            other => self.notes.add(format!("{at} {other}"), NoteKind::Equivalent, "a `.sh` step can run any tool"),
        }
    }

    fn provision_shell(&mut self, at: &str, m: &mut Machine, c: &Call) {
        match (text(c.opt("path")), c.opt("inline")) {
            (Some(path), _) if std::path::Path::new(&path).extension().is_some_and(|e| e == "sh" || e == "ps1") => {
                m.steps.push(path.trim_start_matches("./").into());
            }
            (Some(path), _) => self
                .notes
                .add(at, NoteKind::Changed, format!("`{path}`: steps are .sh (Linux) or .ps1 (Windows) files")),
            (None, Some(_)) => self
                .notes
                .add(at, NoteKind::InImage, "an inline script: put it in a file and list it in `app.steps`"),
            _ => {}
        }
        // `reboot: true`: Vagrant restarts the guest after the script (if any).
        if c.opt("reboot").and_then(Value::as_bool) == Some(true) {
            if isoloom_spec::images::is_windows(m.os) {
                self.notes.add(
                    at,
                    NoteKind::Changed,
                    "`reboot: true` on Windows: restart it from the environment's playbooks (`ansible.windows.win_reboot`)",
                );
            } else {
                m.steps.push(REBOOT.into());
            }
        }
    }
}

/// The block of a private network's address: its netmask's (an address or a length), /24 when
/// it gives none, and never wider than /24 or narrower than /29 (the spec's sizes); with the
/// length asked when it was outside those.
fn block(ip: Ipv4Addr, netmask: Option<&str>) -> (Cidr, Option<u32>) {
    let asked = netmask
        .and_then(|m| m.parse::<Ipv4Addr>().ok().map(|mask| u32::from(mask).count_ones()).or_else(|| m.parse().ok()))
        .unwrap_or(24);
    let clamped = asked.clamp(24, 29);
    // The clamp keeps it within 24..=29, so it fits a `u8`.
    let len = u8::try_from(clamped).unwrap_or(24);
    (Cidr::enclosing(ip, len), (asked != clamped).then_some(asked))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_private_network_block_follows_its_netmask_within_the_spec_sizes() {
        let ip = Ipv4Addr::new(192, 168, 33, 77);
        let at = |mask| {
            let (cidr, asked) = block(ip, mask);
            (cidr.to_string(), asked)
        };
        assert_eq!(at(None), ("192.168.33.0/24".to_owned(), None));
        assert_eq!(at(Some("255.255.255.240")), ("192.168.33.64/28".to_owned(), None));
        assert_eq!(at(Some("255.255.0.0")), ("192.168.33.0/24".to_owned(), Some(16)));
        assert_eq!(at(Some("16")), ("192.168.33.0/24".to_owned(), Some(16)));
        assert_eq!(at(Some("30")), ("192.168.33.72/29".to_owned(), Some(30)));
    }

    #[test]
    fn a_vagrantfile_without_define_is_one_machine_with_the_global_settings_once() -> Result<()> {
        let recorded: Recording = r#"{"children":{"vm":{"settings":{"box":"bento/debian-12"},"calls":[
            {"name":"network","args":["private_network",{"ip":"10.9.0.10"}]}]}}}"#
            .parse()?;
        let d = draft(&recorded, "solo", "Vagrantfile")?;
        assert!(d.yaml.contains("  default:\n    networks: { net: 10 }\n"), "{}", d.yaml);
        Ok(())
    }

    /// A private network the spec can't take as given is said: an address that isn't IPv4 (left
    /// out), and a netmask wider than /24 (narrowed to the address's /24).
    #[test]
    fn a_private_network_left_out_or_narrowed_is_said() -> Result<()> {
        let recorded: Recording = r#"{"children":{"vm":{"calls":[
            {"name":"network","args":["private_network",{"ip":"fd00::10"}]},
            {"name":"network","args":["private_network",{"ip":"10.9.3.10","netmask":"255.255.0.0"}]}]}}}"#
            .parse()?;
        let d = draft(&recorded, "lab", "Vagrantfile")?;
        let said: Vec<&str> = d.notes.iter().map(|n| n.text.as_str()).collect();
        assert!(said.iter().any(|t| t.starts_with("`fd00::10` isn't an IPv4 address")), "{said:?}");
        assert!(said.contains(&"10.9.3.10/16 isn't a /24 to /29; re-addressed to 10.9.3.0/24"), "{said:?}");
        assert!(d.yaml.contains("cidr: 10.9.3.0/24"), "{}", d.yaml);
        Ok(())
    }

    #[test]
    fn two_defines_named_alike_get_two_machines() -> Result<()> {
        let recorded: Recording = r#"{"children":{"vm":{"calls":[
            {"name":"define","args":["web_1"]},{"name":"define","args":["web-1"]}]}}}"#
            .parse()?;
        let d = draft(&recorded, "lab", "Vagrantfile")?;
        assert!(d.yaml.contains("\n  web-1:\n") && d.yaml.contains("\n  web-1-2:\n"), "{}", d.yaml);
        Ok(())
    }
}
