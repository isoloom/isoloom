# Cloud modules: inputs and outputs

`isoloom generate` writes a Terraform root module for each cloud target. This page describes
their input variables and outputs. Any tool that runs these modules can rely on them: your own
scripts, a CI job, a dashboard. `isoloom run`, `isoloom test` and `isoloom down` use the same
interface.

| Target | Module | One per |
|---|---|---|
| `cloud-docker` | `.isoloom/cloud-docker/<cloud>/main.tf` | cloud: `aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci` |
| `cloud-vm` | `.isoloom/cloud-vm/<cloud>/main.tf` | cloud: `aws`, `azure`, `gcp`, `linode`, `oci` (a cloud that can't express a spec gets no module) |
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
| `inputs.tfvars.json` | Written by `isoloom run`: each variable whose value is exactly `{{ inputs.NAME }}`, taken from the environment variable `NAME`. It is kept for `down` and is mode 0600. |
| `up.sh` | `terraform init` and `apply` on the module, with both variable files. |
| `down.sh` | `terraform destroy`, with the same variables. |
| `outputs.sh` | The module's outputs as JSON (`terraform output -json`). |

The state (`terraform.tfstate`) and the provider plugins (`.terraform/`) stay in that folder,
not in the module. `cloud.outputs` maps names to module outputs. Checks and the message use
them as `{{ cloud.outputs.<name> }}`.
