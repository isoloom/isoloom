# Architecture

Isoloom reads one spec (`isoloom.yml`) and writes the files each target runs: a Compose file,
Kubernetes manifests and a Helm chart, Vagrantfiles, Terraform modules, an Ansible inventory.
This page is the map: how a spec becomes files, which crate holds what, and how to extend it.
The decisions behind it are in [docs/adr/](docs/adr/).

## Pipeline

```
isoloom.yml -> spec -> plan -> render -> backends -> .isoloom/<target>/...
```

1. **Spec** (`isoloom-spec`): parse the YAML, then normalize it. `count:` clones, `common:` and
   `groups:` fold into the machines, VLANs written under their LAN become networks, profiles
   and `-s key=value` overrides merge in, an instance number shifts names and addresses, and the
   defaults and image table resolve. `validate` returns every `Problem` with its field path
   (`machines.web.depends_on`) and a reason. Generators assume a validated spec.
2. **Plan** (`isoloom-plan`): what every target must produce, independent of any target: the
   `Environment` (networks, machines with their addresses, routes, volumes and set-up steps,
   `lifecycle::Step`), the checks resolved to positions and probes, and the `requirements` a
   backend must meet to render it.
3. **Render** (`isoloom-render`): the pieces every backend shares: addresses and names, the
   generated files' header, the controller's Ansible runs, HCL, Ruby and PowerShell quoting,
   the project copy, the hosts' setup, each set-up step's Linux and Windows script (`steps`),
   the router, VLAN trunks, image constraints and `.isoloomignore`; `boot` keeps a step's
   effect across restarts (a boot-time service). `backend` defines the
   `Backend` trait: a target, its capabilities, and `render(&Environment)`.
4. **Backends** (`isoloom-compose`, `-kubernetes`, `-vagrant`, `-terraform`, `-external`):
   each turns the plan (Compose, through typed structs in `file`; Kubernetes, through typed objects in `objects`; external) or, until it migrates, the
   spec into its target's files. Every generator is a pure function of the
   spec: no file system access, so `isoloom check` can regenerate in memory and compare with
   what is committed.
5. **Targets** (`isoloom-targets`): the registry of backends (`BACKENDS`; generators not on
   the plan yet run through `LegacyBackend`), the refusals (requirements minus capabilities), `generate_all`, and the resolved snapshot
   (`resolved.json`) written beside every target's files.

### Where it is heading

Wave 1 unit 6 introduced the typed plan IR ([ADR 0002](docs/adr/0002-plan-ir.md)): an
`Environment` (networks, machines, start order, checks, controller, constraints,
requirements), a `MachineLifecycle` made of `Step`s with one Linux and one Windows rendering
each, and a `Backend` trait (`capabilities()` plus `render(&Environment)`). Refusal then becomes
"requirements minus capabilities" instead of each generator deciding on its own. Until every
backend moves to the plan (Wave 2), `LegacyBackend` keeps the other generators running
unchanged; Compose, Kubernetes, external, docker-vm, hybrid and Vagrant are on the plan, the Vagrant ones
with the one Ruby builder (`isoloom_vagrant::ruby`) every Vagrantfile is meant to be written with;
Vagrant's `transport` runs each planned step as a Vagrant provisioner.

### The Ansible inventory

Every target that runs playbooks writes the same inventory, rendered once by
`isoloom_render::inventory::render(env, &access)` from the plan: the hosts (VMs at their first
address; on `external`, the existing hosts at their `external` address), the `linux` and
`windows` groups, each host's `host_vars` (typed as Python literals), `ansible_become` on Linux,
then the inventory groups (`Environment::inventory_groups`: the `provision:` steps' `groups:`,
then the spec's `groups:`, members limited to the hosts). What differs per target is how a host
is reached, an `Access`: `Access::vagrant()`, `Access::proxmox()`, `Access::cloud(user_of_os,
windows_password)`, `Access::external()`. A backend never writes inventory lines itself.

### The Terraform VM core

Proxmox, and AWS, Azure and Google Cloud on the `cloud-vm` target, render from the plan through
`isoloom_terraform::vm`, written with the HCL builder (`isoloom_render::hcl_builder`: `hcl-rs`
structures, values escaped by the builder, written in the layout `terraform fmt` keeps). The core
holds what every VM-per-machine Terraform target shares: a machine's set-up as a `remote-exec`
transport (connection, project archive, inputs file, the plan's `Step`s run as root, a reboot
between two provisioners) or a cloud-init transport (the `Prelude` step as `bootcmd`, the others
as `runcmd`, a unit for what follows a reboot), Windows over WinRM, the controller and its
inventory, the check runners and the `checks` output, and each target's limits (refusals read
from the plan). A target adds its resources and hooks (interfaces found by MAC, the auto-stop,
published-port redirects) and leaves out the steps its platform does itself; `vm::cloud` holds
what every cloud module repeats (the operator's variables, the auto-stop, interfaces found by
MAC, the Windows password, the outputs). On `cloud-vm`, every cloud (AWS, Azure, Google Cloud,
DigitalOcean, Linode, Oracle Cloud) is a `Backend` of its own (`cloud_vm::CLOUDS`, AWS first):
AWS's refusal refuses the target, another cloud's refusal drops only its module, and
`cloud_vm::cloud_refusals` says which cloud refuses and why.

## Crate map

| Crate | Holds |
|---|---|
| `isoloom-spec` | The model, parsing and loading, the JSON schema, normalization (groups, clones, VLANs, profiles, instances, defaults, the image table), validation, shell quoting |
| `isoloom-plan` | The target-neutral plan: `Environment`, addressing, routing, images, the lifecycle steps, checks, requirements and refusal |
| `isoloom-render` | The `Backend` trait and shared rendering pieces: addresses, header, Ansible runs, the Ansible inventory, the HCL builder (`hcl_builder`), HCL, Ruby and PowerShell quoting, step scripts, project copy, host setup, router, trunks, constraints, `.isoloomignore` |
| `isoloom-compose` | The Docker Compose backend (`docker`, `hosted`, and the Compose file Docker-on-a-VM targets carry), with the network appliances |
| `isoloom-kubernetes` | The Kubernetes backend: manifests, Kustomize, the Helm chart |
| `isoloom-vagrant` | The Vagrant backends: `vagrant` (every provider), `docker-vm`, `hybrid`; the Ruby builder (`ruby`) and the Vagrantfile pieces they share (`vagrantfile`) |
| `isoloom-terraform` | The Terraform backends: Proxmox, one cloud VM per machine (AWS, Azure, GCP, DigitalOcean, Linode, OCI), Docker on one cloud VM, cloud services; the Terraform VM core (`vm`) the VM targets share |
| `isoloom-external` | The `external` backend: hosts that already exist |
| `isoloom-targets` | Dispatch to every backend, refusals, `generate_all`, the resolved snapshot |
| `isoloom-import` | Drafting a spec from Compose, Vagrant and Terraform files, with notes for what the draft leaves out |
| `isoloom-coverage` | The coverage map: what each output format can do and what Isoloom produces |
| `isoloom-runtime` | Runtime state: the registry of running environments (`~/.isoloom/status.yml`), host readiness, the environment report; the lifecycle's JSON events and errors (`events`, with an `Emitter` for embedders) and a tool run with its output as events (`tool`): see [docs/json.md](docs/json.md) |
| `isoloom-core` | The facade other tools embed: re-exports the supported surface of the crates above |
| `isoloom` | The command line: clap definitions in `cli.rs`, one module per subcommand in `commands/`, tool runners in `tools/` (every tool started by `tools::process::command`), `--json` output in `output.rs` and the error codes in `failure.rs` |

Dependencies point down the pipeline: `spec` depends on nothing of ours, `plan` on `spec`,
`render` on both, backends on `render`, `targets` on the backends. Only `isoloom-core` and the
CLI see everything.

## How to add a feature

1. **Spec:** add the field to `isoloom-spec/src/model.rs` with a doc comment (it becomes the
   schema description). Default it so existing specs still parse.
2. **Validate:** reject what can't work in the section's validator under
   `isoloom-spec/src/validate/` (`networks.rs`, `machines.rs`, `checks.rs`...), with the field
   path and a reason that says how to fix it. `validate` runs the sections in a fixed order:
   the order of the problems is part of the output.
3. **Plan:** when the feature is target-neutral (a check, a lifecycle step), put it in the
   plan, not in a backend.
4. **Each backend:** produce it, or refuse the spec for that target with a reason (see the
   refusals in `isoloom-targets`). A target that silently drops the field is a bug.
5. **Coverage:** mark the rows the feature fills in each format's table (`Support::Emitted`
   with the spec field it comes from) and the spec-side row in `isoloom-coverage/src/spec.rs`.
6. **Docs:** regenerate `docs/COVERAGE.md` (`cargo run -q -- coverage --markdown > docs/COVERAGE.md`).
7. **Example:** use the feature in an example and regenerate its `.isoloom/` folder. The
   coverage tests require every claimed row to be exercised by an example.

## How to add a backend

1. Create `crates/isoloom-<name>` (copy a small one such as `isoloom-external` for the
   layout), inherit the workspace lints, and depend on `isoloom-spec`, `isoloom-plan` and
   `isoloom-render`. Reuse the render helpers: no copied quoting or address code.
2. Add the `Target` variant in `isoloom-spec` (model and `targets.rs`, which derives the
   targets a spec can run on).
3. Wire it in `isoloom-targets`: `target_files_raw`, the refusals, and the resolved snapshot.
4. Add the runtime side in the CLI: a tool runner in `crates/isoloom/src/tools/` and the
   lifecycle commands (`run`, `test`, `down`).
5. Add a coverage table for its format (below), an example with committed output, and a CI
   job that runs the output for real when a runner can host it.

## How to add a coverage table

A table is a `Format` (`isoloom-coverage/src/lib.rs`): a name, the generated file it
describes, the source of its feature list, and rows of `(feature, Support)`. `Support` says
what Isoloom does with the feature: `Emitted`, `Partial`, `Equivalent`, `InImage`, `Planned`,
`NotPortable`, `Tooling`. `Unclassified` fails the tests.

1. Pin the format's machine-readable definition under `crates/isoloom-coverage/coverage/`
   (the Compose schema JSON, Vagrant's config classes as `.txt`, Terraform provider resource
   lists) and note where it came from. `scripts/terraform-resources.sh` and
   `scripts/vagrant-settings.py` refresh the existing ones.
2. Write the module (see `compose.rs`, `vagrant.rs`, `terraform.rs`): a `format()` returning
   the `Format`, with every feature of the definition classified.
3. Register it in `formats()`.
4. Extend `crates/isoloom-core/tests/coverage.rs`: the rows are exactly the definition's
   features, and the features marked as written match what the generators write for the
   committed examples.
5. Regenerate `docs/COVERAGE.md`. [ADR 0005](docs/adr/0005-coverage-methodology.md) has the rules.

## Quality gates and lints

The gates every change passes are listed in [CONTRIBUTING.md](CONTRIBUTING.md). Lints are set
once in the root `Cargo.toml` (`[workspace.lints]`) and inherited by every crate with
`[lints] workspace = true`; `clippy.toml` holds the shape limits.

Enforced now (`deny`, so `cargo clippy --workspace --all-targets -- -D warnings` fails on them):

- rust: `unsafe_code` is `forbid`, with no exception (a profile's inputs reach every tool through
  `Command::envs`, from `tools::process::command` in the command line); rustdoc:
  `broken_intra_doc_links`, `bare_urls`.
- clippy: `pedantic`, except the commented allow-list in `Cargo.toml` (`must_use_candidate`,
  `missing_errors_doc`, `missing_panics_doc`, `doc_markdown`, `similar_names`); `unwrap_used`,
  `panic`, `dbg_macro`, `todo` (tests may unwrap, expect and panic: `clippy.toml`);
  `print_stdout` (the command line allows it at its root: printing is its interface);
  `too_many_arguments` above 5 (two functions carry a targeted, commented `#[allow]`:
  `appliance` and `readiness` in isoloom-compose's `render.rs`).

Tracked (`allow` today, denied by the end of Wave 2, issue #106): `missing_docs` for library
crates; `expect_used`; `too_many_lines` (60) and `cognitive_complexity` (15), whose thresholds
are already in `clippy.toml`; and the pedantic lints still hit by code moving onto the plan IR
(`case_sensitive_file_extension_comparisons`, `cast_possible_truncation`, `format_collect`,
`format_push_string`, `map_unwrap_or`, `match_wildcard_for_single_variants`,
`stable_sort_primitive`).

### Tests

Unit tests live beside the code; cross-crate behavior is tested in `crates/isoloom-core/tests`
and the CLI in `crates/isoloom/tests`. Snapshots use `insta`
(`crates/isoloom-spec/tests/snapshots`): a change to output shows up as a reviewed diff
(`cargo insta review`). `proptest` suites cover CIDR parsing and addressing, and the quoting
helpers for shell, HCL, Ruby and PowerShell (round trip and injection safety). A determinism
test renders every example twice in one process and compares the bytes. CI runs
`cargo llvm-cov` and holds each crate at or above its floor in `.github/coverage-floors.txt`,
checked by `scripts/coverage-floors.sh`; floors only go up.
