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

checks: [build/check/check.sh]   # black-box checks, run against every target
```

- **Targets are derived, not declared.** Every machine has `docker:` → Docker targets; every
  machine has `vm:` → VM targets (Vagrant, Proxmox, cloud). A Windows machine has only `vm:`, so
  that environment never gets a broken Docker file. `targets:` can only narrow the list.
- **Behavior is the contract.** The same checks run on every target; a target whose checks fail
  isn't offered.
- **Generic.** Nothing is specific to one use: launch-time values reach machines through
  `inputs:`, and `access: true` marks the machine a user lands on.

## Commands

```
isoloom validate [DIR]    # mistakes, with the exact field and what to do (--json)
isoloom targets [DIR]     # where it can run, and why not elsewhere
isoloom resources [DIR]   # machines, CPUs, memory, disk
isoloom generate [DIR]    # each target's files under .isoloom/ (--target docker|vagrant)
isoloom check [DIR]       # fails when .isoloom/ doesn't match the spec (for CI)
```

`DIR` holds `isoloom.yml` (`isoloom.yaml` also works).

Run what it generates:

```
docker compose -f .isoloom/docker/compose.yml up -d --wait          # containers
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check
cd .isoloom/vagrant && vagrant up                                    # one VM per machine
```

## Layout

- `crates/isoloom-core`: the format as a library (parse, validate, derive targets, generators).
  Other Rust tools can embed it directly.
- `crates/isoloom`: the command line.
- `examples/`: three environments used as tests (a two-machine app, a segmented network, an
  Active Directory domain).

## Status

v0.5: the format, validation, target derivation, and generators for **Docker Compose** and
**Vagrant** (VirtualBox, VMware, Parallels, libvirt), with **routers**: `reach` rules between
networks are enforced by a router machine (nftables) on both targets, `internet: false` works on
both, and `depends_on` waits for services to answer. **Gateways**: a network can name a machine
of the environment (an edge firewall) as its `gateway`; it routes that network with its own
rules instead of Isoloom's router. `examples/hello-stack`, `examples/segmented` and
`examples/edge-firewall` run for real in CI with their checks. Not yet: Windows and appliance
images (OPNsense), Proxmox and cloud generators, release binaries.

Already have a Compose file? `isoloom import compose` drafts the `isoloom.yml` from it: what the
format expresses goes into the draft, and every other key is listed with why (belongs in the
image, not expressible yet, by design).

Coverage: everything each format can do (every key of the Compose schema, every setting of
Vagrant and each provider plugin, every resource type of the Terraform Proxmox provider) and
whether Isoloom produces it: [docs/COVERAGE.md](docs/COVERAGE.md) (`isoloom coverage`). Tests fail
on an unclassified entry, or when the tables disagree with the generated files.
