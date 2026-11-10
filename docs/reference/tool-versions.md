# Tool versions

`isoloom` itself needs nothing else to validate and generate. Running what it generates needs
each target's tools. A run manifest lists them under `tools`, with the minimum versions below, and
a runner checks them before it creates anything.

## The tools each target needs

| Target | Tools | Minimum |
|---|---|---|
| `docker`, `hosted` | Docker, Docker Compose | Compose 2.17.0 |
| `kubernetes` | `kubectl` (with Kustomize), or Helm for the chart | none set |
| `vagrant`, `docker-vm` (local) | Vagrant, and the provider's hypervisor (VirtualBox, VMware, Parallels, libvirt, Hyper-V, UTM, QEMU) | Vagrant 2.4.0 |
| `hybrid` | Docker Compose and Vagrant | as above |
| `proxmox`, `docker-vm` on Proxmox | Terraform, an OpenSSH client | Terraform 1.6.0 |
| `esxi` | Terraform, `ovftool`, an OpenSSH client | Terraform 1.6.0 |
| `cloud-vm`, `cloud-docker` | Terraform, an OpenSSH client | Terraform 1.6.0 |
| `cloud-services` | Terraform, and what the spec's `cloud.requires` lists | none set |
| `external` | Ansible, an OpenSSH client | none set |

Terraform 1.6 or later works for every target. On macOS the Proxmox run manifest may run
Terraform from the pinned image `hashicorp/terraform:1.16.7` instead.

A VM target installs on the controller VM the Ansible it runs (`ansible-core` 2.15 or 2.16, with
`pywinrm`) and the pinned Galaxy collections `ansible.posix` 2.2.2 and `ansible.windows` 3.8.0
(`catalogue/collections.yml`, checked by `sha256`).

Vagrant providers other than VirtualBox and Hyper-V need their Vagrant plugin: `vagrant-libvirt`,
`vagrant-qemu`, `vagrant_utm`, `vagrant-parallels`, `vagrant-vmware-desktop` (with the VMware
Utility), or `vagrant-vmware-esxi` for a remote ESXi host ([Vagrant guide](../guide/targets/vagrant.md)).

## Terraform providers

The generated modules pin each provider to a patch range (`~>`): a new minor version of a provider
reaches you only through a new Isoloom release, after the modules are validated against it.

| Provider | Source | Version | Used by |
|---|---|---|---|
| AWS | `hashicorp/aws` | `~> 6.67.0` | `cloud-vm`, `cloud-docker` on AWS |
| Azure | `hashicorp/azurerm` | `~> 5.8.0` | on Azure |
| Google Cloud | `hashicorp/google` | `~> 8.5.0` | on Google Cloud |
| DigitalOcean | `digitalocean/digitalocean` | `~> 2.103.0` | on DigitalOcean |
| Linode | `linode/linode` | `~> 4.7.0` | on Linode |
| Oracle Cloud | `oracle/oci` | `~> 9.8.0` | on Oracle Cloud |
| Proxmox VE | `bpg/proxmox` | `~> 0.115.0` | `proxmox`, `docker-vm` on Proxmox |
| ESXi | `josenk/esxi` | `~> 1.10.3` | `esxi` |
| TLS | `hashicorp/tls` | `~> 4.4.1` | the controller's SSH key |
| Random | `hashicorp/random` | `~> 3.9.1` | Windows machines' and Linode's root passwords |
| Local | `hashicorp/local` | `~> 2.9.1` | the project's exclusion list for its archive |
| Time | `hashicorp/time` | `~> 0.14.2` | a Windows machine's restart after its renaming |

`terraform init` downloads them from the Terraform Registry, or from your provider mirror
([offline guide](../guide/offline.md#running-without-internet-access)). `cloud-services` runs your
own module, with the providers it pins.

OpenTofu is not tested: no CI job runs the generated modules with it.

## Building Isoloom

From source, Isoloom needs Rust 1.88 or later ([MSRV policy](policies.md#minimum-supported-rust-version-msrv)).
