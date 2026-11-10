<p align="center">
  <img src=".github/isoloom-mark.svg" width="76" alt="Isoloom">
</p>

<h1 align="center">Isoloom</h1>

<p align="center">
  <strong>One spec. Every tool.</strong><br>
  <a href="docs/README.md">Documentation</a> · <a href="docs/spec.md">Spec reference</a> · <a href="docs/matrix.md">Target matrix</a>
</p>

<p align="center">
  <a href="https://github.com/isoloom/isoloom/actions/workflows/ci.yml"><img src="https://github.com/isoloom/isoloom/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <a href="https://crates.io/crates/isoloom"><img src="https://img.shields.io/crates/v/isoloom.svg" alt="crates.io"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="Apache-2.0"></a>
</p>

Describe an environment once (its machines, the services on them, the networks, who reaches
whom) in one `isoloom.yml`, and Isoloom writes the native files of each tool that can run it:
Docker Compose, Kubernetes manifests with a Kustomize overlay and a Helm chart, Vagrantfiles,
Terraform modules for Proxmox VE, ESXi and six clouds, and Ansible inventories and playbooks.
Every target produces the same behavior from the same spec, and the same checks prove it.

**Isoloom is a converter. It runs nothing.** Next to most targets' files it writes a versioned
**run manifest** (`run.json`) saying how to bring the environment up, check it and tear it down.
You run the files with their own tools, or hand the manifest to a runner:
[**Isothrum**](https://github.com/Isothrum/isothrum) is the reference runner.

## Who it is for

- **Platform and DevOps teams** who need the same test, demo or training environment on a
  laptop (Compose, Vagrant), in a cluster, on an on-premises hypervisor and in a cloud account,
  without maintaining one set of files per tool.
- **CI pipelines** that bring a realistic multi-machine environment up for a job, prove it with
  checks, and tear it down.
- **Teams that review infrastructure as code**: the generated files are plain, readable and
  deterministic, so you can commit them, diff them and run them without Isoloom.
- **Tool builders** who embed the Rust crates to generate environments from their own interface.

**When not to use it:**

- To manage long-lived production infrastructure: Isoloom generates whole environments to create
  and destroy; it doesn't import, drift-detect or update resources you already run.
- When you need one tool's full feature set: the spec covers what every target can do the same
  way (the [target matrix](docs/matrix.md) says what each target refuses). Write that tool's
  files directly, or layer your own on top of the generated ones.
- When a single `docker-compose.yml` already does the job and will never run anywhere else.

## Install

Release binaries for Linux (x86_64, aarch64; static), macOS (x86_64, arm64) and Windows (x86_64),
from 2.0.0, with checksums, a Sigstore signature, build provenance and an SBOM
([verify a download](docs/guide/verify-a-release.md)):

```sh
# Linux, macOS: the latest release into ~/.local/bin, checked against SHA256SUMS
curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/main/packaging/install.sh | sh
# a given release, elsewhere
curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/main/packaging/install.sh \
  | ISOLOOM_VERSION=v2.0.0 ISOLOOM_INSTALL_DIR=/usr/local/bin sh
```

On Windows, download `isoloom-x86_64-pc-windows-msvc.zip` from the
[releases page](https://github.com/isoloom/isoloom/releases) and check it against `SHA256SUMS`.

With [cargo-binstall](https://github.com/cargo-bins/cargo-binstall), or from source (Rust 1.88 or
later):

```sh
cargo binstall isoloom
cargo install --locked isoloom
```

In GitHub Actions, pinned to a commit ([CI guide](docs/guide/ci.md#pinning)):

```yaml
- uses: isoloom/isoloom@<commit sha>   # v2.0.0
  with:
    version: v2.0.0
```

## Quickstart

With `isoloom` and Docker (Compose 2.17 or later), run the `hello-stack` example, a web server
and a cache:

```sh
version="$(isoloom --version | cut -d' ' -f2)"
git clone --depth 1 --branch "v$version" https://github.com/isoloom/isoloom.git isoloom-src
cp -R isoloom-src/examples/hello-stack . && cd hello-stack

isoloom validate                    # ✓ hello-stack is valid
isoloom targets                     # where it can run, and each target's tier
isoloom generate --target docker    # writes .isoloom/docker/compose.yml and run.json

docker compose -f .isoloom/docker/compose.yml up -d --build --wait
docker compose -f .isoloom/docker/compose.yml port web 80     # 127.0.0.1:<port>
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check
docker compose -f .isoloom/docker/compose.yml down -v
```

Or let the runner do it, checks included: `isothrum up . --target docker`, `isothrum check .
--target docker`, `isothrum down . --target docker`. The
[getting-started guide](docs/guide/getting-started.md) walks through each step.

A spec looks like this ([every field](docs/spec.md)):

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
      image: "postgres:16"                     # as a container: the official image
      steps: [{ packages: [{ name: postgresql }] }, { service: postgresql }]   # as a VM: set up natively
  web:
    os: debian@12
    networks: { lab: 31 }
    service: nginx                             # a packaged catalogue service, both forms
    ports: { http: { publish: 8080 } }
    depends_on: [database]
checks:                                        # plus the checks Isoloom derives from ports and reach
  - { from: web, tcp: "database:5432" }
```

## Targets

`isoloom targets` lists the targets a spec can be generated for, and why the others are refused
(a Windows machine has no container form, so that environment never gets a broken Compose file).
Each target sits in a **tier** that states how far it has been proven, worked out from recorded
real runs: **Tier 1**, a full real run (up, checks, down) from every host OS it exists on;
**Tier 2**, every generated file passes its format's official validator. The
[target matrix](docs/matrix.md) (`isoloom matrix`) has the evidence.

| Target | Converts the spec into | Tier | Run manifest | Guide |
|---|---|---|---|---|
| `docker` | A Compose project for your Docker | **1** (Linux, macOS, Windows) | yes | [Docker](docs/guide/targets/docker.md) |
| `hosted` | The same Compose file, for a hosting service | 2 | no | [Docker](docs/guide/targets/docker.md) |
| `kubernetes` | Manifests with NetworkPolicies for `reach`, a Kustomize overlay and a Helm chart | 2 | no | [Kubernetes](docs/guide/targets/kubernetes.md) |
| `vagrant` | One local VM per machine: VirtualBox, VMware, Parallels, libvirt, Hyper-V, QEMU, remote ESXi | **1** on libvirt (Linux) and Hyper-V (Windows); 2 on the others | yes | [Vagrant](docs/guide/targets/vagrant.md) |
| `docker-vm` | The containers on one VM (Vagrant, UTM included, or Proxmox) | 2 | yes | [Vagrant](docs/guide/targets/vagrant.md), [Proxmox](docs/guide/targets/proxmox.md) |
| `hybrid` | Containers and VMs together on the same networks | 2 | no | [Vagrant](docs/guide/targets/vagrant.md) |
| `proxmox` | A Terraform module: one VM per machine on Proxmox VE, with its SDN, router and firewall | 2 | yes | [Proxmox](docs/guide/targets/proxmox.md) |
| `esxi` | A Terraform module: one VM per machine on a standalone ESXi host | 2 | no | [ESXi](docs/guide/targets/esxi.md) |
| `cloud-vm` | A Terraform module per cloud: one VM per machine on AWS, Azure, Google Cloud, DigitalOcean, Linode or Oracle Cloud | 2 | yes, per cloud | [Clouds](docs/guide/targets/cloud.md) |
| `cloud-docker` | A Terraform module per cloud: the containers on one cloud VM | 2 | yes, per cloud | [Clouds](docs/guide/targets/cloud.md) |
| `cloud-services` | Your own Terraform module, applied into your AWS, Azure or Google Cloud account | 2 | yes | [Clouds](docs/guide/targets/cloud.md) |
| `external` | Hosts that already exist, set up and checked over SSH with Ansible | 2 | no | [Existing hosts](docs/guide/targets/external.md) |

The cloud targets' default regions are all in Europe; [change them](docs/guide/targets/cloud.md#regions)
in your defaults.

## Requirements

`isoloom` needs nothing else to validate and generate, and no network access. Running the files
needs each target's tools ([tool versions](docs/reference/tool-versions.md)):

- Docker with **Compose 2.17** or later: `docker`, `hosted`, `docker-vm`'s VM;
- **Vagrant 2.4** or later and a hypervisor (with its Vagrant plugin): `vagrant`, `docker-vm`;
- **Terraform 1.6** or later for the clouds, Proxmox and ESXi, with the
  providers the modules pin (`hashicorp/aws ~> 6.67.0`, `hashicorp/azurerm ~> 5.8.0`,
  `hashicorp/google ~> 8.5.0`, `digitalocean/digitalocean ~> 2.103.0`, `linode/linode ~> 4.7.0`,
  `oracle/oci ~> 9.8.0`, `bpg/proxmox ~> 0.116.1`, `josenk/esxi ~> 1.10.3`);
- `kubectl` or Helm, and a network plugin that enforces NetworkPolicies: `kubernetes`;
- Ansible: `external`.

OpenTofu is not tested.

## Configuration

What belongs to you or your site rather than to the environment lives outside the spec
([configuration reference](docs/reference/configuration.md)):

- **Defaults**: each cloud's region, the Vagrant provider, the image table. Built in, then
  `~/.isoloom/defaults.yml`, the project's `isoloom.defaults.yml`, the profile, `--images`,
  `ISOLOOM_<KEY>` variables (`ISOLOOM_CLOUD__AWS__REGION=eu-central-1`), and `-s
  defaults.<key>=<value>`. `isoloom defaults` shows what is in effect and where each value comes
  from.
- **Profiles**: `profiles/<name>.yml` holds what differs for one customer or site (the target,
  spec values, defaults), used with `--profile <name>` or `ISOLOOM_PROFILE`.
- **Your own images**: `--images <file>` replaces the built-in boxes, cloud images and templates
  with your golden images ([how-to](docs/guide/golden-images.md)).
- **Corporate networks**: the spec's `constraints:` sets a proxy, registry and package mirrors,
  time servers and a corporate CA for the machines ([how-to](docs/guide/corporate-network.md)).

## Running what Isoloom generates

Each tool's files run by hand (`docker compose up`, `kubectl apply -k`, `vagrant up`,
`terraform apply`, `ansible-playbook`); each file's header says how. A runner reads the run
manifest instead: it checks the tools' versions, brings the environment up, delivers secrets
(never written into a file), waits until it is ready, runs every check, and tears it down.

- **A runner is required** for `cloud-vm` and `cloud-docker` (their machines wait for the project
  and secrets the runner delivers over SSH) and for sealing `internet: false` networks on the
  clouds once the machines are set up. It also enforces cloud expiry and reaps leftovers.
- **It is optional** for `docker`, `vagrant`, `docker-vm`, `proxmox`, `esxi` and
  `cloud-services`; on `proxmox` and `esxi` it also seals `internet: false` networks at the
  router once the machines are set up.
- `kubernetes`, `hybrid`, `external` and `hosted` have no run manifest yet: run them by hand.

[Isothrum](https://github.com/Isothrum/isothrum) is the reference runner; any program that follows
[the run manifest contract](docs/reference/run-manifest.md) can be one.

## Stability

Isoloom follows Semantic Versioning ([policies](docs/reference/policies.md)):

| Surface | Promise |
|---|---|
| The spec format (`version: 1`) | Strict: a spec valid on a release stays valid, with the same meaning, for the whole major version |
| The command line | Strict: commands, flags, exit codes and `--json` output |
| The run manifest | Versioned: `manifest_version: 1`; a minor release may add an optional field, which a runner reads once it upgrades the `isoloom-manifest` it pins |
| Catalogue entries | Removable with notice: deprecated in a minor release, removed no earlier than three months later |
| The library API | `isoloom-spec`, `isoloom-targets` and `isoloom-manifest` are supported; the other crates are building blocks whose API may change |
| The generated files | Their behavior, proven by the checks; their text may change in any release |

Version 2.0 removes library API and refuses a few specs 1.x took:
[migrating to 2.0](docs/guide/migrating-to-2.0.md).

## Documentation

The [documentation index](docs/README.md) has the guides (getting started, one per target, corporate
networks, CI, offline use), the reference (the [spec](docs/spec.md), the
[command line](docs/reference/cli.md), the [target matrix](docs/matrix.md), configuration, the run
manifest) and the explanations (architecture, [security considerations](docs/explanation/security.md),
determinism). The [CHANGELOG](CHANGELOG.md) lists every change.

## Using the library

The crates are on [crates.io](https://crates.io/crates/isoloom), all at one version:

```toml
isoloom-spec = "2"       # parse, load and validate a spec; the spec model
isoloom-targets = "2"    # generate every target's files, refusals, the resolved snapshot, tiers
isoloom-manifest = "2"   # the run manifest's types, for a runner
```

Those three are the supported API. `isoloom-catalogue`, `isoloom-plan`, `isoloom-render`, the
backends (`isoloom-compose`, `isoloom-kubernetes`, `isoloom-vagrant`, `isoloom-terraform`,
`isoloom-ansible`), `isoloom-import` and `isoloom` (the command line) are published so the
supported crates build from crates.io; their API is not a promise. Get the model's types by
parsing a spec rather than writing struct literals: a new spec key is then not a breaking change.
[ARCHITECTURE.md](ARCHITECTURE.md) maps the crates.

## Support, security and licence

- Questions and bugs: [getting help](.github/SUPPORT.md). Contributions:
  [CONTRIBUTING.md](.github/CONTRIBUTING.md), under the [code of conduct](.github/CODE_OF_CONDUCT.md)
  and the project's [governance](.github/GOVERNANCE.md).
- Vulnerabilities: report them privately as [SECURITY.md](.github/SECURITY.md) says. Supported
  releases: the latest 2.x, and 1.0.9 for security fixes for three months after 2.0.0.
- Isoloom is licensed under the [Apache License, Version 2.0](LICENSE). Contributions are accepted
  under the same licence.
