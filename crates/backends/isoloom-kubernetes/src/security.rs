//! What a pod may do. Every pod but a privileged machine's runs under the runtime's seccomp
//! filter, with no privilege escalation and its capabilities dropped: Isoloom's own helpers
//! (busybox copies and waits) run as a non-root user with none, a container running an image
//! Isoloom can't vouch for (the machine's, its jobs, the root-owned copies, the checks) keeps
//! only the capabilities a root process of a stock image needs (`chown`, `setuid` to its own
//! user, a port under 1024, `chroot` for sshd's privilege separation), without `NET_RAW` or any
//! the Pod Security Standards' `baseline` level refuses. The namespace enforces that level, or `privileged` when a machine needs more
//! (`privileged: true`, a sysctl outside the safe set).

use crate::machine::Machine;
use crate::objects::{Capabilities, ContainerSecurity, PodSecurity, SeccompProfile};

/// The namespace label the Pod Security admission enforces.
pub(crate) const ENFORCE_LABEL: &str = "pod-security.kubernetes.io/enforce";

/// What a container running a stock image as root keeps: the runtime's default set (containerd's
/// and Docker's 14) without `NET_RAW` (raw sockets: spoofing on the pod network), all of which
/// `baseline` allows.
const ROOT_CAPABILITIES: [&str; 13] = [
    "AUDIT_WRITE",
    "CHOWN",
    "DAC_OVERRIDE",
    "FOWNER",
    "FSETID",
    "KILL",
    "MKNOD",
    "NET_BIND_SERVICE",
    "SETFCAP",
    "SETGID",
    "SETPCAP",
    "SETUID",
    "SYS_CHROOT",
];

/// The sysctls `baseline` allows (the kubelet's safe set); any other needs `privileged`.
const SAFE_SYSCTLS: [&str; 10] = [
    "kernel.shm_rmid_forced",
    "net.ipv4.ip_local_port_range",
    "net.ipv4.ip_local_reserved_ports",
    "net.ipv4.ip_unprivileged_port_start",
    "net.ipv4.ping_group_range",
    "net.ipv4.tcp_syncookies",
    "net.ipv4.tcp_keepalive_time",
    "net.ipv4.tcp_fin_timeout",
    "net.ipv4.tcp_keepalive_intvl",
    "net.ipv4.tcp_keepalive_probes",
];

/// The user Isoloom's helpers run as: `nobody`.
const NOBODY: u32 = 65_534;

/// A container running an image Isoloom can't vouch for, as the image's own user (often root).
pub(crate) fn image_user() -> ContainerSecurity {
    ContainerSecurity {
        allow_privilege_escalation: Some(false),
        capabilities: Some(Capabilities {
            drop: vec!["ALL"],
            add: ROOT_CAPABILITIES.to_vec(),
        }),
        ..ContainerSecurity::default()
    }
}

/// One of Isoloom's helpers, which needs neither root nor a capability.
pub(crate) fn helper() -> ContainerSecurity {
    ContainerSecurity {
        run_as_user: Some(NOBODY),
        run_as_group: Some(NOBODY),
        run_as_non_root: Some(true),
        allow_privilege_escalation: Some(false),
        capabilities: Some(Capabilities {
            drop: vec!["ALL"],
            add: Vec::new(),
        }),
        ..ContainerSecurity::default()
    }
}

/// The machine's own container: privileged when the spec says so (explicitly, nothing dropped),
/// else as [`image_user`]; read-only when the spec says so.
pub(crate) fn machine(m: &Machine) -> ContainerSecurity {
    let base = if m.spec.privileged {
        ContainerSecurity {
            privileged: true,
            ..ContainerSecurity::default()
        }
    } else {
        image_user()
    };
    ContainerSecurity {
        read_only_root_filesystem: m.spec.read_only,
        ..base
    }
}

/// A pod's own settings: the runtime's seccomp filter unless the machine is privileged.
pub(crate) fn pod(privileged: bool, sysctls: Vec<crate::objects::NameValue>) -> Option<PodSecurity> {
    let seccomp_profile = (!privileged).then_some(SeccompProfile { kind: "RuntimeDefault" });
    (seccomp_profile.is_some() || !sysctls.is_empty()).then_some(PodSecurity { seccomp_profile, sysctls })
}

/// The level the namespace enforces: `privileged` when a machine needs it, else `baseline`.
pub(crate) fn level(machines: &[Machine]) -> &'static str {
    let needs_more = machines
        .iter()
        .any(|m| m.spec.privileged || m.plan.settings.sysctls.keys().any(|k| !SAFE_SYSCTLS.contains(&k.as_str())));
    if needs_more { "privileged" } else { "baseline" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stock_image_keeps_the_runtimes_default_set_but_raw_sockets() {
        let c = image_user();
        let caps = c.capabilities.unwrap();
        assert_eq!(caps.drop, ["ALL"]);
        let runtime_default = [
            "AUDIT_WRITE",
            "CHOWN",
            "DAC_OVERRIDE",
            "FOWNER",
            "FSETID",
            "KILL",
            "MKNOD",
            "NET_BIND_SERVICE",
            "NET_RAW",
            "SETFCAP",
            "SETGID",
            "SETPCAP",
            "SETUID",
            "SYS_CHROOT",
        ];
        let kept: Vec<&str> = runtime_default.into_iter().filter(|cap| *cap != "NET_RAW").collect();
        assert_eq!(caps.add, kept);
        assert_eq!((c.allow_privilege_escalation, c.privileged, c.run_as_non_root), (Some(false), false, None));
    }

    #[test]
    fn a_helper_runs_as_nobody_with_no_capability() {
        let c = helper();
        assert_eq!(
            (c.run_as_user, c.run_as_non_root, c.allow_privilege_escalation),
            (Some(NOBODY), Some(true), Some(false))
        );
        assert!(c.capabilities.is_some_and(|caps| caps.add.is_empty() && caps.drop == ["ALL"]));
    }

    #[test]
    fn a_privileged_pod_has_no_seccomp_filter() {
        assert!(pod(true, Vec::new()).is_none());
        assert!(pod(false, Vec::new()).is_some_and(|p| p.seccomp_profile.is_some_and(|s| s.kind == "RuntimeDefault")));
    }
}
