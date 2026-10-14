# The `esxi` target

`isoloom generate` writes `.isoloom/esxi/main.tf`, a Terraform root module for a standalone
ESXi host (no vCenter), with the community provider
[josenk/esxi](https://registry.terraform.io/providers/josenk/esxi/latest) 1.10.x. It is built
like the `proxmox` target: one VM per machine, a router VM, and the machines set up from the
plan.

This module is generated, checked by `terraform validate` and snapshot tests, and has not been
run on a real ESXi host yet.

## What it creates

- **Networks.** Each network gets its own vSwitch with no uplink, and one port group on it
  (`iso<slot>-<network>`). Networks never share a layer 2 segment. The port group's VLAN is
  4095, so a machine's own VLAN tags (trunks) pass. A network's `mtu` is its vSwitch's, and
  each guest interface on it gets it too.
- **Networks outside the environment.** An `external` network is an existing port group, by
  its name. A `bridged` network is a port group of the environment's on the host's LAN, tagged
  with its `vlan` when it has one: on the host's own vSwitch (`uplink_vswitch`), or, when it
  names an `interface`, on a vSwitch of its own whose uplink is that physical NIC (`vmnic1`,
  which no other vSwitch may hold). As on Proxmox, the router and the controller aren't on
  them, and a machine takes its fixed address there or the LAN's DHCP (its default route too
  with `default_route`).
- **The router.** It is an Ubuntu 24.04 VM on the host's port group (`uplink_port_group`, DHCP)
  and on every network at its router address. It works as on Proxmox: the `reach` rules between
  networks (nftables), the published ports forwarded from its uplink address, and NAT to the
  internet. VMware Tools reports its uplink address (`address` output). On an IPv6 network it
  also has its IPv6 router address; the machines get theirs from their set-up steps.
- **The machines.** Each machine is a VM deployed from an OVA (`images`), with its CPUs, memory
  and disk. cloud-init's VMware data source reads two documents from `guestinfo`. The metadata
  holds the VM's addresses, each interface found by its MAC, with the router as its gateway.
  The user data creates the `isoloom` user with `ssh_public_key`. Terraform then sets the
  machine up over SSH, through the router: the project is uploaded to `/opt/isoloom`, then the
  machine's steps from the plan run, with a restart between parts when it has a `reboot` step.
  `vm.boot.firmware` sets its firmware (`boot_firmware`: `bios` or `efi`); without it, the
  OVA's is kept.
- **Data disks.** Each of a machine's `disks` is an `esxi_virtual_disk` (thin, on `disk_store`,
  in `iso<slot>-<machine>-disks`), attached to its VM by SCSI slot from `0:1` (14 at most:
  `0:7` is the controller's). The machine's set-up finds each blank disk by its size, then
  formats and mounts it.
- **Windows machines.** A Windows machine is a VM from the OVA you give for its OS in `images`,
  with VMware Tools and cloudbase-init installed (its VMware guestinfo metadata service), then
  generalized with sysprep. The VM gets the guest OS type ESXi tunes it for (`guestos`, such as
  `windows2019srv-64`). cloudbase-init reads its addresses from the same metadata, then runs
  its set-up at the first boot, as on Proxmox: the `isoloom` administrator with the generated
  password, its name, the project in `C:\isoloom`, its ports' firewall openings, its steps,
  and WinRM over HTTP last. Terraform's WinRM can't go through the router, so the controller
  waits for its ready marker over WinRM before the playbooks.
- **The controller.** When the spec has `provision:` playbooks, an Ubuntu VM on every network
  runs them over SSH once every machine is set up, with the same inventory as Proxmox.
- **Secrets.** `isoloom run` delivers them while Terraform runs: a Linux machine's set-up
  writes its address and the router's to `.isoloom-reach/<machine>`, and `isoloom run` writes
  the values over SSH through the router. A Windows machine gets them over WinRM, through an SSH
  tunnel by the router, once Terraform is done. See [secrets](secrets.md).

## Running it

You need Terraform, VMware's `ovftool` on the machine that runs Terraform (the provider deploys
the OVAs with it), and SSH enabled on the ESXi host (the provider runs its commands over SSH).

```
export TF_VAR_esxi_hostname=esxi.lab.local TF_VAR_esxi_password=...
export TF_VAR_ssh_public_key="$(cat ~/.ssh/id_ed25519.pub)" TF_VAR_ssh_private_key_file=~/.ssh/id_ed25519
isoloom run esxi        # terraform init && apply in .isoloom/esxi
isoloom test esxi       # the check runners, piped to each machine through the router
isoloom connect web     # ssh -J isoloom@<router> isoloom@<web>
isoloom down esxi
```

`isoloom targets --host` says whether `ovftool`, `TF_VAR_esxi_hostname` and
`TF_VAR_esxi_password` are there.

## Input variables

| Variable | Type | Default | Meaning |
|---|---|---|---|
| `esxi_hostname` | string | required | The ESXi host's name or address. |
| `esxi_hostport` | string | `"22"` | Its SSH port. |
| `esxi_hostssl` | string | `"443"` | Its HTTPS port. |
| `esxi_username` | string | `"root"` | The account the provider logs in as. |
| `esxi_password` | string | required | Its password (sensitive). |
| `disk_store` | string | `"datastore1"` | The datastore the VMs' disks go on. |
| `uplink_port_group` | string | `"VM Network"` | The port group the router reaches the internet through, with DHCP. |
| `slot` | number | `1` | 1 to 63, unique per environment on the host. It names the vSwitches and VMs (`iso<slot>-...`), and makes the MACs (`00:50:56:<slot>:...`). |
| `images` | map(string) | Ubuntu's cloud OVAs | An OVA (a path or a URL) per OS the environment runs. Ubuntu 20.04, 22.04 and 24.04 default to Ubuntu's own cloud OVAs. Any other Linux OS (Debian, Rocky, AlmaLinux) needs an OVA you give, with cloud-init and open-vm-tools in it; a Windows OS, one with VMware Tools and cloudbase-init, sysprepped. The variable's validation names the missing ones. |
| `uplink_vswitch` | string | `"vSwitch0"` | The host's vSwitch on its LAN, when a `bridged` network names no interface: its port group goes there. |
| `ssh_public_key` | string | required | Installed for the `isoloom` user on every VM. |
| `ssh_private_key_file` | string | required | Its private key. Terraform sets the machines up over SSH, through the router. |
| `inputs` | map(string) | `{}` | The spec's inputs, when it has some. |

## Outputs

These are the same as the `proxmox` module's: `address` (the router's uplink address),
`published` (each published service there), `machines` (each machine's address behind the
router), `ssh_user` (`isoloom`) and `checks` (which runner runs where).

## Not supported yet

The target refuses these, with the reason: arm64, networks with a `gateway` machine, a
machine's own image, a boot other than its firmware (a kernel, firmware files, a boot order
other than its disk, a network boot: the provider has no setting for them), a TPM, GPUs,
nested virtualization and `rate_limit` (the provider has no setting for them either), more than
14 data disks on a machine, and more than nine networks (an ESXi VM has at most ten network
interfaces, and the router needs one per network plus its uplink). A Windows machine has
Proxmox's limits: PowerShell provisioning steps, a name of 15 characters at most, and no
volumes, inputs, gateway routes or `internet: false` yet. A volume's `virtiofs` setting is Proxmox's and is ignored here: a
standalone host has no folder to share into a VM, so the volume is shared over NFS from its
owner.

## The `esxi` target or Vagrant's ESXi provider?

Both run on a standalone ESXi host. They differ in what they build.

| | `esxi` (this module) | `vagrant` with `provider: vmware_esxi` |
|---|---|---|
| Tool | Terraform, josenk/esxi | Vagrant, the vagrant-vmware-esxi plugin |
| Images | OVAs (Ubuntu's cloud OVAs, or yours) | Vagrant boxes built for ESXi |
| Networks | Isoloom creates a vSwitch and port group per network, isolated, with its own router enforcing `reach` | Port groups you already have, listed in `ESXI_VIRTUAL_NETWORK` |
| Reaching the machines | Through the router (one address on your network) | Each machine on the management port group |
| Windows | From your OVA, set up by cloudbase-init | Supported (WinRM) |
| State | Terraform state in `.isoloom/esxi` | Vagrant's `.vagrant` folder |

Use the `esxi` target when the host should hold a self-contained lab: its own isolated
networks, `reach` enforced between them, nothing on the host's network but the router, and the
same Terraform workflow as Proxmox and the clouds. Use Vagrant's ESXi provider when you already
work with Vagrant, have Windows boxes but no Windows OVA, or want every machine directly on
port groups you manage yourself.
