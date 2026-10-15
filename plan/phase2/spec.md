# The spec, version 1: one declaration per machine

The syntax design for Phase 2's front end (PLAN.md §2 "Syntax", "Compatibility: none"; Phase 2 step 1). It replaces today's `docker:`/`vm:` machine blocks with one declaration that says *what* a machine is. The run manifest (`run-manifest-v0.md`) is the other half of Phase 2's contract; §8 lists what this design changes there.

**Inputs:** PLAN.md; the earlier catalogue design (`os`, `service`, `with`); today's spec model (`crates/isoloom-spec/src/model*`) and plan IR (`crates/isoloom-plan/src/environment.rs`); the feature counts of the 376 CTF lab specs (private, counts only: no lab content is used here). The examples in `spec-examples/` are made up.

**Status:** design. Nothing here is implemented. Open questions are in §12, each with a recommendation.

---

## 1. Decisions at a glance

| # | Decision | Why |
|---|---|---|
| 1 | A machine is `os` plus **one** of `service:` (catalogue) or `app:` (custom). `os` alone is a bare machine. | The machine says what it is; each target decides how to deliver it. |
| 2 | No `docker:`, `vm:` or `external:` blocks. Platform specifics live in a top-level `overrides:`, keyed by target. | One declaration; the rare platform fact stays visible and checkable. |
| 3 | `app:` is one recipe. `app.steps` (the configuration resource model) is portable: Isoloom renders it as a Dockerfile **and** as VM set-up. `app.image` and `app.build` (your Dockerfile) are container-only recipes. | 85% of corpus containers build from a folder, so the Dockerfile stays first-class; portability comes from steps, never from translating a Dockerfile. |
| 4 | A run-time value becomes a file through `files:` (`input:` or `secret:` as the file's source). Machine `inputs:` and `secrets:` go. A sensitive input can never be an environment variable. | One way to place a file, whatever its source; the flag never shows in `docker inspect`. See §6. |
| 5 | `ports:` is a map keyed by a **name**. Messages reference `machines.<m>.ports.<name>.published`. | Indexes (`services.0.publish`, 371 messages) break when a list is reordered; names don't. |
| 6 | One port `protocol` enum (`tcp`, `udp`, `http`, `https`, `tls`) replaces `http`/`tls`/`protocol`. One `tpm` enum replaces `Option<bool>`. | Parse, don't validate: no invalid combinations (`udp` + `http`). |
| 7 | `setup:` (scripts run once the machine is ready) replaces `docker.init`. | The same contract on both forms. |
| 8 | Lowering, not rewriting: the new syntax lowers into today's model, which becomes an internal type, and `Environment::build` stays as it is. | The backends and their 800+ tests keep guarding the output. |
| 9 | Errors carry a code, the field path, the line, the reason and the fix. An old-format key gets a targeted fix, never an alias. | "Compatibility: none" without leaving authors stranded. |
| 10 | `version: 1` stays. | PLAN §2. |

---

## 2. The spec's top level

| Field | Type | Default | Change from today |
|---|---|---|---|
| `version` | `1` | required | kept |
| `name` | kebab-case | required | kept |
| `networks` | map name → network | `{}` | kept; `docker.cidr` → `overrides.docker.networks`; `external` → `overrides.<target>.networks` |
| `reach` | list of `{ from, to, ports }` | `[]` | kept |
| `dns` | `{ private, domain }` | off | kept |
| `inputs` | list of names, or map name → input (§3.4) | `{}` | **changed**: each input says whether it is sensitive |
| `secrets` | map name → `{ env \| file \| vault \| sops }` | `{}` | kept |
| `volumes` | map name → `{ owner, persistent }` | `{}` | kept; `external` → `overrides.<target>.volumes`; `virtiofs` → `overrides.proxmox.volumes` |
| `machines` | map name → machine (§3) | `{}` | **changed** |
| `common`, `groups` | shared machine fields | none | kept; they share any machine field except `networks`, `ports`, `access`, `count` and `aliases` |
| `provision` | list of `{ ansible, inventory, groups, vars, host_vars, requirements }` | `[]` | kept: the environment's own Ansible, run from Isoloom's Linux controller |
| `controller` | `{ resources, keep_running }` | 1 CPU, 512 MB | kept; `image` → `overrides.vagrant.controller` |
| `checks` | list (§3.7) | `[]` | kept; `wait` takes a duration |
| `message` | Markdown with `{{ path }}` | none | **changed**: named port paths, checked at validation |
| `overrides` | map target → overrides (§3.8) | `{}` | **new** |
| `tools`, `targets`, `constraints`, `cloud` | as today | | kept |

---

## 3. The machine

Every field, grouped. "VM only" or "container only" means the field rules out the other form, and the target that needs that form refuses the machine with a reason (§9.1).

### 3.1 What it is

| Field | Type | Default | Validation | Example |
|---|---|---|---|---|
| `os` | `family@version`, or `family` (its pinned default, recorded in `resolved.json`) | required, except for a machine whose only recipe is `app.image`, `app.build` or `app.appliance`, and for an `access` machine the runner supplies | a known family and version; the error lists them. Windows has no container form in 1.0 | `os: debian@12` |
| `service` | `name[@version]`, or a list of them | none | not with `app`; the entry exists, lists the version and supports `os`; a list has no container form (one process per container) | `service: postgres@16` |
| `with` | map | `{}` | only with `service`; keys and types from the entry's settings schema; a value may be `{ secret: name }` or `{ input: NAME }` where the setting allows it; with a list of services, keyed by service | `with: { database: books }` |
| `app` | §4 | none | not with `service` | `app: { build: build/web }` |

**Forms, derived:**

| The machine has | Container form | VM form |
|---|---|---|
| `os` only | the OS's official image, pinned by digest, kept running idle | a fresh VM of `os` (the image table's box, template or cloud image) |
| `os` + `service` | the entry's container form for that OS: its official image, or a build from the OS image | a fresh VM + the entry's role |
| `os` + `app.steps` | a Dockerfile Isoloom generates: `FROM` the OS image, then the steps | a fresh VM + the steps, rendered by the chosen CM tool |
| `app.image`, `app.build` or `app.appliance` only | that image or build | none |
| one of those + `os` + `app.steps` | that image or build | a fresh VM + the steps |
| a Windows `os` | none in 1.0 (dockur is 1.x) | as above |
| `access: true`, no `os` | supplied by the runner | supplied by the runner |

### 3.2 Networks, addresses, ports

| Field | Type | Default | Validation | Example |
|---|---|---|---|---|
| `networks` | map network → last octet, or a list of octets (the first is primary) | required, except a LAN's `switch` appliance | the network exists; the address is inside the block and isn't the host's or the router's; unique per network | `networks: { lab: 10, mgmt: [20, 21] }` |
| `aliases` | list of DNS names | `[]` | valid names; unique in the environment | `aliases: [www.shop.test]` |
| `count` | 2 to 99 | 1 | clones `<name>-01…`, one address further on each network | `count: 2` |
| `access` | bool | `false` | at most one machine | `access: true` |
| `ports` | map **name** → port, or name → port number (TCP) | `{}`; a service machine gets its entry's ports | names match `[a-z][a-z0-9-]{0,30}` and are unique per machine; a TCP and a UDP port may share a number, never a name | `ports: { site: { port: 8000, protocol: http } }` |

**A port:**

| Field | Type | Default | Validation |
|---|---|---|---|
| `port` | 1 to 65535 | required; from the entry on a service machine | a service's port comes from its settings (`with.port`), never from here |
| `protocol` | `tcp`, `udp`, `http`, `https`, `tls` | `tcp` | `http`/`https`/`tls` are TCP with a probe and checks that speak them |
| `publish` | `true` (a free host port, preferably the same number), a number (the preferred host port), or `{ host, fixed: true }` (that port or nothing) | not published | a fixed host port is unique in the environment |
| `dormant` | bool | `false` | nothing waits for it (a bind shell an exploit opens) |

**Stable paths.** A message or a check writes `{{ machines.<m>.ports.<name>.port }}`, `.published` (the host port it got, at run time) or `.url` (`http(s)://<host>:<published>/`, for `http` and `https` ports), and `{{ machines.<m>.addresses.<network> }}`. Renaming a port breaks only the messages that name it, and validation reports them.

Networks keep every field they have today (`cidr`, `internet` (default `true`), `gateway`, `vlans`, `switch`, `tc`, `ipv6`, `bridged`, `mtu`), except the two platform names that move to `overrides:`.

### 3.3 Process and readiness

| Field | Type | Default | Validation | Example |
|---|---|---|---|---|
| `command` | a text (run by the shell), a list (run as is), or `{ run, cwd }` | the image's or the entry's own; an `os`-only or steps-only machine stays up idle | not with `service` (the entry owns the process); not empty; `cwd` absolute | `command: [python3, app.py]` |
| `setup` | list of project scripts | `[]` | the files exist; `.sh`, or `.ps1` on Windows | `setup: [db/seed.sh]` |
| `healthcheck` | `{ test \| http: { port, path }, interval, timeout, retries, start_period }` | none (the ports are probed) | exactly one of `test` and `http`; `port` is a declared port's name or number | `healthcheck: { http: { port: site, path: /health } }` |
| `depends_on` | list of machine names | `[]` | declared; no cycle | `depends_on: [db]` |

A machine is **ready** when its TCP ports that aren't dormant answer, its healthcheck passes, and its `setup` scripts have run. `depends_on` waits for exactly that.

**`setup`** runs each script once, in order, after the machine is up, as root, with its network, volumes, files (inputs included) and privileges. Container form: a one-shot job of the machine's image in its network namespace, with its volumes, files and capabilities (today's `docker.init`, plus the files and capabilities). VM form: a step inside the VM after the command started. Contract: a setup script reaches the machine through its network, volumes and files; on the container form it doesn't see the main container's own filesystem (open question 4).

`before` and `working_dir` go: `files` from inputs and `setup` cover `before`'s purpose (a per-run value written where the app reads it), and `cwd` moves into `command`.

### 3.4 Inputs, secrets, files, volumes, disks

**An input** (top level) is a value the launcher, CI or the CLI provides at each start. `inputs: [FLAG]` is short for `inputs: { FLAG: {} }`.

| Field | Type | Default | Validation |
|---|---|---|---|
| `sensitive` | bool | `true` | a sensitive input may be a file's source or a `with:` value, never an `environment` value |
| `description` | text | none | shown when a runner asks for it |

Input names are environment variable names, as today (letters, digits and `_`, not starting with a digit), and don't start with `ISOLOOM_`. Secrets keep today's form: a kebab-case name and the store it comes from.

**Machine fields:**

| Field | Type | Default | Validation | Example |
|---|---|---|---|---|
| `environment` | map NAME → a text, number or bool, or `{ input: NAME }` | `{}` | the input exists and is `sensitive: false`; `{ secret: … }` is refused (secrets are files) | `environment: { API_URL: { input: API_URL } }` |
| `files` | list of files (below) | `[]` | paths unique per machine | |
| `volumes` | map name → absolute path, or `{ path, subpath }` | `{}`; a service machine gets its entry's data volumes | `subpath` only on a top-level (shared) volume | `volumes: { data: /var/lib/app }` |
| `disks` | map name → `{ size_gb, fs, mount }` | `{}` | name kebab-case, at most 12 characters; 1 to 16384 GB; `fs` `ext4` (Linux default), `xfs`, `ntfs` (Windows default), `raw`; `mount` absolute on Linux, a drive letter on Windows, none for `raw` | `disks: { data: { size_gb: 50, mount: /srv/data } }` |
| `tmpfs` | list of paths or `{ path, size, mode }` | `[]` | as today | `tmpfs: [/run]` |

**A file:**

| Field | Type | Default | Validation |
|---|---|---|---|
| `path` | absolute path; a drive path (`C:\…`) on Windows | required | |
| `source` \| `content` \| `input` \| `secret` | a project file, inline text, an input's name, a secret's name | required | exactly one; the input or secret is declared |
| `uid`, `gid` | integers | 0 | not on Windows |
| `mode` | octal text (`"0444"`) | `0644` for `source`/`content`, `0400` for `input`/`secret` | at most 0777; not on Windows, where a value file is readable by SYSTEM and Administrators (widen it in the machine's own steps) |

| Source | When it lands | Container form | VM form | In generated files? |
|---|---|---|---|---|
| `source`, `content` | before the process starts | Compose `configs`, a Kubernetes ConfigMap; in a generated image, a `COPY` | a set-up step | yes: it is configuration |
| `input`, `secret` | at each start, before the process | a Compose secret read from the runner's environment; a Kubernetes Secret the runner creates | delivered by the runner over SSH or WinRM, waited for by the `Secrets` step | **never**: no generated file, image layer, environment variable or `resolved.json` holds the value |

### 3.5 Size, hardware, system

| Field | Type | Default | Validation | Example |
|---|---|---|---|---|
| `resources` | `{ cpus, memory_mb, disk_gb }` | 1, 1024, 20 | at least 1 each | `resources: { cpus: 2, memory_mb: 4096 }` |
| `hardware.arch` | `amd64`, `arm64` | `amd64` | | `hardware: { arch: arm64 }` |
| `hardware.tpm` | `auto`, `required`, `none` | `auto` (Windows 11 gets one where the target can) | `required`: VM only | `hardware: { tpm: required }` |
| `hardware.gpus` | a count, `all`, or `{ count, ids, driver, capabilities, options }` | none | as today | |
| `hardware.devices` | list of host devices | `[]` | container only | |
| `hardware.nested_virtualization` | bool | `false` | VM only | |
| `hardware.boot` | `{ kernel, initrd, cmdline, firmware, order, network, … }` | the image's | VM only | |
| `dns` | `{ servers, search, domain, options }` | the target's resolver | a server is an address or a **machine name** sharing a network (new) | `dns: { servers: [dc01], search: [corp.test] }` |
| `privileged`, `read_only`, `shm_size`, `sysctls`, `ulimits`, `rate_limit` | as today | as today | as today | `sysctls: { net.core.somaxconn: 4096 }` |

### 3.6 Shared fields

`common:` and `groups.<g>` take any machine field except `networks`, `ports`, `access`, `count` and `aliases`. The order is unchanged: common, then groups (most specific wins), then the machine's own. Groups are also Ansible inventory groups and `overrides` keys.

### 3.7 Checks and the message

Checks keep today's shape. One change: `wait` is a duration (`90`, `90s`, `3m`).

| Field | Type | Default | Notes |
|---|---|---|---|
| a text | a project script | | `.sh` from where a user stands; `.yml` (a playbook) from the controller |
| `name` | text | what it does | |
| `from` | machine | the access machine, else the environment's networks | required for `exec` |
| `http` | URL | | `expect`: a status (default 200), `any` or `blocked`; `method`, `headers`, `body`, `contains`, `follow` |
| `tcp` | `host:port` | | `expect`: `open` (default) or `blocked`; `send`, `contains` |
| `exec` | command run inside `from` | | `expect`: text the output contains |
| `script` | project script run from `from` | | exits 0 when the behaviour holds |
| `wait` | duration | 30 s for `http`/`tcp` (not `blocked`), 0 otherwise | the run manifest derives each runner's timeout from it |

**The message** may use the paths of §3.2 and `{{ cloud.outputs.<name> }}`. Every placeholder is resolved against the spec at validation, so a typo or an index (`ports.0`) fails there, not at run time.

### 3.8 `overrides:`

Per-target facts with no portable meaning. Each key has a closed schema; anything else is an error.

| Key | Applies to | Allowed | Lowered to (today's model) |
|---|---|---|---|
| `docker` | every target that runs the Compose file: `docker`, `hosted`, and `docker-vm` and `cloud-docker` (the same file in a VM) | `networks.<n>.cidr`, `networks.<n>.external`, `volumes.<v>.external` | `networks.n.docker.cidr`, `networks.n.external`, `volumes.v.external` |
| `kubernetes` | `kubernetes` | `volumes.<v>.external` (a claim) | `volumes.v.external` |
| `vagrant` | `vagrant`, and the VMs of `hybrid` | `machines.<m or group>.{ box, box_version, box_url, box_server_url, boxes: { qemu, hyperv }, winrm: plaintext\|ssl, winssh }`; `controller.{ box, box_version }` | `vm.image.*`, `controller.image` |
| `proxmox` | `proxmox` | `machines.<m or group>.{ image, lxc }`; `volumes.<v>.virtiofs`; `networks.<n>.external` (a bridge) | `vm.image.proxmox`, `docker.lxc`, `volumes.v.virtiofs`, `networks.n.external` |
| `esxi` | `esxi` | `networks.<n>.external` (a port group) | `networks.n.external` |
| `aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci` | `cloud-vm` on that cloud | `machines.<m or group>.image` | `vm.image.<cloud>` |
| `external` | `external` | `machines.<m>.ssh: { address, user, port, key }` | `external` |

Validation: the key is a known target; the machine or group exists (a machine's own entry wins over its group's); the machine has the form that target uses (an override nothing can apply is an error, never silently ignored).

---

## 4. `app:`: one recipe, two forms

### 4.1 Fields

| Field | Type | Default | Validation | Example |
|---|---|---|---|---|
| `image` | an image reference, by tag or digest | none | not with `build` | `image: example/clamd:1.4` |
| `build` | a project folder holding a `Dockerfile`, or `{ context, dockerfile, target, args }` | none | the folder and file exist; `dockerfile` anywhere in the project (default `<context>/Dockerfile`) | `build: build/web` |
| `steps` | list of steps (4.2) | `[]` | needs `os` | `steps: [{ run: provision/web.sh }]` |
| `appliance` | `cisco-iol`, `cisco-iol-l2`, `cisco-vios`, `cisco-vios-l2`, `cisco-csr1000v`, `cisco-c8000v`, `cisco-dynamips` | none | needs `image`, or `firmware` for `cisco-dynamips` | `appliance: cisco-iol` |
| `config` | a project file | none | only with `appliance` | `config: configs/r1.cfg` |
| `firmware` | a project file | none | only with `cisco-dynamips` | `firmware: images/c7200.bin` |

At least one of `image`, `build`, `steps` and `appliance`. With `image` or `build`, the `steps` are the VM form's recipe only (open question 9).

### 4.2 Steps: the configuration resource model

Steps run in order, as root, from the project copy at `/opt/isoloom` (the same path in both forms). They **install and configure**: a step has no network to the other machines, no inputs or secrets, and no init system, because on the container form it runs at image build time. Anything that needs those belongs in `setup`.

| Step | Container form (generated Dockerfile) | VM form (shell, PowerShell, Ansible or cloud-init) | Phase |
|---|---|---|---|
| `run: <script>`, or `{ run, creates }` | `COPY` the script, `RUN` it | a provisioning step; `creates` skips it when the path exists | 2 |
| `copy: { from, to, owner, mode }` | `COPY --chown --chmod` | a file resource | 2 |
| `packages: [names]` | `RUN` the family's installer (`apt-get`, `apk`, `dnf`), caches cleaned | a package resource | 2 |
| `reboot` | none: the machine has no container form | restarts the VM, then the next step | 2 |
| `playbook: <file>` | none in 1.0 | `ansible-playbook -c local` in the VM | 2 |
| `user`, `group`, `repository`, `template`, `firewall` | Dockerfile instructions | the CM tool's resources | 4 |

A `.ps1` script means Windows, so no container form in 1.0.

### 4.3 What is portable and what is refused

| Recipe | Container form | VM form | Reason |
|---|---|---|---|
| `steps` (`run`, `copy`, `packages`, …) | yes: a generated Dockerfile `FROM` the OS's official image, pinned by digest | yes | One resource list rendered twice, so parity holds by construction. |
| `app.image` | yes | **refused** | An image's filesystem can't be reproduced on a fresh VM. Running Docker inside each VM would hide a second runtime: the `docker-vm` and `cloud-docker` targets already run container labs on a VM. |
| `app.build` (your Dockerfile) | yes | **refused** | A Dockerfile depends on its base image (`FROM php:8-apache`), multi-stage `COPY --from`, BuildKit mounts and the image's own entrypoint. Translating it to a VM is unsound, so Isoloom never tries. Add `os` and `steps` for a VM form. |
| `app.appliance` | yes | **refused** | The vrnetlab and Dynamips packaging exists only as containers. |
| a `reboot` or `playbook` step, a `.ps1` script, a Windows `os` | **refused** | yes | A container has no kernel or boot of its own; Windows containers aren't a 1.0 target. |
| `setup` | yes: a one-shot job on the machine's network, with its volumes and files | yes: inside the VM | Same contract (3.3). |
| `command` | `CMD` | a service (systemd, OpenRC, a Windows scheduled task), restarted on failure | |

**How today's blocks become one recipe:**

| Today | New |
|---|---|
| `docker.image` | `app.image` |
| `docker.build`, `dockerfile`, `target`, `args` | `app.build` |
| `docker.init` writing a flag into a volume | a `files` entry with `input:` (§6); the volume goes |
| other `docker.init` | `setup` |
| `docker.idle` on a stock OS image | `os` alone |
| `docker.appliance`, `config`, `firmware` | `app.appliance`, `app.config`, `app.firmware` |
| `docker.lxc` | `overrides.proxmox.machines.<m>.lxc` |
| `vm.os` | `os` |
| `vm.provision` | `app.steps` (`run`, `reboot`, `playbook`): portable when the scripts only install and configure |
| `vm.image.*` (`vagrant`, `qemu`, `hyperv`, `winrm`, `proxmox`, clouds) | `overrides.<target>.machines.<m>` |
| `vm.tpm`, `vm.boot` | `hardware.tpm`, `hardware.boot` |
| `external` | `overrides.external.machines.<m>.ssh` |

**Generated container build:** `.isoloom/docker/build/<machine>/Dockerfile`, with the project folder as its context and an include list of the paths the steps reference. It is a generated file: snapshot-tested and run through `hadolint` in `verify`. The OS table gains each version's official image and digest (`debian:12@sha256:…`).

---

## 5. Defaults chosen once

| Choice | Value | Why |
|---|---|---|
| `internet` | `true` | Isoloom is a generic converter; a lab says `internet: false` in one line. |
| A value file's mode | `0400`, owner root, on every form | Secure by default and the same everywhere. The 304 corpus flag scripts that `chmod 444` state `mode: "0444"` instead. |
| A port's `publish` number | preferred, not fixed | Labs never collide; `fixed: true` is explicit. |
| `os` without a version | the family's pinned default, recorded in `resolved.json` | An environment never changes under its author. |

---

## 6. The flag pattern: a value delivered as a file

**Today**, 330 labs repeat three constructs: an input (`CTF_FLAG_MAIN`), an init script (`ctf/flag.sh`), and a named volume. 267 of those scripts write the value to `/ctf/flag`, and 304 make it world-readable (`chmod 444`). The value also sits in the long-running container's environment, so `docker inspect` shows it. The init job runs only once the app is ready, after the app has started, and 105 labs carry an entrypoint wrapper that reads `/ctf/flag` when the container starts.

**The need, stated generically:** a value given at launch must appear as a file at a path in one or more machines, with an owner and a mode, before the process starts, never in the environment, an image layer or a generated file.

### Option A: delivery on the input

```yaml
inputs:
  CTF_FLAG_MAIN:
    file: { path: /ctf/flag, mode: "0444" }
    machines: [web]
```

- **For:** one place for one input; reads well for one flag on one machine.
- **Against:** a second way to place a file beside `files:`; one path per input, so two machines with two paths need a list of placements (a second `files:`); reading a machine no longer shows what lands in it; secrets need the same again.

### Option B: a value source on `files:` (recommended)

```yaml
inputs: [CTF_FLAG_MAIN]
machines:
  web:
    files:
      - { path: /ctf/flag, input: CTF_FLAG_MAIN, mode: "0444" }
```

- **For:** one way to place a file, whatever its source (`source`, `content`, `input`, `secret`), with one set of owner and mode rules. Per-machine paths come free. The machine's declaration is complete. Machine `secrets:` and machine `inputs:` both go. It lowers onto the secret-file path every backend already has (Compose secrets, Kubernetes Secrets, the VM `Secrets` step), so the IR change is small (§7.3). Several flags, several machines and Windows paths need nothing new.
- **Against:** `files:` mixes configuration (in generated files) and run-time values (never in them); the lowering splits them by source, and §3.4 documents it. One line longer than A for the one-machine case.

### Option C: an input as a secret store

```yaml
secrets:
  flag: { input: CTF_FLAG_MAIN }
machines:
  web:
    secrets: [{ source: flag, target: /ctf/flag, mode: "0444" }]
```

- **For:** no new delivery machinery at all.
- **Against:** two names for one value; every input becomes a "secret", the non-sensitive ones too; a machine keeps two file-placing fields (`files`, `secrets`).

### Recommendation: B

On the `docker` target, B lowers to:

```yaml
secrets:
  input-ctf-flag-main: { environment: CTF_FLAG_MAIN }   # read from the runner's environment by `up`
services:
  web:
    secrets:
      - { source: input-ctf-flag-main, target: /ctf/flag, mode: 0444 }
```

- Neither `Config.Env` nor a host bind mount holds the value, so `docker inspect` shows nothing.
- The file is there before the process starts, so an entrypoint wrapper can rely on it, or go.
- No init job and no volume. `resume` (`compose start`) keeps the file without the value again (manifest §7.3).
- On VMs, the runner's guest delivery writes it. That also fixes today's bug where a Windows VM never gets its inputs (manifest §3.6).
- **Flags that go into a database** (about 20 labs write through `mysql`, `psql`, `mongo` or `sqlite`): the same file, plus a `setup` script that reads it. The value stays out of the environment there too (example `bookshelf`).

---

## 7. Lowering into today's plan IR

### 7.1 Where it happens

```
isoloom.yml ──parse──► syntax::Spec ──validate──► problems (code, path, span, reason, fix)
                            │
                            └─lower(&syntax::Spec, &Catalogue, Target)──► model::Spec (today's types)
                                                                            │
                                                                            └─Environment::build ──► backends (unchanged)
```

- **`isoloom-spec`:**
  - `syntax`, the new types: parsing, the generated JSON Schema, and the checks that need only the spec.
  - `model`, today's types: they become the **lowered form**. Users no longer write them, and they stay the input of `Environment::build`, `env.spec()` (about 120 backend reads today) and `resolved.json`.
- **`isoloom-catalogue`:** the entries (Phase 4). Phase 2 ships the lookup type with no entries, so `service:` parses and is refused with "no entry yet".
- **`isoloom-plan::lower`:** a pure function, run once per target, because the overrides and the form a machine takes depend on the target. It resolves `os` and `service`, keeps the form the target uses (both on `hybrid`), and fills `docker:` or `vm:` of the lowered machine. Then `Environment::build` runs as it does today, and `isoloom targets` lowers once per target to list the refusals.
- **Later:** fields move from `model` into `Environment` proper as backends stop reading `env.spec()`. `model` shrinks and finally goes.

### 7.2 Field by field

| New field | Lowered into (`model`) | IR (`Environment`, `MachinePlan`) | Gap |
|---|---|---|---|
| `os` | `Machine.os`; `vm.os` when there is a VM form | `MachinePlan.os`, `Image::Vm { os }` | none |
| `os` alone, container form | `docker: { image: <OS image@digest> }`, idle entrypoint | `Image::Container` | the OS table's container images (7.3, item 3) |
| `service` + `with` | from the entry: `docker.image` or a generated `docker.build`; `vm.os` + a role step; `environment`, `files`, `secrets`, `ports`, `volumes`, `healthcheck` from its mappings | as today, plus `origin` | items 5 and 8 |
| `app.image` | `docker.image` | `Image::Container` | none |
| `app.build` | `docker.build`, `dockerfile`, `target`, `args` | `Build` | none |
| `app.steps`, container form | `docker.build` → the generated Dockerfile | `Build` | item 3 |
| `app.steps`, VM form | `vm.provision` (Phase 2: `run`, `reboot`, `playbook` map one to one) | `Step::Run`, `Step::Reboot` | `copy` and `packages` need item 4 |
| `app.appliance`, `config`, `firmware` | `docker.appliance`, `config`, `firmware` | read from `env.spec()` as today | none |
| `setup` | `docker.init` | Compose init jobs | the job's files and capabilities, and the VM form: item 6 |
| `command` (with `cwd`) | `command`, `working_dir` | `Process` | none |
| `healthcheck` (port by name) | `healthcheck` (the number) | `Process.health` | none |
| `ports` (map) | `ports: Vec<Port>`: `name` = the key, `http`/`tls`/`protocol` from the enum, `publish`/`fixed` from `publish` | `PublishedPort` | item 2 |
| `environment`, literal | `environment` | `Settings.environment` | none |
| `environment`, `{ input }` | `inputs: [NAME]` when the variable has the input's name | Compose `${NAME:-}` | another name: item 7 |
| `files`, `source`/`content` | `files` | `MachinePlan.files` | none |
| `files`, `input`/`secret` | `secrets` mounts (`target`, `uid`, `gid`, `mode`); an input becomes a lowered secret `input-<name>` with the new source `SecretSource::Input` | `SecretFile` | item 1 |
| `volumes`, `disks`, `tmpfs`, `resources` | same fields | `Volume`, `DataDisk`, `Size` | none |
| `hardware.arch`, `gpus`, `devices`, `nested_virtualization` | `arch`, `gpus`, `devices`, `nested_virtualization` | `arch`, `Hardware` | none |
| `hardware.tpm`, `boot` | `vm.tpm` (`required` → `Some(true)`, `none` → `Some(false)`), `vm.boot` | `tpm`, `boot` | none |
| `dns` (machine names) | `dns` with addresses | | item 9 |
| `inputs` (map) | `inputs: Vec<String>` | | `sensitive`: item 7 |
| `overrides.*` | their lowered fields (§3.8), for the target being lowered | `vm_image`, `winrm`, `shape` (LXC), `NetworkPlan.cidr`, `external` | none |

### 7.3 What the IR must gain

1. **A value source on secret files.** `SecretFile.source: ValueSource::{ Secret(name), Input(name) }` and a lowered `SecretSource::Input`. Compose writes `environment: <INPUT>` for it. The VM `Secrets` step and the runner's guest delivery cover inputs as well as secrets. Phase 2 for `docker`, Phase 3 for the others.
2. **Port names.** A `PortName` newtype; `PublishedPort` gains `name`; `port_label` becomes the name, always. A `PortKind` enum (`tcp`, `udp`, `http`, `https`, `tls`) replaces the booleans in the plan. Phase 2.
3. **Generated builds.** `Build` becomes `Build::Own { context, dockerfile, target, args }` or `Build::Generated { base: PinnedImage, steps: Recipe }`. A Dockerfile renderer goes in `isoloom-render`, and the OS table gains each version's official image and digest. Phase 2 for `run`, `copy` and `packages`.
4. **Configuration steps on VMs.** `Step::Configure(Recipe)`, rendered by `isoloom-render`'s `steps` (shell, PowerShell) in Phase 2, and by the Ansible and cloud-init backends in Phase 4. Catalogue roles add `Step::Role { entry, vars }`.
5. **Refusals with the new field path.** A machine with no form for the target gets a `Refusal` from lowering that names the new field (`machines.web.app.build`), where today it silently becomes `Supplied` on a container target (`environment.rs` `shape()`); only an `access` machine may be supplied now. Lowering keeps one form per target, so `shape()` keeps working, and on Proxmox the `lxc` override keeps the container form even when the machine has a VM form (today's validation refuses `lxc` beside `vm:`).
6. **Setup on both forms.** `Step::Setup { paths }` after `Step::Command` and before `Step::Ready`, so a VM runs `setup` with its service up, as the container form does. The Compose setup job runs as root and gains the machine's files (its value files included), `privileged` and a gateway's `NET_ADMIN`.
7. **Inputs as data.** `Environment.inputs: IndexMap<InputName, InputPlan { sensitive }>` feeds the manifest's slots. `Settings.environment` values become `EnvValue::{ Text, Input(name) }`.
8. **Origin.** `MachinePlan.origin: Origin::{ Os, Service { name, version, form: Image | Built }, App(kind) }`, written to `resolved.json` and the manifest.
9. **DNS by machine name.** The plan resolves `dns.servers` names to addresses on a shared network.
10. **Spans.** A `FieldPath → (line, column)` index built from the YAML events at load, so every problem, lowering's included, points at its line.

---

## 8. What changes in the run manifest v0

- **`published` keys** become the port's name: `${published.web.site}`, not `${published.web.80}`. Names are stable; numbers clash for TCP and UDP.
- **A file-delivered input** is a slot with `env` delivery to `up` only. The flag's `job-web-init-1` step and its ready condition go.
- **Minimum Compose version** in `tools[]`: the one that reads secrets from the environment, which Isoloom's `secrets` already rely on.

---

## 9. Validation errors

### 9.1 The format

```
error[removed-field] machines.web.docker  (isoloom.yml:12:5)
  a machine says what it runs, never how a platform runs it, so `docker:` is gone
  fix: `app: { build: build/web }` for your Dockerfile, or `app: { image: <ref> }`; a `docker.init` flag script becomes `files: [{ path, input }]`
```

- **Line 1:** `error[<code>]`, the field path, then the file, line and column.
- **Line 2:** the reason.
- **Line 3:** the fix, written as the YAML to use where there is one.
- **Order:** all problems are reported at once, in file order.
- **JSON:** `isoloom validate --format json` prints `[{ code, path, line, column, reason, fix }]` for the launcher and editors.
- **Codes:** a stable kebab-case enum: `unknown-field`, `removed-field`, `missing-field`, `invalid-value`, `conflict`, `unknown-reference`, `sensitive-value`, `refused`.
- **Types:** `Problem { code: Code, path: FieldPath, span: Option<Span>, reason, fix: Option<String> }`, and one `thiserror` enum per crate.

**A target's refusal** uses the same three lines, prefixed by the target: `vagrant: error[refused] machines.web.app.build …`. It is not a spec error: `isoloom validate` passes, and `isoloom targets` and `generate` report it.

**Old keys:** a raw pass over the YAML recognises today's keys and gives the targeted fix before the typed parse. It reads nothing as the new form. The table is one module, deleted once the CTF labs are rewritten (Phase 7).

### 9.2 Common mistakes

| Mistake | Path | Reason | Fix |
|---|---|---|---|
| Old `docker:` block | `machines.web.docker` | `removed-field`: a machine says what it runs, not how a platform runs it | `app: { build: … }` or `app: { image: … }`; `init` → `setup`, or `files` from an input |
| Old `vm:` block | `machines.dc01.vm` | `removed-field` | `os: windows-server@2019`; `provision` → `app.steps: [{ run: … }]`; `image.vagrant` → `overrides.vagrant.machines.dc01.box` |
| Old `services:` | `machines.web.services` | `removed-field`: ports are `ports:`, keyed by name | `ports: { web: { port: 80, protocol: http, publish: 8080 } }` |
| `ports` as a list | `machines.web.ports` | `invalid-value`: a map of named ports | `ports: { web: 80 }` |
| Machine `inputs:` | `machines.web.inputs` | `removed-field`: an input reaches a machine as a file or an environment value | `files: [{ path: /ctf/flag, input: CTF_FLAG_MAIN }]` |
| Sensitive input in `environment` | `machines.web.environment.FLAG` | `sensitive-value`: `FLAG` is sensitive and would show in `docker inspect` | `files: [{ path: …, input: FLAG }]`, or `inputs: { FLAG: { sensitive: false } }` |
| A secret in `environment` | `machines.db.environment.PASSWORD` | `sensitive-value`: secrets are files | `files: [{ path: /run/secrets/db, secret: db-password }]` |
| `service` and `app` together | `machines.web.app` | `conflict`: a machine is a catalogue service or a custom app | keep one; content for a service goes in `with:` or `files:` |
| Unknown OS version | `machines.web.os` | `invalid-value`: `debian@10` isn't known; known: `debian@11`, `debian@12`, `debian@13` | `os: debian@12` |
| A setting the entry lacks | `machines.db.with.dbname` | `unknown-field`: `postgres` has no `dbname`; did you mean `database`? It has `database`, `user`, `password`, `port` | `with: { database: … }` |
| A service's port changed | `machines.db.ports.postgres.port` | `invalid-value`: a catalogue service's port comes from its settings | `with: { port: 6543 }` |
| `steps` without `os` | `machines.web.app.steps` | `missing-field`: steps install onto an OS | `os: debian@12` |
| Index in the message | `message` | `unknown-reference`: `machines.web.services.0.publish` isn't a path; ports are named | `{{ machines.web.ports.web.published }}` |
| A file with two sources | `machines.web.files[0]` | `conflict`: `source` and `input` both given | keep one |
| `dns` names the wrong machine | `machines.station.dns.servers[0]` | `unknown-reference`: `dc1` isn't a machine; did you mean `dc01`? | `servers: [dc01]` |
| An override nothing applies | `overrides.vagrant.machines.web` | `unknown-reference`: `web` has no VM form (`app.build`), so Vagrant never runs it | remove it, or give `web` an `os` and `app.steps` |
| Container-only machine on a VM target | `vagrant: machines.web.app.build` | `refused`: a Dockerfile builds a container only | add `os` and `app.steps`, or use `docker`, `docker-vm` or `cloud-docker` |

---

## 10. Examples

Made-up specs in `spec-examples/<name>/isoloom.yml`. The project files they name aren't included.

| Example | Shows |
|---|---|
| `help-desk` | One container built from a folder (`app.build`), the flag as a file, a healthcheck on a named port, `http`, `exec` and `script` checks, the message's `.url` |
| `bookshelf` | `service: nginx` and `service: postgres@16` with `with:`, a secret as a setting, a published service port, a flag stored through `setup` |
| `forest-mini` | A GOAD-style Windows domain: bare Windows VMs configured by the environment's Ansible from Isoloom's Linux controller, flags as Windows files, a Linux access station with portable steps and DNS by machine name, Vagrant box overrides by group |
| `legacy-intranet` | `overrides:` for Docker, Vagrant, Proxmox (LXC, virtiofs) and AWS; portable recipes, and a vendor image beside steps (two recipes) |
| `branch-office` | Four networks, three routed by a gateway machine whose firewall the `reach` rules describe; several addresses, aliases, `setup` with a gateway's rights, `blocked` checks |

**Today's `examples/`, rewritten by hand in Phase 2:**

| Example | Rewrite |
|---|---|
| `air-gapped` | `store` → `service: redis`, `app` → `service: nginx` (Phase 4; until then `app: { image, steps: [{ run: provision/*.sh }] }` with `os`); `networks.lab.docker` → `overrides.docker.networks.lab.cidr` |
| `ansible-pair` | `common.vm.os` → `common.os`; `vm: {}` goes (bare OS machines, configured by `provision:`) |
| `arm-lab` | `docker.image` → `app.image`; `arch` → `hardware.arch`; ports named |
| `arm-vm` | `os: ubuntu@24.04`, `app.steps: [{ run: provision/setup.sh }]`, `hardware.arch: arm64` |
| `cisco-dynamips` | `app: { appliance: cisco-dynamips, firmware, config }`; idle Alpine hosts → `os: alpine`; `host-b` → `app.image` |
| `cisco-iol`, `cisco-qemu`, `cisco-switch` | `app: { appliance, image, config }`; idle hosts → `os: alpine` |
| `cloud-bucket` | unchanged (`cloud:`) |
| `corp-ad-basics` | Windows `vm.provision` playbooks → `os` + `app.steps: [{ playbook: … }]`; `web01` → `app: { build: build/web01, steps: [{ playbook: provision/web01.yml }] }` |
| `dual-stack` | `web` → `service: nginx`; `user` → `os: debian@12`, `access: true` |
| `edge-firewall` | `fw`, `web` → `app: { build, steps: [{ run }] }` with `os`; `cache` → `service: redis` |
| `existing-hosts` | `external` → `overrides.external.machines.*.ssh`; `vm.provision` with `reboot` → `app.steps` |
| `hello-stack` | `cache` `init` → `setup`; message `ports.0.publish` → `ports.web.published` |
| `mixed-office` | `vm.tpm: true` → `hardware.tpm: required`; `.ps1` → `app.steps: [{ run: provision/share.ps1 }]` |
| `pivot-dmz` | build + playbook → `app: { build, steps: [{ playbook }] }`; `db` → `service: postgres@16`; `user` → `os: kali` |
| `private-names` | `client`'s idle `debian:12-slim` → `os: debian@12` |
| `segmented` | the dormant port → `ports: { shell: { port: 4444, dormant: true } }` |
| `several-addresses` | `vm.provision` → `app.steps` (gains a container form if the scripts only install and configure) |
| `shared-files` | `docker.init` and the VM script → `setup: [steps/write.sh]`; idle Alpine → `os: alpine` |
| `slow-link` | `docker.build/dockerfile/args` → `app.build: { context, dockerfile, args }` + `steps` |
| `solo-web` | `app: { build: build/web, steps: [{ run: provision/web.sh }] }`, `os: debian@12` |
| `supplier-portal-api` | machine `inputs` → `environment: { API_URL: { input: API_URL } }` (non-sensitive) and `files` for `LAUNCH_TOKEN`; `fixed: true` → `publish: { host: 3206, fixed: true }`; `init` → `setup` |
| `tuned-web` | `vm.tpm: true` → `hardware.tpm: required`; the rest unchanged |
| `udp-dns` | the two `dns` ports need two names (`dns`, `dns-udp`); message `ports.0.publish` → `ports.dns-udp.published` |
| `vlan-office` | idle Alpine → `os: alpine`; `intranet` → `service: nginx` |
| `windows-hello` | `vm.image.*` → `overrides.vagrant.machines.web01.{ box, box_version, boxes, winrm }`; `.ps1` → `app.steps` |
| `workbench` | `bench` → `os: alpine`; `web` → `app.image` |
| `plan/phase2/examples/specs/*` | `flag-portal` as `help-desk` (plus `db` → `app.image`); `mini-ad` as `forest-mini` |
| invoice-portal-api (private) | each machine: `docker.build` + `vm.provision` → `app: { build, steps: [{ run }] }` with `os`; the flag script and volume → a `files` entry |

---

## 11. Phasing

| Phase | Front end | Lowering |
|---|---|---|
| 2 | Every field parses and validates; spans; old-key fixes; `service:` refused (no entries) | `docker` target: `os`, `app.image`, `app.build`, `app.steps` (`run`, `copy`, `packages`), `setup`, `files` with every source, named ports, `environment` inputs, `overrides.docker` |
| 3 | | VM forms on the launcher's targets: steps as shell and PowerShell, `Step::Setup`, value files over SSH and WinRM, the `vagrant`, `proxmox` and cloud overrides |
| 4 | Catalogue entries | `service:`/`with:`, the full step model, the Ansible and cloud-init backends |
| 5 | | The other targets' overrides (`kubernetes`, `external`, `esxi`) |

---

## 12. Open questions

1. **The flag construct.** Recommended: B, a value source on `files:` (§6).
2. **`os` on container-only machines.** Should a machine built from its own Dockerfile or image still state an `os`? Recommended: optional. It would be an unverifiable claim (the Dockerfile's `FROM` decides), and every container is Linux in 1.0. It becomes required the moment `steps` give it a VM form.
3. **Ports as a map.** A map keyed by name (recommended), or a list with a required `name`? The map makes uniqueness structural, and merging a service's ports by name trivial.
4. **`setup` on the container form.** Keep today's sibling job (network, volumes and files shared; the container's own filesystem not), or use Compose's `post_start` hook, which runs inside the container but before readiness? Recommended: the sibling job in Phase 2 (proven, ordered by `depends_on`, waited for by `up --wait`), with its contract documented; try `post_start` in Phase 3.
5. **A value file's default mode.** `0400` on every form (recommended), or today's `0444` in containers and `0400` on VMs? One secure default; authors widen it, as the corpus does with `chmod 444` already.
6. **The manifest's `published` keys.** Key them by port name (recommended, §8), changing run manifest v0 before its code lands.
7. **Old-key fixes.** Keep the targeted fixes for `docker:`, `vm:`, `services:` and the other old keys until the CTF labs are rewritten in Phase 7, then delete them (recommended). They read nothing, so they aren't compatibility.
8. **`internet` default.** Keep `true` (recommended: a generic converter), or follow the corpus (327 of 363 labs set `false`)?
9. **Two recipes on one machine.** `app.build` or `image` plus `steps` gives each form its own recipe, so parity is the author's job. Allow it (recommended), mark it "author parity" in the matrix, and let checks prove behaviour? The alternative, refusing it, would break today's both-form labs (invoice-portal-api) until their scripts are made portable.
10. **Where lowering lives.** `isoloom-plan::lower`, with today's `model` kept as the lowered form (recommended), or a new crate? Plan already depends on spec and will depend on the catalogue, so a new crate would only add an edge.
11. **Existing hosts.** `overrides.external.machines.<m>.ssh` (recommended), or a top-level `hosts:` inventory? An address on someone's network is a fact of that target, not of the machine.
12. **The catalogue in Phase 2.** Parse `service:` and refuse it with "no entry yet" (recommended: Phase 2 is app-first, like the corpus), or land the salvaged `nginx` entry early?
