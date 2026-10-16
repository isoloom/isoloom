<p align="center">
  <a href="https://www.isoloom.com"><img src=".github/isoloom-mark.svg" width="76" alt="Isoloom"></a>
</p>

<h1 align="center">Isoloom</h1>

<p align="center">
  <strong>One spec. Every tool.</strong><br>
  <a href="https://www.isoloom.com">Website</a> · <a href="docs/spec.md">Spec reference</a> · <a href="docs/matrix.md">Target matrix</a>
</p>

Describe an environment once — its machines, the services on them, the networks, who reaches
whom — and Isoloom converts that one spec into the native files of every infrastructure and
configuration tool: Compose, Kubernetes manifests, Helm, Kustomize, Vagrantfiles,
Terraform/OpenTofu modules for Proxmox, ESXi and six clouds, cloud-init, and Ansible inventories
and playbooks. Like a loom weaving one pattern into the same cloth each time, every target
produces **the same external behavior** from the same spec, its own way, and the same checks
prove it.

**Isoloom is a converter. It runs nothing itself.** It reads one `isoloom.yml`, writes each
tool's files under `.isoloom/`, and emits a versioned **run manifest** (`run.json`) describing
how to bring the environment up, check it and tear it down. You run the native tools yourself, or
hand the manifest to the [Isothrum runner](https://github.com/isoloom/isothrum). Isoloom also
goes the other way: `isoloom import` drafts a spec from files you already have.

```yaml
# isoloom.yml
version: 1
name: shop
networks:
  lab: { cidr: 10.20.0.0/24, internet: false }
machines:
  database:
    os: debian@12
    networks: { lab: 32 }
    ports: { postgres: 5432 }
    app:
      image: "postgres:16"                     # a container: the official image
      steps: [{ run: provision/database.sh }]  # a VM: the same machine, set up natively
  web:
    os: debian@12
    networks: { lab: 31 }
    ports: { web: { port: 80, protocol: http, publish: 8080 } }
    depends_on: [database]
    app:
      build: build/web
      steps: [{ run: provision/web.sh }]
checks:                       # plus the checks Isoloom derives from ports and reach
  - { from: web, tcp: "database:5432" }
```

```sh
isoloom validate                 # check the spec, with the files it names
isoloom generate                 # write every possible target's files under .isoloom/
isoloom generate --target docker # or just one target
```

- **One declaration per machine.** A machine says what it *is* — an OS plus an `app:` (its own
  image, a build recipe, or an appliance) or a catalogue `service:` — never how a platform runs
  it. There are no `docker:` or `vm:` blocks. Each target decides delivery: an official or built
  image on container targets, a fresh VM plus set-up on VM targets.
- **Behavior is the contract.** Checks are declared, scripted, or derived from the spec (every
  service answers from where `reach` allows and from nowhere else), and run from inside the
  environment on every target.
- **Readable output.** Each target's files are written under `.isoloom/`, to run by hand, commit
  or hand to the runner; `isoloom check` fails in CI when they drift from the spec.
- **Secrets stay secret.** Values come from a store at run time and reach machines as files,
  never baked into a generated file.
- **Honest proof.** Every generated file passes its format's official validator, and each target
  sits in a [tier](docs/matrix.md) that states how far it has been proven.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/main/packaging/install.sh | sh
```

Release binaries for macOS, Linux and Windows are on the
[releases page](https://github.com/isoloom/isoloom/releases). The current release is **0.11.0**,
the release candidate for 1.0.0.

## Targets

| Target | Converts the spec into |
|---|---|
| `docker` | A Compose project for your Docker |
| `hosted` | The same Compose file, for a hosting service |
| `kubernetes` | Manifests for your cluster, with NetworkPolicies for `reach` |
| `helm`, `kustomize` | The same manifests as a Helm chart or a Kustomize overlay |
| `vagrant` | One local VM per machine: VirtualBox, VMware, Parallels, libvirt, Hyper-V, UTM, QEMU, ESXi |
| `docker-vm` | The containers on one VM (Vagrant, or Proxmox) |
| `hybrid` | Containers and VMs together on the same networks |
| `proxmox` | A Terraform module: one VM (or LXC) per machine on Proxmox VE, with its SDN, router and firewall |
| `esxi` | A Terraform module: one VM per machine on a standalone ESXi host |
| `cloud-vm` | A Terraform module: one cloud VM per machine on AWS, Azure, Google Cloud, DigitalOcean, Linode or Oracle Cloud |
| `cloud-docker` | A Terraform module: the containers on one cloud VM, on the same six clouds |
| `cloud-services` | Your own Terraform module, applied into your cloud account |
| `external` | Hosts that already exist, set up and checked over SSH with Ansible |

Targets are derived from the spec: a Windows machine has no container form, so that environment
never gets a broken Compose file. `targets:` can only narrow the list. Each target's tier and the
evidence behind it are in the [target matrix](docs/matrix.md); the launcher's targets (local,
server and cloud) are Tier 1 for 1.0, the rest Tier 2 (generated and validated).

## Running what Isoloom generates

Isoloom doesn't run anything. Each target writes a run manifest (`.isoloom/<target>/run.json`)
alongside its files — the native commands, readiness criteria, outputs and checks. The
[Isothrum runner](https://github.com/isoloom/isothrum) executes it (`isothrum up`, `check`,
`park`, `resume`, `down`), or you run each tool's files by hand (`docker compose up`,
`kubectl apply -k`, `vagrant up`, `terraform apply`, `ansible-playbook`).

## Documentation

- [Spec reference](docs/spec.md): every field, its type, defaults, allowed values and limits,
  generated from the schema.
- [Target matrix](docs/matrix.md): what each target does with each spec field, and each target's
  tier with its evidence.
- [Contributing](.github/CONTRIBUTING.md), the [code of conduct](.github/CODE_OF_CONDUCT.md) and
  the [security policy](.github/SECURITY.md).

## Using the library

The crates are published on [crates.io](https://crates.io/crates/isoloom) under Apache-2.0, all at
one workspace version. Depend on the ones you use:

```toml
isoloom-spec = "0.11.0"
isoloom-targets = "0.11.0"
```

| Crate | Holds |
|---|---|
| `isoloom-spec` | The authored spec: parsing, loading, the model, validation, the JSON schema and the spec reference |
| `isoloom-manifest` | The run manifest: its types, generated JSON schema and fixtures |
| `isoloom-catalogue` | The service catalogue: entries, release matching, typed `with:` settings and parity contracts |
| `isoloom-plan` | Lowering the spec into the target-neutral plan, the checks and refusals |
| `isoloom-render` | The `Backend` trait, the builders (HCL, Ruby, YAML, shell, PowerShell, Dockerfile) and the shared container pieces |
| `isoloom-compose` | The Compose backend: `docker`, `hosted` |
| `isoloom-kubernetes` | The Kubernetes backend: manifests, Kustomize, the Helm chart |
| `isoloom-vagrant` | The Vagrant backend: `vagrant` and its providers |
| `isoloom-terraform` | The Terraform backends: `proxmox`, `esxi`, `cloud-vm`, `cloud-docker`, `cloud-services` |
| `isoloom-ansible` | The Ansible backend, which renders the `external` target |
| `isoloom-targets` | Every backend behind one call, the composite targets, refusals, the resolved snapshot, the matrix and tiers |
| `isoloom-import` | Drafting a spec from Compose, Vagrant and Terraform files |
| `isoloom` | The command line |

- **Versions**: while Isoloom is at 0.x, a breaking change to the public surface bumps the minor
  version (0.10 → 0.11), never the patch. Getting the model's types by parsing a spec, rather than
  writing struct literals, keeps you forward-compatible: a new spec key is not a breaking change.

## License

Isoloom is licensed under the [Apache License, Version 2.0](LICENSE). Contributions are accepted
under the same license, with a DCO sign-off.
