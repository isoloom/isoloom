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
  that environment never gets a broken Docker file. `targets:` can only narrow the list.
- **Behavior is the contract.** The same checks run on every target; a target whose checks fail
  isn't offered.
- **Say it once.** `common:` and `groups:` hold what several machines share, and `count: 5` makes
  five of a machine; the spec stays short when a fleet gets the same box, size and playbook.
- **Generic.** Nothing is specific to one use: launch-time values reach machines through
  `inputs:`, and `access: true` marks the machine a user lands on.

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

`DIR` holds `isoloom.yml` (`isoloom.yaml` also works). VMs get a copy of the project in
`/opt/isoloom`; an `.isoloomignore` next to `isoloom.yml` (gitignore-style) lists what they don't
need, on every target that copies it.

Or run what it generates by hand:

```
docker compose -f .isoloom/docker/compose.yml up -d --wait          # containers
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check
cd .isoloom/vagrant && vagrant up                                    # one VM per machine
kubectl kustomize --load-restrictor LoadRestrictionsNone .isoloom/kubernetes | kubectl apply -f -
```

On Windows with **Hyper-V** (`vagrant up --provider hyperv`, or `vagrant: { provider: hyperv }` in
the defaults), run Vagrant from an elevated shell (an administrator, or a member of Hyper-V
Administrators); `isoloom doctor` says whether Hyper-V is on and usable from yours. Each VM's
first adapter is on `ISOLOOM_HYPERV_SWITCH` (default: the Default Switch, which Windows Server
doesn't have); each network becomes a private switch `isoloom-<env>-<network>` (between the VMs
only, removed with the last VM), and each machine's fixed address there is set inside the guest.
Published ports aren't forwarded to the host's loopback on Hyper-V (Vagrant can't): `vagrant up`
warns with the VM's address to use instead, and `isoloom report services` says so when Hyper-V is
the provider (the defaults' `vagrant.provider`, or `VAGRANT_DEFAULT_PROVIDER`). OS names without a Hyper-V box (debian-13, ubuntu-14.04,
fedora-42) need `vm.image.hyperv`.

Checks come in three kinds, and `isoloom test` runs them all from inside the environment:
**derived** from the spec (every service answers from the machines `reach` lets through and from
nowhere else; offline networks stay offline; without a Linux machine to run them, every service
answers from the controller), **declared** (`http`, `tcp`, `exec`, with `from`,
`expect` and `wait`), and **scripts**.

## Layout

- `crates/isoloom-core`: the format as a library (parse, validate, derive targets, generators).
  Other Rust tools can embed it directly (see [Using the library](#using-the-library)).
- `crates/isoloom`: the command line.
- `examples/`: environments used as tests, from a two-machine app and a segmented network to an
  Active Directory domain, Cisco routers and cloud services. Those with a committed `.isoloom/`
  are checked against the generators in CI.

## Using the library

`isoloom-core` isn't on crates.io: depend on it from git, pinned to a release tag.

```toml
isoloom-core = { git = "https://github.com/isoloom/isoloom", tag = "v0.10.1" }
```

- **Supported surface**: the crate root (`load`, `parse`, `validate`, `validate_files`,
  `derive`, `generate`, `refusal`, `refusal_on`, `totals`, the model types) and the modules `model`,
  `validate`, `targets`, `generate` (with `resolved`, the snapshot), `checks`, `instance`,
  `defaults`, `registry` and `host`. Every other public module (`clones`, `groups`, `vlans`,
  `ignore`, `images`, `import`, `coverage`, `report`, `schema`, `shell`) is internal: public for
  the command line, and may change in any release.
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
**Proxmox** (one VM per Linux machine, its own SDN network and router), **Docker on one cloud
VM** and **one cloud VM per machine** (AWS, Azure, Google Cloud, DigitalOcean, Linode, Oracle),
**existing hosts** reached over SSH, and **cloud services** (a Terraform module applied into
your own cloud account). Routers enforce `reach` between networks; a network can name its own
`gateway` (an edge firewall); `internet: false`, published ports, volumes, inputs, Windows VMs
(WinRM, PowerShell steps), environment-level Ansible from a controller, and checks on every
target. Your own image table (`--images`) sets OS images and supplies the access machine.

Run for real in CI: Docker (hello-stack, air-gapped, segmented, edge-firewall, slow-link, udp-dns), Kubernetes on kind
(hello-stack, segmented). Run by hand: VirtualBox (several examples, Windows Server 2019).
Generated and validated only: Proxmox, the clouds, ESXi.

Already have a Compose file? `isoloom import compose` drafts the `isoloom.yml` from it: what the
format expresses goes into the draft, and every other key is listed with why (belongs in the
image, not expressible yet, by design).

Coverage: everything each format can do (every key of the Compose schema, every setting of
Vagrant and each provider plugin, every resource type of the Terraform Proxmox provider) and
whether Isoloom produces it: [docs/COVERAGE.md](docs/COVERAGE.md) (`isoloom coverage`). Tests fail
on an unclassified entry, or when the tables disagree with the generated files.
