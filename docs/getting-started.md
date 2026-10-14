# Getting started

This page takes you from nothing to a running environment and back: install Isoloom, write a
first spec, bring it up on Docker, run its checks, and tear it down. Then it points to where to
go next.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/v1.0.0/install.sh | ISOLOOM_VERSION=v1.0.0 sh
```

The installer checks the binary's SHA-256 sum and puts it in `~/.local/bin`; leave
`ISOLOOM_VERSION` out for the latest release. The 1.0.0 binaries for macOS, Linux (x86_64,
arm64) and Windows, with their checksums, are on the
[release page](https://github.com/isoloom/isoloom/releases/tag/v1.0.0). From source:
`cargo install --git https://github.com/isoloom/isoloom --tag v1.0.0 isoloom`. In GitHub Actions:

```yaml
- uses: isoloom/isoloom@v1.0.0
- run: isoloom check
```

`isoloom doctor` tells you what this machine can run: the tools and credentials it found, and
those it didn't.

## A first spec

Make a folder with an `isoloom.yml`. This one is two machines on one network: a web server
built from the project, and a Redis cache it depends on. Each machine says how it runs as a
container (`docker:`) and as a VM (`vm:`), so the environment can run on both kinds of target.

```yaml
# isoloom.yml
# yaml-language-server: $schema=https://www.isoloom.com/schema/v1.json
version: 1
name: first-stack

networks:
  app: { cidr: 10.60.0.0/24, internet: false }

machines:
  cache:
    networks: { app: 20 }
    services: [{ port: 6379, name: redis }]
    docker: { image: "redis:7-alpine" }
    vm: { os: debian-12, provision: [provision/cache.sh] }
  web:
    networks: { app: 10 }
    services: [{ port: 80, http: true, publish: 8080 }]
    depends_on: [cache]
    docker: { build: build/web }
    vm: { os: debian-12, provision: [provision/web.sh] }

checks:
  - name: the page greets
    http: http://web/
    expect: 200
  - { from: web, tcp: "cache:6379" }
```

The second line points editors that use the YAML language server (VS Code, Neovim, JetBrains)
at the schema: completion, hover documentation and errors as you type. The
[spec reference](spec.md) has every field. `build/web` is a folder with a Dockerfile, and the
two `provision` scripts install the same services on a VM. The
[hello-stack example](../examples/hello-stack) is this environment with every file in place.

Check it:

```sh
isoloom validate        # mistakes, each with its exact field and what to do
isoloom targets         # where it can run, and why not elsewhere
isoloom targets --host  # and whether this machine can run each target now
```

`isoloom targets` derives the targets from the spec: every machine has `docker:`, so the
container targets apply; every machine has `vm:`, so the VM targets do too. A `targets:` list in
the spec can only narrow that.

## Run, test, down

```sh
isoloom run docker      # generate .isoloom/docker/, then docker compose up --wait
isoloom test docker     # the checks, from inside the environment
isoloom down docker     # tear it down, volumes included
```

`isoloom run` writes each target's files under `.isoloom/<target>/` (also `isoloom generate`
on its own) and brings the environment up with the target's own tool. `isoloom test` runs
three kinds of checks:

- **declared** in the spec (`http`, `tcp`, `exec`, `script`, with `from`, `expect` and `wait`);
- **scripts** in the project;
- **derived** from the spec: every service answers from the machines `reach` lets through and
  from nowhere else, and an offline network stays offline.

While it runs, `isoloom status` lists what is up on this host, `isoloom connect web` opens a
shell on a machine, `isoloom exec web -- uname -a` runs a command, and `isoloom message` prints
the spec's `message` with its placeholders filled.

The same three commands run the same environment on another target: `isoloom run vagrant`
brings up one VM per machine, `isoloom run kubernetes` applies the manifests to the current
kubectl context. Each target and what it needs are in [Targets](targets.md).

## Commit the generated files

The files under `.isoloom/` are plain Compose files, manifests, Vagrantfiles and Terraform
modules: you can read them, run them by hand and commit them. `isoloom check` fails when they
no longer match the spec, which is what CI runs.

VMs get a copy of the project in `/opt/isoloom`. An `.isoloomignore` next to `isoloom.yml`
(gitignore-style) lists what they don't need, on every target that copies it.

## Profiles

Every command takes `--profile NAME` (or `ISOLOOM_PROFILE`): `profiles/<name>.yml`, what
differs for one customer or site. It can pin the target, change spec values (`spec:`, merged in
before `-s`), add a layer of defaults, and give inputs their values (the environment still
wins):

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

`isoloom profiles` lists the project's profiles and checks each against the spec.

## Already have a Compose file?

`isoloom import compose` drafts the `isoloom.yml` from it: what the format expresses goes into
the draft, and every other key is listed with why (it belongs in the image, it isn't
expressible yet, or it is left out by design). Vagrant and Terraform files can be imported the
same way.

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
isoloom status            # what is up on this host, and whether it still runs
isoloom connect MACHINE   # a shell on a machine (compose exec, vagrant ssh, kubectl exec, ssh)
isoloom exec MACHINE -- … # a command on one machine, or `all`
isoloom capture MACHINE NETWORK   # tcpdump on its interface there
isoloom down TARGET [DIR] # tear it down
isoloom schema            # the JSON schema of isoloom.yml, for editors
isoloom coverage          # what each output format can do, and what Isoloom produces
isoloom docs --spec       # the spec reference (docs/spec.md), generated from the code
```

## Where to go next

- [Targets](targets.md): every target, what it builds, what the host needs, its limits.
- [Spec reference](spec.md): every field, its type and default, and which targets render or
  refuse it.
- [Secrets](secrets.md): passwords and keys from a store, delivered as files, never written in
  a generated file.
- [Storage](storage.md): volumes that are shared, kept or existing, `tmpfs`, and data disks.
- [Coverage](COVERAGE.md): everything each output format can do and whether Isoloom
  produces it.
- [JSON output](json.md) and [the registry](registry.md), for tools that drive Isoloom.
- The [examples](../examples): from a two-machine app and a segmented network to an Active
  Directory domain, Cisco routers and cloud services.
