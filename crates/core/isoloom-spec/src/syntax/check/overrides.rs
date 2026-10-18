//! `overrides:` name things that exist, and each applies to something on its target: an override
//! nothing can apply is an error, never silently ignored.

use super::super::Spec;
use super::super::forms;
use super::super::machine::Machine;
use super::super::problem::{Code, FieldPath, Problem, Problems};

/// The form an override needs its machines to have.
#[derive(Clone, Copy)]
enum Needs {
    /// A VM (a box, an image).
    Vm,
    /// A container (an LXC container from its image).
    Container,
    /// Anything (an existing host).
    Any,
}

/// Every override's references.
pub(super) fn check(spec: &Spec, out: &mut Problems) {
    let o = &spec.overrides;
    let at = FieldPath::new("overrides");
    if let Some(d) = &o.docker {
        networks(spec, &at.key("docker"), d.networks.keys(), out);
        volumes(spec, &at.key("docker"), d.volumes.keys(), out);
    }
    if let Some(k) = &o.kubernetes {
        volumes(spec, &at.key("kubernetes"), k.volumes.keys(), out);
    }
    if let Some(v) = &o.vagrant {
        machines(spec, &at.key("vagrant"), "Vagrant", v.machines.keys().map(|k| (k, Needs::Vm)), out);
    }
    if let Some(p) = &o.proxmox {
        let needs = p.machines.iter().map(|(k, m)| (k, if m.lxc { Needs::Container } else { Needs::Vm }));
        machines(spec, &at.key("proxmox"), "Proxmox", needs, out);
        volumes(spec, &at.key("proxmox"), p.volumes.keys(), out);
        networks(spec, &at.key("proxmox"), p.networks.keys(), out);
    }
    if let Some(e) = &o.esxi {
        networks(spec, &at.key("esxi"), e.networks.keys(), out);
    }
    for (cloud, c) in o.clouds() {
        if let Some(c) = c {
            machines(spec, &at.key(cloud), "the cloud", c.machines.keys().map(|k| (k, Needs::Vm)), out);
        }
    }
    if let Some(x) = &o.external {
        machines(
            spec,
            &at.key("external"),
            "the external target",
            x.machines.keys().map(|k| (k, Needs::Any)),
            out,
        );
    }
}

fn networks<'a>(spec: &Spec, at: &FieldPath, names: impl Iterator<Item = &'a String>, out: &mut Problems) {
    for n in names {
        if !spec.networks.contains_key(n) {
            out.push(Problem::new(
                Code::UnknownReference,
                at.key("networks").key(n),
                format!("no network named `{n}`"),
            ));
        }
    }
}

fn volumes<'a>(spec: &Spec, at: &FieldPath, names: impl Iterator<Item = &'a String>, out: &mut Problems) {
    for v in names {
        if !spec.volumes.contains_key(v) {
            out.push(
                Problem::new(Code::UnknownReference, at.key("volumes").key(v), format!("no shared volume named `{v}`"))
                    .fix(format!("declare it: `volumes: {{ {v}: {{}} }}`")),
            );
        }
    }
}

/// Each machine or group an override names exists, and its machines have the form it needs.
fn machines<'a>(spec: &Spec, at: &FieldPath, target: &str, keys: impl Iterator<Item = (&'a String, Needs)>, out: &mut Problems) {
    for (key, needs) in keys {
        let at = at.key("machines").key(key);
        let members = members(spec, key);
        if members.is_empty() {
            out.push(Problem::new(Code::UnknownReference, at, format!("`{key}` is no machine or group")));
            continue;
        }
        for (name, m) in members {
            let lacking = match needs {
                Needs::Vm => forms::vm(m).err().map(|why| ("VM", why)),
                Needs::Container => forms::container(m).err().map(|why| ("container", why)),
                Needs::Any => None,
            };
            if let Some((form, why)) = lacking {
                out.push(
                    Problem::new(
                        Code::UnknownReference,
                        at.clone(),
                        format!("`{name}` has no {form} form (`{}`), so {target} never runs it this way", why.field),
                    )
                    .fix(format!("remove it, or {}", why.fix)),
                );
            }
        }
    }
}

/// The machines `key` names: itself, a counted machine's clones, or a group's members.
fn members<'a>(spec: &'a Spec, key: &str) -> Vec<(&'a str, &'a Machine)> {
    if let Some((n, m)) = spec.machines.get_key_value(key) {
        return vec![(n.as_str(), m)];
    }
    let names: Vec<String> = match (spec.clones.get(key), spec.groups.get(key)) {
        (Some(clones), _) => clones.clone(),
        (None, Some(g)) => spec.machines.keys().filter(|m| g.members.iter().any(|p| member(spec, p, m))).cloned().collect(),
        (None, None) => Vec::new(),
    };
    spec.machines.iter().filter(|(n, _)| names.contains(n)).map(|(n, m)| (n.as_str(), m)).collect()
}

/// Whether a group's member pattern takes machine `m`: its name, a glob, or a nested group's
/// member. A counted machine is no pattern here: reading the spec put its clones in the members.
fn member(spec: &Spec, pattern: &str, m: &str) -> bool {
    crate::groups::matches(pattern, m)
        || spec
            .groups
            .get(pattern)
            .is_some_and(|g| g.members.iter().any(|p| p != pattern && crate::groups::matches(p, m)))
}
