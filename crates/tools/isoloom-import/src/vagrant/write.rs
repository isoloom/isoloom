//! Writing the Vagrant draft: the shared parts through [`Writer`]; the steps, the boxes kept as
//! overrides and the environment's playbooks here.

use super::{Import, Machine, REBOOT};
use crate::draft::quoting::{quoted, scalar};
use crate::draft::writer::{Header, Port, Writer};
use crate::naming::network_names;

/// The draft: the header, the networks, the machines, then the boxes kept and the
/// environment-level playbooks.
pub(super) fn draft_yaml(import: &Import, machines: &[Machine], name: &str, source: &str) -> String {
    // Network names: the VirtualBox internal network's, else lab, lab-2...
    let names = network_names(
        import
            .nets
            .iter()
            .map(|(key, (_, named))| (key.clone(), (!named.is_empty()).then(|| named.trim_start_matches("isoloom-").to_owned()))),
    );
    let mut y = Writer::new(&Header {
        tool: "vagrant",
        source,
        review: "Review it with the notes the import printed.",
        next: "add `app.image` or `app.build` to machines that should be a container rather than their steps' build, and `checks` that prove the behavior.",
        name,
    });
    for (key, (block, _)) in &import.nets {
        y.network(names.get(key).map_or("lab", String::as_str), 0, *block, true);
    }
    y.machines();
    for m in machines {
        y.machine(&m.name);
        y.machine_networks(m.networks.iter().filter_map(|(k, o)| Some((names.get(k)?, *o))));
        y.ports(m.publish.iter().map(|&(number, host, udp)| Port {
            number,
            udp,
            publish: Some(host),
        }));
        y.resources(m.cpus, m.memory_mb);
        y.os(m.os);
        if !m.steps.is_empty() {
            let steps: Vec<String> = m.steps.iter().map(|s| step(s)).collect();
            y.line(format!("    app:\n      steps: [{}]", steps.join(", ")));
        }
    }
    boxes(&mut y, machines);
    if !import.env_playbooks.is_empty() {
        y.line("\nprovision:");
        for p in &import.env_playbooks {
            y.line(format!("  - ansible: {}", scalar(p.trim_start_matches("./"))));
        }
    }
    y.finish()
}

/// A provisioning step as an `app.steps` item: a playbook, a script, or a reboot.
fn step(s: &str) -> String {
    let playbook = std::path::Path::new(s)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("yml") || e.eq_ignore_ascii_case("yaml"));
    if s == REBOOT {
        s.to_owned()
    } else if playbook {
        format!("{{ playbook: {} }}", scalar(s))
    } else {
        format!("{{ run: {} }}", scalar(s))
    }
}

/// The boxes the Vagrantfile pins, as `overrides.vagrant`, when any machine has one.
fn boxes(y: &mut Writer, machines: &[Machine]) {
    let pinned: Vec<(&str, &str, Option<&str>)> = machines
        .iter()
        .filter_map(|m| m.image.as_ref().map(|(b, v)| (m.name.as_str(), b.as_str(), v.as_deref())))
        .collect();
    if pinned.is_empty() {
        return;
    }
    y.line("\noverrides:\n  vagrant:\n    machines:");
    for (name, b, v) in pinned {
        let version = v.map(|v| format!(", box_version: {}", quoted(v))).unwrap_or_default();
        y.line(format!("      {name}: {{ box: {}{version} }}", quoted(b)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_is_a_script_a_playbook_or_a_reboot() {
        assert_eq!(step("provision/web.sh"), "{ run: provision/web.sh }");
        assert_eq!(step("site.YML"), "{ playbook: site.YML }");
        assert_eq!(step(REBOOT), REBOOT);
        assert_eq!(step("my scripts/a.sh"), "{ run: \"my scripts/a.sh\" }");
    }
}
