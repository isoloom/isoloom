//! Every [`Feature`], and each one's name: what the capability matrix (`isoloom matrix`) lists and
//! names a refusal by. Both come from one list, so a feature added to the enum fails to build
//! here until it is named: [`Feature::id`]'s `match` covers every one, and [`Feature::ALL`] is
//! the same list.

use super::Feature;

/// [`Feature::ALL`] and [`Feature::id`] from one list of `variant => "name"`.
macro_rules! named {
    ($($feature:ident => $id:literal,)*) => {
        impl Feature {
            /// Every feature, in the order the enum declares them.
            pub const ALL: &'static [Feature] = &[$(Feature::$feature),*];

            /// Its name, in kebab case (`ipv6-only`, `vm-gpus`).
            #[must_use]
            pub fn id(self) -> &'static str {
                match self {
                    $(Feature::$feature => $id,)*
                }
            }
        }
    };
}

named! {
    EnvironmentProvisioning => "environment-provisioning",
    PlaybookChecks => "playbook-checks",
    ExecWithoutContainer => "exec-without-container",
    NetworkAppliances => "network-appliances",
    Gateways => "gateways",
    VolumesAcrossShapes => "volumes-across-shapes",
    VmAfterContainer => "vm-after-container",
    MachineAliases => "machine-aliases",
    Ulimits => "ulimits",
    TrafficControl => "traffic-control",
    Ipv6 => "ipv6",
    Ipv6Only => "ipv6-only",
    Ipv6Auto => "ipv6-auto",
    Secrets => "secrets",
    SecretOwners => "secret-owners",
    ExternalNetworks => "external-networks",
    BridgedNetworks => "bridged-networks",
    OwnedProjectFiles => "owned-project-files",
    ExistingHostFiles => "existing-host-files",
    Tpm => "tpm",
    MachineProcess => "machine-process",
    OwnBoot => "own-boot",
    ContainerGpus => "container-gpus",
    ContainerDevices => "container-devices",
    ContainerNestedVirtualization => "container-nested-virtualization",
    VmGpus => "vm-gpus",
    VmDevices => "vm-devices",
    NestedVirtualization => "nested-virtualization",
    NetworkMtu => "network-mtu",
    RateLimit => "rate-limit",
    PrivateDns => "private-dns",
    PersistentVolumes => "persistent-volumes",
    ExternalVolumes => "external-volumes",
    DataDisks => "data-disks",
    RawDisks => "raw-disks",
    TmpfsModes => "tmpfs-modes",
    VmMoreAddresses => "vm-more-addresses",
    ContainerMoreAddresses => "container-more-addresses",
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_feature_has_its_own_kebab_case_name_and_is_listed_in_order() {
        let names: BTreeSet<&str> = Feature::ALL.iter().map(|f| f.id()).collect();
        assert_eq!(names.len(), Feature::ALL.len());
        assert!(
            names
                .iter()
                .all(|n| n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
        );
        assert!(Feature::ALL.is_sorted(), "in the order the enum declares them");
    }
}
