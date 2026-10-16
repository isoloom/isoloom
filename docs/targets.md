# Targets, providers and tiers

A **target** is one tool Isoloom converts the spec into. The authoritative, always-current grid
of what each target does with each spec field — and each target's tier with its evidence — is the
[target matrix](matrix.md), generated from the generators themselves (`isoloom matrix`). This page
is the prose around it.

## The targets

| Target | Converts the spec into |
|---|---|
| `docker` | A Compose project for your Docker |
| `hosted` | The same Compose file, for a hosting service |
| `kubernetes` | Manifests for your cluster, with NetworkPolicies for `reach` |
| `helm` | The same manifests as a Helm chart |
| `kustomize` | The same manifests as a Kustomize overlay |
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
  Parallels, libvirt, Hyper-V, UTM and QEMU, plus the remote `vmware_esxi` provider. A provider
  exists only on the hosts that have it (Hyper-V on Windows, libvirt on Linux, Parallels and UTM
  on macOS, QEMU on Linux and macOS); the others run on all three.
- **`cloud-vm`, `cloud-docker` and `cloud-services`** target six clouds: AWS, Azure, Google
  Cloud, DigitalOcean, Linode and Oracle Cloud.

The run manifest is one file per target; the provider or cloud is chosen when the runner brings
it up, not baked into the manifest.

## Tiers: how far a target is proven

Each target sits in a tier that states how far it has been proven. Tiers are worked out from
recorded real runs, never written by hand, and the [matrix](matrix.md) shows each with its
evidence.

- **Tier 1** — proven by a full real run (up, checks, down) from every host OS the target exists
  on. These are the launcher's targets, the ones 1.0 commits to.
- **Tier 2** — generated and validated: every generated file passes its format's official
  validator (`cargo xtask verify`), but a full real run from every host isn't yet recorded.
- **Tier 3** — experimental.

A later failing run takes a target's Tier 1 back; a target aiming for Tier 1 lists the host OSes
still pending.

### What is proven at 0.11.0

- **Local** is proven: `docker` is Tier 1 (macOS, Linux and Windows, full cycle through the
  runner); `vagrant/libvirt` is Tier 1 on Linux; `vagrant/virtualbox` and `docker-vm/virtualbox`
  have real macOS runs; `vagrant/hyperv` has a partial Windows run.
- **Server** (`proxmox`, `vagrant/vmware_esxi`, the `esxi` Terraform target) is proven against
  real lab servers from macOS, through the runner and the launcher. Tier 1 for server mode still
  needs runs from Linux and Windows hosts, so these remain Tier 2 for now.
- **Cloud** (`cloud-vm`, `cloud-docker`, `cloud-services`) ships **generated and validated**: the
  modules are proven by `terraform validate` and by `terraform test` with mocked providers
  (`cargo xtask tf-test`), not yet by a real run against a paid cloud account. Treat cloud mode as
  Tier 2 until real-run evidence is recorded.

Recording a real run (`cargo xtask tiers record`) updates `tiers.json` and regenerates the
matrix, so the published tiers always reflect the evidence.

## See also

- [Spec reference](spec.md) — the fields each target reads.
- [The target matrix](matrix.md) — the generated grid and the full tier table.
- [The runner](runner.md) — how a target's run manifest is executed.
