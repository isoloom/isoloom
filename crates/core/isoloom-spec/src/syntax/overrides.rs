//! `overrides:`: per-target facts with no portable meaning (a Docker block, a Vagrant box, a
//! Proxmox LXC container, a cloud image). Each target has a closed set of fields; anything else
//! is an error, and an override nothing on its target applies is refused, never ignored.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::model::{External, Target, Virtiofs, Winrm};

/// The overrides, by target.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    /// Every target that runs the Compose file: `docker`, `hosted`, `docker-vm`, `cloud-docker`.
    #[serde(default)]
    pub docker: Option<DockerOverrides>,
    /// The `kubernetes` target.
    #[serde(default)]
    pub kubernetes: Option<KubernetesOverrides>,
    /// The `vagrant` target, and the VMs of `hybrid`.
    #[serde(default)]
    pub vagrant: Option<VagrantOverrides>,
    /// The `proxmox` target.
    #[serde(default)]
    pub proxmox: Option<ProxmoxOverrides>,
    /// The `esxi` target.
    #[serde(default)]
    pub esxi: Option<EsxiOverrides>,
    /// `cloud-vm` on AWS.
    #[serde(default)]
    pub aws: Option<CloudOverrides>,
    /// `cloud-vm` on Azure.
    #[serde(default)]
    pub azure: Option<CloudOverrides>,
    /// `cloud-vm` on Google Cloud.
    #[serde(default)]
    pub gcp: Option<CloudOverrides>,
    /// `cloud-vm` on DigitalOcean.
    #[serde(default)]
    pub digitalocean: Option<CloudOverrides>,
    /// `cloud-vm` on Linode.
    #[serde(default)]
    pub linode: Option<CloudOverrides>,
    /// `cloud-vm` on Oracle Cloud.
    #[serde(default)]
    pub oci: Option<CloudOverrides>,
    /// The `external` target: machines that already exist.
    #[serde(default)]
    pub external: Option<ExternalOverrides>,
}

/// On the targets that run the Compose file.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DockerOverrides {
    /// Networks by name: their block on Docker, or an existing Docker network.
    #[serde(default)]
    pub networks: IndexMap<String, DockerNetwork>,
    /// Volumes by name: an existing Docker volume.
    #[serde(default)]
    pub volumes: IndexMap<String, ExternalVolume>,
}

/// A network on Docker.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DockerNetwork {
    /// The block to use on Docker instead of `cidr` (same size, inside 10.0.0.0/8). Without it,
    /// a network outside 10.0.0.0/8 moves into it (Docker Desktop and home networks use
    /// 172.16.0.0/12 and 192.168.0.0/16).
    #[serde(default)]
    pub cidr: Option<String>,
    /// An existing Docker network, by name: the machines join it, and Isoloom never creates or
    /// removes it.
    #[serde(default)]
    pub external: Option<String>,
}

/// An existing volume on the platform, by its name there.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExternalVolume {
    /// Its name on the platform (a Docker volume, a Kubernetes claim in the environment's
    /// namespace): mounted, never created or removed.
    pub external: String,
}

/// An existing network on the target, by its name there.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExternalNetwork {
    /// Its name on the target (a Proxmox bridge, an ESXi port group): the machines join it, and
    /// Isoloom never creates or removes it.
    pub external: String,
}

/// On Kubernetes.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KubernetesOverrides {
    /// Volumes by name: an existing claim.
    #[serde(default)]
    pub volumes: IndexMap<String, ExternalVolume>,
}

/// On Vagrant.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VagrantOverrides {
    /// Machines (or groups) by name: the box each boots.
    #[serde(default)]
    pub machines: IndexMap<String, VagrantMachine>,
    /// The controller's box.
    #[serde(default)]
    pub controller: Option<VagrantBox>,
}

/// A machine's box on Vagrant.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VagrantMachine {
    /// The box (`StefanScherer/windows_2019`), instead of the built-in one for its OS.
    #[serde(default, rename = "box")]
    pub box_name: Option<String>,
    /// The box version to pin.
    #[serde(default)]
    pub box_version: Option<String>,
    /// Where Vagrant downloads the box from when it isn't on Vagrant Cloud: a URL, or a `.box`
    /// file in the project.
    #[serde(default)]
    pub box_url: Option<String>,
    /// The box server to look the box up on, instead of Vagrant Cloud.
    #[serde(default)]
    pub box_server_url: Option<String>,
    /// Other boxes for the providers the box has no build for.
    #[serde(default)]
    pub boxes: Option<ProviderBoxes>,
    /// How this Windows box answers WinRM: `plaintext` (HTTP on 5985, the default) or `ssl`
    /// (HTTPS on 5986).
    #[serde(default)]
    pub winrm: Option<Winrm>,
    /// This Windows box answers OpenSSH instead of WinRM. Not with `winrm`.
    #[serde(default)]
    pub winssh: bool,
}

/// Boxes for the providers the machine's box has no build for.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderBoxes {
    /// The box the QEMU provider boots (libvirt format).
    #[serde(default)]
    pub qemu: Option<String>,
    /// The box the Hyper-V provider boots.
    #[serde(default)]
    pub hyperv: Option<String>,
}

/// The controller's box on Vagrant.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VagrantBox {
    /// The box, instead of Isoloom's (a specific Ansible version, a mirror).
    #[serde(default, rename = "box")]
    pub box_name: Option<String>,
    /// The box version to pin.
    #[serde(default)]
    pub box_version: Option<String>,
}

/// On Proxmox.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProxmoxOverrides {
    /// Machines (or groups) by name: their image, or an LXC container.
    #[serde(default)]
    pub machines: IndexMap<String, ProxmoxMachine>,
    /// Volumes by name: a folder of the server shared into each VM (virtiofs).
    #[serde(default)]
    pub volumes: IndexMap<String, ProxmoxVolume>,
    /// Networks by name: an existing bridge.
    #[serde(default)]
    pub networks: IndexMap<String, ExternalNetwork>,
}

/// A machine on Proxmox.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProxmoxMachine {
    /// The image: a cloud image's URL, or a template VM's id on the node.
    #[serde(default)]
    pub image: Option<String>,
    /// Run its container image as an LXC container (Proxmox VE 9.1 or later) instead of a VM:
    /// lighter, but no Docker runtime, healthcheck or named volumes.
    #[serde(default)]
    pub lxc: bool,
}

/// A volume on Proxmox.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProxmoxVolume {
    /// A folder of the server, shared into each VM that mounts the volume, instead of NFS from
    /// its owner: `virtiofs: {}` takes the defaults.
    pub virtiofs: Virtiofs,
}

/// On ESXi.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EsxiOverrides {
    /// Networks by name: an existing port group.
    #[serde(default)]
    pub networks: IndexMap<String, ExternalNetwork>,
}

/// On one cloud of `cloud-vm`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CloudOverrides {
    /// Machines (or groups) by name: their image on this cloud.
    #[serde(default)]
    pub machines: IndexMap<String, CloudMachine>,
}

/// A machine's image on a cloud.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CloudMachine {
    /// The image: an AMI id on AWS, a resource id on Azure, `projects/…/images/…` on Google
    /// Cloud, a slug or id on DigitalOcean, `linode/…` on Linode, an OCID on Oracle Cloud.
    pub image: String,
}

/// On the `external` target.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExternalOverrides {
    /// Machines by name: where each already exists.
    #[serde(default)]
    pub machines: IndexMap<String, ExternalMachine>,
}

/// An existing machine.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExternalMachine {
    /// Its SSH endpoint: `{ address, user, port, key }`.
    pub ssh: External,
}

impl Overrides {
    /// The cloud overrides, each with its key.
    pub fn clouds(&self) -> [(&'static str, Option<&CloudOverrides>); 6] {
        [
            ("aws", self.aws.as_ref()),
            ("azure", self.azure.as_ref()),
            ("gcp", self.gcp.as_ref()),
            ("digitalocean", self.digitalocean.as_ref()),
            ("linode", self.linode.as_ref()),
            ("oci", self.oci.as_ref()),
        ]
    }

    /// The targets an override key applies to.
    pub fn targets_of(key: &str) -> &'static [Target] {
        match key {
            "docker" => &[Target::Docker, Target::Hosted, Target::DockerVm, Target::CloudDocker],
            "kubernetes" => &[Target::Kubernetes],
            "vagrant" => &[Target::Vagrant, Target::Hybrid],
            "proxmox" => &[Target::Proxmox],
            "esxi" => &[Target::Esxi],
            "external" => &[Target::External],
            "aws" | "azure" | "gcp" | "digitalocean" | "linode" | "oci" => &[Target::CloudVm],
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_target_has_its_own_fields() {
        let o: Overrides = serde_yaml_ng::from_str(
            "{ docker: { networks: { office: { cidr: 10.70.0.0/24 } } }, vagrant: { machines: { portal: { box: example/debian, box_version: '1' } } }, proxmox: { machines: { status: { lxc: true } }, volumes: { uploads: { virtiofs: {} } } }, aws: { machines: { portal: { image: ami-1 } } } }",
        )
        .unwrap();
        assert_eq!(o.vagrant.as_ref().unwrap().machines["portal"].box_name.as_deref(), Some("example/debian"));
        assert!(o.proxmox.as_ref().unwrap().machines["status"].lxc);
        assert_eq!(o.clouds()[0].1.unwrap().machines["portal"].image, "ami-1");
    }

    #[test]
    fn a_field_another_target_has_is_refused() {
        let e = serde_yaml_ng::from_str::<Overrides>("{ docker: { machines: { web: { box: x } } } }").unwrap_err();
        assert!(e.to_string().contains("unknown field `machines`"));
        assert!(serde_yaml_ng::from_str::<Overrides>("{ podman: {} }").is_err());
    }
}
