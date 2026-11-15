//! A requirement said in the user's terms: why a backend without its capability refuses the
//! environment.

use std::fmt;

use super::Requirement;

/// Why a target without [`super::Feature::MachineProcess`] refuses a machine's own process: only
/// `external` lacks it, and its hosts already run what they run.
const OWN_PROCESS: &str = "sets its own process (`command` or `healthcheck`): an existing host runs what it already runs, Isoloom starts nothing on it";

/// Why an `exec` check needs a container where it stands.
const IN_A_CONTAINER: &str = "needs `from:` a machine with its own container (it runs inside it)";

impl fmt::Display for Requirement {
    /// Why a backend without the capability refuses the environment, in the user's terms.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Requirement::EnvironmentProvisioning => f.write_str("environment-level provisioning (`provision:`) runs on the VM targets only"),
            Requirement::PlaybookChecks => f.write_str("Ansible checks (.yml) run on the VM targets only"),
            Requirement::ExecWithoutContainer { check } => write!(f, "`exec` check \"{check}\" {IN_A_CONTAINER}"),
            Requirement::NetworkAppliance { machine } => write!(
                f,
                "machine `{machine}` is a network appliance (`app.appliance`), which runs on the Docker targets"
            ),
            Requirement::Gateway { network } => write!(
                f,
                "network `{network}` is routed by a `gateway` machine, which this target doesn't route through (`isoloom matrix` lists the targets that do)"
            ),
            Requirement::VolumeAcrossShapes { volume } => write!(
                f,
                "volume `{volume}` is shared by containers and VMs: one storage can't be both a Docker volume and an NFS export"
            ),
            Requirement::VmDependsOnContainer { vm, container } => {
                write!(f, "VM `{vm}` depends on container `{container}`: VMs start before the containers beside them")
            }
            // Kubernetes is the target that lacks these: the messages say so in its terms.
            Requirement::MachineAliases => f.write_str("machine `aliases` (dotted DNS names) aren't Kubernetes Service names"),
            Requirement::Tools { tool } => write!(
                f,
                "tool `{tool}` (`tools:`) is added beside the environment on the Docker and Vagrant targets only"
            ),
            Requirement::Ulimits { machine } => write!(f, "machine `{machine}` sets `ulimits`: Kubernetes has no per-container process limits"),
            Requirement::TrafficControl => f.write_str("link impairment (`tc`) needs a router in the path; Kubernetes has none"),
            Requirement::Ipv6 { network } => write!(f, "network `{network}` is IPv6 (`ipv6`), which this target's networks don't carry"),
            Requirement::Ipv6Only { network } => crate::ipv6::no_ipv6_only(f, network),
            Requirement::Ipv6Auto { network } => crate::ipv6::no_auto_addresses(f, network),
            Requirement::Secrets { machine } => crate::secrets::no_delivery(f, machine),
            Requirement::SecretOwner { machine } => crate::secrets::no_owner(f, machine),
            Requirement::ExternalNetwork { network, name } => crate::outside::no_existing_network(f, network, name),
            Requirement::BridgedNetwork { network } => crate::outside::no_bridge(f, network),
            Requirement::OwnedProjectFile { machine, path } => crate::machine_files::no_owned_project_file(f, machine, path),
            Requirement::ExistingHostFiles { machine } => crate::machine_files::no_existing_host_files(f, machine),
            Requirement::ExistingHostSettings { machine, field } => crate::machine_files::no_existing_host_settings(f, machine, field),
            Requirement::SharedHostPort { first, second, port } => crate::published::no_shared_host_port(f, first, second, *port),
            Requirement::ExistingHostScriptCheck { check } => crate::existing_hosts::no_script_check(f, check),
            Requirement::ExistingHostCheckFromNoHost { check } => crate::existing_hosts::no_host_to_run_from(f, check),
            Requirement::Tpm { machine } => crate::hardware::no_tpm(f, machine),
            Requirement::MachineProcess { machine } => write!(f, "machine `{machine}` {OWN_PROCESS}"),
            Requirement::OwnBoot { machine } => crate::boot::no_own_boot(f, machine),
            Requirement::Hardware { machine, need } => need.reason(machine, f),
            Requirement::NetworkMtu { network } => crate::links::no_mtu(f, network),
            Requirement::RateLimit { machine } => crate::links::no_rate_limit(f, machine),
            // Only Kubernetes and existing hosts lack it: a cluster names Services under its own
            // domain, and Isoloom leaves an existing host's resolver and hosts file as they are.
            Requirement::PrivateDns { domain } => write!(
                f,
                "the names are served under a private zone (`dns.private`, `{domain}`), and this target has no DNS or hosts file Isoloom sets: a cluster names Services under its own domain, an existing host keeps its own resolver"
            ),
            Requirement::MoreAddresses { machine, network, shape } => crate::more_addresses::refused(f, machine, network, *shape),
            Requirement::PersistentVolume { volume } => crate::storage::no_persistent_volume(f, volume),
            Requirement::ExternalVolume { volume } => crate::storage::no_existing_volume(f, volume),
            Requirement::DataDisks { machine } => crate::storage::no_data_disks(f, machine),
            Requirement::RawDisks { machine } => crate::storage::no_raw_disks(f, machine),
            Requirement::TmpfsModes { machine } => crate::storage::no_tmpfs_modes(f, machine),
            Requirement::HostFolders { machine } => crate::storage::no_host_folders(f, machine),
        }
    }
}
