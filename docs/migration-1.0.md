# Upgrading from 0.10 to 1.0

Isoloom 1.0 follows 0.10.1. A spec that worked on 0.10.1 still loads and validates on 1.0: no
field was removed or renamed, and every example spec shipped with 0.10.1 passes
`isoloom validate` on 1.0 unchanged (three of them ship without their scripts, and report only
those missing files, as on 0.10.1). What changes is what some targets do with a spec, a few defaults, the JSON
the command line prints, and, for code that embeds Isoloom, the crates to depend on.

The [changelog](../CHANGELOG.md) lists everything new. This page lists only what an upgrade
can trip over.

## The spec

- **Nested virtualization is its own field.** A machine whose VM runs VMs of its own sets
  `nested_virtualization: true`. `privileged` doesn't turn it on, on any target or provider:
  `privileged` asks for kernel-level access, which a container gets by running privileged and
  a VM already has. `nested_virtualization` needs a `vm:` (`validate` says so), and a target
  whose VMs can't nest refuses the environment with its reason.
- **Volume names are at most 58 characters.** On Kubernetes a volume is seeded from the image by
  an init container named `seed-<volume>`, a 63-character label. `validate` reports a longer
  name. The older rule stays: the machine's name and the volume's together are at most 62.
- **Oracle Cloud runs Debian and Ubuntu only.** On 0.10.1 an Oracle Cloud `cloud-vm` module ran
  Ubuntu whatever the machine's `os`. On 1.0 it runs `debian-12`, `debian-13`, `ubuntu-22.04`
  and `ubuntu-24.04` (still on Ubuntu, since Oracle Cloud has no Debian image of its own), and
  refuses any other OS. A machine that names its own image with `vm.image.oci` still runs.
- **`http` checks compare the first answer.** Without `follow: true`, every check runner now
  compares the first answer's status. On 0.10.1 a runner with only wget (the busybox check
  image) compared the last one after the redirects, so a check that passed there only past a
  redirect now reports `got 301`. Add `follow: true` to follow redirects (up to 5) on every
  runner.

## Defaults

- **Secret files on Linux VMs are mode 0400.** Secrets are new in 1.0
  ([Secrets](secrets.md)). A secret file without a `mode` is `0444` in a container (Docker,
  Kubernetes) and `0400` on a Linux VM, owned by root or the secret's `uid`, since a VM has other
  users. Set `mode` on the machine's secret to choose another.
- **The controller is smaller.** The VM that runs the `provision:` playbooks defaults to 1 CPU
  and 512 MB of memory on every target. On local VMs its box is `generic/alpine319`; on AWS,
  Azure and Google Cloud its size is `t3.micro`, `Standard_B1s` and `e2-small`. On 0.10.1 the
  AWS controller was a `t3.small`. `controller: { resources: { cpus, memory_mb } }` sizes it,
  and `controller: { image: ... }` names a box of your own.
- **The controller halts after provisioning on local VMs.** On Vagrant, docker-vm and hybrid,
  `isoloom test` boots it for the checks and halts it again. `controller: { keep_running: true }`
  keeps it up, as a jump host.
- **Waits for a dependency allow 900 seconds** on every VM target (Vagrant waited 300 seconds,
  Proxmox 600).
- **Provider versions are pinned.** The generated Terraform modules require each provider at
  the minor version the coverage tables were measured against (`~> 6.67.0` for AWS, `~> 5.8.0`
  for azurerm, `~> 8.5.0` for google, `~> 0.115.0` for `bpg/proxmox`...), where 0.10.1 allowed a
  whole major version (`~> 6.0`, `~> 2.0`). The list is `isoloom_terraform::vm::versions`.

## The resolved snapshot

`.isoloom/resolved.json` keeps `resolved_version: 1`. Every key 0.10.1 wrote is still there
with the same meaning; 1.0 only adds keys, so a reader that ignores unknown keys keeps working.

- `targets.clouds`: for a generated `cloud-vm`, the clouds whose module is written
  (`generated`) and each refused cloud with its reason (`refused`).
- `targets.generated` and `targets.not_possible` can name the new `esxi` target, and
  `cloud-vm` is generated when any cloud accepts the environment (it used to follow AWS).
- Each machine has a `role` (`target`), and the controller `role: infra` with `keep_running`.
- A network can have `ipv6`, a machine `ipv6_addresses`.
- A machine with several addresses on a network (`lan: [10, 11]`) keeps its first in
  `addresses` and lists the others in `more_addresses`, by network.
- A service can carry `protocol: udp` and `dormant: true`, a published port its `protocol`, an
  `http` check `follow: true`, and a machine's volume `shared`.

No secret value is ever written to the snapshot.

## cloud-vm and `--cloud`

On 0.10.1 a command on `cloud-vm` without `--cloud` used the AWS module. On 1.0 each cloud's
module is written on its own: a cloud that refuses the environment drops only its module, and
`isoloom targets` lists the refused clouds under `cloud-vm` with their reasons.

`run`, `down`, `reset` and `test` on `cloud-vm` pick the cloud in this order, and `connect`,
`exec` and `capture` the same from step 2:

1. the one named by `--cloud`;
2. else the one `run` recorded for this environment (in `~/.isoloom/status.yml`);
3. else AWS, when AWS can run the environment;
4. else the only cloud that can.

When AWS can't and several clouds can, the command stops and names them
(`--cloud gcp, --cloud digitalocean`). `run --cloud <cloud>` with a cloud that refuses the
environment stops with that cloud's reason. The cloud picked is in the `result` event's `cloud`.
[ADR 0007](adr/0007-one-cloud-vm-target-many-clouds.md) says why `cloud-vm` stays one target.
`cloud-docker` still needs `--cloud`, as on 0.10.1.

## Google Cloud environments

An environment already applied on Google Cloud with 0.10's `cloud-vm` module can't be updated in
place by 1.0's. 0.10.1 wrote one VPC network for the whole environment
(`google_compute_network.env`) and a firewall rule per machine (`<machine>_net`); 1.0 writes a
VPC network per spec network (`google_compute_network.<network>`) and a rule per machine and
network (`<machine>_net_<network>`), so that a machine can sit on several networks and a
`gateway:` machine can route between them. An instance can't move its interface to another VPC
network, so applying 1.0's module over 0.10's state would try to replace the network under
running instances.

Destroy the environment with the old module first (`isoloom down cloud-vm --cloud gcp` with
0.10.1, or `terraform -chdir=.isoloom/cloud-vm/gcp destroy` before regenerating), then generate
and run it with 1.0. To keep an environment, move what keeps its shape with `terraform state mv`;
a machine on several networks is rebuilt either way.
[Upgrading an existing Google Cloud environment](cloud-modules.md#upgrading-an-existing-google-cloud-environment)
has the details.

## The command line

New commands and options:

- `isoloom provision [MACHINE...] [--target T]` runs the provisioning again on a running local
  VM environment (vagrant, docker-vm, hybrid).
- `isoloom profiles` lists and checks the project's profiles, and `--profile <name>` (or
  `ISOLOOM_PROFILE`) runs any command with `profiles/<name>.yml`.
- `isoloom chart [DIR] --version X.Y.Z` packages the Helm chart with `helm package`.
- `isoloom docs --spec` prints the spec reference, [docs/spec.md](spec.md).
- `isoloom schema --events` prints the JSON Schema of the `--json` events and errors.
- `isoloom down --volumes` also removes the volumes kept past `down` (`persistent: true`).

**`--json` changed.** `run`, `down`, `reset`, `provision`, `test` and `capture` print
newline-delimited events with `--json`, the last a `result` event; `status`, `exec`, `tc`, `gc`
and `connect` print one document. A failed command's last line is `{ "error": { ... } }` with a
stable `code`. [JSON output](json.md) describes them all.

- **`test --json` is not the 0.10.1 list.** It used to print one pretty JSON document of
  the results. It now prints a `check` event per result and the summary in the
  `result` event's `data`. A tool reading the old array reads the `check` lines instead.
- Without `--json`, every command prints what it printed before. The exit codes are unchanged:
  0 for success, 1 when the outcome is negative (a check failed), 2 for an error.

`~/.isoloom/status.yml` now starts with `version: 1`. A file written by 0.10.1 has none and
reads as version 1, so nothing needs migrating ([the registry](registry.md)).

## The library: `isoloom-core` is gone

The `isoloom-core` crate of 0.10.1 is split into a crate per layer and backend, and the facade
is removed. Depend on the crates that hold what you use, pinned to the release tag:

```toml
isoloom-spec = { git = "https://github.com/isoloom/isoloom", tag = "v1.0.0", version = "1" }
isoloom-plan = { git = "https://github.com/isoloom/isoloom", tag = "v1.0.0", version = "1" }
isoloom-targets = { git = "https://github.com/isoloom/isoloom", tag = "v1.0.0", version = "1" }
isoloom-runtime = { git = "https://github.com/isoloom/isoloom", tag = "v1.0.0", version = "1" }
```

Each item keeps its name; only the crate changes.

| 0.10.1 (`isoloom_core::`) | 1.0 |
|---|---|
| `parse`, `find`, `load`, `load_with`, `totals`, `validate`, `validate_files`, `derive`, `effective`, `Problem`, `LoadError`, the model types | `isoloom_spec::` |
| `model`, `validate`, `targets`, `instance`, `defaults`, `clones`, `groups`, `vlans`, `images`, `schema` | `isoloom_spec::` (same module) |
| `checks` | `isoloom_plan::checks` |
| `generate`, `generate_all`, `generate_instance`, `refusal`, `qemu_refusal`, `GeneratedFile`, `GenerateError`, `OUTPUT_DIR`, `resolved` | `isoloom_targets::` |
| `registry`, `host`, `report` | `isoloom_runtime::` (same module) |
| `coverage` | `isoloom_coverage` |
| `import` | `isoloom_import` |

New in 1.0: `isoloom_spec::load_profiled` and `profile`, `isoloom_targets::refusal_on` and
`cloud_refusals`, and `isoloom_runtime::events` and `tool`. `qemu_refusal` is deprecated: use
`refusal_on(spec, Target::Vagrant, Some("qemu"))`.

Errors are typed. `LoadError::Parse(String)` is replaced by `Yaml`, `Expand { stage, reason }`
and `Profile { problems }`, and `Read(path, error)` is `Read { path, source }`; the messages are
the same. The supported surface, and what counts as internal, is in the README's
[Using the library](../README.md#using-the-library).
