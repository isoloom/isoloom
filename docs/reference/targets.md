# Targets and providers

A **target** is one tool Isoloom converts the spec into. The authoritative, always-current grid
of what each target does with each spec field — and the validators of each target's files — is the
[target matrix](../matrix.md), generated from the generators themselves (`isoloom matrix`). This page
is the prose around it.

## The targets

| Target | Converts the spec into |
|---|---|
| `docker` | A Compose project for your Docker |
| `hosted` | The same Compose file, for a hosting service |
| `kubernetes` | Manifests for your cluster, with NetworkPolicies for `reach`; the same as a Kustomize overlay and as a Helm chart |
| `vagrant` | One local VM per machine, through a Vagrant provider |
| `proxmox` | A Terraform module for Proxmox VE: one VM (or LXC) per machine, with SDN, router and firewall |
| `esxi` | A Terraform module for a standalone ESXi host |
| `cloud-vm` | A Terraform module: one cloud VM per machine |
| `cloud-docker` | A Terraform module: the containers on one cloud VM |
| `external` | Hosts that already exist, set up and checked over SSH with Ansible |

## Targets are derived, not declared

Isoloom works out which targets a spec can be generated for from the machines themselves. A
machine built from an image or an OS plus container-renderable steps has a container form; a
machine with a Windows OS, a `reboot` or `playbook` step, or VM-only hardware (`tpm: required`,
`nested_virtualization`, `boot`) has only a VM form. So an environment with a Windows machine
never gets a broken Compose file. A `targets:` list in the spec can only narrow the derived set.

`isoloom targets` prints the targets a spec can be generated for, the reason each other target is
refused (naming the field at fault), and the validators of each generated target.

## Providers and clouds

Two targets fan out:

- **`vagrant`** generates one Vagrantfile that every local provider reads: VirtualBox, VMware,
  Parallels, libvirt, Hyper-V, QEMU and UTM, plus the remote `vmware_esxi` provider. A provider
  exists only on the hosts that have it (Hyper-V on Windows, libvirt on Linux, Parallels and UTM
  on macOS, QEMU on Linux and macOS); the others run on all three. On UTM each network is an
  isolated vmnet network the Vagrantfile attaches the VMs to, and a VM boots a UTM box.
- **`cloud-vm` and `cloud-docker`** target six clouds: AWS, Azure, Google Cloud, DigitalOcean,
  Linode and Oracle Cloud. `cloud-services` is not supported in Isoloom 3.0: a spec with `cloud:` is refused.

The Vagrantfile is one file for every provider, chosen at `vagrant up --provider`; the cloud
targets write one module per cloud (`.isoloom/cloud-vm/<cloud>/main.tf`).

## Validators

Isoloom is a converter: a target is the files it writes. Every one of them is checked by its
format's official validators on every example, inside a pinned toolchain image (`cargo xtask
verify`); the Terraform modules' logic is also checked by `terraform test`, every provider
mocked and every run a plan (`cargo xtask tf-test`).

| Target | Writes | Validated by |
|---|---|---|
| `docker` | a Compose file (`compose.yml`) and the files it mounts | `docker compose config` |
| `hosted` | the `docker` target's Compose file, for a hosting service | `docker compose config` |
| `cloud-docker` | the `docker` target's Compose file, and a Terraform module per cloud for its Docker host | `docker compose config`, `terraform validate`, `tflint`, `terraform test` |
| `kubernetes` | Kubernetes manifests with a Kustomize overlay, and a Helm chart | `kubeconform`, `helm lint` |
| `vagrant` | a Vagrantfile, every provider's block in it | `vagrant validate` |
| `proxmox` | a Terraform module for Proxmox VE | `terraform validate`, `tflint`, `terraform test` |
| `esxi` | a Terraform module for a standalone ESXi host | `terraform validate`, `tflint`, `terraform test` |
| `cloud-vm` | a Terraform module per cloud | `terraform validate`, `tflint`, `terraform test` |
| `external` | an Ansible inventory and playbook for hosts that exist | `ansible-lint` |

`isoloom targets` shows the validators of a spec's targets, and `isoloom matrix` (and the
[target matrix](../matrix.md) generated from it) all of them.

## See also

- [Spec reference](../spec.md) — the fields each target reads.
- [The target matrix](../matrix.md) — the generated grid and the validators table.
- The guide of each target: [Docker](../guide/targets/docker.md),
  [Kubernetes](../guide/targets/kubernetes.md), [Vagrant](../guide/targets/vagrant.md),
  [Proxmox](../guide/targets/proxmox.md), [ESXi](../guide/targets/esxi.md),
  [clouds](../guide/targets/cloud.md), [existing hosts](../guide/targets/external.md).
