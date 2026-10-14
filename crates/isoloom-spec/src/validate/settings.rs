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
                "a container can only set the namespaced ones (net.*, kernel.shm*, kernel.msg*, kernel.sem, fs.mqueue.*): leave `docker:` out to make the machine VM-only",
            );
        }
        if v.trim().is_empty() || v.contains('\n') {
            p.add(at(k), "a value on one line");
        }
    }
}
