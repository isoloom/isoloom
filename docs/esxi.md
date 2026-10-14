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
  4095, so a machine's own VLAN tags (trunks) pass.
- **The router.** It is an Ubuntu 24.04 VM on the host's port group (`uplink_port_group`, DHCP)
  and on every network at its router address. It works as on Proxmox: the `reach` rules between
  networks (nftables), the published ports forwarded from its uplink address, and NAT to the
  internet. VMware Tools reports its uplink address (`address` output).
- **The machines.** Each machine is a VM deployed from an OVA (`images`), with its CPUs, memory
  and disk. cloud-init's VMware data source reads two documents from `guestinfo`. The metadata
  holds the VM's addresses, each interface found by its MAC, with the router as its gateway.
  The user data creates the `isoloom` user with `ssh_public_key`. Terraform then sets the
  machine up over SSH, through the router: the project is uploaded to `/opt/isoloom`, then the
  machine's steps from the plan run, with a restart between parts when it has a `reboot` step.
- **The controller.** When the spec has `provision:` playbooks, an Ubuntu VM on every network
  runs them over SSH once every machine is set up, with the same inventory as Proxmox.

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
| `images` | map(string) | Ubuntu's cloud OVAs | An OVA (a path or a URL) per OS the environment runs. Ubuntu 20.04, 22.04 and 24.04 default to Ubuntu's own cloud OVAs. Any other OS (Debian, Rocky, AlmaLinux) needs an OVA you give, with cloud-init and open-vm-tools in it. The variable's validation names the missing ones. |
| `ssh_public_key` | string | required | Installed for the `isoloom` user on every VM. |
| `ssh_private_key_file` | string | required | Its private key. Terraform sets the machines up over SSH, through the router. |
| `inputs` | map(string) | `{}` | The spec's inputs, when it has some. |

## Outputs

These are the same as the `proxmox` module's: `address` (the router's uplink address),
`published` (each published service there), `machines` (each machine's address behind the
router), `ssh_user` (`isoloom`) and `checks` (which runner runs where).

## Not supported yet

The target refuses these, with the reason: Windows machines, arm64, IPv6 networks, networks
with a `gateway` machine, secrets, external and bridged networks, a machine's own image or boot,
TPM, GPUs, nested virtualization, a network's `mtu`, a machine's `rate_limit`, and more than nine networks (an ESXi VM has at most ten
network interfaces, and the router needs one per network plus its uplink).

## The `esxi` target or Vagrant's ESXi provider?

Both run on a standalone ESXi host. They differ in what they build.

| | `esxi` (this module) | `vagrant` with `provider: vmware_esxi` |
|---|---|---|
| Tool | Terraform, josenk/esxi | Vagrant, the vagrant-vmware-esxi plugin |
| Images | OVAs (Ubuntu's cloud OVAs, or yours) | Vagrant boxes built for ESXi |
| Networks | Isoloom creates a vSwitch and port group per network, isolated, with its own router enforcing `reach` | Port groups you already have, listed in `ESXI_VIRTUAL_NETWORK` |
| Reaching the machines | Through the router (one address on your network) | Each machine on the management port group |
| Windows | Refused for now | Supported (WinRM) |
| State | Terraform state in `.isoloom/esxi` | Vagrant's `.vagrant` folder |

Use the `esxi` target when the host should hold a self-contained lab: its own isolated
networks, `reach` enforced between them, nothing on the host's network but the router, and the
same Terraform workflow as Proxmox and the clouds. Use Vagrant's ESXi provider when you already
work with Vagrant, need Windows machines, or want the machines directly on port groups you
manage yourself.
