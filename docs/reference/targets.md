# Targets, providers and tiers

A **target** is one tool Isoloom converts the spec into. The authoritative, always-current grid
of what each target does with each spec field — and each target's tier with its evidence — is the
[target matrix](../matrix.md), generated from the generators themselves (`isoloom matrix`). This page
is the prose around it.

## The targets

| Target | Converts the spec into |
|---|---|
| `docker` | A Compose project for your Docker |
| `hosted` | The same Compose file, for a hosting service |
| `kubernetes` | Manifests for your cluster, with NetworkPolicies for `reach`; the same as a Kustomize overlay and as a Helm chart |
| `vagrant` | One local VM per machine, through a Vagrant provider |
| `docker-vm` | The containers on one VM (Vagrant, or Proxmox) |
| `hybrid` | Containers and VMs together on the same networks |
| `proxmox` | A Terraform module for Proxmox VE: one VM (or LXC) per machine, with SDN, router and firewall |
| `esxi` | A Terraform module for a standalone ESXi host |
| `cloud-vm` | A Terraform module: one cloud VM per machine |
| `cloud-docker` | A Terraform module: the containers on one cloud VM |
| `cloud-services` | Your own Terraform module, applied into your cloud account |
| `external` | Hosts that already exist, set up and checked over SSH with Ansible |

## Targets are derived, not declared

Isoloom works out which targets a spec can be generated for from the machines themselves. A
machine built from an image or an OS plus container-renderable steps has a container form; a
machine with a Windows OS, a `reboot` or `playbook` step, or VM-only hardware (`tpm: required`,
`nested_virtualization`, `boot`) has only a VM form. So an environment with a Windows machine
never gets a broken Compose file. A `targets:` list in the spec can only narrow the derived set.

`isoloom targets` prints the targets a spec can be generated for, the reason each other target is
refused (naming the field at fault), and the tier of each generated target.

## Providers and clouds

Two targets fan out:

- **`vagrant`** generates one Vagrantfile that every local provider reads: VirtualBox, VMware,
  Parallels, libvirt, Hyper-V and QEMU, plus the remote `vmware_esxi` provider. A provider
  exists only on the hosts that have it (Hyper-V on Windows, libvirt on Linux, Parallels on
  macOS, QEMU on Linux and macOS); the others run on all three. UTM (macOS) runs `docker-vm`'s
  one VM only: `vagrant_utm` gives a VM no private network, and every machine is on one.
- **`cloud-vm`, `cloud-docker` and `cloud-services`** target six clouds: AWS, Azure, Google
  Cloud, DigitalOcean, Linode and Oracle Cloud.

A Vagrant run manifest is one file for every provider, chosen when the runner brings it up; the
cloud targets write one per cloud (`.isoloom/cloud-vm/<cloud>/run.json`). `kubernetes`,
`hybrid`, `external` and `hosted` write no run manifest, and `cloud-services` none
without `cloud:`: [the run manifest](run-manifest.md) lists which target has one, and where.

## Tiers: how far a target is proven

Each target sits in a tier that states how far it has been proven. Tiers are worked out from
recorded real runs, never written by hand, and the [matrix](../matrix.md) shows each with its
evidence.

- **Tier 1** — proven by a full real run (up, checks, down) from every host OS the target exists
  on.
- **Tier 2** — generated and validated: every generated file passes its format's official
  validator (`cargo xtask verify`), with no full real run recorded from every host.
- **Tier 3** — experimental.

A later failing run takes a target's Tier 1 back; a target aiming for Tier 1 lists the host OSes
still pending.

### Tier 1 today

| Target | Proven from |
|---|---|
| `docker` | Linux, macOS, Windows |
| `vagrant` on Hyper-V | Windows |
| `vagrant` on libvirt | Linux |

Every other target, provider and cloud is Tier 2: its files pass their official validators, and
some have real runs from one host OS (`proxmox`, `vagrant` on VirtualBox and remote ESXi,
`docker-vm` on VirtualBox, and `cloud-vm`, `cloud-docker` and `cloud-services` on AWS, from
macOS). No target is Tier 3. `isoloom targets` shows the tiers of a spec's targets, and
`isoloom matrix` all of them, with their evidence.

### What is proven

`isoloom matrix` (and the [target matrix](../matrix.md) generated from it) lists each target's tier
with its evidence: the real runs recorded, from which host OS and when, and the host OSes a target
aiming for Tier 1 still needs.

Recording a real run (`cargo xtask tiers record`) updates `tiers.json` and regenerates the
matrix, so the published tiers always reflect the evidence.

## See also

- [Spec reference](../spec.md) — the fields each target reads.
- [The target matrix](../matrix.md) — the generated grid and the full tier table.
- [The run manifest](run-manifest.md) — how a target's run manifest is executed.
- The guide of each target: [Docker](../guide/targets/docker.md),
  [Kubernetes](../guide/targets/kubernetes.md), [Vagrant](../guide/targets/vagrant.md),
  [Proxmox](../guide/targets/proxmox.md), [ESXi](../guide/targets/esxi.md),
  [clouds](../guide/targets/cloud.md), [existing hosts](../guide/targets/external.md).
