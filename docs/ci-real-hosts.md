# Real-host certification CI

These workflows run real labs on real hosts and record tier evidence (`#145`). They are what turns a
target from "generated and validated" (Tier 2) into "proven by real runs" (Tier 1). They are
**manual only** — `workflow_dispatch` is the sole trigger, with no push, pull_request or schedule,
matching the manual-only cross-platform decision — and **opt-in**: a job whose runner or credentials
are not configured is skipped, not failed, so nothing ever sits queued for a runner that does not
exist. Nothing here runs until a maintainer provides what this page lists.

Isoloom converts and Isothrum runs. Each job generates the tree with `isoloom generate`, then
executes it with the Isothrum runner (`isothrum up`, `isothrum check`, `isothrum down`, and
`isothrum reap` for the cloud). There is no `isoloom run`.

## The workflows

| Repository | Workflow | Target(s) | Runs on |
|---|---|---|---|
| isoloom | `.github/workflows/real-vagrant.yml` | `vagrant/virtualbox`, `vagrant/libvirt` (local VMs) | self-hosted, per provider label |
| isoloom | `.github/workflows/real-proxmox.yml` | `proxmox` (server mode) | self-hosted `proxmox` |
| isoloom | `.github/workflows/real-cloud.yml` | `cloud-vm/aws` (budget-capped) | GitHub-hosted `ubuntu-latest` |
| isothrum | `.github/workflows/real-runner.yml` | `proxmox` + `cloud-vm/aws` (the runner itself) | self-hosted `proxmox` / `ubuntu-latest` |

The isoloom workflows certify that Isoloom's generated files run, and record into Isoloom's tier
list. The isothrum workflow certifies the runner binary on the targets it uniquely owns (server and
cloud); it has no tier list of its own and uploads its evidence for a maintainer to record into
Isoloom's `tiers.json`.

AWS is the only cloud with real runs for 1.0; the other clouds ship generate+validate (Tier 2) and
get real runs at 1.1, so no Azure/GCP/DigitalOcean/Linode/OCI job is wired.

## Self-hosted runners

Register a self-hosted runner for each real local/server target, labelled so the jobs can find it:

| Label | For | Needs installed |
|---|---|---|
| `virtualbox` | `vagrant/virtualbox` | Vagrant, VirtualBox, Rust (stable), git |
| `libvirt` | `vagrant/libvirt` | Vagrant, libvirt/KVM + `vagrant-libvirt`, Rust, git |
| `proxmox` | `proxmox` server mode | Terraform, SSH client, Rust, git, network reach to the cluster API, its node (SSH) and the lab router |

Then declare which runners exist, so the gate jobs only schedule what can run:

| Variable | Workflow | Meaning |
|---|---|---|
| `REAL_VAGRANT_PROVIDERS` | vagrant | Comma list of providers with a runner, e.g. `virtualbox,libvirt` |
| `REAL_VAGRANT_WINDOWS` | vagrant | Subset of the above whose provider and licence allow Windows guests (adds the `windows-hello` example) |
| `REAL_PROXMOX_RUNNER` | proxmox | `true` when a `proxmox` runner is registered |

The AWS job runs on GitHub-hosted `ubuntu-latest`; it needs no self-hosted runner.

## Cloud credentials, OIDC and the budget cap

Give AWS credentials in **one** of two ways:

- **Static keys** — repository secrets `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`.
- **OIDC (preferred)** — set the repository variable `REAL_AWS_ROLE_ARN` to a role the repository's
  GitHub OIDC token may assume. The workflow already requests `id-token: write` and assumes the role
  with short-lived credentials; no long-lived key is stored. Scope the role to the CI account and
  region only.

The budget guard caps spend and guarantees teardown:

| Variable | Default | Meaning |
|---|---|---|
| `REAL_AWS_REGION` | `eu-west-3` | Region the lab runs in |
| `REAL_AWS_SIZE` | `t3.small` | Smallest instance that runs the example |
| `REAL_CLOUD_MAX_HOURS` | `3` | Expiry tag lifetime; also the runner lifetime passed to `isothrum up --lifetime` |

On top of the cap, every instance is tagged to expire, told to stop itself after 100 minutes, and
opened for SSH only to the runner's own IP with a key made for that run. `down` always runs; if it
fails or is cancelled, `isothrum reap aws --region <r>` destroys the account's expired runs and
fails loudly if anything untracked is left. The job can also be dispatched with `reap_only: true` to
sweep leftovers without bringing anything up. The best cost control is a hard AWS Budgets action on
the CI account, independent of this workflow.

## Host credentials model

Secrets are repository secrets; nothing is written into the checkout.

**Proxmox** (both repos' proxmox jobs):

| Secret | Meaning |
|---|---|
| `PROXMOX_VE_ENDPOINT` | Cluster API URL |
| `PROXMOX_VE_API_TOKEN` | API token (`user@realm!tokenid=uuid`) |
| `PROXMOX_SSH_PRIVATE_KEY` | Private key for the node, for uploading cloud-init snippets over SSH |

Cluster layout comes from repository variables, each optional (an unset one keeps the module's
default — node `pve`, `local-lvm`, `local`, `vmbr0`): `PROXMOX_NODE`, `PROXMOX_DATASTORE`,
`PROXMOX_IMAGE_DATASTORE`, `PROXMOX_SNIPPETS_DATASTORE`, `PROXMOX_UPLINK_BRIDGE`,
`PROXMOX_SSH_USERNAME`, `PROXMOX_SSH_ADDRESS`, `PROXMOX_INSECURE`.

Each run also makes a throwaway SSH keypair for the lab's VMs and deletes both keys in an `always()`
step. Guest SSH from the runner (for checks) uses that per-run key.

## Where the tier records come from and go

`tiers.json` and `docs/matrix.md` live in **isoloom** (`crates/targets/isoloom-targets/tiers.json`).
They are evidence, not product config: a run records what it proved.

In the isoloom workflows, the **Record tier evidence** step, after the cycle, runs:

```
cargo xtask tiers record <entry> <host> <pass|partial|fail> [--server <name>] [--os <os>] --note "<what ran>"
```

`cargo xtask tiers record` writes both `tiers.json` and the regenerated `docs/matrix.md`.
`pass` only when up, checks and down all passed; otherwise `partial` (with a note) or `fail`. The
step then uploads the updated `tiers.json` and `docs/matrix.md` as the job's artifact and writes a
summary. Because the workflow has read-only `contents` permission and never pushes, **the maintainer
downloads the artifact and commits both files** (always together). These variables name the host as
the tier list knows it:

| Variable | Meaning |
|---|---|
| `REAL_TIERS_HOST` | The host name in `tiers.json`'s `hosts` (`mac`, `jarvis`, `karen`, `friday`, …). Unset means the run is not recorded, only logged. |
| `REAL_TIERS_OS` | The host's OS (`linux`, `macos`, `windows`), used with `--os` to add a host the list does not name yet |
| `REAL_PROXMOX_SERVER` | The server name recorded for the proxmox run (default `pve-lab`) |

The **isothrum** runner-cert workflow has no tier list; it uploads its up/check/down/reap records as
an artifact and prints the exact `cargo xtask tiers record …` line to run in the isoloom repo. A
maintainer records it there.

## Cross-repository build pins

Each workflow builds the sibling binary from source:

| Variable | In | Meaning |
|---|---|---|
| `ISOTHRUM_REF` | isoloom workflows | Git ref of `isoloom/isothrum` to build the `isothrum` runner from (default `main`) |
| `ISOLOOM_REF` | isothrum workflow | Git ref of `isoloom/isoloom` to build the `isoloom` generator from (default `main`) |

Pin these to a released tag for a reproducible certification run.
