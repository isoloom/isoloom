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
- **Generic.** Nothing is specific to one use: launch-time values reach machines through
  `inputs:`, and `access: true` marks the machine a user lands on.

## Install

```
curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/main/install.sh | sh
```

Release binaries for macOS, Linux (x86_64, arm64) and Windows are on the
[releases page](https://github.com/isoloom/isoloom/releases). In GitHub Actions:

```yaml
- uses: isoloom/isoloom@v0.6.0
- run: isoloom check
```

From source: `cargo install --git https://github.com/isoloom/isoloom isoloom`.

## Commands

```
isoloom validate [DIR]    # mistakes, with the exact field and what to do (--json)
isoloom targets [DIR]     # where it can run, and why not elsewhere
isoloom resources [DIR]   # machines, CPUs, memory, disk
isoloom generate [DIR]    # each target's files under .isoloom/ (--target, --images FILE)
isoloom check [DIR]       # fails when .isoloom/ doesn't match the spec (for CI)
isoloom inspect [PATH]    # the resolved snapshot (.isoloom/resolved.json), or one part of it
isoloom run TARGET [DIR]  # generate, then bring the environment up with the target's own tool
isoloom test TARGET [DIR] # run the checks against it: the spec's, and the ones derived from it
isoloom down TARGET [DIR] # tear it down
```

`DIR` holds `isoloom.yml` (`isoloom.yaml` also works).

Or run what it generates by hand:

```
docker compose -f .isoloom/docker/compose.yml up -d --wait          # containers
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check
cd .isoloom/vagrant && vagrant up                                    # one VM per machine
kubectl kustomize --load-restrictor LoadRestrictionsNone .isoloom/kubernetes | kubectl apply -f -
```

Checks come in three kinds, and `isoloom test` runs them all from inside the environment:
**derived** from the spec (every service answers from the machines `reach` lets through and from
nowhere else; offline networks stay offline), **declared** (`http`, `tcp`, `exec`, with `from`,
`expect` and `wait`), and **scripts**.

## Layout

- `crates/isoloom-core`: the format as a library (parse, validate, derive targets, generators).
  Other Rust tools can embed it directly.
- `crates/isoloom`: the command line.
- `examples/`: three environments used as tests (a two-machine app, a segmented network, an
  Active Directory domain).

## Status

v0.6. Generators: **Docker Compose**, **Kubernetes** (manifests, NetworkPolicies for networks
and `reach`), **Vagrant** (VirtualBox, VMware, Parallels, libvirt, Hyper-V, UTM, QEMU, ESXi),
**Docker on one VM** (Vagrant, Proxmox), **Proxmox** (one VM per Linux machine, its own SDN
network and router) and **Docker on one cloud VM** (AWS, Azure, Google Cloud, DigitalOcean,
Linode, Oracle). Routers enforce `reach` between networks; a network can name its own
`gateway` (an edge firewall); `internet: false`, published ports, volumes, inputs, Windows VMs
(WinRM, PowerShell steps), environment-level Ansible from a controller, and checks on every
target. Your own image table (`--images`) sets OS images and supplies the access machine.

Run for real in CI: Docker (hello-stack, air-gapped, segmented, edge-firewall), Kubernetes on kind
(hello-stack, segmented). Run by hand: VirtualBox (several examples, Windows Server 2019).
Generated and validated only: Proxmox, the clouds, ESXi.

Already have a Compose file? `isoloom import compose` drafts the `isoloom.yml` from it: what the
format expresses goes into the draft, and every other key is listed with why (belongs in the
image, not expressible yet, by design).

Coverage: everything each format can do (every key of the Compose schema, every setting of
Vagrant and each provider plugin, every resource type of the Terraform Proxmox provider) and
whether Isoloom produces it: [docs/COVERAGE.md](docs/COVERAGE.md) (`isoloom coverage`). Tests fail
on an unclassified entry, or when the tables disagree with the generated files.
