# Vagrant: `vagrant`

**`vagrant`**: one local VM per machine, set up natively by the machine's `steps` (or its
catalogue service's recipe, or the VM form its Dockerfile translates to), in one Vagrantfile
that every provider reads. Every machine is a true VM: no container runs inside a helper VM.

## Providers

The Vagrantfile is checked by `vagrant validate`. Each provider runs where its hypervisor does:

| Provider | Host OS | Plugin |
|---|---|---|
| VirtualBox | Linux, macOS, Windows | none |
| libvirt | Linux | `vagrant-libvirt` |
| Hyper-V | Windows | none |
| VMware Desktop (Workstation, Fusion) | Linux, macOS, Windows | `vagrant-vmware-desktop` and the VMware Utility |
| Parallels | macOS | `vagrant-parallels` |
| QEMU | Linux, macOS | `vagrant-qemu` |
| UTM | macOS (Apple silicon) | `vagrant_utm` 0.2.0 or later, UTM 4.6.5 or later from GitHub or Homebrew |
| Remote ESXi | any | `vagrant-vmware-esxi` |

On UTM, `vagrant_utm` makes no adapter for a private network and sets no address in the guest,
so the Vagrantfile does both. Each network is an isolated vmnet network: QEMU's `vmnet-host`
with a network id made from the project copy's folder and the network's name, so its VMs see
each other and nothing else (not the Mac, not another network, no DHCP). Before each boot, a
trigger sets the VM's adapters there (virtio, a fixed MAC) as QEMU arguments of its UTM
configuration; once the guest answers, its addresses are set by MAC, as on Hyper-V. Published
ports are Vagrant's forwards on the box's emulated adapter, and the box's shared adapter is the
VM's way out. UTM boots UTM-format boxes only: `utm/bookworm` (Debian 12, also Isoloom's own
VMs) and `utm/ubuntu-24.04` are built in; another OS needs
`overrides.vagrant.machines.<m>.boxes.utm`, or the spec is refused. UTM attaches no data disk,
bridges no network, gives no TPM, boots no machine its own way and has no nested
virtualization: those specs are refused (nesting stops `vagrant up`). Every call to UTM is an
Apple event: UTM must run in your session, and the terminal or app driving Vagrant must be
allowed to control it (System Settings, Privacy & Security, Automation). The App Store build of
UTM has no vmnet. The [target matrix](../../matrix.md) lists what each provider refuses.

## Prerequisites

- Vagrant 2.4.0 or later, the provider's hypervisor and its plugin (above). QEMU on Linux also
  needs `mkisofs` (`genisoimage` on Debian and Ubuntu): vagrant-qemu gives a VM its network
  settings on an ISO that Vagrant builds with it.
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
([configuration](../../reference/configuration.md#defaults)).

`vagrant up` ends once every machine is ready: each machine's last provisioner is its readiness
check, which tries its healthcheck and fails `vagrant up` when it doesn't pass. The access
machine is defined `autostart: false`: `vagrant up` leaves it down, `vagrant up <machine>`
starts it. Inputs are read from the environment of each `vagrant` command by their own names,
and secrets as `ISOLOOM_SECRET_<NAME>` (unset: empty, or the value given before).

`vagrant provision` (or `vagrant up --provision`) runs again safely: Isoloom's own steps
converge, and a project script without `creates` runs once, until it changes (its mark is in
`/var/lib/isoloom/done/` on Linux, `C:\ProgramData\isoloom\done\` on Windows).

Each VM with checks has them installed at `/var/lib/isoloom/checks.sh` at every provisioning,
and Vagrant runs them:

```sh
vagrant ssh web -c 'sudo sh /var/lib/isoloom/checks.sh'
```

Each prints a PASS or FAIL line per check and fails when one does; `sudo ISOLOOM_DERIVED=0 sh
...` leaves the checks Isoloom derives from the spec out. The checks that stand on every network
run on `isoloom-controller`, which halts once set up unless `controller.keep_running` is set:
`vagrant up isoloom-controller --no-provision` boots it first. The comment at the top of the
Vagrantfile lists these commands for your spec.

A runner runs the same Vagrantfile, then the checks:

```sh
isothrum up . --target vagrant --provider libvirt
isothrum check . --target vagrant
isothrum down . --target vagrant
```


### A remote ESXi host

The `vmware_esxi` provider runs the VMs on an ESXi host, which it reads from `ESXI_HOSTNAME` and
`ESXI_PASSWORD` (and `ESXI_HOSTPORT`, `ESXI_USERNAME`, `ESXI_DATASTORE`, `ESXI_VIRTUAL_NETWORK`).
Vagrant picks each box's build for the computer it runs on, but these VMs run on the ESXi host,
which is x86-64. A runner sets Vagrant's `VAGRANT_HOST_ARCHITECTURE=amd64` on ESXi (the run
manifest's param), so Vagrant takes each box's amd64 build, or the one build of an older box that
names no architecture. Running Vagrant yourself from an ARM computer (an Apple Silicon Mac), set
it too:

```sh
export VAGRANT_HOST_ARCHITECTURE=amd64
vagrant up --provider=vmware_esxi
```

Without it, Vagrant downloads the boxes' arm64 builds, and `ovftool` fails to upload them to the
host ("There was an OVF Tool error"). An arm64 build already in Vagrant's cache isn't used then.
ESXi-Arm isn't supported: a machine with `hardware.arch: arm64` is refused on `vmware_esxi`.

## Networking and published ports

Each spec network is a private network of the provider, with the spec's addresses. Forwarded
ports bind to `127.0.0.1` on the host. On Hyper-V, machines join the switch named by
`ISOLOOM_HYPERV_SWITCH` (default `Default Switch`); Hyper-V forwards no port, so the Vagrantfile
relays each published TCP port from `127.0.0.1` with Windows' port proxy, and a UDP one is reached
at the machine's address. A Windows box's RDP forward is disabled.

Windows machines are reached over WinRM, by default plain HTTP on port 5985 with basic
authentication and the box's `vagrant` account, as Vagrant's Windows boxes are built. That traffic
stays on the host's private network; see [security considerations](../../explanation/security.md).

## Teardown

`vagrant destroy -f` in `.isoloom/vagrant/` (or `isothrum down`) removes the VMs and their disks.
The boxes stay in Vagrant's cache (`vagrant box list`, `vagrant box prune`).

## Cost

None beyond your host.
