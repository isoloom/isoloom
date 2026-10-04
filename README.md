# Isoform

Describe an environment once (its machines, networks, who reaches whom, the services they
expose) and run it anywhere: containers on your machine, local VMs, a Proxmox server, or the
cloud. Each target produces **the same external behavior** its own way, like isoforms of a
protein: different forms, same function.

```yaml
# isoform.yaml
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
isoform validate [DIR]    # mistakes, with the exact field and what to do (--json)
isoform targets [DIR]     # where it can run, and why not elsewhere
isoform resources [DIR]   # machines, CPUs, memory, disk
isoform generate [DIR]    # each target's files (coming next)
```

`DIR` holds `isoform.yaml` (or `.ctf/range.yaml` for Cyber CTF labs).

## Layout

- `crates/isoform-core`: the format as a library (parse, validate, derive targets, generators).
  Tools embed it directly, e.g. the Cyber CTF launcher.
- `crates/isoform`: the command line.
- `examples/`: three environments used as tests (a two-machine app, a segmented network, an
  Active Directory domain).

## Status

v0.1: format, validation, target derivation. Next: Docker Compose and Vagrant generators, then
Proxmox, a CI action, and release binaries (macOS, Linux, Windows).
