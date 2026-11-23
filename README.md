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

**Isoloom is a converter. It runs nothing.** It writes each tool's native files only, complete
enough that the tool alone brings the environment to ready: you run them with their own tools,
or let a runner run them: [**Isothrum**](https://github.com/Isothrum/isothrum) is the reference
runner.

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
  | ISOLOOM_VERSION=v2.1.9 ISOLOOM_INSTALL_DIR=/usr/local/bin sh
```

On Windows, download `isoloom-x86_64-pc-windows-msvc.zip` from the
[releases page](https://github.com/isoloom/isoloom/releases) and check it against `SHA256SUMS`.

With [cargo-binstall](https://github.com/cargo-bins/cargo-binstall), or from source (Rust 1.89 or
later):

```sh
cargo binstall isoloom
cargo install --locked isoloom
```

In GitHub Actions, pinned to a commit ([CI guide](docs/guide/ci.md#pinning)):

```yaml
- uses: isoloom/isoloom@<commit sha>   # v2.1.9
  with:
    version: v2.1.9
```

## Quickstart

With `isoloom` and Docker (Compose 2.17 or later), run the `hello-stack` example, a web server
and a cache:

```sh
version="$(isoloom --version | cut -d' ' -f2)"
git clone --depth 1 --branch "v$version" https://github.com/isoloom/isoloom.git isoloom-src
cp -R isoloom-src/examples/hello-stack . && cd hello-stack

isoloom validate                    # ✓ hello-stack is valid
isoloom targets                     # where it can run, and each target's validators
isoloom generate --target docker    # writes .isoloom/docker/compose.yml

docker compose -f .isoloom/docker/compose.yml up -d --build --wait
docker compose -f .isoloom/docker/compose.yml port web 80     # 127.0.0.1:<port>
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check
docker compose -f .isoloom/docker/compose.yml down -v
```

Or let a runner run the same files: `isothrum up .isoloom/docker`, then `isothrum down
.isoloom/docker`. The
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
Isoloom is a converter: each target is the files it writes, and every one of those files is
checked by its format's official validators on every example (`cargo xtask verify`, and
`cargo xtask tf-test` for `terraform test`). The [target matrix](docs/matrix.md)
(`isoloom matrix`) lists them with what each target does with each spec field.

| Target | Converts the spec into | Validated by | Guide |
|---|---|---|---|
| `docker` | A Compose project for your Docker | `docker compose config` | [Docker](docs/guide/targets/docker.md) |
| `hosted` | The same Compose file, for a hosting service | `docker compose config` | [Docker](docs/guide/targets/docker.md) |
| `kubernetes` | Manifests with NetworkPolicies for `reach`, a Kustomize overlay and a Helm chart | `kubeconform`, `helm lint` | [Kubernetes](docs/guide/targets/kubernetes.md) |
| `vagrant` | One local VM per machine: VirtualBox, VMware, Parallels, libvirt, Hyper-V, QEMU, UTM, remote ESXi | `vagrant validate` | [Vagrant](docs/guide/targets/vagrant.md) |
| `proxmox` | A Terraform module: one VM per machine on Proxmox VE, with its SDN, router and firewall | `terraform validate`, `tflint`, `terraform test` | [Proxmox](docs/guide/targets/proxmox.md) |
| `esxi` | A Terraform module: one VM per machine on a standalone ESXi host | `terraform validate`, `tflint`, `terraform test` | [ESXi](docs/guide/targets/esxi.md) |
| `cloud-vm` | A Terraform module per cloud: one VM per machine on AWS, Azure, Google Cloud, DigitalOcean, Linode or Oracle Cloud | `terraform validate`, `tflint`, `terraform test` | [Clouds](docs/guide/targets/cloud.md) |
| `cloud-docker` | The Compose file and a Terraform module per cloud: the containers on one cloud VM | `docker compose config`, `terraform validate`, `tflint`, `terraform test` | [Clouds](docs/guide/targets/cloud.md) |
| `external` | Hosts that already exist, set up and checked over SSH with Ansible | `ansible-lint` | [Existing hosts](docs/guide/targets/external.md) |

`cloud-services` is not supported in Isoloom 3.0: a spec with `cloud:` is refused.

The cloud targets' default regions are all in Europe; [change them](docs/guide/targets/cloud.md#regions)
in your defaults.

## Requirements

`isoloom` needs nothing else to validate and generate, and no network access. Running the files
needs each target's tools ([tool versions](docs/reference/tool-versions.md)):

- Docker with **Compose 2.17** or later: `docker`, `hosted`;
- **Vagrant 2.4** or later and a hypervisor (with its Vagrant plugin): `vagrant`;
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
`terraform apply`, `ansible-playbook`); each file's header says how. A runner runs the same
native files for you: it checks the tools' versions, brings the environment up, gives the
secrets (never written into a file), waits until it is ready, runs every check, and tears it
down.

- **The tools alone bring an environment to ready.** `docker compose up -d --wait` waits for the
  healthchecks and init jobs; `vagrant up` ends after each machine's last provisioner, its
  readiness check; `terraform apply` brings the project into each cloud machine over SSH, waits
  for every machine's set-up (its ready file) and seals `internet: false` networks in the same
  apply. The values given at launch reach each tool from its environment: Compose interpolates
  `${NAME}`, the Vagrantfile reads `ENV.fetch`, Terraform takes `TF_VAR_<NAME>`.
- **A runner is optional** everywhere: it adds cloud expiry and reaps leftovers.

[Isothrum](https://github.com/Isothrum/isothrum) is the reference runner: it runs the native files
as they are.

## Stability

Isoloom follows Semantic Versioning ([policies](docs/reference/policies.md)):

| Surface | Promise |
|---|---|
| The spec format (`version: 1`) | Strict: a spec valid on a release stays valid, with the same meaning, for the whole major version |
| The command line | Strict: commands, flags, exit codes and `--json` output |
| Catalogue entries | Removable with notice: deprecated in a minor release, removed no earlier than three months later |
| The library API | `isoloom-spec` and `isoloom-targets` are supported; the other crates are building blocks whose API may change |
| The generated files | Their behavior, proven by the checks; their text may change in any release |

Version 2.0 removes library API and refuses a few specs 1.x took:
[migrating to 2.0](docs/guide/migrating-to-2.0.md).

## Documentation

The [documentation index](docs/README.md) has the guides (getting started, one per target, corporate
networks, CI, offline use), the reference (the [spec](docs/spec.md), the
[command line](docs/reference/cli.md), the [target matrix](docs/matrix.md), configuration) and the
explanations (architecture, [security considerations](docs/explanation/security.md),
determinism). The [CHANGELOG](CHANGELOG.md) lists every change.

## Using the library

The crates are on [crates.io](https://crates.io/crates/isoloom), all at one version:

```toml
isoloom-spec = "2"       # parse, load and validate a spec; the spec model
isoloom-targets = "2"    # generate every target's files, refusals, the resolved snapshot, validators
```

Those two are the supported API. `isoloom-catalogue`, `isoloom-plan`, `isoloom-render`, the
backends (`isoloom-compose`, `isoloom-kubernetes`, `isoloom-vagrant`, `isoloom-terraform`,
`isoloom-ansible`), `isoloom-import`, `isoloom` (the command line), `isoloom-topology` and
`isoloom-wasm` (a topology editor's compiler and its WebAssembly engine) are published so the
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
