<p align="center">
  <a href="https://www.isoloom.com"><img src=".github/isoloom-mark.svg" width="76" alt="Isoloom"></a>
</p>

<h1 align="center">Isoloom</h1>

<p align="center">
  <strong>One spec. Every environment.</strong><br>
  <a href="https://www.isoloom.com">Website</a> · <a href="docs/getting-started.md">Getting started</a> · <a href="docs/spec.md">Spec reference</a> · <a href="docs/targets.md">Targets</a>
</p>

Describe an environment once (its machines, networks, who reaches whom, the services they
expose) and run it anywhere: containers on your machine, a Kubernetes cluster, local VMs, a
Proxmox or ESXi server, or the cloud. Like a loom weaving one pattern into the same cloth every
time, each target produces **the same external behavior** from the same spec, its own way, and
the same checks prove it on every target.

```yaml
# isoloom.yml
version: 1
name: shop
networks:
  lab: { cidr: 10.20.0.0/24 }
machines:
  database:
    networks: { lab: 32 }
    ports: [{ port: 5432, name: postgres }]
    docker: { image: "postgres:16" }                            # as a container
    vm: { os: debian@12, provision: [provision/database.sh] }   # as a VM (native, no Docker)
  web:
    networks: { lab: 31 }
    ports: [{ port: 80, http: true, publish: 8080 }]
    depends_on: [database]
    docker: { build: build/web }
    vm: { os: debian@12, provision: [provision/web.sh] }
checks:                  # plus the checks Isoloom derives from ports and reach
  - { from: web, tcp: "database:5432" }
```

```sh
isoloom run docker && isoloom test docker && isoloom down docker
isoloom run vagrant            # the same environment, one VM per machine
```

- **Targets are derived, not declared.** Every machine has `docker:`, so the Docker targets;
  every machine has `vm:`, so the VM targets. A Windows machine has only `vm:`, so that
  environment never gets a broken Docker file. `targets:` can only narrow the list.
- **Behavior is the contract.** Checks are declared, scripted, or derived from the spec (every
  service answers from where `reach` allows and from nowhere else), and run from inside the
  environment on every target.
- **Readable output.** Each target's files (a Compose file, manifests, a Helm chart,
  Vagrantfiles, Terraform modules) are written under `.isoloom/`, to run by hand or commit;
  `isoloom check` fails in CI when they drift from the spec.
- **Secrets stay secret.** Values come from a store at run time and reach machines as files,
  never in a generated file: [Secrets](docs/secrets.md).

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/main/install.sh | sh
```

Release binaries for macOS, Linux and Windows are on the
[releases page](https://github.com/isoloom/isoloom/releases); in GitHub Actions,
`uses: isoloom/isoloom@v0.10.1`. Then follow [Getting started](docs/getting-started.md).

## Targets

| Target | Runs the environment as |
|---|---|
| [`docker`](docs/targets.md#docker) | Containers on your Docker, with Compose |
| [`hosted`](docs/targets.md#hosted) | The same Compose file, on a hosting service |
| [`kubernetes`](docs/targets.md#kubernetes) | Manifests on your cluster, with NetworkPolicies for `reach` |
| [`helm`](docs/targets.md#helm), [`kustomize`](docs/targets.md#kustomize) | The same manifests as a Helm chart or a Kustomize overlay |
| [`vagrant`](docs/targets.md#vagrant) | One local VM per machine: VirtualBox, VMware, Parallels, libvirt, Hyper-V, UTM, QEMU, ESXi |
| [`docker-vm`](docs/targets.md#docker-vm) | The containers on one VM (Vagrant, or Proxmox) |
| [`hybrid`](docs/targets.md#hybrid) | Containers and VMs together on the same networks |
| [`proxmox`](docs/targets.md#proxmox) | One VM (or LXC container) per machine on Proxmox VE, its own SDN, router and firewall |
| [`esxi`](docs/targets.md#esxi) | One VM per machine on a standalone ESXi host |
| [`cloud-vm`](docs/targets.md#cloud-vm) | One cloud VM per machine: AWS, Azure, Google Cloud, DigitalOcean, Linode, Oracle Cloud |
| [`cloud-docker`](docs/targets.md#cloud-docker) | The containers on one cloud VM, on the same six clouds |
| [`cloud-services`](docs/targets.md#cloud-services) | A Terraform module of yours, applied into your cloud account |
| [`external`](docs/targets.md#external) | Hosts that already exist, set up and checked over SSH |

Which targets run for real in CI, and which are generated and validated only, is said for each
one in [Targets](docs/targets.md).

## Documentation

- [Getting started](docs/getting-started.md): install, a first spec, run, test, down.
- [Targets](docs/targets.md): what each target builds, what the host needs, its limits.
- [Spec reference](docs/spec.md): every field, generated from the schema, with the targets that
  render or refuse it.
- [Secrets](docs/secrets.md): stores, delivery per target, and what never happens.
- [Storage](docs/storage.md): volumes (shared, persistent, external, `subpath`), `tmpfs` and data
  disks, and how each target handles them.
- [Coverage](docs/COVERAGE.md): everything each output format can do, and whether Isoloom
  produces it (`isoloom coverage`).
- [The `esxi` target](docs/esxi.md), [cloud modules](docs/cloud-modules.md): variables and
  outputs of the generated Terraform modules.
- [JSON output](docs/json.md) and [the registry](docs/registry.md), for tools that drive
  Isoloom.
- [Real targets in CI](docs/real-targets.md): the workflows that run the VM and cloud targets.
- [Architecture](ARCHITECTURE.md), [Contributing](CONTRIBUTING.md), the
  [decision records](docs/adr/) and the [changelog](CHANGELOG.md).
- [Stability](docs/stability.md), [comparisons with other tools](docs/comparisons/README.md), [RFCs](rfcs/README.md), the [code of conduct](CODE_OF_CONDUCT.md) and the [security policy](SECURITY.md).

## Using the library

The crates aren't on crates.io: depend on the ones you use from git, pinned to a release tag.

```toml
isoloom-spec = { git = "https://github.com/isoloom/isoloom", tag = "vX.Y.Z" }
isoloom-targets = { git = "https://github.com/isoloom/isoloom", tag = "vX.Y.Z" }
isoloom-runtime = { git = "https://github.com/isoloom/isoloom", tag = "vX.Y.Z" }
```

| Crate | Holds |
|---|---|
| `isoloom-spec` | The model, parsing and loading, normalization, validation, the JSON schema, profiles |
| `isoloom-plan` | The target-neutral plan, the checks, requirements and refusal |
| `isoloom-render` | The `Backend` trait and the pieces every backend shares |
| `isoloom-compose` | The Compose backend: `docker`, `hosted`, and the Compose file of the Docker-on-a-VM targets |
| `isoloom-kubernetes` | The Kubernetes backend: manifests, Kustomize, the Helm chart |
| `isoloom-vagrant` | The Vagrant backends: `vagrant`, `docker-vm`, `hybrid` |
| `isoloom-terraform` | The Terraform backends: `proxmox`, `esxi`, `cloud-vm`, `cloud-docker`, `cloud-services` |
| `isoloom-external` | The `external` backend |
| `isoloom-targets` | Every backend behind one call, refusals, the resolved snapshot |
| `isoloom-import` | Drafting a spec from Compose, Vagrant and Terraform files |
| `isoloom-coverage` | The coverage map and the generated spec reference |
| `isoloom-runtime` | Running environments: the registry, host readiness, secrets' stores, events, reports |
| `isoloom` | The command line |

- **Supported surface**:
  - `isoloom-spec`: the crate root (`load`, `load_with`, `load_profiled`, `parse`, `find`,
    `validate`, `validate_files`, `derive`, `effective`, `totals`, the model types) and the
    modules `model`, `validate`, `targets`, `instance`, `defaults` and `profile`;
  - `isoloom-plan`: `checks` (the check plan and the runner's output lines);
  - `isoloom-targets`: `generate`, `generate_all`, `generate_instance`, `refusal`, `refusal_on`,
    `cloud_refusals`, `GeneratedFile`, `GenerateError`, and `resolved` (the snapshot);
  - `isoloom-runtime`: `registry` (`~/.isoloom/status.yml`), `host`, `events` (the lifecycle's
    JSON events and errors, with an `Emitter`) and `tool`.

  Every other public item (the spec's `clones`, `groups`, `vlans`, `images`, `schema`, `shell`;
  `isoloom-render`, the backends, `isoloom-import`, `isoloom-coverage`,
  `isoloom_runtime::report` and `isoloom_runtime::secrets`) is internal: public for the command
  line, and may change in any release. Up to 0.10 the `isoloom-core` crate re-exported the
  supported surface; it is gone, and each item is used from the crate that holds it
  (`isoloom_core::generate` is `isoloom_targets::generate`, `isoloom_core::registry` is
  `isoloom_runtime::registry`, `isoloom_core::checks` is `isoloom_plan::checks`).
- **Versions**: while Isoloom is at 0.x, a breaking change to the supported surface bumps the
  minor version (0.10 -> 0.11), never the patch, and the CHANGELOG lists it under a
  "Breaking (library)" line. A new public field or enum variant isn't breaking (a new spec key
  is one): get the model's types by parsing a spec rather than writing struct literals. CI
  reports every pull request's API changes (`cargo semver-checks`, against the base and the
  last tag) in its summary.
- **Files**: `~/.isoloom/status.yml` has a `version:` and is documented in
  [docs/registry.md](docs/registry.md); the cloud modules' variables and outputs in
  [docs/cloud-modules.md](docs/cloud-modules.md); the resolved snapshot
  (`.isoloom/resolved.json`) carries its own `version`.

## License

Isoloom is dual-licensed under the [MIT License](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option. Contributions are accepted under the same terms.
