# The run manifest

Isoloom converts a spec into files, and alongside the files of most targets it writes a **run
manifest** (`run.json`, under the target's folder): a versioned, self-contained description of how
to bring the environment up, check it and tear it down. Isoloom runs nothing itself. The run
manifest is the contract between Isoloom and a runner.

Which targets have one, and where:

| Target | Run manifest |
|---|---|
| `docker` | `.isoloom/docker/run.json` |
| `vagrant` | `.isoloom/vagrant/run.json` (one for every provider) |
| `docker-vm` | `.isoloom/docker-vm/run.json` (the VM made by Vagrant), and `.isoloom/docker-vm/proxmox/run.json` (made on a Proxmox server) unless a container mounts a folder of the computer that runs the environment |
| `proxmox` | `.isoloom/proxmox/run.json` |
| `esxi` | `.isoloom/esxi/run.json` |
| `cloud-vm` | `.isoloom/cloud-vm/<cloud>/run.json`, one per cloud that takes the environment |
| `cloud-docker` | `.isoloom/cloud-docker/<cloud>/run.json`, one per cloud that takes the environment |
| `cloud-services` | `.isoloom/cloud-services/run.json`, when the spec has `cloud:`, unless it has `before` steps (host scripts) or a variable holding a launch value inside a larger value |
| `kubernetes`, `hybrid`, `external`, `hosted` | none: their files are run by hand |

A runner executes it: any program that reads the manifest and follows this contract.
[Isothrum](https://github.com/Isothrum/isothrum) is the reference runner. You can
also run each tool's files by hand (`docker compose up`, `kubectl apply -k`, `vagrant up`, `terraform apply`,
`ansible-playbook`); the manifest exists so a runner doesn't have to know each tool's commands.

## What the manifest holds

A run manifest holds only documented data — the driver does the work:

- the **native commands** of the lifecycle and their order;
- **readiness criteria** (a ready file, running machines, a port answering);
- **outputs**: addresses, host ports and URLs; a published port's is named
  `<machine>__<port>` in snake case (`web__portal`), and no two outputs share a name;
- how to run the **checks**, with self-contained timeouts (waits + 15 s per check + 60 s), so a
  long-waiting environment is never cut off by a fixed cap;
- **secret slots**: each secret's and input's name and destination, and an optional `default`,
  **never a value**.

The manifest is strict (`additionalProperties: false`) and carries its own version. Its JSON
schema is committed at [`schemas/run-manifest-v1.schema.json`](../../schemas/run-manifest-v1.schema.json),
and `isoloom-manifest` holds the types and fixtures runners can test against.

A few names the generated files and a runner share live outside `run.json`: the output folder
(`.isoloom`), the controller's machine, the variables a Vagrantfile reads (the ESXi host and
port, the SSH proxy command, the chosen host ports, `ISOLOOM_PUBLISH_FIXED`, the consoles the
runner chose, `ISOLOOM_CONSOLES`), the cloud tags a
reaper reads (`isoloom-instance`, `isoloom-expires-at`), and the paths a machine's set-up uses
(its secrets folder and their delivery mark, the controller's halting mark). They are part of
the contract too, each defined once in `isoloom_manifest::contract`, which the backends write
from and a runner reads from.

## Versions and compatibility

`manifest_version` is the contract's version: **1** from Isoloom 2.0. Within version 1, a minor
release of Isoloom may add an optional field. Removing or renaming a field, changing what a field
holds or means, making a field required, or adding a value a runner must act on (a step action, a
delivery, a condition) raises `manifest_version`, in a major release of Isoloom.

The reader (`isoloom-manifest`) is strict: it refuses a field or value it doesn't know, as the
schema does (`additionalProperties: false`). A manifest that uses a field added in a minor release
needs a runner built with that release's `isoloom-manifest`, so a runner pins the
`isoloom-manifest` it reads and upgrades it with the Isoloom it runs. The policy is
`isoloom_manifest::COMPATIBILITY`, which the schema's description repeats.

A runner reads the versions it was built for and refuses any other before running anything.
Isothrum's release notes say which `manifest_version` each of its releases reads. Isoloom 1.x
wrote `manifest_version: 0`, an unstable contract read only by runners built for exactly that
release line.

## The lifecycle

The manifest's `lifecycle` holds each operation as ordered steps; a runner runs an operation's
steps in order, each under its own policy (time limit, retries, whether a failure fails the
operation):

| Operation | What a runner does |
|---|---|
| `up` | Checks the manifest's `tools` first (it fails before creating anything when one is missing or too old), runs `lifecycle.up`, delivers the secrets and inputs, waits for the readiness criteria, then runs `lifecycle.seal` when there is one |
| `check` | Runs the manifest's `checks` (below) and reports each check |
| `status` | Runs `lifecycle.status`, a read-only probe, and reads each machine's state from it |
| `park` | Runs `lifecycle.park.shutdown` (machines powered off) or `lifecycle.park.pause` (frozen, memory kept), whichever the target has and the runner chooses; the run's state is kept |
| `resume` | Runs `lifecycle.resume`: the parked machines come back as they were, no container re-created and no VM set up again, so no input is needed again |
| `provision` | Runs `lifecycle.provision.all`, or `lifecycle.provision.machine` with `${machine}` bound to one machine: the set-up again on running VMs |
| `down` | Runs `lifecycle.down`, which removes everything the run created |

A runner keeps each run's state (its record, the values it set, the host ports it pinned) outside
the environment folder. A secret or input value reaches only the steps its delivery names, and a runner
never writes a sensitive value to a log or an error.

### Downloads

A step's `timeout_s` is how long it may go without progress (a new line of output), and a step
that keeps making progress still stops at a ceiling. A step marked `"download": true` fetches
something whose size and speed nobody knows in advance: the images a Compose start in a VM pulls
(`pull-<step>`, before the `up` it pulls for, with the same project, files and pull policy, the
images a service builds left out). A runner lets a download run as long as it makes progress,
with no ceiling, stops it once it shows none for its `timeout_s` (900 s, the 15 minutes a stalled
download is given), and reports the stall and the failure as a download's, so a slow or broken
connection isn't taken for a broken environment. The step after it then runs under its own limit
with nothing left to fetch.

### The boxes a VM boots

Each Vagrant machine's `native.boxes` lists the box its VM boots under each provider, so a runner
can download them in a step of its own, with a download's rule, before `vagrant up` (which would
otherwise download a box inside its own time limit): `vagrant box add <box> --provider <provider>
[--box-version <version>] [--architecture <architecture>]`, skipping a box already there.

```json
"boxes": [
  { "provider": "virtualbox", "box": "bento/debian-12" },
  { "provider": "libvirt", "box": "generic/debian12", "architecture": "amd64" },
  { "provider": "virtualbox", "box": "generic/alpine319", "host_architecture": "amd64" },
  { "provider": "virtualbox", "box": "bento/debian-12", "host_architecture": "arm64" }
]
```

`version` is absent when the box isn't pinned (Vagrant takes the newest version with the build);
`architecture` is absent when the build is the computer's, which Vagrant picks itself;
`host_architecture` says the entry is only for a computer of that architecture (the controller's
Alpine box has no arm64 build for the desktop hypervisors, so an arm64 computer boots Debian's).
On ESXi the build is the ESXi host's, `amd64`. The access machine's box is its default one: a
runner that gives it another downloads that one instead.

### What a VM takes only when it boots

Vagrant applies some of a VM's settings only when it boots it (`vagrant up` of a stopped VM,
`vagrant reload`): its synced folders (`host_folders`, and `docker-vm`'s folders shared into its
VM), networks, forwarded ports, size and provider settings. `vagrant up --provision` on a running
VM runs its provisioners and nothing else. So each Vagrant machine's `native` carries a
`boot_digest`: a digest of its Vagrantfile configuration other than its provisioners (the
statements its VMs share included). It is opaque and only compared: a runner keeps the digest
each VM was booted with and, when it brings a running run up to date with a regenerated manifest,
reloads a running VM whose digest changed before it runs `up`'s steps again. A manifest written
before the field existed has none.

### A resource already gone

A Terraform run's `down` is `terraform destroy` without a refresh (a refresh would wait on a VM
whose guest agent never ran), so a resource the server lost meanwhile (a reset server, an apply
cut short) reaches its provider's delete. Most deletes count a resource that is gone done; a
few fail instead, and the destroy then fails every time. `identity.terraform.gone` names those
errors, one per resource type, the instance's attributes in the state written `{<attribute>}`:

```json
"gone": [{ "resource": "proxmox_virtual_environment_firewall_ipset", "error": "no such IPSet '{id}'" }]
```

When the destroy fails, a runner fills each error with the attributes of each instance of that
type in the state. An instance whose text one of the destroy's lines holds is gone: the runner
drops it from the state (`terraform state rm`), says which, and destroys again; any other
failure stays the `down`'s. The text names one instance, by its own attributes, in its
provider's words, so a resource that might still exist is never forgotten. The Proxmox
manifests name bpg/proxmox's IP set, VM and container.

## The checks

`check` runs the manifest's `checks` in three phases, and a runner keeps to them:

1. **`checks.setup`**, in order: what the check runners need first, such as the stand-in for an
   access machine the environment brings up on demand (`stand-ins`: the checks that run from its seat run
   from the stand-in's addresses). A failed set-up step fails the checks: no runner runs, but the
   teardown still does.
2. **`checks.runners`**, in order: each runner's step, under its own time limit, prints its
   results in the `checks.protocol` line format (`isoloom_manifest::CheckLine::parse` reads
   it). A runner cut off by its limit runs its `on_timeout` steps (the one-off container it
   leaves behind removed); the next runner still runs. A check its runner never reported fails.
3. **`checks.teardown`**, after the runners **whatever happened** — every check passed, one
   failed, a runner timed out, or the set-up failed. It takes the stand-ins down
   (`stand-ins-down`), so the access machine they stand in for can take its addresses when it is
   brought up. A teardown step is a clean-up: Isoloom marks it `failure: tolerate`, so its
   failure is logged and fails no check (a step's `failure` is `fail` unless it says so).

The stand-in (`isoloom-access`) is a machine of role `access` at the access machine's addresses,
in the `check` profile, never `on_demand`: a runner tells the seat from its stand-in by
`on_demand`. Compose takes both at one address because they are in different profiles, but they
can't run at once: a runner that runs the checks while it has the access machine up gets a
failed set-up (Docker: "Address already in use"), so it takes the access machine out first, or
leaves the checks for later.

Compose in a VM (`docker-vm`, `cloud-docker`) has the same steps, each run in the VM through the
manifest's way in. The docker-vm Vagrantfile's `checks` provisioner, for running the checks without
a runner, does the same by itself: the stand-ins up, the runners, then the stand-ins down whatever
the runners found.

## Secrets and inputs

The spec names secrets and inputs and where each reaches a machine (usually a file); the manifest
turns each into a **slot** — a name and a destination, with an optional `default`, but no value.
At run time the runner reads each value (the embedder's, else from the slot's `source`: the
runner's environment, a file, HashiCorp Vault or a SOPS-encrypted file) and delivers it as the
slot says: an environment variable of some steps, a Terraform variable, a file in the run's state
folder, or a file written into the machine over SSH (guest delivery). Values never land in a generated file, the
run's state, an event or a log.

A guest delivery over SSH (`transport: ssh`, a cloud VM's) goes once its `during_step` has
succeeded, else once `up`'s steps all have, and before the ready conditions: the machine's set-up
waits for it, so no step waits for that set-up. It reaches its machine where the module's
outputs say: at `address_<machine>`, as `ssh_user_<machine>` (the machine's name with `-` and `.`
written `_`). The steps that reach the same machine over SSH (its `deliver-<machine>` step, its
ready file, its check runner) name the same address output, and the same user as a literal: the
cloud's login for the machine's OS, the one rule that writes `ssh_user_<machine>` too.

## Graphical consoles

A VM's machine lists its **graphical consoles** (`machines[].consoles`): its screen, keyboard and
mouse as its hypervisor, its server or its own OS serves them, and how a runner finds each one.
A console listens only on the loopback of the computer that runs the environment, or is reached
through an SSH tunnel the runner opens there: never on the local network, never at a public
address. A Vagrant machine lists one console per provider (`providers`); elsewhere a machine
lists one. A machine says so when it has none, and why (`protocol: none` with a `note`), so a
runner can tell a user why there is no screen to show. A container lists none.

```json
"consoles": [
  { "providers": ["libvirt"], "protocol": "vnc", "source": { "from": "libvirt" } },
  { "providers": ["parallels"], "protocol": "none", "note": "Isoloom sets up no graphical console on Parallels Desktop (its VNC server stays off)" }
]
```

| `source.from` | Protocol | Where | How a runner finds it |
|---|---|---|---|
| `libvirt` | VNC | `127.0.0.1`, a port libvirt picks at boot | `virsh domdisplay <domain>` (the machine's `native.vm_names.libvirt`), on the connection Vagrant uses |
| `qemu_monitor` | VNC | `127.0.0.1`, the first free port from 5900 | the VM's QEMU monitor (vagrant-qemu's socket): `info vnc` |
| `vrde` | RDP | `127.0.0.1`, a free port of 3390 to 3489 | `VBoxManage showvminfo`; on only with the Oracle VM VirtualBox Extension Pack installed when the VM boots |
| `runner` | VNC | `127.0.0.1`, at a port the runner picks | the runner chooses the port and an 8-character password for the run, keeps them until `down`, and gives them to the run's own Vagrant processes in `ISOLOOM_CONSOLES` (`{"<machine>": {"port": 5901, "password": "…"}}`), never to a command it hands an embedder |
| `proxmox_api` | VNC | Proxmox VE's API | `vncproxy` then `vncwebsocket` of `vm_id` on `node`, signed in with the `token` slot, else `username` and the `password` slot; the ticket `vncproxy` gives is the VNC password |
| `ssh_tunnel` | RDP or VNC | the machine's `host:port` | an SSH tunnel from this computer's loopback through `via`, as `user`, with the runner's key |

An RDP console to a Windows machine's own Remote Desktop names its `account`: the user and a
template naming the output that holds its password (a sensitive one).

What each target gives a Linux and a Windows VM is in the [capability matrix](../matrix.md#graphical-consoles):

- **Vagrant**: libvirt and QEMU give VNC; VMware Workstation and Fusion their VNC server
  (`RemoteDisplay.vnc.*`, at the runner's port and password); VirtualBox its remote display (RDP),
  only with the Oracle extension pack, which brings the server (without it, it stays off and a
  runner says so). Hyper-V has no VNC or remote display server of its own; Parallels' VNC server
  stays off; an ESXi VM's console goes through ESXi's own
  API (a WebMKS ticket), which Isoloom doesn't use.
- **Proxmox**: every VM but an Arm one (whose console is the serial port) has Proxmox's own
  console, through its API; the module outputs each VM's id (`vm_id_<machine>`).
- **ESXi**: none, as on Vagrant's ESXi provider.
- **Clouds**: a Linux VM has none (its way in is SSH). A Windows VM whose spec opens Remote
  Desktop's port (3389/tcp) has its own Remote Desktop, which its set-up turns on, reached through
  an SSH tunnel through a Linux machine of the run on one of its networks (as the environment's
  `reach` rules let that machine reach it), signed in as `isoloom` with the generated password
  (the module's sensitive `windows_password` output). A Windows VM without the port, or with no
  Linux machine beside it, has none and says why.

## The egress seal

For a cloud target and a server (`proxmox`, `esxi`), an `internet: false` network is sealed at
the network level **after** the machines are set up: the manifest carries an optional `seal` step
that the runner applies once the environment is ready. On a cloud it is a second Terraform apply
with the module's seal variable on; on a server, one `ssh.run` on the router that applies the
sealed ruleset it keeps from its first boot. Egress to the internet is then cut for those
networks, while the runner's SSH and the published ports still answer. The seal fails closed — a
failed seal means the run is not ready — and every later operation replays it, so a later apply
can't silently undo it.

## Running it

`isoloom generate` writes each target's files under `.isoloom/<target>/`, with its run
manifest where the target has one (see the table above); a runner then reads the `run.json` of
the target it runs. Its own commands, options and messages are its own, documented
with it.

## See also

- [Spec reference](../spec.md) — secrets, inputs, `files:` and checks.
- [Targets and tiers](targets.md) — each target's run manifest.
- [Getting started](../guide/getting-started.md).
- [Policies](policies.md) — what is stable, and how each surface is versioned.
