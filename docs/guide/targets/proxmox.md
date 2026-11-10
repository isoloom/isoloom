# Proxmox VE: `proxmox` and `docker-vm` on Proxmox

The `proxmox` target writes a Terraform module (`.isoloom/proxmox/main.tf`, provider
`bpg/proxmox ~> 0.115.0`) that makes one VM per machine on a Proxmox VE server, with an SDN zone
and VNet per network, a resource pool, a router VM where `reach` or `internet: false` need one,
and Proxmox firewall rules. `docker-vm` can also put its single Docker VM on Proxmox
(`.isoloom/docker-vm/proxmox/`). Both write a run manifest.

Tier 2: real runs from macOS are recorded; Linux and Windows hosts are pending.

## Prerequisites

- Proxmox VE with SDN available, reachable from where Terraform runs (its API on port 8006, SSH
  to the node).
- Terraform 1.6 or later ([tool versions](../../reference/tool-versions.md)),
  and an OpenSSH client.
- Datastores: `local-lvm` for VM disks, and one with `iso` content and one with `snippets`
  content (`local` by default) for the cloud images and cloud-init files. Each is a variable.
- An uplink bridge for the router's way out (`vmbr0` by default).

## Credentials

Two things, both given as Terraform variables (a runner fills them from its own secret inputs, as
`TF_VAR_*` in the Terraform steps' environment only):

| Variable | What |
|---|---|
| `proxmox_endpoint` | `https://<server>:8006/` |
| `proxmox_api_token` | An API token, `user@realm!name=secret` (or `proxmox_username` and `proxmox_password`) |
| `proxmox_ssh_username`, `proxmox_ssh_private_key_file` | SSH to the node, which uploads the cloud-init snippets (default user `root`) |
| `node` | The node the VMs go on (required: `pvesh get /nodes` lists them) |

Prefer an API token of a dedicated user over `root@pam`. An outline of the privileges its role
needs, on `/` or narrowed to the pool, the datastores, `/sdn` and the node (check them against the
`bpg/proxmox` provider's documentation for your Proxmox version):

- `VM.Allocate`, `VM.Audit`, `VM.Clone`, `VM.Config.*` (CD-ROM, Cloudinit, CPU, Disk, HWType,
  Memory, Network, Options), `VM.PowerMgmt`, and the guest agent's audit privilege;
- `Datastore.Allocate`, `Datastore.AllocateSpace`, `Datastore.AllocateTemplate`,
  `Datastore.Audit` (cloud images and snippets);
- `SDN.Allocate`, `SDN.Audit`, `SDN.Use` (the zone and VNets);
- `Pool.Allocate`, `Pool.Audit`;
- `Sys.Audit`, `Sys.Modify` (firewall options and rules).

The SSH user must be able to write to the snippets datastore's folder on the node.

### TLS

`proxmox_insecure` defaults to `false`: the module verifies the Proxmox API's certificate. A
server that presents its own self-signed certificate, as a fresh Proxmox install does, needs
`proxmox_insecure=true`, or better, a certificate your machines trust (from your corporate CA, or
ACME). See
[security considerations](../../explanation/security.md#proxmox-tls).

## Generate and run

```sh
isoloom generate --target proxmox
terraform -chdir=.isoloom/proxmox init
terraform -chdir=.isoloom/proxmox apply \
  -var proxmox_endpoint=https://pve.example.internal:8006/ \
  -var proxmox_api_token='isoloom@pve!ci=...' \
  -var node=pve
```

Or with a runner, which also delivers the project and the secrets, waits for every set-up, seals
offline networks and runs the checks:

```sh
isothrum up . --target proxmox
isothrum check . --target proxmox
isothrum down . --target proxmox
```

A runner whose machine can't route to the run's bridge goes through the node:
`isothrum up --ssh-jump root@pve.example.internal`.

## Teardown

`terraform destroy` with the same variables, or `isothrum down`. Everything the module makes is in
its resource pool and named after the environment. Proxmox runs have no expiry: nothing stops
them but a teardown.

## Cost

None beyond your server. A VM defaults to 1 CPU, 1024 MB and 20 GB (`machines.<name>.resources`);
`isoloom resources` adds them up. Each environment on one server needs its own `slot` (1 to 99,
default 1): SDN ids are short, and the slot keeps them apart.
