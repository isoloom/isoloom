# ESXi: `esxi`

The `esxi` target writes a Terraform module (`.isoloom/esxi/main.tf`, provider
`josenk/esxi ~> 1.10.3`) that makes one VM per machine on a standalone ESXi host (no vCenter),
with a vSwitch and port groups for the networks and a router VM, then sets each machine up over
SSH through the router.

Tier 2, with a partial real run from macOS. There is no run manifest: run the module with
Terraform. (VMs on a remote ESXi host through Vagrant are the `vagrant` target's `vmware_esxi`
provider: [Vagrant](vagrant.md).)

## Prerequisites

- A standalone ESXi host with SSH enabled (the provider works over SSH, port 22) and its API on
  443.
- Terraform 1.6 or later (1.11 recommended), VMware's `ovftool` and an OpenSSH client on the
  machine that runs Terraform.
- An OVA per OS, with cloud-init and open-vm-tools: the `images` variable maps each OS
  (`debian-12`, `ubuntu-24.04`) to an OVA's path or URL. The router runs Ubuntu 24.04, whose
  default is Canonical's cloud OVA; any other OS needs yours, and the module refuses to plan until
  every OS it names has one.
- A datastore for the disks (`disk_store`, default `datastore1`) and a port group with DHCP for
  the router's uplink (`uplink_port_group`, default `VM Network`).

## Credentials

| Variable | What |
|---|---|
| `esxi_hostname` | The host's name or address |
| `esxi_username`, `esxi_password` | An ESXi account (default user `root`) |
| `ssh_public_key`, `ssh_private_key_file` | The key the machines are set up with |

A dedicated ESXi account needs the rights to create and delete VMs, vSwitches and port groups, and
to write to the datastore; a standalone host's built-in *Administrator* role on the host is the
simplest that works.

## Generate and run

```sh
isoloom generate --target esxi
terraform -chdir=.isoloom/esxi init
export TF_VAR_esxi_password="$ESXI_PASSWORD"   # never on the command line
terraform -chdir=.isoloom/esxi apply \
  -var esxi_hostname=esxi.example.internal \
  -var-file="$PWD/images.tfvars" \
  -var ssh_public_key="$(cat ~/.ssh/id_ed25519.pub)" -var ssh_private_key_file=~/.ssh/id_ed25519
```

where `images.tfvars` (in the project folder: `-chdir` reads relative paths from the module's) gives every OS its OVA (the default map is replaced, so Ubuntu's too):

```hcl
images = {
  "debian-12"    = "https://images.example.internal/debian-12.ova"
  "ubuntu-24.04" = "https://cloud-images.ubuntu.com/noble/20260926/noble-server-cloudimg-amd64.ova"
}
```

The checks are scripts under `.isoloom/esxi/checks/`, run from the machines.

## Security notes

The ESXi provider connects over SSH with a password; prefer an account used only for this.
Windows machines are set up over WinRM in plain HTTP on the environment's private networks (see
[security considerations](../../explanation/security.md#winrm)). The ESXi target refuses secrets
and inputs delivered as files.

## Teardown

`terraform -chdir=.isoloom/esxi destroy` with the same variables. ESXi runs have no expiry.

## Cost

None beyond your host. Each environment on one host needs its own `slot` (1 to 63): it names the
vSwitches and the VMs and makes the MAC addresses.
