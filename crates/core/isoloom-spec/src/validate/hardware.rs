//! A machine's `gpus`, `devices` and `nested_virtualization`: values that make sense, and the
//! implementation each needs.

use std::collections::HashSet;

use super::Problems;
use crate::model::{Device, GpuCount, Gpus, Machine, Seal, Spec};

/// The machine's hardware fields, each checked in turn.
pub(super) fn machine(name: &str, m: &Machine, p: &mut Problems) {
    let at = format!("machines.{name}");
    if m.nested_virtualization && m.vm.is_none() {
        p.add(
            format!("{at}.nested_virtualization"),
            "VMs run inside a VM, not a container: run it on a VM target",
        );
    }
    if let Some(g) = &m.gpus {
        gpus(&format!("{at}.gpus"), g, p);
    }
    let mut guests = HashSet::new();
    for (i, d) in m.devices.iter().enumerate() {
        let at = format!("{at}.devices[{i}]");
        device(&at, d, p);
        if !guests.insert(d.guest()) {
            p.add(at, format!("`{}` is already given to the machine: one device per path inside it", d.guest()));
        }
    }
}

/// An `internet: false` network keeps a container offline by removing its default route inside
/// its own network namespace (the host doesn't route it out). A privileged container can add the
/// route back and reach the internet, so the seal is a guarantee it could lift. Refuse a
/// privileged container on such a network unless it opts in with `seal: best-effort`, which
/// acknowledges the seal is advisory for it; otherwise it must drop `privileged` or sit on a
/// network with internet. A VM is sealed at the hypervisor or router, out of its own reach, so
/// this is for containers only.
pub(super) fn offline_privilege(spec: &Spec, name: &str, m: &Machine, p: &mut Problems) {
    if m.vm.is_some() || !m.privileged || m.seal == Seal::BestEffort {
        return;
    }
    if let Some(net) = m.networks.keys().find(|net| spec.networks.get(*net).is_some_and(|n| !n.internet)) {
        p.add(
            format!("machines.{name}.privileged"),
            format!(
                "a privileged container can lift the seal of `{net}` (an `internet: false` network) from inside: drop `privileged`, put the machine on a network with internet, or set `seal: best-effort` to accept that the seal is advisory for it"
            ),
        );
    }
}

/// A word a runtime takes as a name: lowercase letters, digits, `.`, `_` and `-`.
fn word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

fn gpus(at: &str, g: &Gpus, p: &mut Problems) {
    if g.count == Some(GpuCount::Count(0)) {
        p.add(format!("{at}.count"), "at least 1 (leave `gpus` out for none)");
    }
    if g.count.is_some() && !g.ids.is_empty() {
        p.add(at, "give `count` or `ids`, not both: the ids say how many");
    }
    let mut seen = HashSet::new();
    for (i, id) in g.ids.iter().enumerate() {
        if id.trim().is_empty() || id.contains(char::is_whitespace) {
            p.add(
                format!("{at}.ids[{i}]"),
                "a GPU's id on the host: an index (`0`), a UUID, or a PCI address (`0000:01:00.0`)",
            );
        } else if !seen.insert(id) {
            p.add(format!("{at}.ids[{i}]"), format!("`{id}` is listed twice"));
        }
    }
    if g.driver.as_deref().is_some_and(|d| !word(d)) {
        p.add(format!("{at}.driver"), "a driver's name, e.g. nvidia, amd or intel");
    }
    for (i, c) in g.capabilities.iter().enumerate() {
        if !word(c) {
            p.add(
                format!("{at}.capabilities[{i}]"),
                "a capability's name, e.g. compute, utility, video or graphics",
            );
        }
    }
    for k in g.options.keys().filter(|k| k.trim().is_empty()) {
        p.add(format!("{at}.options"), format!("`{k}`: an option needs a name"));
    }
}

fn device(at: &str, d: &Device, p: &mut Problems) {
    let path = |s: &str| s.starts_with('/') && !s.contains(char::is_whitespace) && !s.contains(':');
    if !path(&d.host) {
        p.add(format!("{at}.host"), "the device's absolute path on the host, e.g. /dev/fuse");
    }
    if d.guest.as_deref().is_some_and(|g| !path(g)) {
        p.add(format!("{at}.guest"), "an absolute path inside the machine, e.g. /dev/fuse");
    }
    if let Some(perms) = &d.permissions {
        let mut seen = HashSet::new();
        if perms.is_empty() || !perms.chars().all(|c| matches!(c, 'r' | 'w' | 'm') && seen.insert(c)) {
            p.add(
                format!("{at}.permissions"),
                "`r` (read), `w` (write) and `m` (create the device node), each at most once, e.g. rw",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::validate::validate;

    fn problems(machine: &str) -> Vec<String> {
        let spec = crate::testing::model(&format!(
            "version: 1\nname: hw\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  a: {{ networks: {{ lan: 10 }}, {machine} }}\n"
        ))
        .unwrap();
        validate(&spec).into_iter().map(|p| p.to_string()).collect()
    }

    /// The problems of a spec whose `lan` network is given by `lan` and whose machine `a` by
    /// `machine`.
    fn offline_problems(lan: &str, machine: &str) -> Vec<String> {
        let spec = crate::testing::model(&format!(
            "version: 1\nname: hw\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24, {lan} }} }}\nmachines:\n  a: {{ networks: {{ lan: 10 }}, {machine} }}\n"
        ))
        .unwrap();
        validate(&spec).into_iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn a_privileged_container_is_refused_on_an_internet_false_network_without_opt_in() {
        assert_eq!(
            offline_problems("internet: false", "docker: { image: x }, privileged: true"),
            [
                "machines.a.privileged: a privileged container can lift the seal of `lan` (an `internet: false` network) from inside: drop `privileged`, put the machine on a network with internet, or set `seal: best-effort` to accept that the seal is advisory for it"
            ]
        );
    }

    #[test]
    fn seal_best_effort_opts_a_privileged_offline_container_in() {
        assert_eq!(
            offline_problems("internet: false", "docker: { image: x }, privileged: true, seal: best-effort"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn privilege_is_fine_with_internet_or_on_a_vm() {
        // Internet on the network: nothing to seal.
        assert_eq!(
            offline_problems("internet: true", "docker: { image: x }, privileged: true"),
            Vec::<String>::new()
        );
        // A VM is sealed at the hypervisor or router, out of its own reach.
        assert_eq!(
            offline_problems("internet: false", "vm: { os: debian@12, provision: [a.sh] }, privileged: true"),
            Vec::<String>::new()
        );
        // An unprivileged container on an offline network is the normal case.
        assert_eq!(offline_problems("internet: false", "docker: { image: x }"), Vec::<String>::new());
    }

    #[test]
    fn sound_hardware_passes() {
        let ok = "vm: { os: debian@12, provision: [a.sh] }, nested_virtualization: true, gpus: { ids: ['0000:01:00.0'], capabilities: [compute] }";
        assert_eq!(problems(ok), Vec::<String>::new());
        assert_eq!(
            problems("docker: { image: x }, gpus: all, devices: [/dev/fuse, '/dev/a:/dev/b:rw']"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_driver_is_a_lowercase_word_of_letters_digits_and_separators() {
        assert_eq!(problems("vm: { os: debian@12 }, gpus: { driver: nvidia-535.104_x }"), Vec::<String>::new());
        assert_eq!(
            problems("vm: { os: debian@12 }, gpus: { driver: 'nvidia 535' }"),
            ["machines.a.gpus.driver: a driver's name, e.g. nvidia, amd or intel"]
        );
    }

    #[test]
    fn a_gpu_id_is_one_word() {
        for bad in ["''", "'0 1'"] {
            assert_eq!(
                problems(&format!("vm: {{ os: debian@12 }}, gpus: {{ ids: [{bad}] }}")),
                ["machines.a.gpus.ids[0]: a GPU's id on the host: an index (`0`), a UUID, or a PCI address (`0000:01:00.0`)"],
                "{bad}"
            );
        }
    }

    #[test]
    fn each_mistake_says_where_and_how_to_fix_it() {
        let found = problems(
            "docker: { image: x }, nested_virtualization: true, gpus: { count: 0, ids: ['1', '1', ' '], driver: NV, capabilities: ['Comp'], options: { ' ': x } }, \
             devices: [dev/fuse, { host: /dev/a, guest: b, permissions: rwx }, /dev/c, '/dev/d:/dev/c']",
        );
        let expect = [
            "machines.a.nested_virtualization: VMs run inside a VM",
            "machines.a.gpus.count: at least 1",
            "machines.a.gpus: give `count` or `ids`, not both",
            "machines.a.gpus.ids[1]: `1` is listed twice",
            "machines.a.gpus.ids[2]: a GPU's id on the host",
            "machines.a.gpus.driver: a driver's name",
            "machines.a.gpus.capabilities[0]: a capability's name",
            "machines.a.gpus.options: ` `: an option needs a name",
            "machines.a.devices[0].host: the device's absolute path",
            "machines.a.devices[1].guest: an absolute path inside the machine",
            "machines.a.devices[1].permissions: `r` (read)",
            "machines.a.devices[3]: `/dev/c` is already given",
        ];
        for e in expect {
            assert!(found.iter().any(|f| f.starts_with(e)), "no `{e}` in {found:#?}");
        }
        assert_eq!(found.len(), expect.len(), "{found:#?}");
    }
}
