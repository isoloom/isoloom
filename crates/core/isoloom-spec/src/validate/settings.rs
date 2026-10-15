//! A machine's `environment`, `sysctls` and `ulimits`: names each can take, values that fit,
//! and what its implementations can do with them.

use super::{Problems, variable_name};
use crate::model::Machine;

/// The sysctls a container can set: those the kernel keeps per network or IPC namespace.
pub const CONTAINER_SYSCTLS: &[&str] = &[
    "net.",
    "kernel.shm",
    "kernel.msgmax",
    "kernel.msgmnb",
    "kernel.msgmni",
    "kernel.sem",
    "fs.mqueue.",
];

/// The machine's settings, each checked in turn; a Windows machine has neither sysctls nor ulimits.
pub(super) fn machine(name: &str, m: &Machine, p: &mut Problems) {
    let at = |field: &str, key: &str| format!("machines.{name}.{field}.{key}");
    for k in m.environment.keys() {
        if !variable_name(k) {
            p.add(at("environment", k), "a variable name: letters, digits and _, not starting with a digit");
        }
    }
    sysctls(m, &|k| at("sysctls", k), p);
    for (k, u) in &m.ulimits {
        if !crate::model::ULIMITS.contains(&k.as_str()) {
            p.add(at("ulimits", k), format!("one of {}", crate::model::ULIMITS.join(", ")));
        }
        if u.soft() > u.hard() {
            p.add(at("ulimits", k), "the soft limit can't be above the hard one");
        }
    }
    if m.vm.as_ref().is_some_and(|v| crate::images::is_windows(&v.os)) {
        for (field, set) in [("sysctls", !m.sysctls.is_empty()), ("ulimits", !m.ulimits.is_empty())] {
            if set {
                p.add(format!("machines.{name}.{field}"), "Linux only: a Windows machine has none");
            }
        }
    }
}

/// Whether `k` is written like a kernel parameter (`net.core.somaxconn`).
fn kernel_parameter(k: &str) -> bool {
    k.contains('.')
        && k.split('.')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'))
}

/// Kernel parameters, namespaced ones only on a container, each with a one-line value.
fn sysctls(m: &Machine, at: &dyn Fn(&str) -> String, p: &mut Problems) {
    for (k, v) in &m.sysctls {
        if !kernel_parameter(k) {
            p.add(at(k), "a kernel parameter, e.g. net.core.somaxconn");
        } else if m.docker.is_some() && !CONTAINER_SYSCTLS.iter().any(|pre| k.starts_with(pre)) {
            p.add(
                at(k),
                "a container can only set the namespaced ones (net.*, kernel.shm*, kernel.msg*, kernel.sem, fs.mqueue.*): run it on a VM target, which sets any",
            );
        }
        if v.trim().is_empty() || v.contains('\n') {
            p.add(at(k), "a value on one line");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// A container.
    const CONTAINER: &str = "docker: { image: nginx }";

    /// The problems of machine `a`, implemented as `machine`, with `settings`.
    fn of(machine: &str, settings: &str) -> Vec<String> {
        problems(&lab(&format!("machines:\n  a: {{ networks: {{ lan: 10 }}, {machine}, {settings} }}\n")))
    }

    #[test]
    fn sound_settings_pass() {
        let container = "environment: { _A1: x }, sysctls: { net.ipv4.tcp_syncookies: '1', net.ipv4.conf.br-lan.forwarding: '1', kernel.shmmax: '1' }, ulimits: { nofile: { soft: 1024, hard: 1024 }, nproc: 64 }";
        assert_eq!(of(CONTAINER, container), Vec::<String>::new());
        assert_eq!(of("vm: { os: debian@12 }", "sysctls: { vm.swappiness: '10' }"), Vec::<String>::new());
    }

    #[test]
    fn an_environment_variable_is_well_named() {
        assert_eq!(
            of(CONTAINER, "environment: { 1A: x }"),
            ["machines.a.environment.1A: a variable name: letters, digits and _, not starting with a digit"]
        );
    }

    #[test]
    fn a_ulimit_is_known_and_its_soft_limit_within_the_hard_one() {
        let known = format!("machines.a.ulimits.nofiles: one of {}", crate::model::ULIMITS.join(", "));
        assert_eq!(
            of(CONTAINER, "ulimits: { nofiles: 1, nofile: { soft: 2, hard: 1 } }"),
            [known.as_str(), "machines.a.ulimits.nofile: the soft limit can't be above the hard one"]
        );
    }

    #[test]
    fn a_sysctl_is_a_kernel_parameter() {
        for bad in ["somaxconn", "net.Core.somaxconn", "net..somaxconn", "net.core.so+maxconn"] {
            let why = format!("machines.a.sysctls.{bad}: a kernel parameter, e.g. net.core.somaxconn");
            assert_eq!(of(CONTAINER, &format!("sysctls: {{ '{bad}': '1' }}")), [why], "{bad}");
        }
    }

    #[test]
    fn a_container_sets_only_namespaced_sysctls() {
        assert_eq!(
            of(CONTAINER, "sysctls: { vm.swappiness: '10' }"),
            [
                "machines.a.sysctls.vm.swappiness: a container can only set the namespaced ones (net.*, kernel.shm*, kernel.msg*, kernel.sem, fs.mqueue.*): run it on a VM target, which sets any"
            ]
        );
    }

    #[test]
    fn a_sysctls_value_is_one_line() {
        for bad in ["' '", "\"1\\n2\""] {
            let why = "machines.a.sysctls.net.core.somaxconn: a value on one line";
            assert_eq!(of(CONTAINER, &format!("sysctls: {{ net.core.somaxconn: {bad} }}")), [why], "{bad}");
        }
    }
}
