# Real targets in CI

CI runs the examples for real on Docker and on Kubernetes (kind) at every push (`ci.yml`).
Three more workflows run them on local VMs, on Proxmox VE and on the six clouds. They need
machines and credentials that only the maintainer can provide, so each one does nothing until
they are there.

| Workflow | Target | Examples | Where | When |
|---|---|---|---|---|
| `real-vagrant.yml` | `vagrant` | solo-web, segmented, and windows-hello where Windows boxes are allowed | a self-hosted runner per provider, labelled `virtualbox` or `libvirt` | weekly (Wednesday), or by hand |
| `real-proxmox.yml` | `proxmox` | hello-stack, segmented | a self-hosted runner labelled `proxmox` that reaches the cluster | weekly (Thursday), or by hand |
| `real-cloud.yml` | `cloud-docker` | solo-web | GitHub's runners, one job per cloud: `aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci` | weekly (Tuesday), or by hand; reaping only, daily |

Each job runs `isoloom run`, `isoloom test` and `isoloom down` with `--json`, and uploads a
[record](#records) of the run as an artifact. Its summary on the run's page shows the verdict,
each phase and each check.

## How a job is skipped

Every workflow starts with a `gate` job on a GitHub-hosted runner. It reads the secrets and
variables and decides which jobs run. A job without what it needs is skipped, not failed. The
gate leaves a notice saying what is missing, and the run is green.

- **Secrets** can't appear in a job's `if`. The gate tests them (`secrets.X != ''`) and passes
  the answer on as an output.
- **Self-hosted runners** can't be detected: listing them needs an administrator's token, and
  a job whose `runs-on` matches no runner waits in the queue for a day before it fails.
  A repository variable says which runners exist: `REAL_VAGRANT_PROVIDERS` and
  `REAL_PROXMOX_RUNNER`.
- **Forks:** no workflow runs on `pull_request`, so a fork's pull request never starts one. In
  a fork itself the gate is skipped (`github.event.repository.fork`), and a fork has no
  secrets anyway.

You can check this locally. With no secret and no variable, each gate's step writes `any=false`
(or `enabled=false`) and every other job is skipped.

## What the maintainer provides

Secrets and variables are set under Settings, Secrets and variables, Actions (the
"Secrets" and "Variables" tabs).

### Self-hosted runners

Register a runner under Settings, Actions, Runners, New self-hosted runner. GitHub shows the
download and a one-time token. Give the runner its label when you configure it, then run it as
a service:

```sh
./config.sh --url https://github.com/isoloom/isoloom --token <token> \
  --name lab-virtualbox --labels virtualbox --unattended
sudo ./svc.sh install && sudo ./svc.sh start
```

| Runner | Label | Needs on the machine | Then set |
|---|---|---|---|
| VirtualBox | `virtualbox` | VirtualBox, Vagrant, git, curl, jq, a C toolchain (for `cargo`; Rust is installed by the job). Disk and RAM for windows-hello (3 GB for the VM) if you enable it. | variable `REAL_VAGRANT_PROVIDERS` to include `virtualbox` |
| libvirt (Linux) | `libvirt` | libvirt with KVM, Vagrant with the `vagrant-libvirt` plugin, the runner's user in the `libvirt` group, and the same tools | variable `REAL_VAGRANT_PROVIDERS` to include `libvirt` |
| Proxmox | `proxmox` | network access to the cluster's API (port 8006), to the node over SSH, and to the environment's router on the uplink bridge; git, curl, jq, a C toolchain (Terraform is installed by the job) | variable `REAL_PROXMOX_RUNNER` = `true` |

`REAL_VAGRANT_PROVIDERS` is a comma-separated list: `virtualbox`, `libvirt`, or
`virtualbox,libvirt`. `REAL_VAGRANT_WINDOWS` lists the providers where windows-hello also
runs, such as `virtualbox`. Only list a provider there when its runner may download and run the
Windows evaluation box.

A self-hosted runner on a public repository runs whatever a workflow tells it to. These
workflows only start from a schedule or from a maintainer's dispatch, never from a pull
request. Keep it that way:

- In Settings, Actions, General, require approval for all outside collaborators.
- Don't give these runners' labels to a workflow that runs on `pull_request`.
- In an organization, put the runners in a runner group limited to the three `real-*.yml`
  workflows.

### Proxmox

| Name | Kind | What |
|---|---|---|
| `PROXMOX_VE_ENDPOINT` | secret | `https://<node>:8006/` |
| `PROXMOX_VE_API_TOKEN` | secret | `isoloom-ci@pve!ci=<uuid>` |
| `PROXMOX_SSH_PRIVATE_KEY` | secret | The private key Terraform uses to upload cloud-init snippets to the node over SSH |
| `PROXMOX_NODE`, `PROXMOX_DATASTORE`, `PROXMOX_IMAGE_DATASTORE`, `PROXMOX_SNIPPETS_DATASTORE`, `PROXMOX_UPLINK_BRIDGE`, `PROXMOX_SSH_USERNAME`, `PROXMOX_SSH_ADDRESS`, `PROXMOX_INSECURE` | variables, optional | The cluster's layout. An unset one keeps the module's default: node `pve`, `local-lvm`, `local`, `local`, `vmbr0`, `root`, the API's address, `true`. |

Least privilege: a user and a token of its own, with a role that only covers VMs, storage and
the SDN objects the module creates.

```sh
pveum role add IsoloomCI -privs "VM.Allocate VM.Audit VM.Clone VM.Config.CDROM VM.Config.Cloudinit \
  VM.Config.CPU VM.Config.Disk VM.Config.HWType VM.Config.Memory VM.Config.Network VM.Config.Options \
  VM.PowerMgmt Datastore.AllocateSpace Datastore.AllocateTemplate Datastore.Audit \
  SDN.Allocate SDN.Audit SDN.Use Sys.Audit Sys.Modify"
pveum user add isoloom-ci@pve
pveum acl modify / -user isoloom-ci@pve -role IsoloomCI
pveum user token add isoloom-ci@pve ci --privsep 0
```

The snippet upload logs in to the node over SSH, as `root` by default, which is more than the
token can do. Use a lab node that holds nothing else.

### Clouds

Give each cloud an account, subscription, project or compartment that only CI uses.
The reaper deletes every Isoloom resource it finds there, and `--all` after a failed run
deletes them all, whatever their age. A cloud is enabled when all of its secrets are set.

| Cloud | Secrets | Least privilege |
|---|---|---|
| AWS | `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` | An IAM user with the policy below, in an account used only by CI. Variable `REAL_AWS_REGION` (default `eu-west-3`). |
| Azure | `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID` | A service principal with `Contributor` on a subscription used only by CI: `az ad sp create-for-rbac --name isoloom-ci --role Contributor --scopes /subscriptions/<id>`. It has to create resource groups, so the scope is the subscription. |
| Google Cloud | `GCP_CREDENTIALS_JSON` (a service account key), `GCP_PROJECT` | A project used only by CI, with the Compute Engine API enabled. A service account with `roles/compute.instanceAdmin.v1`, `roles/compute.networkAdmin` and `roles/compute.securityAdmin` on that project only. |
| DigitalOcean | `DIGITALOCEAN_TOKEN` | A personal access token with custom scopes: `droplet`, `vpc`, `firewall`, `ssh_key` and `tag` (create, read, update, delete), plus read on `regions`, `sizes` and `image`. Give it an expiry. |
| Linode | `LINODE_TOKEN` | A personal access token with Linodes and Firewalls set to read/write and every other scope set to none. Give it an expiry. |
| Oracle Cloud | `OCI_USER_OCID`, `OCI_TENANCY_OCID`, `OCI_FINGERPRINT`, `OCI_PRIVATE_KEY` (the API key, PEM), `OCI_COMPARTMENT_OCID` | A user in a group `isoloom-ci` whose policies are limited to a CI compartment: `Allow group isoloom-ci to manage instance-family in compartment isoloom-ci`, `... manage virtual-network-family in compartment isoloom-ci`, `... inspect compartments in tenancy`. Variable `REAL_OCI_REGION` (default `eu-paris-1`). |

The AWS policy is limited to one region. It lets the user create what the module creates, and
it only lets the user destroy what carries the `managed-by = isoloom` tag that the module puts
on everything:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "ReadAndCreate",
      "Effect": "Allow",
      "Action": [
        "ec2:Describe*", "ec2:RunInstances", "ec2:CreateTags", "ec2:ImportKeyPair",
        "ec2:CreateVpc", "ec2:ModifyVpcAttribute", "ec2:CreateSubnet", "ec2:ModifySubnetAttribute",
        "ec2:CreateInternetGateway", "ec2:AttachInternetGateway",
        "ec2:CreateRouteTable", "ec2:CreateRoute", "ec2:AssociateRouteTable",
        "ec2:CreateSecurityGroup", "ec2:AuthorizeSecurityGroupIngress",
        "ec2:AuthorizeSecurityGroupEgress", "ec2:RevokeSecurityGroupEgress"
      ],
      "Resource": "*",
      "Condition": { "StringEquals": { "aws:RequestedRegion": "eu-west-3" } }
    },
    {
      "Sid": "DestroyOnlyIsoloom",
      "Effect": "Allow",
      "Action": [
        "ec2:TerminateInstances", "ec2:StopInstances", "ec2:DeleteKeyPair",
        "ec2:DeleteVpc", "ec2:DeleteSubnet", "ec2:DetachInternetGateway", "ec2:DeleteInternetGateway",
        "ec2:DeleteRouteTable", "ec2:DisassociateRouteTable", "ec2:DeleteRoute",
        "ec2:DeleteSecurityGroup", "ec2:RevokeSecurityGroupIngress"
      ],
      "Resource": "*",
      "Condition": {
        "StringEquals": { "aws:RequestedRegion": "eu-west-3", "aws:ResourceTag/managed-by": "isoloom" }
      }
    }
  ]
}
```

A newer AWS provider can call an action that isn't listed. Terraform's error then carries an
encoded message: `aws sts decode-authorization-message --encoded-message <msg>` names the action
to add.

### Optional variables

| Variable | Default | What |
|---|---|---|
| `REAL_CLOUD_MAX_HOURS` | `3` | A cloud environment's lifetime: its `isoloom-expires-at` tag is the start plus this. |
| `REAL_AWS_SIZE`, `REAL_AZURE_SIZE`, `REAL_GCP_SIZE`, `REAL_DIGITALOCEAN_SIZE`, `REAL_LINODE_SIZE` | see the budget | The VM size, when the default isn't offered in your region. |

## Budget

The cloud jobs are capped in four ways.

- **Size:** each cloud's smallest size that runs the example. solo-web is one web server, run
  with Docker on one VM.
- **Time:** the job has a 130-minute timeout, and each step has its own (`run` 35 minutes,
  `test` 10, `down` 20). They add up to less than the job's, so the teardown steps still run
  when a step overruns.
- **Teardown:** `isoloom down` runs in an `always()` step. When it fails, or the job was
  cancelled before it, the reaper deletes every Isoloom resource in that cloud's CI account.
  Each VM also shuts itself down 100 minutes after it boots (`auto_stop_minutes`). AWS then
  terminates it. On the other clouds a stopped VM still costs its disk and, on Azure,
  DigitalOcean and Linode, its compute too.
- **Leftovers:** every resource is tagged `isoloom-expires-at = start + REAL_CLOUD_MAX_HOURS`.
  The reaper runs before every job and on its own every day, so a runner that dies mid-run
  leaves its VM for at most a day and three hours.

| Cloud | Size | Approximate price | One run (at most 65 minutes up) | Worst leak (27 hours) |
|---|---|---|---|---|
| AWS | `t3.small` (2 vCPU, 2 GB) | $0.03/h with its public IPv4 address | < $0.05 | < $1 |
| Azure | `Standard_B2als_v2` (2 vCPU, 4 GB) | $0.05/h with its public IP | < $0.06 | < $1.50 |
| Google Cloud | `e2-small` (2 vCPU shared, 2 GB) | $0.02/h | < $0.03 | < $1 |
| DigitalOcean | `s-1vcpu-2gb` | $0.018/h | < $0.02 | < $0.50 |
| Linode | `g6-standard-1` (1 vCPU, 2 GB) | $0.018/h | < $0.02 | < $0.50 |
| Oracle Cloud | `VM.Standard.E4.Flex`, 1 OCPU, 4 GB (the module's only shape) | $0.04/h | < $0.05 | < $1.10 |

Prices are list prices in the default regions and change over time. Four or five weekly runs
over six clouds come to about $1 a month. Set a budget alert in each cloud's console as a last
line of defence.

The VM jobs cost nothing per run, but each takes its runner for up to 140 minutes. Each matrix runs
one job at a time (`max-parallel: 1`). After a failed `down`, Vagrant destroys what is left;
Proxmox tries `down` once more and, if that fails too, reports an error asking for a check by
hand.

## Reaper

`scripts/real-targets/reap.sh <cloud> [--all] [--dry-run]` stands in for `isoloom reap` (#137)
until that command exists. It uses each cloud's own API or CLI and the marks Isoloom's modules
put on what they create:

| Cloud | Found by | Goes when |
|---|---|---|
| AWS | tag `managed-by = isoloom`: instances, then each VPC with its security groups, subnets, route tables and gateway, then key pairs | `isoloom-expires-at` is past |
| Azure | tag `managed-by = isoloom` on the resource group, which holds everything | `isoloom-expires-at` is past |
| Google Cloud | label `managed-by = isoloom` on instances; name `isoloom-*` on networks (with their firewall rules and subnetworks) | the label's expiry is past; a network older than `REAP_MAX_AGE_HOURS` (6) |
| DigitalOcean | tag `isoloom` on droplets; name `isoloom-*` on firewalls, VPCs and SSH keys | the `isoloom-expires-<time>` tag is past; the others older than 6 hours |
| Linode | tag `isoloom` on instances; label `isoloom-*` on firewalls | the `isoloom-expires-<time>` tag is past; firewalls older than 6 hours |
| Oracle Cloud | name `isoloom-*` in the CI compartment (the modules don't tag): instances (boot volume included), then VCNs with their subnets, route tables, security lists and gateways | older than 6 hours |

`--all` deletes every match, whatever its age. `--dry-run` only lists the matches. A deletion
that fails is reported and doesn't stop the others, and the script then exits 1. To reap by hand
from a reap-only run, start `real-cloud.yml` with "Only reap leftovers".

## Records

Each job uploads `record.json` as the artifact
`record-<target>-<cloud or provider>-<example>` (`record-proxmox-<example>` for Proxmox).
`scripts/real-targets/record.sh` builds it from the JSON events of `run`, `test` and `down`
([json.md](json.md)). This small format is versioned as `isoloom-real-target/1`. The
certification records of #144 will replace it, and this page will say so then.

| Field | What |
|---|---|
| `format` | `isoloom-real-target/1` |
| `target`, `cloud`, `provider`, `example` | What ran: the Isoloom target, the cloud (cloud targets) or the Vagrant provider, and the example. A field that doesn't apply is `null`. |
| `isoloom` | `commit` (the commit built) and `version` (`isoloom --version`) |
| `runner`, `workflow_run` | The runner's name and OS, and the run's URL |
| `versions` | What ran the environment, when present on the runner. Vagrant targets: `vagrant`, `vagrant_plugins`, `virtualbox`, `libvirt`, `qemu`. The others: `terraform`, `terraform_providers` (from the module's lock file, provider to version), `proxmox` (the cluster's version, from its API). |
| `started`, `finished`, `duration_seconds` | From the first event to the last (UTC) |
| `result` | `pass` when `run`, `test` and `down` all succeeded and every check passed, else `fail` |
| `phases` | `run`, `test`, `down`: each `ran`, `ok`, `error` (`code` and `message` of the error document), `started`, `finished`, `duration_seconds` |
| `checks` | Every check `test` reported: `position`, `name`, `ok`, `detail` |
| `passed`, `failed` | The checks' counts |

```json
{
  "format": "isoloom-real-target/1",
  "target": "cloud-docker", "cloud": "aws", "provider": null, "example": "solo-web",
  "isoloom": { "commit": "9d0f22ff…", "version": "isoloom 0.10.1" },
  "runner": { "name": "GitHub Actions 12", "os": "Linux 6.8.0-1021-azure x86_64" },
  "workflow_run": "https://github.com/isoloom/isoloom/actions/runs/123",
  "versions": { "terraform": "Terraform v1.13.4", "terraform_providers": { "registry.terraform.io/hashicorp/aws": "6.17.0" } },
  "started": "2026-10-14T04:31:02Z", "finished": "2026-10-14T04:39:40Z", "duration_seconds": 518,
  "result": "pass",
  "phases": [
    { "phase": "run", "ran": true, "ok": true, "error": null, "started": "2026-10-14T04:31:02Z", "finished": "2026-10-14T04:36:10Z", "duration_seconds": 308 }
  ],
  "checks": [ { "position": "from the environment's networks", "name": "checks/web-answers.sh", "ok": true, "detail": "" } ],
  "passed": 1, "failed": 0
}
```

The workflow's own steps carry the verdict: a failed `run` or `test` fails the job, and
`record.sh` exits 0 whatever the result. A failed real-target job doesn't block anything. It
shows in the run's summary and in its record.
