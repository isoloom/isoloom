# Clouds: `cloud-vm`, `cloud-docker` and `cloud-services`

- **`cloud-vm`**: one cloud VM per machine, set up by the machine's steps, on AWS, Azure, Google
  Cloud, DigitalOcean, Linode or Oracle Cloud. `.isoloom/cloud-vm/<cloud>/`, one Terraform module
  and run manifest per cloud that can take the environment.
- **`cloud-docker`**: the `docker` target's containers on one cloud VM, on the same six clouds.
  `.isoloom/cloud-docker/<cloud>/`.
- **`cloud-services`**: your own Terraform module (the spec's `cloud:`), applied into your account
  on AWS, Azure or Google Cloud, with the spec's checks. `.isoloom/cloud-services/`.

Every cloud target is Tier 2: CI runs `terraform validate` on every example's modules, and AWS
has real runs from macOS. `isoloom targets` says which clouds can
take an environment and why the others can't (DigitalOcean takes one machine on one network for
`cloud-vm`; Windows runs on AWS and Azure only).

## Prerequisites

- Terraform 1.6 or later and an OpenSSH client ([tool versions](../../reference/tool-versions.md)).
- An account on the cloud, and the credentials below in your environment.
- Your public IP, or the range your connections come from, for `allowed_cidr`.
- An SSH key pair: the machines let in `ssh_public_key`.

Isoloom only writes files; it calls no cloud API. A runner applies the module. Without one, apply
it with Terraform: the machines then wait for the project, which only a runner delivers (over
SSH), so a hand-applied `cloud-vm` module gives you machines that aren't set up.

## Credentials

Each cloud's own Terraform credentials, from the environment of whoever applies the module. A run
manifest declares them as slots; a runner passes them to the Terraform steps only, never to a
machine or a generated file.

| Cloud | Environment | Module variables |
|---|---|---|
| AWS | `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN` (optional), or `AWS_PROFILE` | `region` |
| Azure | `ARM_CLIENT_ID`, `ARM_CLIENT_SECRET`, `ARM_TENANT_ID` (or `az login`), `ARM_SUBSCRIPTION_ID` | `subscription_id`, `region` |
| Google Cloud | `GOOGLE_CREDENTIALS` or `GOOGLE_OAUTH_ACCESS_TOKEN` (or `gcloud auth application-default login`) | `project`, or `billing_account` (and `org_id`) for a project of its own; `region` |
| DigitalOcean | `DIGITALOCEAN_TOKEN` | `region` |
| Linode | `LINODE_TOKEN` | `region` |
| Oracle Cloud | the OCI CLI's `~/.oci/config` | `compartment_id`, `config_file_profile`, `region` |

Every module also takes `allowed_cidr` (required), `ssh_public_key`, `expires_at` and, when an
offline network needs sealing, `isoloom_sealed` (set by the runner).

Prefer short-lived credentials: an assumed role (AWS), a workload identity or service principal
with a federated credential (Azure), a service account impersonated or federated (Google Cloud).

## Least-privilege permissions

These are outlines of what the generated modules create, to start a policy from; check them with
your security team and tighten them with tags or a dedicated account, subscription, project or
compartment.

### AWS

The modules create a VPC (with extra CIDR blocks), subnets, an internet gateway, route tables,
security groups, network interfaces, Elastic IPs, a key pair, EC2 instances, and a Route 53
private hosted zone when the spec names machines (`dns`). An IAM policy outline:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "IsoloomEc2",
      "Effect": "Allow",
      "Action": [
        "ec2:Describe*",
        "ec2:CreateVpc", "ec2:DeleteVpc", "ec2:ModifyVpcAttribute",
        "ec2:AssociateVpcCidrBlock", "ec2:DisassociateVpcCidrBlock",
        "ec2:CreateSubnet", "ec2:DeleteSubnet", "ec2:ModifySubnetAttribute",
        "ec2:CreateInternetGateway", "ec2:DeleteInternetGateway",
        "ec2:AttachInternetGateway", "ec2:DetachInternetGateway",
        "ec2:CreateRouteTable", "ec2:DeleteRouteTable", "ec2:CreateRoute", "ec2:DeleteRoute",
        "ec2:AssociateRouteTable", "ec2:DisassociateRouteTable",
        "ec2:CreateSecurityGroup", "ec2:DeleteSecurityGroup",
        "ec2:AuthorizeSecurityGroupIngress", "ec2:AuthorizeSecurityGroupEgress",
        "ec2:RevokeSecurityGroupIngress", "ec2:RevokeSecurityGroupEgress",
        "ec2:CreateNetworkInterface", "ec2:DeleteNetworkInterface",
        "ec2:AttachNetworkInterface", "ec2:DetachNetworkInterface",
        "ec2:ModifyNetworkInterfaceAttribute",
        "ec2:AllocateAddress", "ec2:ReleaseAddress",
        "ec2:AssociateAddress", "ec2:DisassociateAddress",
        "ec2:ImportKeyPair", "ec2:DeleteKeyPair",
        "ec2:RunInstances", "ec2:TerminateInstances", "ec2:StopInstances", "ec2:StartInstances",
        "ec2:ModifyInstanceAttribute",
        "ec2:CreateTags", "ec2:DeleteTags"
      ],
      "Resource": "*"
    },
    {
      "Sid": "IsoloomPrivateDns",
      "Effect": "Allow",
      "Action": [
        "route53:CreateHostedZone", "route53:DeleteHostedZone", "route53:GetHostedZone",
        "route53:ListHostedZones", "route53:ChangeResourceRecordSets",
        "route53:ListResourceRecordSets", "route53:GetChange",
        "route53:ChangeTagsForResource", "route53:ListTagsForResource"
      ],
      "Resource": "*"
    }
  ]
}
```

Narrow it with a condition on the `managed-by = isoloom` tag where the action supports one, and
with an AWS Budgets action on the account. A Kali machine boots a Marketplace image, whose terms
the account accepts once.

### Azure

The modules create a resource group per environment and, in it, a virtual network, subnets,
network security groups, route tables, public IPs, network interfaces, managed disks, Linux or
Windows VMs, and a private DNS zone. The built-in **Contributor** role on the subscription is the
simple choice; a custom role needs `Microsoft.Resources/subscriptions/resourceGroups/*`,
`Microsoft.Network/*` (virtual networks, NSGs, route tables, public IPs, NICs, private DNS zones)
and `Microsoft.Compute/*` (virtual machines, disks).

### Google Cloud

With an existing `project`: **Compute Admin** (`roles/compute.admin`) for the networks, subnets,
routes, firewall rules, disks and instances, **DNS Administrator** (`roles/dns.admin`) for the
private zone, and **Service Usage Admin** (`roles/serviceusage.serviceUsageAdmin`), since the
module enables the Compute and DNS APIs. With no project, the module creates one per environment,
billed to `billing_account` and deleted with it: that also needs **Project Creator** on the
organization or folder and **Billing Account User** on the billing account.

### DigitalOcean, Linode, Oracle Cloud

- **DigitalOcean**: a personal access token with read and write on droplets, VPCs, firewalls and
  SSH keys (a custom-scoped token).
- **Linode**: a personal access token with read/write on Linodes, VPCs, firewalls and volumes.
- **Oracle Cloud**: a user or group whose IAM policy lets it `manage virtual-network-family`,
  `manage instance-family` and `manage dns` in the environment's compartment, and `read`
  `instance-images`.

## Regions

The built-in regions are all in Europe (Paris where the cloud has a region there, else the
nearest):

| Cloud | Default region |
|---|---|
| AWS | `eu-west-3` (Paris) |
| Azure | `swedencentral` |
| Google Cloud | `europe-west9` (Paris) |
| DigitalOcean | `fra1` (Frankfurt) |
| Linode | `fr-par` (Paris) |
| Oracle Cloud | `eu-paris-1` |

Change one for everything you generate in your defaults (`~/.isoloom/defaults.yml`), for one
project in `isoloom.defaults.yml`, or for one command:

```sh
isoloom generate -s defaults.cloud.aws.region=us-east-1
ISOLOOM_CLOUD__AZURE__REGION=westeurope isoloom generate
```

The region becomes the default of the module's `region` variable, so `-var region=...` at apply
time overrides it too ([configuration](../../reference/configuration.md#defaults)).

## Network exposure

- Every machine gets a public IP address (AWS also an Elastic IP for a machine on several
  networks), so the runner can reach it.
- SSH (22), WinRM (5985, Windows machines) and the published ports accept connections only from
  `allowed_cidr`. Nothing is open to the whole internet. Set it to your address (`/32`) or your
  egress range, never `0.0.0.0/0`.
- An `internet: false` network is sealed once the machines are set up (the runner applies the
  module again with `isoloom_sealed = true`): its machines' egress is then limited to the
  environment's own networks, while SSH from `allowed_cidr` and the published ports keep working.
  The seal fails closed, and every later apply keeps it.

See [security considerations](../../explanation/security.md#public-ip-addresses-on-cloud-vms).

## Cost

Isoloom doesn't know your prices and bills nothing. What it gives you to keep costs in check:

- **Sizes** follow each machine's `resources` (1 CPU, 1024 MB and 20 GB by default for a VM):
  `cloud-vm` picks the smallest instance type with as many vCPUs as the machine's CPUs, as much
  memory, the disk where the size fixes it (DigitalOcean, Linode), a network interface per network
  and, on AWS, room for the addresses of its busiest interface (AWS `t3.micro` and up, Azure
  `Standard_B2ts_v2` and up, Google `e2-small` and up, DigitalOcean `s-1vcpu-1gb` and up, Linode
  `g6-nanode-1` and up). The controller, on every network, gets an interface on each. Oracle's
  `VM.Standard.E4.Flex` is sized to fit: an OCPU per two CPUs or per 8 GB, whichever is more. A
  machine bigger than a cloud's largest size is refused, not silently shrunk. `cloud-docker` sizes its one VM from the
  containers' total memory, and the size is a module variable (`instance_type`, `size`,
  `machine_type` or `type`), except on Oracle Cloud, whose shape takes its OCPUs and memory from
  that total, with no variable. `isoloom resources` adds up what the machines ask for.
- **Expiry.** A run manifest carries an expiry (240 minutes by default; `isothrum up --lifetime`
  changes it). The runner sets `expires_at`: every resource is tagged `isoloom-expires-at`, and the
  machines shut themselves down at that time (an AWS `cloud-vm` instance that shuts itself down is
  terminated). A stopped machine stops most compute charges, not disks or addresses: `isothrum reap <cloud>` destroys the runs past their expiry and lists what
  the cloud still holds past its expiry that no run records. Run it on a schedule. Applied by hand,
  `expires_at` is empty: no end.
- **Tags.** Every resource carries `managed-by = isoloom`, `isoloom-environment` and
  `isoloom-instance` (labels on Google Cloud), so a cost report can group by environment. The
  spec's `tags:` map adds your own cost-allocation tags to every resource.
- **`cloud.hourly_usd`** (for `cloud-services`): the spec author's estimate of what the
  environment costs per hour. Isoloom copies it into `.isoloom/resolved.json`; a runner or a
  portal can show it before applying or keep to a budget with it. It is an estimate, not a limit.
- The best guard is the cloud's own: a budget with an action (AWS Budgets, Azure cost alerts,
  Google Cloud budgets) on a dedicated account.

## Generate and run

```sh
isoloom generate --target cloud-vm
export AWS_ACCESS_KEY_ID=... AWS_SECRET_ACCESS_KEY=...
isothrum up .isoloom/cloud-vm/aws/run.json \
  --secret-from-env AWS_ACCESS_KEY_ID --secret-from-env AWS_SECRET_ACCESS_KEY \
  --param allowed_cidr="$(curl -fsS https://checkip.amazonaws.com)/32" --lifetime 120
isothrum check .isoloom/cloud-vm/aws/run.json
isothrum down .isoloom/cloud-vm/aws/run.json
```

The runner's parameters, secret inputs and lifetime are documented with it:
[Isothrum](https://github.com/Isothrum/isothrum).

## Teardown

`isothrum down`, or `terraform destroy` with the same variables. Each run's Terraform state is
local, in the runner's state folder ([remote state](../remote-terraform-state.md)). Leftovers of
an interrupted run are found by their tags and destroyed by `isothrum reap`. On Google Cloud, a
project created for the environment is deleted with it.
