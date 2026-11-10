//! Vagrant's providers for a run: where each exists, its plugin, whether its commands need
//! elevation, whether its VMs run on a server, and why this lab can't run on it (Isoloom's own
//! refusal); and the tools a run probes before it starts.

use std::collections::BTreeSet;

use isoloom_manifest::{Elevation, HostOs, Identity, PathTemplate, Provider, Tool, ToolName, VagrantIdentity, VagrantProvider, Version};
use isoloom_plan::Environment;
use isoloom_render::run_manifest;

use super::vagrant_argv;
use crate::error::Result;
use crate::vms::provider::{Known, Provider as Vagrant, provider_refusal};

/// Known provider `k` as a manifest provider, with `elevation` and this lab's `refusal` on it.
fn provider(k: &Known, elevation: Elevation, refusal: Option<String>) -> Result<VagrantProvider> {
    Ok(VagrantProvider {
        id: k.manifest,
        hosts: k.hosts.iter().copied().collect(),
        plugin: k.manifest.plugin().map(ToolName::parse).transpose()?,
        elevation,
        remote: k.manifest == Provider::VmwareEsxi,
        refusal,
    })
}

/// The providers the Vagrantfile has a block for ([`Vagrant::runs_vms`]: all but UTM), with
/// this lab's refusal on each, and Vagrant's state folder for the run.
pub(super) fn identity(env: &Environment, dotfile: &PathTemplate) -> Result<Identity> {
    let providers = Vagrant::ALL
        .iter()
        .filter(|k| k.provider.runs_vms())
        .map(|k| {
            // Hyper-V takes management commands from an administrator only.
            let elevation = if k.manifest == Provider::Hyperv { Elevation::Hyperv } else { Elevation::None };
            let name = k.provider.id();
            provider(k, elevation, provider_refusal(env, name))
        })
        .collect::<Result<_>>()?;
    Ok(Identity::Vagrant(VagrantIdentity {
        providers,
        dotfile: Some(dotfile.clone()),
    }))
}

/// The providers `docker-vm`'s Vagrantfile has a block for (all but Hyper-V), its one VM a plain
/// Debian one, so only the containers' host folders rule a provider out (one that shares none);
/// and Vagrant's state folder for the run.
pub(super) fn docker_vm_identity(env: &Environment, dotfile: &PathTemplate) -> Result<Identity> {
    let providers = Vagrant::ALL
        .iter()
        .filter(|k| docker_vm_offers(k.manifest))
        .map(|k| provider(k, Elevation::None, crate::host_folders::docker_vm_refusal(env, k.provider.id())))
        .collect::<Result<_>>()?;
    Ok(Identity::Vagrant(VagrantIdentity {
        providers,
        dotfile: Some(dotfile.clone()),
    }))
}

/// Whether `docker-vm` offers `provider`: all but Hyper-V.
pub(super) fn docker_vm_offers(provider: Provider) -> bool {
    provider != Provider::Hyperv
}

/// The oldest Vagrant a run takes: 2.4.0, the first that honours a box's architecture
/// (`config.vm.box_architecture`, the host's by default). An older one takes any architecture's
/// build of a multi-architecture box: Debian's packaged 2.3.8 booted `generic/debian12`'s
/// ppc64le libvirt build on an amd64 host, which never came up.
const VAGRANT_MIN_VERSION: &str = "2.4.0";

/// Vagrant itself (at [`VAGRANT_MIN_VERSION`] or later), and each local hypervisor's own tool
/// where it has one, for its providers.
pub(super) fn tools() -> Result<Vec<Tool>> {
    let hypervisor = |name: &str, probe: &[&str], provider: Provider, hosts: &[HostOs]| -> Result<Tool> {
        let (program, args) = probe.split_first().unwrap_or((&"", &[]));
        let mut t = run_manifest::tool(name, run_manifest::argv(program, args)?)?;
        t.providers = BTreeSet::from([provider]);
        t.hosts = hosts.iter().copied().collect();
        Ok(t)
    };
    let mut vagrant = run_manifest::tool("vagrant", vagrant_argv(&["--version"])?)?;
    vagrant.min_version = Some(Version::parse(VAGRANT_MIN_VERSION)?);
    Ok(vec![
        vagrant,
        hypervisor("virtualbox", &["VBoxManage", "--version"], Provider::Virtualbox, &[])?,
        hypervisor("vmware", &["vmrun", "list"], Provider::VmwareDesktop, &[])?,
        hypervisor("parallels", &["prlctl", "--version"], Provider::Parallels, &[HostOs::Macos])?,
        hypervisor("libvirt", &["virsh", "--version"], Provider::Libvirt, &[HostOs::Linux])?,
        hypervisor("qemu", &["qemu-img", "--version"], Provider::Qemu, &[])?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::environment;

    #[test]
    fn vagrant_is_needed_at_2_4_the_first_to_pick_a_box_by_architecture() {
        let tools = tools().unwrap();
        let vagrant = tools.iter().find(|t| t.name.to_string() == "vagrant").unwrap();
        assert_eq!(vagrant.min_version.as_ref().map(Version::as_str), Some("2.4.0"));
        assert!(tools.iter().filter(|t| t.name.to_string() != "vagrant").all(|t| t.min_version.is_none()));
    }

    #[test]
    fn every_provider_has_its_hosts_plugin_and_this_labs_refusal() {
        let env = environment(
            "version: 1
name: t
networks: { a: { cidr: 10.9.0.0/24 }, b: { cidr: 10.9.1.0/24 } }
machines:
  web: { networks: { a: 10, b: 10 }, os: debian@12, app: { steps: [{ run: a.sh }] } }
",
        );
        let dotfile = PathTemplate::parse("${run.state_dir}/vagrant").unwrap();
        let Identity::Vagrant(v) = identity(&env, &dotfile).unwrap() else {
            panic!("a vagrant identity")
        };
        assert_eq!(v.providers.len(), 7, "every provider but UTM, which runs no VM on a network");
        assert!(v.providers.iter().all(|p| p.id != Provider::Utm));
        let by = |p: Provider| v.providers.iter().find(|x| x.id == p).unwrap();
        assert_eq!(by(Provider::Hyperv).elevation, Elevation::Hyperv);
        assert_eq!(by(Provider::Hyperv).hosts, BTreeSet::from([HostOs::Windows]));
        assert!(by(Provider::VmwareEsxi).remote && !by(Provider::Virtualbox).remote);
        assert_eq!(by(Provider::Libvirt).plugin.as_ref().map(ToolName::as_str), Some("vagrant-libvirt"));
        assert_eq!(by(Provider::Virtualbox).refusal, None);
        // QEMU links a VM on one network only: Isoloom's own refusal reaches the manifest.
        assert!(
            by(Provider::Qemu).refusal.as_deref().is_some_and(|r| r.contains("web")),
            "{:?}",
            by(Provider::Qemu).refusal
        );
        assert_eq!(v.dotfile, Some(dotfile));
    }
}
