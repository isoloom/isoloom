# Vagrant: `vagrant`, `docker-vm` and `hybrid`

- **`vagrant`**: one local VM per machine, set up natively by the machine's `steps` (or its
  catalogue service's recipe), in one Vagrantfile that every provider reads.
- **`docker-vm`**: the `docker` target's containers, inside one VM. Useful where Docker can't run
  on the host, or to keep the environment off it.
- **`hybrid`**: containers and VMs together on the same networks, when some machines have only a
  container form and others only a VM form.

## Providers and tiers

| Provider | Host OS | Plugin | `vagrant` tier |
|---|---|---|---|
| VirtualBox | Linux, macOS, Windows | none | 2 (real runs from macOS) |
| libvirt | Linux | `vagrant-libvirt` | **1** (Linux) |
| Hyper-V | Windows | none | **1** (Windows) |
| VMware Desktop (Workstation, Fusion) | Linux, macOS, Windows | `vagrant-vmware-desktop` and the VMware Utility | 2 |
| Parallels | macOS | `vagrant-parallels` | 2 |
| UTM | macOS | `vagrant_utm` | 2 |
| QEMU | Linux, macOS | `vagrant-qemu` | 2 |
| Remote ESXi | any | `vagrant-vmware-esxi` | 2 (real runs from macOS) |

`docker-vm` is Tier 2 on every provider (real runs on VirtualBox from macOS); `hybrid` is Tier 2.
The [target matrix](../../matrix.md) lists what each provider refuses (UTM gives a VM no private
network, for one).

## Prerequisites

- Vagrant 2.4.0 or later, the provider's hypervisor and its plugin (above).
- Enough memory and disk for every VM at once: a machine's VM defaults to 1 CPU, 1024 MB and
  20 GB (`machines.<name>.resources`). `isoloom resources` adds them up.
- Network access to the boxes (Vagrant Cloud by default), or your own box server
  ([golden images](../golden-images.md)).
- No credentials, except for a remote ESXi host (its password, given to Vagrant).

## Generate and run

```sh
isoloom generate --target vagrant
cd .isoloom/vagrant
vagrant up --provider=libvirt
vagrant destroy -f
```

The provider is the one you choose at `vagrant up`, or `vagrant.provider` in your defaults
([configuration](../../reference/configuration.md#defaults)). A runner reads
`.isoloom/vagrant/run.json` and runs the same steps, then the checks:

```sh
isothrum up . --target vagrant --provider libvirt
isothrum check . --target vagrant
isothrum down . --target vagrant
```

`docker-vm` is the same with `.isoloom/docker-vm/`.

## Networking and published ports

Each spec network is a private network of the provider, with the spec's addresses. Forwarded
ports bind to `127.0.0.1` on the host. On Hyper-V, machines join the switch named by
`ISOLOOM_HYPERV_SWITCH` (default `Default Switch`). A Windows box's RDP forward is disabled.

Windows machines are reached over WinRM, by default plain HTTP on port 5985 with basic
authentication and the box's `vagrant` account, as Vagrant's Windows boxes are built. That traffic
stays on the host's private network; see [security considerations](../../explanation/security.md).

## Teardown

`vagrant destroy -f` in `.isoloom/vagrant/` (or `isothrum down`) removes the VMs and their disks.
The boxes stay in Vagrant's cache (`vagrant box list`, `vagrant box prune`).

## Cost

None beyond your host.
