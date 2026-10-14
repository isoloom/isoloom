<p align="center">
  <a href="https://www.isoloom.com"><img src=".github/isoloom-mark.svg" width="76" alt="Isoloom"></a>
</p>

<h1 align="center">Isoloom</h1>

<p align="center">
  <strong>One spec. Every environment.</strong><br>
  <a href="https://www.isoloom.com">Website</a> · <a href="https://www.isoloom.com/en/docs/introduction">Docs</a> · <a href="https://www.isoloom.com/en/docs/spec-reference">Spec reference</a>
</p>

Describe an environment once (its machines, networks, who reaches whom, the services they
expose) and run it anywhere: containers on your machine, local VMs, a Proxmox server, or the
cloud. Like a loom weaving one pattern into the same cloth every time, each target produces
**the same external behavior** from the same spec, its own way.

```yaml
# isoloom.yml
version: 1
name: supplier-portal-api

networks:
  lab: { cidr: 10.20.0.0/24 }

machines:
  database:
    networks: { lab: 32 }
    services: [{ port: 3207, name: mysql }]
    docker: { image: "mysql:8.0", init: [build/database/init] }     # as a container
    vm: { os: debian-12, provision: [provision/database.sh] }       # as a VM (native, no Docker)
  web:
    networks: { lab: 31 }
    services: [{ port: 3206, http: true }]
    depends_on: [database]
    docker: { build: build/web }
    vm: { os: debian-12, provision: [provision/web.sh] }

checks:                          # plus the checks Isoloom derives from services and reach
  - build/check/check.sh
  - { from: web, tcp: database:3207 }
```

- **Targets are derived, not declared.** Every machine has `docker:` → Docker targets; every
  machine has `vm:` → VM targets (Vagrant, Proxmox, cloud). A Windows machine has only `vm:`, so
  that environment never gets a broken Docker file. On Proxmox, a machine with only `docker:` may
  opt in to run as an LXC container (`docker.lxc: true`, Proxmox VE 9.1 or later): no Docker
  runtime, so what a container can't carry (volumes, files, healthchecks...) is refused. `targets:` can only narrow the list.
- **Behavior is the contract.** The same checks run on every target; a target whose checks fail
  isn't offered.
- **Say it once.** `common:` and `groups:` hold what several machines share, and `count: 5` makes
  five of a machine; the spec stays short when a fleet gets the same box, size and playbook.
- **Generic.** Nothing is specific to one use: launch-time values reach machines through
  `inputs:`, and `access: true` marks the machine a user lands on.
- **Secrets stay secret.** `secrets:` names where each value lives (the runner's environment, a
  file, Vault, SOPS); a machine lists the ones it gets, as files. `isoloom run` reads them and
  hands them to the target's own mechanism (Compose secrets, a Kubernetes Secret) or, on a VM
  or a Docker host, writes them over the machine's own connection (SSH; WinRM or OpenSSH on
  Windows) while its set-up waits for them: no value is ever in `.isoloom/`, the snapshot,
  cloud-init, Terraform's state or a log. A file is `0444` in a container (as Compose and
  Kubernetes do), `0400` on a Linux VM, whose other users shouldn't read it, and readable by
  SYSTEM and Administrators only on Windows. Delivering over WinRM needs `python3` with
  `pywinrm` on the runner.

```yaml
secrets:
  db-password: { env: DB_PASSWORD }               # or { file: ~/.lab/db }, { vault: 'secret/app#db' }, { sops: 'enc.yaml#db.password' }
machines:
  db:
    secrets: [db-password]                        # /run/secrets/db-password
  web:
    secrets: [{ source: db-password, target: /etc/app/db, uid: 101, gid: 101, mode: 0400 }]
```

## Install

```
curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/main/install.sh | sh
```

Release binaries for macOS, Linux (x86_64, arm64) and Windows are on the
[releases page](https://github.com/isoloom/isoloom/releases). In GitHub Actions:

```yaml
- uses: isoloom/isoloom@v0.10.1
- run: isoloom check
```

From source: `cargo install --git https://github.com/isoloom/isoloom isoloom`.

## Commands

```
isoloom validate [DIR]    # mistakes, with the exact field and what to do (--json)
isoloom targets [DIR]     # where it can run, and why not elsewhere (--host: can this machine?)
isoloom doctor            # what this machine can run: tools and credentials found, and missing
isoloom resources [DIR]   # machines, CPUs, memory, disk
isoloom generate [DIR]    # each target's files under .isoloom/ (--target, --images FILE)
isoloom check [DIR]       # fails when .isoloom/ doesn't match the spec (for CI)
isoloom inspect [PATH]    # the resolved snapshot (.isoloom/resolved.json), or one part of it
isoloom message           # how to use the environment (the spec's message, placeholders filled)
isoloom tc set far --delay 200ms   # link impairment on a network, while it runs
isoloom connect shell     # the `shell` tool: a toolbox on every network (tools: { shell: {} })
isoloom graph -o lab.svg  # the environment as a diagram (D2 or Graphviz)
isoloom report services   # tables: addressing, services, wiring, resources
isoloom defaults          # the defaults in effect (images, Vagrant provider, cloud regions) and their sources
isoloom chart --version 4.2.1   # the Helm chart (.isoloom/kubernetes/chart/), packaged with helm
isoloom profiles          # the project's profiles (profiles/<name>.yml), each checked against the spec
isoloom run TARGET [DIR]  # generate, then bring the environment up with the target's own tool
isoloom test TARGET [DIR] # run the checks against it: the spec's, and the ones derived from it
isoloom provision [MACHINE...] --target vagrant   # provisioning again, in place (after a step failed)
isoloom run docker --instance 2   # a second copy, with its own names, blocks and ports
isoloom run external      # machines that already exist: provision and check them over SSH
isoloom status            # what is up on this host, and whether it still runs
isoloom connect MACHINE   # a shell on a machine (compose exec, vagrant ssh, kubectl exec, ssh)
isoloom exec MACHINE -- … # a command on one machine, or `all`
isoloom capture MACHINE NETWORK   # tcpdump on its interface there
isoloom down TARGET [DIR] # tear it down
```

Every command takes `--profile NAME` (or `ISOLOOM_PROFILE`): `profiles/<name>.yml`, what differs
for one customer or site. It can pin the target, change spec values (`spec:`, merged in before
`-s`), add a layer of defaults, and give inputs their values (the environment still wins):

```yaml
# profiles/bank-fr.yml
description: Bank, Paris site
target: proxmox
spec:
  networks: { lan: { internet: false } }
  machines: { web: { resources: { cpus: 4, memory_mb: 8192 } } }
defaults:
  images: { os: { debian-12: { proxmox: local:iso/debian-12-bank.qcow2 } } }
inputs:
  REGISTRY: registry.bank.local
```

`DIR` holds `isoloom.yml` (`isoloom.yaml` also works). VMs get a copy of the project in
`/opt/isoloom`; an `.isoloomignore` next to `isoloom.yml` (gitignore-style) lists what they don't
need, on every target that copies it.

Or run what it generates by hand:

```
docker compose -f .isoloom/docker/compose.yml up -d --wait          # containers
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check
cd .isoloom/vagrant && vagrant up                                    # one VM per machine
kubectl kustomize --load-restrictor LoadRestrictionsNone .isoloom/kubernetes | kubectl apply -f -
helm install env .isoloom/kubernetes/chart -n env --create-namespace && helm test env -n env   # the same, as a chart
```

On Windows with **Hyper-V** (`vagrant up --provider hyperv`, or `vagrant: { provider: hyperv }` in
the defaults), run Vagrant from an elevated shell (an administrator, or a member of Hyper-V
Administrators); `isoloom doctor` says whether Hyper-V is on and usable from yours. Each VM's
first adapter is on `ISOLOOM_HYPERV_SWITCH` (default: the Default Switch, which Windows Server
doesn't have); each network becomes a private switch `isoloom-<env>-<network>` (between the VMs
only, removed with the last VM), and each machine's fixed address there is set inside the guest.
Published ports aren't forwarded to the host's loopback on Hyper-V (Vagrant can't): `vagrant up`
warns with the VM's address to use instead, and `isoloom report services` says so when Hyper-V is
the provider (the defaults' `vagrant.provider`, or `VAGRANT_DEFAULT_PROVIDER`). OS names without
a Hyper-V box (debian-13, ubuntu-14.04, fedora-42) need `vm.image.hyperv`; `isoloom run vagrant`
refuses them on Hyper-V until they have one.

A machine can boot an image of its own: `vm.image.vagrant` with `box_url` (a URL, or a `.box`
file in the project) or `box_server_url` (a private box server); `vm.image.proxmox` (a cloud
image's URL, or a template VM's id to clone); `vm.image.aws`, `azure`, `gcp`, `digitalocean`,
`linode`, `oci` (each cloud's image id). `vm.os` still says what the image is. `vm.boot` boots it
its own way: `kernel` with `initrd`, `cmdline` and `dtb`, the `firmware` (`bios` or `uefi`), a UEFI
`loader` with its `nvram`, the boot `order`, and a `network` boot (`tftp_root`, `file`, `server`)
on its first network. Vagrant's libvirt provider boots all of it, QEMU UEFI with its
`firmware_format`, Proxmox the firmware (OVMF with an EFI disk) and the order; the other
providers and targets refuse such a machine and say why.

Checks come in three kinds, and `isoloom test` runs them all from inside the environment:
**derived** from the spec (every service answers from the machines `reach` lets through and from
nowhere else; offline networks stay offline; without a Linux machine to run them, every service
answers from the controller), **declared** (`http`, `tcp`, `exec`, with `from`,
`expect` and `wait`), and **scripts**.

## Layout

- `crates/`: the format as libraries, split by layer and backend (other Rust tools can embed
  them: see [Using the library](#using-the-library)):
  - `isoloom-spec` (model, parsing, normalization, validation), `isoloom-plan` (the plan and the
    checks), `isoloom-render` (pieces shared by the backends);
  - the backends `isoloom-compose`, `isoloom-kubernetes`, `isoloom-terraform`, `isoloom-vagrant`
    and `isoloom-external`, wired together by `isoloom-targets` (with the resolved snapshot);
  - `isoloom-import`, `isoloom-coverage` and `isoloom-runtime` (registry, host readiness, events,
    report).
- `crates/isoloom`: the command line.
- `examples/`: environments used as tests, from a two-machine app and a segmented network to an
  Active Directory domain, Cisco routers and cloud services. Those with a committed `.isoloom/`
  are checked against the generators in CI.

## Using the library

The crates aren't on crates.io: depend on the ones you use from git, pinned to a release tag
(up to 0.10.1, a single `isoloom-core` crate: see below).

```toml
isoloom-spec = { git = "https://github.com/isoloom/isoloom", tag = "vX.Y.Z" }
isoloom-targets = { git = "https://github.com/isoloom/isoloom", tag = "vX.Y.Z" }
isoloom-runtime = { git = "https://github.com/isoloom/isoloom", tag = "vX.Y.Z" }
```

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
  `isoloom-render`, the backends, `isoloom-import`, `isoloom-coverage`, `isoloom_runtime::report`)
  is internal: public for the command line, and may change in any release. Up to 0.10 the
  `isoloom-core` crate re-exported the supported surface; it is gone, and each item is used from
  the crate that holds it (`isoloom_core::generate` is `isoloom_targets::generate`,
  `isoloom_core::registry` is `isoloom_runtime::registry`, `isoloom_core::checks` is
  `isoloom_plan::checks`).
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

## Status

See the [changelog](CHANGELOG.md) for the current version. Generators: **Docker Compose** (also
for a **hosting service**), **Kubernetes** (manifests, NetworkPolicies for networks and
`reach`), **Vagrant** (VirtualBox, VMware, Parallels, libvirt, Hyper-V, UTM, QEMU, ESXi),
**hybrid** (containers and VMs on the same networks), **Docker on one VM** (Vagrant, Proxmox),
**Proxmox** (one VM per machine, or an LXC container with `docker.lxc`; its own SDN network,
router and firewall, VLANs as tags, `networks.*.mtu` and `machines.*.rate_limit` on each device,
arm64 VMs emulated, everything in a pool), **Docker on one cloud
VM** and **one cloud VM per machine** (AWS, Azure, Google Cloud, DigitalOcean, Linode, Oracle),
**existing hosts** reached over SSH, and **cloud services** (a Terraform module applied into
your own cloud account). Routers enforce `reach` between networks; a network can name its own
`gateway` (an edge firewall); IPv6 networks (dual-stack or IPv6-only); `internet: false`, published ports, volumes, inputs, Windows VMs
(WinRM, PowerShell steps), environment-level Ansible from a controller, and checks on every
target. Your own image table (`--images`) sets OS images and supplies the access machine.

Run for real in CI: Docker (hello-stack, air-gapped, segmented, edge-firewall, slow-link, udp-dns), Kubernetes on kind
(hello-stack, segmented). Run by hand: VirtualBox (several examples, Windows Server 2019); on
AWS, one cloud VM per machine with Ansible provisioning, Docker on a cloud VM, and cloud services
(cloud-bucket and thirteen AWS labs); Docker on a cloud VM on Azure. Generated and validated
only: Proxmox, the other clouds, ESXi. The workflows that run VirtualBox, libvirt, Proxmox and the
six clouds for real are in place. Each one starts once its runner or secrets are provided
([docs/real-targets.md](docs/real-targets.md)).

Already have a Compose file? `isoloom import compose` drafts the `isoloom.yml` from it: what the
format expresses goes into the draft, and every other key is listed with why (belongs in the
image, not expressible yet, by design).

Coverage: everything each format can do (every key of the Compose schema, every setting of
Vagrant and each provider plugin, every resource type of the Terraform Proxmox provider) and
whether Isoloom produces it: [docs/COVERAGE.md](docs/COVERAGE.md) (`isoloom coverage`). Tests fail
on an unclassified entry, or when the tables disagree with the generated files.
