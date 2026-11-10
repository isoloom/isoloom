# The run manifest

Isoloom converts a spec into files, and alongside the files of most targets it writes a **run
manifest** (`run.json`, under the target's folder): a versioned, self-contained description of how
to bring the environment up, check it and tear it down. Isoloom runs nothing itself. The run
manifest is the contract between Isoloom and a runner.

Which targets have one, and where:

| Target | Run manifest |
|---|---|
| `docker` | `.isoloom/docker/run.json` |
| `vagrant` | `.isoloom/vagrant/run.json` (one for every provider) |
| `docker-vm` | `.isoloom/docker-vm/run.json` (the VM made by Vagrant), and `.isoloom/docker-vm/proxmox/run.json` (made on a Proxmox server) unless a container mounts a folder of the computer that runs the lab |
| `proxmox` | `.isoloom/proxmox/run.json` |
| `cloud-vm` | `.isoloom/cloud-vm/<cloud>/run.json`, one per cloud that takes the lab |
| `cloud-docker` | `.isoloom/cloud-docker/<cloud>/run.json`, one per cloud that takes the lab |
| `cloud-services` | `.isoloom/cloud-services/run.json`, when the spec has `cloud:`, unless it has `before` steps (host scripts) or a variable holding a launch value inside a larger value |
| `kubernetes`, `hybrid`, `esxi`, `external`, `hosted` | none yet: their files are run by hand |

A runner executes it: any program that reads the manifest and follows this contract.
[Isothrum](https://github.com/Isothrum/isothrum) is the reference runner. You can
also run each tool's files by hand (`docker compose up`, `kubectl apply -k`, `vagrant up`, `terraform apply`,
`ansible-playbook`); the manifest exists so a runner doesn't have to know each tool's commands.

## What the manifest holds

A run manifest holds only documented data — the driver does the work:

- the **native commands** of the lifecycle and their order;
- **readiness criteria** (a ready file, running machines, a port answering);
- **outputs**: addresses, host ports and URLs; a published port's is named
  `<machine>__<port>` in snake case (`web__portal`), and no two outputs share a name;
- how to run the **checks**, with self-contained timeouts (waits + 15 s per check + 60 s), so a
  long-waiting lab is never cut off by a fixed cap;
- **secret slots**: each secret's and input's name and destination, and an optional `default`,
  **never a value**.

The manifest is strict (`additionalProperties: false`) and carries its own version. Its JSON
schema is committed at [`schemas/run-manifest-v1.schema.json`](../../schemas/run-manifest-v1.schema.json),
and `isoloom-manifest` holds the types and fixtures runners can test against.

## Versions and compatibility

`manifest_version` is the contract's version: **1** from Isoloom 2.0. Within version 1, a minor
release of Isoloom may add an optional field. Removing or renaming a field, changing what a field
holds or means, making a field required, or adding a value a runner must act on (a step action, a
delivery, a condition) raises `manifest_version`, in a major release of Isoloom.

The reader (`isoloom-manifest`) is strict: it refuses a field or value it doesn't know, as the
schema does (`additionalProperties: false`). A manifest that uses a field added in a minor release
needs a runner built with that release's `isoloom-manifest`, so a runner pins the
`isoloom-manifest` it reads and upgrades it with the Isoloom it runs. The policy is
`isoloom_manifest::COMPATIBILITY`, which the schema's description repeats.

A runner reads the versions it was built for and refuses any other before running anything.
Isothrum's release notes say which `manifest_version` each of its releases reads. Isoloom 1.x
wrote `manifest_version: 0`, an unstable contract read only by runners built for exactly that
release line.

## The lifecycle

The manifest's `lifecycle` holds each operation as ordered steps; a runner runs an operation's
steps in order, each under its own policy (time limit, retries, whether a failure fails the
operation):

| Operation | What a runner does |
|---|---|
| `up` | Checks the manifest's `tools` first (it fails before creating anything when one is missing or too old), runs `lifecycle.up`, delivers the secrets and inputs, waits for the readiness criteria, then runs `lifecycle.seal` when there is one |
| `check` | Runs the manifest's `checks` (below) and reports each check |
| `status` | Runs `lifecycle.status`, a read-only probe, and reads each machine's state from it |
| `park` | Runs `lifecycle.park.shutdown` (machines powered off) or `lifecycle.park.pause` (frozen, memory kept), whichever the target has and the runner chooses; the run's state is kept |
| `resume` | Runs `lifecycle.resume`: the parked machines come back as they were, no container re-created and no VM set up again, so no input is needed again |
| `provision` | Runs `lifecycle.provision.all`, or `lifecycle.provision.machine` with `${machine}` bound to one machine: the set-up again on running VMs |
| `down` | Runs `lifecycle.down`, which removes everything the run created |

A runner keeps each run's state (its record, the values it set, the host ports it pinned) outside
the lab folder. A secret or input value reaches only the steps its delivery names, and a runner
never writes a sensitive value to a log or an error.

### What a VM takes only when it boots

Vagrant applies some of a VM's settings only when it boots it (`vagrant up` of a stopped VM,
`vagrant reload`): its synced folders (`host_folders`, and `docker-vm`'s folders shared into its
VM), networks, forwarded ports, size and provider settings. `vagrant up --provision` on a running
VM runs its provisioners and nothing else. So each Vagrant machine's `native` carries a
`boot_digest`: a digest of its Vagrantfile configuration other than its provisioners (the
statements its VMs share included). It is opaque and only compared: a runner keeps the digest
each VM was booted with and, when it brings a running run up to date with a regenerated manifest,
reloads a running VM whose digest changed before it runs `up`'s steps again. A manifest written
before the field existed has none.

## The checks

`check` runs the manifest's `checks` in three phases, and a runner keeps to them:

1. **`checks.setup`**, in order: what the check runners need first, such as the stand-in for an
   access machine the lab brings up on demand (`stand-ins`: the checks that run from its seat run
   from the stand-in's addresses). A failed set-up step fails the checks: no runner runs, but the
   teardown still does.
2. **`checks.runners`**, in order: each runner's step, under its own time limit, prints its
   results in the `checks.protocol` line format. A runner cut off by its limit runs its
   `on_timeout` steps (the one-off container it leaves behind removed); the next runner still
   runs. A check its runner never reported fails.
3. **`checks.teardown`**, after the runners **whatever happened** — every check passed, one
   failed, a runner timed out, or the set-up failed. It takes the stand-ins down
   (`stand-ins-down`), so the access machine they stand in for can take its addresses when it is
   brought up. A teardown step is a clean-up: Isoloom marks it `failure: tolerate`, so its
   failure is logged and fails no check (a step's `failure` is `fail` unless it says so).

Compose in a VM (`docker-vm`, `cloud-docker`) has the same steps, each run in the VM through the
manifest's way in. The docker-vm Vagrantfile's `checks` provisioner, for running the checks without
a runner, does the same by itself: the stand-ins up, the runners, then the stand-ins down whatever
the runners found.

## Secrets and inputs

The spec names secrets and inputs and where each reaches a machine (usually a file); the manifest
turns each into a **slot** — a name and a destination, with an optional `default`, but no value.
At run time the runner reads each value (the embedder's, else from the slot's `source`: the
runner's environment, a file, HashiCorp Vault or a SOPS-encrypted file) and delivers it as the
slot says: an environment variable of some steps, a Terraform variable, a file in the run's state
folder, or a file written into the machine over SSH (guest delivery). Values never land in a generated file, the
run's state, an event or a log.

## The egress seal

For a cloud target, an `internet: false` network is sealed at the network level **after** the
machines are set up: the manifest carries an optional `seal` step that the runner applies once the
environment is ready (a second Terraform apply with the module's seal variable on). Egress to the
internet is then cut for those networks, while the runner's SSH and the published ports still
answer. The seal fails closed — a failed seal means the run is not ready — and every later
operation replays it, so a later apply can't silently undo it.

## Running it

`isoloom generate` writes each target's files under `.isoloom/<target>/`, with its run
manifest where the target has one (see the table above); a runner then reads the `run.json` of
the target it runs. Its own commands, options and messages are its own, documented
with it.

## See also

- [Spec reference](../spec.md) — secrets, inputs, `files:` and checks.
- [Targets and tiers](targets.md) — each target's run manifest.
- [Getting started](../guide/getting-started.md).
- [Policies](policies.md) — what is stable, and how each surface is versioned.
