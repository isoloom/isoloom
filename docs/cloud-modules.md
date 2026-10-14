# Cloud modules: inputs and outputs

`isoloom generate` writes a Terraform root module for each cloud target. This page describes
their input variables and outputs. Any tool that runs these modules can rely on them: your own
scripts, a CI job, a dashboard. `isoloom run`, `isoloom test` and `isoloom down` use the same
interface.

| Target | Module | One per |
|---|---|---|
| `cloud-docker` | `.isoloom/cloud-docker/<cloud>/main.tf` | cloud: `aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci` |
| `cloud-vm` | `.isoloom/cloud-vm/<cloud>/main.tf` | cloud: `aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci` (a cloud that can't express a spec gets no module; the others are still written) |
| `cloud-services` | the project's own module, run by `.isoloom/cloud-services/*.sh` | environment |

The cloud's credentials always come from the environment, as each Terraform provider expects
(`AWS_*`, `ARM_*`, `GOOGLE_*`, ...). Isoloom never writes them to a file.

## `cloud-docker` and `cloud-vm`

### Input variables

Shared by every cloud:

| Variable | Type | Default | Meaning |
|---|---|---|---|
| `allowed_cidr` | string | required | Who may reach the machines (SSH and the published ports), e.g. `203.0.113.7/32`. |
| `ssh_public_key` | string | required | The public key installed for the SSH user. |
| `ssh_private_key_file` | string | required | Its private key. Terraform sets the machines up over SSH. |
| `region` | string | per cloud | Where to launch. Defaults: `eu-west-3` (AWS), `swedencentral` (Azure), `europe-west9` (Google Cloud), `fra1` (DigitalOcean), `fr-par` (Linode), `eu-paris-1` (Oracle Cloud). |
| `auto_stop_minutes` | number | `0` | Shut the machines down after this many minutes (`0`: never). Destroying still ends the billing for disks and addresses. |
| `expires_at` | string | `""` | When the environment should end, in Unix seconds (empty: no end). It becomes the `isoloom-expires-at` tag on every resource, so a reaper can find what to destroy. |
| `inputs` | map(string), sensitive | `{}` | Values given at launch, only when the spec declares `inputs`. They reach the machines' set-up (the Compose file, provision steps) as environment variables. |

Per cloud:

| Cloud | Variable | Meaning |
|---|---|---|
| Azure | `subscription_id` | Defaults to `ARM_SUBSCRIPTION_ID` from the environment. |
| Google Cloud | `project` | An existing project. Leave it empty and give `billing_account` to get a project of its own, deleted with the environment. |
| Google Cloud | `billing_account`, `org_id` | For a project of its own. |
| Oracle Cloud | `compartment_id` | The compartment the environment goes in (required). |
| `cloud-docker` only | `instance_type` (AWS), `size` (Azure, DigitalOcean), `machine_type` (Google Cloud), `type` (Linode) | The VM size, with a default sized for every machine at once. |

### Outputs

| Output | Target | Value |
|---|---|---|
| `ip` | both | The public address to start from: the VM (`cloud-docker`); the access machine, else the first machine, else the controller (`cloud-vm`). |
| `ssh_user` | `cloud-docker` | The SSH login on that VM. |
| `machines` | `cloud-vm` | Machine name to public address. |
| `ssh_users` | `cloud-vm` | Machine name to SSH login (`isoloom` on a Windows machine, over its OpenSSH server). |
| `ready_file` | both | `/var/lib/isoloom/ready`. It exists on the machine once set-up has finished. |
| `checks` | `cloud-vm`, when the environment has checks | A list of `{ position, machine, host, user, command }`. Running `ssh <user>@<host> '<command>'` runs one position's checks. `isoloom test cloud-vm` does exactly this. |
| `published` | `cloud-vm`, when a machine publishes a port | `"<machine>/<port>"` to `"<public address>:<published port>"`. |

`isoloom test cloud-docker` reads `ip` and `ssh_user`; `isoloom test cloud-vm` reads `checks`.

### Tags

On AWS, Azure and Google Cloud, every resource that takes tags (labels on Google Cloud) carries
`managed-by = isoloom`, `isoloom-environment = <spec name>`, `isoloom-instance = <unique name of
this run>` and `isoloom-expires-at = <expires_at>`, so leftovers can be found by tag.

### Private DNS (`dns.private`)

With `dns: { private: true }` in the spec (off by default: a zone is billed), a `cloud-vm`
module also serves the environment's names under the zone's domain (`dns.domain`, by default
`<spec name>.internal`): each machine's name and each of its `aliases`, an A record at the
machine's first address.

| Cloud | What the module writes |
| --- | --- |
| AWS | `aws_route53_zone` (private, its `vpc` the environment's), `aws_route53_record` per name; the VPC's `enable_dns_support` and `enable_dns_hostnames` |
| Azure | `azurerm_private_dns_zone`, `azurerm_private_dns_zone_virtual_network_link`, `azurerm_private_dns_a_record` per name and `azurerm_private_dns_aaaa_record` when the machine's first network is IPv6; reverse names: a reverse zone per /24 holding a machine (`10.84.0.0/24` is `0.84.10.in-addr.arpa`), linked the same way, with an `azurerm_private_dns_ptr_record` per machine naming it |
| Google Cloud | `google_project_service` (`dns.googleapis.com`, in a project of its own), `google_dns_managed_zone` (private, on every network's VPC network), `google_dns_record_set` per name |
| Oracle Cloud | `oci_dns_view`, `oci_dns_zone` and `oci_dns_rrset` per name in it, the view attached to the VCN's resolver (`oci_dns_resolver`) |
| DigitalOcean, Linode | Nothing: they have no private DNS |

Every VM's hosts file carries the same names, so they resolve on DigitalOcean and Linode too,
and before a zone answers.

### Networks on each cloud (`cloud-vm`)

Every cloud keeps each network's exact range and each machine's address on it. What differs:

| Cloud | Networks | A machine on several | `reach` | `gateway:` |
|---|---|---|---|---|
| AWS | a subnet each, in one VPC | an interface each | security groups, inbound | yes: every network inside one /16 |
| Azure | a subnet each, in one virtual network | an interface each | network security groups, inbound | yes |
| Google Cloud | a VPC network each, holding its subnetwork; the networks no machine routes peered with each other | an interface each (8 at most, and no more than the machine type's vCPUs: a bigger E2 type when needed) | firewall rules, inbound, in the VPC of the network opened | yes |
| DigitalOcean | one network, one machine | no | firewall | no |
| Linode | a VPC subnet each | no (an instance has one VPC interface) | a firewall per machine, inbound and outbound | refused: a VPC has no route tables |
| Oracle Cloud | a subnet each, in one VCN (5 networks at most) | a VNIC each | a security list per network, inbound and outbound | yes |

Published ports open only on the machine that publishes them, on every cloud.

A dual-stack network (`ipv6:`) keeps its block on Azure (an address space of the virtual
network and its subnet's prefix) and on Oracle Cloud: the VCN takes every block as a private
prefix (`ipv6private_cidr_blocks`, `is_ipv6enabled`) with no Oracle-allocated public prefix beside
them (`is_oracle_gua_allocation_enabled = false`), and the subnet takes its network's
(`ipv6cidr_block`). Each VNIC asks for an address there (`assign_ipv6ip`): the machine's derived
one (`ipv6address_ipv6subnet_cidr_pair_details`), or with `addresses: auto` the one Oracle Cloud
picks, which the set-up reads back from Terraform and puts on the interface. The security lists
mirror the IPv4 rules on the IPv6 blocks, ICMPv6 "packet too big" included. No IPv6 route leaves
the VCN: Oracle Cloud routes no private prefix to the internet, so machines go out over IPv4.
AWS, Google Cloud, DigitalOcean and Linode refuse IPv6 networks: none of their networks takes a
block the spec picks.

On a network a machine routes (`gateway:`), the cloud keeps the subnet's first address for its
own router, so the gateway machine sits at the network's router address (its last usable address,
unused on such a network), and the hosts files and the private zone say so. Google Cloud keeps
the second to last address too, so there the gateway takes the highest address no machine, tool or
controller uses (`.252` in a /24, usually). It forwards, its source and destination checks are
off (IP forwarding on its interfaces on Azure and Google Cloud), and each network's route table
sends traffic between the routed network and the others through it, both ways. The routed
network's machines keep their own public addresses, so their internet traffic leaves directly
rather than through the gateway.

On Google Cloud a route can't be as specific as a subnetwork of its own VPC, so each network is a
VPC network of its own: the routed network's VPC sends every other network to the gateway's
address there, each other VPC the gateway is on sends the routed network to its address in that
one, and a network the gateway isn't on reaches it through a peered network that is (the
peerings exchange these routes). A routed network is peered with none, so its traffic crosses the
gateway.

A network with `internet: false` is closed in the machines once they are set up, on every cloud;
on Azure its subnet also has no default way out (`default_outbound_access_enabled = false`) for
an interface without a public address. Google Cloud networks have Ethernet's MTU (1500), as on
every other target, or their own `mtu` (1300 to 8896): each is a VPC network of its own.

### Upgrading an existing Google Cloud environment

1.0's Google Cloud module replaces 0.x's single VPC network (`google_compute_network.env`) with
one VPC network per network (`google_compute_network.<network>`), and each machine's firewall
rule `<machine>_net` with one per network (`<machine>_net_<network>`). An instance can't move its
interface to another VPC network in place, so `terraform apply` with 1.0's module on an
environment 0.x applied would try to replace the network under running instances and fail.

Destroy an applied 0.x environment with its old module first (`terraform -chdir=.isoloom/cloud-vm/gcp
destroy`, before regenerating), then generate and apply 1.0's. Where an environment must be kept,
`terraform state mv` can carry over what keeps its shape (the project, the subnetworks, the
disks, and on a single-network environment the VPC network itself, from `google_compute_network.env`
to `google_compute_network.<network>`), but the instances of a machine on several networks are
rebuilt either way.

### Data disks (`disks`)

A machine's data disks are block volumes of their size, attached to its VM. The machine's set-up
waits for each blank disk of that size, formats it with the disk's name as its label and mounts
it from `/etc/fstab`. A cloud refuses a disk below its smallest volume, or more disks than a VM
takes.

| Cloud | What the module writes | Smallest volume |
| --- | --- | --- |
| AWS | An `ebs_block_device` block on `aws_instance` per disk (`gp3`, created and deleted with it) | 1 GB |
| Azure | `azurerm_managed_disk` and `azurerm_virtual_machine_data_disk_attachment` per disk (two at most) | 1 GB |
| Google Cloud | `google_compute_disk` per disk (`pd-balanced`), attached by the instance's `attached_disk` | 10 GB |
| DigitalOcean | `digitalocean_volume` per disk, attached by the droplet's `volume_ids` | 1 GB |
| Linode | `linode_volume` per disk, attached to the instance | 10 GB |
| Oracle Cloud | A `launch_volume_attachments` block on the instance per disk (paravirtualized) | 50 GB |

### Running a module by hand

```sh
terraform -chdir=.isoloom/cloud-vm/aws init
terraform -chdir=.isoloom/cloud-vm/aws apply \
  -var allowed_cidr=203.0.113.7/32 \
  -var ssh_public_key="$(cat ~/.ssh/id_ed25519.pub)" \
  -var ssh_private_key_file=~/.ssh/id_ed25519
terraform -chdir=.isoloom/cloud-vm/aws output -json
terraform -chdir=.isoloom/cloud-vm/aws destroy   # same variables
```

`isoloom run cloud-vm` runs `terraform init` then `apply` in the module folder. Variables come
from Terraform's usual sources (`TF_VAR_<name>`, a `*.auto.tfvars` file).

## `cloud-services`

The module is the project's own (`cloud.terraform`), and Isoloom doesn't change it. Its inputs
and outputs are whatever it declares. Under `.isoloom/cloud-services/`, Isoloom writes:

| File | What |
|---|---|
| `terraform.tfvars.json` | The fixed values of `cloud.vars`. |
| `isoloom-instance` | Written by `isoloom run`: this deployment's name, passed to Terraform as `TF_VAR_isoloom_instance` (see below). |
| `inputs.tfvars.json` | Written by `isoloom run`: each variable whose value holds `{{ inputs.NAME }}` (in a string, at any depth: `["{{ inputs.PLAYER_IP }}/32"]`), filled from the environment variable `NAME`. It is kept for `down` and is mode 0600. |
| `up.sh` | Copies the module to `module/`, runs the `cloud.before` steps, then `terraform init` and `apply` on the copy, with both variable files. |
| `down.sh` | `terraform destroy` on the copy, with the same variables. |
| `outputs.sh` | The module's outputs as JSON (`terraform output -json`). |

Terraform runs on a copy of the module, `module/`, refreshed over the old one at every `run`
(`.terraform` folders and state files are left out; the old copy's `*.tf`, `*.tf.json`,
`*.tfvars` and `*.tfvars.json` are removed first, so a file renamed or deleted in the module is
no longer applied). The lock file and whatever the module or a
step writes (archives, keys) land there, not in the project. A path the module reads outside
its own folder (`${path.module}/../x`) doesn't resolve from the copy: keep what it needs inside
it. `down` destroys with the copy as it was applied, and copies the module again (with the
steps) only when it is gone. The state (`terraform.tfstate`) and the provider plugins
(`.terraform/`) stay in `.isoloom/cloud-services/` too. While `up.sh` applies, `deploying`
holds its process id: `isoloom status` says `deploying` until the apply ends.

### Tags and leftovers

`isoloom run` names each deployment (`isoloom-<environment>[-<instance>]-<8 hex>`, kept in
`isoloom-instance` next to the state) and the scripts pass it to Terraform as
`TF_VAR_isoloom_instance`. A module that declares the variable can tag what it creates with it,
on AWS with the provider's `default_tags`:

```hcl
variable "isoloom_instance" {
  type    = string
  default = ""
}

provider "aws" {
  default_tags {
    tags = { "isoloom-instance" = var.isoloom_instance }
  }
}
```

After `terraform destroy`, `isoloom down` (AWS, with the `aws` CLI installed) asks the Resource
Groups Tagging API for anything still tagged `isoloom-instance=<that name>`: what AWS created
outside Terraform (log groups, service-linked roles) or is still deleting (secrets pending
deletion). It lists their ARNs and deletes nothing. The region is `AWS_REGION` or
`AWS_DEFAULT_REGION` from `cloud.env`, else `cloud.vars.region` as applied (an input's value
included), else the user's environment, else the CLI's default. The name is forgotten once
nothing is left in a known region, so the next `run` gets a new one; when the region is unknown
it is kept for the next `down`. Only tagged resources are
found, and only in that region.

`cloud.outputs` maps names to module outputs. Checks and the message use them as
`{{ cloud.outputs.<name> }}`.

The other keys of `cloud:`:

| Key | What |
|---|---|
| `before` | Scripts in the project, run in order before `terraform init` (CloudGoat's `ssh-keygen`). Each runs in the module's copy, as itself when executable (its shebang), else with `sh`, with `env`, the launch-time inputs and `ISOLOOM_PROJECT` (the project folder) in its environment. They run at every `run`, so make them idempotent (`[ -f key ] \|\| ssh-keygen ...`). |
| `env` | Environment variables for Terraform and the steps, e.g. `AWS_REGION: us-east-1`, exported by every script. Not for credentials. The scripts' own names (`here`, `src`, `module`, `tries`, `rc`, `TF_DATA_DIR`, `TF_VAR_isoloom_instance`, `ISOLOOM_*`) and the shell's (`PATH`, `IFS`, `ENV`, `CDPATH`, `PS4`, `HOME`) are refused. |
| `apply_retries` | How many more applies after a failed one (0 to 5, default 0), for a module that only converges on a second apply. |
| `requires` | What the module needs on this machine besides Terraform: a command name (`gsed`, `jq`) looked up on the PATH, or a shell command that must succeed (`python3 -c 'import boto3'`, `bash -c '[ ${BASH_VERSINFO[0]} -ge 5 ]'`). `isoloom doctor` (run in the project folder, or given it) looks up the command names and lists the shell commands without running them (they are the project's code); `isoloom run` checks both and refuses when one is missing. On Windows a command name is looked up on the PATH with `PATHEXT` (`jq` finds `jq.exe`), and a shell command runs in PowerShell. |

```yaml
cloud:
  provider: aws
  terraform: terraform
  vars: { cg_whitelist: ["{{ inputs.PLAYER_CIDR }}"], region: us-east-1 }
  env: { AWS_REGION: us-east-1 }
  before: [keys.sh]
  apply_retries: 1
  requires: [jq, "python3 -c 'import boto3'"]
inputs: [PLAYER_CIDR]
```
