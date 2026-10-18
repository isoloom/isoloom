# Run manifest v0

The contract between Isoloom and the Isothrum runner (PLAN.md §0 "Contracts", Phase 2 step 3). Isoloom writes one manifest next to each target's generated files; the runner executes it. This file designs version 0. The schema is `run-manifest-v0.schema.json`, beside this file. The two worked examples are in `examples/` and validate against it.

**Citation convention:**
- `src/…` is the launcher, relative to `cyber-ctf-launcher/src-tauri/`.
- `crates/…` and `examples/…` are Isoloom, relative to the `isoloom` repository on local `main`.
- Line numbers are as of 2026-10-15 (launcher `48d6330`, Isoloom `bba06a91`).

**Inputs:**
- the launcher's runtime, read in full;
- Isoloom's `isoloom-runtime` and the CLI's run, test and down commands;
- the files today's Isoloom generates;
- the feature counts of the 376 CTF lab specs (private; only counts are used here, no lab content).

---

## 1. What it is

| | |
|---|---|
| **Where** | `.isoloom[-<n>]/<target>/run.json`. A cloud target has one per cloud: `.isoloom/cloud-vm/aws/run.json`. Docker on one VM has two: `docker-vm/run.json` (Vagrant) and `docker-vm/proxmox/run.json` (Terraform). |
| **Written by** | Isoloom's backend for that target, from the plan IR it already builds (`Environment`, the start plan, the checks plan). It is one more generated file: snapshot-tested and run through `verify`. |
| **Read by** | The Isothrum runner. It reads only this file: never `resolved.json`, never the Compose file, Vagrantfile or Terraform module's text. |
| **Types** | A small serde-only crate in Isoloom, `isoloom-manifest`: the types, `schema/run-manifest-v0.schema.json` generated from them (a test keeps it current), and fixtures. The runner depends on the crate, so both sides parse one definition. Both repositories test against the fixtures (PLAN Phase 2 step 3). |
| **Replaces** | The launcher's knowledge of Isoloom's layout and names (`src/runtime/lab/names.rs:1-127`); everything it scrapes from generated files (§3.6); Isoloom's in-CLI runner (`crates/isoloom/src/commands/run/bring_up.rs`, `test/*`). |

**The split of responsibilities** is the main design choice:
- **The manifest holds the native commands** (argv), their order, working folder, environment and timeouts, plus the facts the runner needs (machines, roles, addresses, ports, networks, outputs, checks, slots).
- **The runner's driver holds the mechanics** that need a decision at run time: the read-then-act clean-ups (list, then remove what was listed), status parsing, readiness polling, port pinning, elevation, process trees and retries. Each such primitive is a named `action` whose native commands this document fixes (§6), so nothing a driver runs is undocumented.

Rejected alternatives:
- **Argv only**, as a script language: it can't express "remove the networks Compose no longer defines" without a shell, and the host-neutral rule forbids one.
- **Driver only**, with the manifest naming just the target: that is today's implicit coupling (`names.rs`), and it moves Isoloom's conventions into the runner instead of into a contract.

---

## 2. The rules, and how each is enforced

| Rule | What it means | Enforced by |
|---|---|---|
| **Declarative** | The lifecycle is data: steps, conditions, slots. The runner never evaluates code from it. Guest-side shell (a check script, a provisioner) stays inside the guest; a step only pipes a generated file to a guest's `sh -s` (`stdin`). | `argv[0]` must be a bare tool name, and every shell (`sh`, `bash`, `cmd`, `powershell`, `pwsh`, `env`, `sudo`, `python`) is refused as the program (schema `$defs.argv`). |
| **Deterministic** | The same Isoloom on the same spec writes the same bytes. No timestamps, no absolute paths, no host facts. Keys in a fixed order, arrays in spec order, LF, two-space JSON, trailing newline. Run-time values come in through `${…}` placeholders only. | An `insta` snapshot per example and target (Phase 0's net); paths are `$defs.rel_path` (no leading `/` or drive, no `\`). |
| **No secret values** | Secrets and inputs are slots: a name, a kind, where the value may come from (a store reference), and where it goes. A value never sits on a command line, and the template grammar has no `secrets.` root. | `$defs.template` refuses `${secrets.…}`; `delivery` has no argv form. The runner masks every `sensitive` slot's value in events and logs. |
| **Host-neutral** | One argv for every host, unless a host truly differs (`host_argv` by OS). Paths are `/`-separated and relative; the runner converts them. Tools resolve on PATH with their Windows wrapper (`az.cmd`). A host-specific provider or tool is narrowed by `hosts`. | Schema (`host_argv`, `hosts`, `rel_path`); the cross-platform check runs every fixture on macOS, Windows and Linux. |
| **Versioned** | `manifest_version: 0`, plus `$schema`. Version 0 is unstable: Isoloom and the runner share a workspace through path dependencies until 1.0 (PLAN §2, "Workflow"). The runner refuses any other version with a typed error naming both versions. From 1.0: additive optional fields within a major version; `x-` keys are extensions the runner ignores. | `const: 0`; `additionalProperties: false` in v0, so drift fails tests at once. |

---

## 3. What runs today, mode by mode

This is the derivation. Each table says what the launcher does (L) and, where it differs, what Isoloom's own runtime does (I). §4 maps each manifest field back to these lines.

### 3.1 Container (`docker`): the Phase 2 path

| Aspect | Today |
|---|---|
| **Generate** | **L:** `lab::prepare` regenerates the target's files before every start (`src/runtime/lab/mod.rs:183-201`), then rewrites the Compose file's `name:` to `cyberctf-<id>` (`src/runtime/lab/overrides.rs:23-56`). An instance moves the files to `.isoloom-<n>/` and its Docker blocks (`src/runtime/lab/mod.rs:81-139`). |
| **Pre-flight** | **L:** refuse when the lab runs elsewhere (`src/runtime/launch.rs:46-56,77-85`). Resume instead of starting when parked and something is left (`:117-129`). Refuse when it already runs here; tear down stopped leftovers (`src/runtime/launch/local.rs:9-34`). Engine reachable: `docker info --format {{.ServerVersion}}`, timed (`src/runtime/docker/lifecycle.rs:12-22`). |
| **Commands, in order** (cwd: the lab folder; every Compose call is `docker compose -p cyberctf-<id> -f <out>/docker/compose.yml [-f .cyberctf-ports.yml]`, `src/runtime/docker/compose.rs:26-37`) | 1. Pin ports on a fresh start: `compose config --format json`, then free loopback ports held while picked, written to `.cyberctf-ports.yml` as `ports: !override` (`src/runtime/docker/lifecycle.rs:79-110`; `src/runtime/docker/compose.rs:234-269`).<br>2. Unless already running, each published port must be free over its protocol, on 0.0.0.0 and 127.0.0.1 (`src/runtime/docker/lifecycle.rs:28-33,129-137`).<br>3. Remove stale networks: `network ls --filter label=com.docker.compose.project=P`, minus the config's, then unplug and `network rm` (`:189-210,239-246`).<br>4. `up -d --pull missing --wait --wait-timeout 600 [wait…]`, then each leaf job `up --no-deps --exit-code-from J J`, from Isoloom's start plan parsed out of the Compose text (`:144-164`; `crates/isoloom-compose/src/docker.rs:105-155`).<br>5. Re-plug the attack box; record in Isoloom's registry; read the published ports back from `config` for the message (`src/runtime/launch.rs:168-180`). |
| | **I:** the same `up` with `--build` and no `--wait-timeout`. With leaf jobs, it is `sh -c "… up -d --build --wait <wait…> && … up --no-deps --exit-code-from J J"` (`crates/isoloom/src/commands/run/bring_up.rs:69-98`). Docker secrets travel as `ISOLOOM_SECRET_<NAME>`, project files as `ISOLOOM_FILE_*` (`crates/isoloom/src/commands/run/mod.rs:205-220`). |
| **Env** | **L:** only `CTF_API_URL`, `CTF_LAUNCH_TOKEN`, `CYBERCTF_ATTACKBOX_IMAGE` and `CYBERCTF_PORTS` (`src/labs.rs:240-247`), given to `up` (`src/runtime/docker/lifecycle.rs:147`). Compose interpolates inputs as `${NAME:-}`, only on the machines that list them (`crates/isoloom-compose/src/docker.rs:17-18`). `DOCKER_HOST`/`DOCKER_CONTEXT` are stripped when the user picked a context (`src/exec/mod.rs:93-97`). |
| **Ready** | `up --wait` exits 0, meaning every service with a healthcheck answers. Isoloom gives each port a busybox TCP probe and the route keepers a "no default route" probe. Each leaf job then exits 0. Nothing else is polled. |
| **Outputs** | **L:** status from `compose ps --all --format json` (both output formats, `src/runtime/docker/compose.rs:148-154`), `docker inspect` for addresses and labels (`src/runtime/docker/inspect.rs:95-149`), and `network ls/inspect` (`inspect/networks.rs:6-21`). The "Open" URL is the lowest published TCP port of a running service whose name or image doesn't look like a datastore (`src/runtime/docker/inspect.rs:22-70`). The message fills its placeholders from the resolved snapshot plus the real published ports (`src/runtime/lab/mod.rs:249-266`). |
| | **I:** `compose ps` publishers, unless `ISOLOOM_PUBLISH_FIXED` is set (agent read of `crates/isoloom/src/tools/compose.rs:58-110`). |
| **Checks** | **L, local Docker only** (`src/runtime/commands.rs:112-119`): start the access stand-ins if the Compose text defines them (`src/runtime/docker/check.rs:45-47,70-79`). Then per position `--profile check run --rm --no-deps isoloom-check[-<pos>]`, each capped at 5 min (`:54,80-107`). `exec` checks are `exec -T <m> sh -s < checks/exec-<m>.sh` (`:108-121`). A check the runner never reported fails (`:138-143`). |
| | **I:** the same, plus `-e ISOLOOM_DERIVED=0` for `--no-derived`. Lines are parsed anywhere in a line, and a runner with no `END` is "unfinished" (`crates/isoloom/src/commands/test/local.rs:20-72`; `crates/isoloom-plan/src/checks/runner.rs:254-278`). |
| **Teardown** | **L:** stop the attack box first (it holds the network), `down --volumes --remove-orphans`, `rm -f` every container of the project (one-offs included), unplug and remove every network still labelled, delete the pins (`src/runtime/docker/lifecycle.rs:213-246`; `compose.rs:127-134`). |
| | **I:** `--profile check down -v`. Without `--volumes` it keeps persistent volumes (`sh -c "rm -f -s -v && down && docker volume rm …"`, `bring_up.rs:100-116`). |
| **Park / resume** | **L:** both modes are `compose stop` (after `docker stop -t 1` of the attack box). Resume is `compose start`, never `up`, so containers aren't re-created and inputs aren't needed again (`src/runtime/docker/lifecycle.rs:168-184`; `src/runtime/lifecycle.rs:19-43,80-120`). Nothing left → the parked mark is dropped and a fresh start offered (`:92-95`). |
| **Recovery** | Leftovers are torn down before a start (above). There is no retry. |

### 3.2 LocalVm (`vagrant`, one VM per machine; `docker-vm`, the containers in one VM)

| Aspect | Today |
|---|---|
| **Commands** (cwd `.isoloom/vagrant` or `.isoloom/docker-vm`) | **L:**<br>1. `providers::ensure_usable` (`src/runtime/launch/local.rs:105`).<br>2. A memory warning (`:39-57`).<br>3. On QEMU, Isoloom's refusal and the emulator binary for a foreign CPU (`:78-95`).<br>4. `reconcile_provider`: a machine recorded under another provider gets `destroy --force` (bounded 180 s), then `.vagrant` removed (`src/runtime/vm/recover.rs:12-19`).<br>5. `discard_aborted_saved`: `VBoxManage showvminfo <vm> --machinereadable`, and `discardstate` on `aborted-saved` (`src/runtime/vm.rs:118-126`).<br>6. `vagrant up --provider <p>` (`:129-134`).<br>7. `vagrant halt isoloom-controller` (tolerated; not on QEMU, where the controller is the attacker) (`:64-73`).<br>8. Container labs: the attack box inside the VM (`src/runtime/launch.rs:133-142`). VM labs: the attack VM beside them (`:144-163`). |
| **Hyper-V** | Every command of an operation runs as one elevated batch: a PowerShell script via `-EncodedCommand`, its environment read from a file in its own folder and then deleted, one UAC prompt per batch, status never elevated (`src/exec/elevate.rs:1-20`; `src/exec/vagrant.rs:80-158`). |
| **Env** | The launch env reaches the Vagrantfile, which reads the inputs as `ENV.fetch(name, "")` (`examples/supplier-portal-api/.isoloom/vagrant/Vagrantfile:393-396`). Linux guests get them through a shell provisioner into `/tmp/isoloom-inputs.env` (`:641-646`). Other variables the Vagrantfile reads:<br>- `ISOLOOM_HYPERV_SWITCH` (`:327`)<br>- `ISOLOOM_SSH_PROXY_COMMAND` (`:495`)<br>- `ISOLOOM_DERIVED` (the checks provisioner)<br>- `ESXI_*` (§3.3) |
| **Ready** | `vagrant up` exits 0, after every provisioner and the controller's Ansible play. Isoloom's own `ready` provisioner writes `/var/lib/isoloom/ready` in each Linux guest; nobody reads it locally. **docker-vm:** the `environment` provisioner runs the start plan inside the VM (guest `sh`), with `ISOLOOM_PUBLISH_FIXED=1`, and forwards each published port to the host's loopback at its `publish` number (generated `docker-vm/Vagrantfile`). |
| **Status** | `vagrant status --machine-readable`, 25 s, never elevated (`src/runtime/vm/status.rs:25-28`). Running means every non-controller machine is `running` (`:31-35`). Addresses come from the spec's resolved topology (`:38-49`; `src/runtime/lab/mod.rs:210-239`). The provider comes from `provider-name` lines (`src/runtime/vm/status.rs:57-61`). |
| **Checks** | **L:** none for VM labs (`src/runtime/commands.rs:112-119`).<br>**I:** wake the controller: `vagrant status isoloom-controller`, wait while `/run/isoloom-halting` exists (60×2 s), then `vagrant up isoloom-controller --no-provision`. Run `vagrant provision --provision-with checks`, then halt it (`crates/isoloom/src/tools/vagrant.rs:22-91`; `crates/isoloom/src/commands/test/local.rs:75-82`). |
| **Teardown** | `vagrant destroy --force`, then hypervisor-level removal of this lab's exact VM names: VirtualBox (`list vms`, `discardstate`, `controlvm poweroff`, `unregistervm --delete`, leftover folders) and Parallels (`prlctl stop --kill`, `delete`) (`src/runtime/vm.rs:243-256`; `src/runtime/vm/recover.rs:150-181`). |
| **Park / resume** | Pause is `vagrant suspend`; shutdown is `vagrant halt` (`src/runtime/vm.rs:182-192`). A container lab in a VM only pauses (`src/runtime/lifecycle.rs:55-60`). Resume is `discard_aborted_saved`, then `vagrant up --no-provision --provider <recorded>`, then park the controller (`src/runtime/vm.rs:197-215`). |
| **Provision** | With a controller: `up isoloom-controller --no-provision`, `[provision <m>]`, `provision isoloom-controller --provision-with ansible`, then always `halt isoloom-controller`. Without one: `vagrant provision [m]` (`src/runtime/vm.rs:141-168`). Inputs aren't kept, so a re-provision gets none (`src/runtime/lifecycle.rs:122-125`). |
| **Recovery** | A failed `up` whose output matches `verr_already_exists`, `could not rename`, `is locked`, `different provider` or `single provider at a time` (`src/runtime/vm.rs:262-273`) gets cleared and retried once:<br>1. `destroy` (bounded);<br>2. orphans by exact name;<br>3. `.vagrant` removed (`src/runtime/vm/recover.rs:189-202`; `src/runtime/launch/local.rs:102-124`).<br>A genuine failure is never retried. A failed Ansible play leads its error with the last `fatal: [host]: …` line (`src/runtime/vm.rs:77-112,219-239`). |
| **Isoloom-only** | **I:** a baseline snapshot after `run` (`vagrant snapshot save --force isoloom-baseline`), and `reset` restores it (agent read of `crates/isoloom/src/commands/run/done.rs:27-29,66-85`; `reset.rs:45-97`). Secrets for VMs are delivered while `vagrant up` runs: base64 lines over `vagrant ssh`, or WinRM through Python and pywinrm, retried every 3 s, each try capped at 120 s (agent read of `crates/isoloom/src/secrets/vms.rs`, `windows.rs`, `winrm.py`; summarised in §3.5). |

### 3.3 Server

**Proxmox through Terraform** (`.isoloom/proxmox` for VM labs, `.isoloom/docker-vm/proxmox` for container labs)

| Aspect | Today |
|---|---|
| **Commands** (cwd: the module) | **L:**<br>1. Mark the host first, so a half-created lab can still be destroyed (`src/runtime/launch/on_host.rs:22`).<br>2. Generate the files.<br>3. The launcher's key (`ssh-keygen -t ed25519`, `src/runtime/ssh.rs:38-47`).<br>4. The Proxmox jump: the API over HTTPS. A password host on an internal bridge lets the key in over `ssh` with an `SSH_ASKPASS` `sh` script, which is Unix-only (`src/runtime/server/contract.rs:183-202`; `src/runtime/proxmox/node_ssh.rs:14-57`).<br>5. Write `runtime`, `host` and `ssh-jump` beside the state, plus `run.json`, the replay record (`on_host.rs:83-90`; `src/runtime/terraform/state.rs:26-37`).<br>6. Delete a stale `.terraform.tfstate.lock.info`.<br>7. `terraform init -input=false -no-color -backend-config=path=<state>/terraform.tfstate`, then `terraform apply -auto-approve -input=false -no-color` (`src/runtime/terraform/runner.rs:23-38,138-147`). **On macOS** both run in `hashicorp/terraform:1.9.8@sha256:18f9…` with the lab, state and key folders mounted at their own paths (`:16,44-48,89-115`). |
| **Env** | Variables as `TF_VAR_<name>`; the provider's own environment after them, minus names that clash (`src/runtime/terraform/mod.rs:107-111`). Every run adds `TF_DATA_DIR=<state>/.terraform` and `TF_IN_AUTOMATION=1` (`runner.rs:57-62`). Proxmox variables come from the host profile, with the password or token from the keychain (`src/runtime/lab/vars.rs:77-103`; `src/runtime/server/secrets.rs:8-69`). Run variables: `inputs` (a JSON string), `ssh_via_node`, `ssh_public_key`, `ssh_private_key_file` (`vars.rs:109-130`). |
| **Ready** | **L:** `wait_ready` over SSH runs `cat <ready_file>; tail -n 25 /var/log/cyberctf-lab.log` every 10 s. `ready` means done, `failed: <step>` fails with the tail, `running: <step>` is logged. After 40 min it fails. **If SSH never answers within 10 min it returns OK**, and likewise with no key or address (`src/runtime/terraform/ready.rs:10-100`).<br>**I:** the module's `remote-exec` runs the start plan, then writes `ready` (`examples/supplier-portal-api/.isoloom/docker-vm/proxmox/main.tf:257-266`), so the apply itself is the wait. Nothing writes `running:`, `failed:` or that log. |
| **Outputs** | Parsed from `terraform.tfstate`; `terraform output` never runs (`src/runtime/terraform/state.rs:53-71`). Names: `ip`, `ssh_user` (default `isoloom`), `ready_file` (`src/runtime/lab/names.rs:118-127`). Running means `ip` is set, the run isn't expired, and nothing is tainted. **The VM module outputs `address`, not `ip`** (`crates/isoloom-terraform/src/proxmox/outputs.rs:23-58`), so a Proxmox VM lab never reads as running. |
| **Checks** | **L:** none; container labs on a lab host get a status probe over SSH (`src/runtime/status.rs:97-127`; `src/runtime/docker/inspect.rs:216-223`, a `sh` script run on the lab host).<br>**I:** `ssh -J isoloom@<address> user@host 'sh -s' < checks/<pos>.sh` (agent read of `crates/isoloom/src/commands/test/remote.rs:84-112`). |
| **Teardown** | `terraform destroy` with the host's current variables plus the saved ones (`src/runtime/terraform/mod.rs:50-74`; `src/runtime/lifecycle.rs:177-180`). Then the state and `known_hosts` are removed. A missing state with a record warns instead of claiming a clean stop. |
| **Park** | Refused on a server host (`src/runtime/lifecycle.rs:23-25,82-84`). |

**ESXi through Vagrant** (the same Vagrantfiles, provider `vmware_esxi`)

| Aspect | Today |
|---|---|
| **Commands** | **L:** generate; env = the launch env plus `connection_env`; `reconcile_provider`; `vagrant up --provider vmware_esxi`; halt the controller (`src/runtime/launch/on_host.rs:25-40`). **No stale-state retry** on this path. |
| **Env** | `connection_env` sets the launcher's `CYBERCTF_ESXI_*` and `CYBERCTF_REMOTE_*`, then the same values under the `ESXI_*` names Isoloom's Vagrantfile reads (`src/runtime/server/contract.rs:22-41`; `src/runtime/lab/vars.rs:14-28`). The password travels in the environment, and the plugin reads it with `env:ESXI_PASSWORD`. **On macOS**, `lan_relay` puts loopback relays in front of the host's SSH and HTTPS, sets `ISOLOOM_SSH_PROXY_COMMAND=/usr/bin/nc %h %p`, adds a WinRM HTTP proxy and an `ovftool` wrapper (`src/lan_relay/mod.rs:90-128`; `src/exec/mod.rs:154,219`). |
| **Rest** | As LocalVm, but with no orphan clean-up for ESXi and no park. |

**Proxmox through Vagrant** never runs: a Proxmox host always takes the Terraform path (`src/runtime/server/profile.rs:76-87`), and the `CYBERCTF_PROXMOX_*` variables (`contract.rs:42-56`) reach no tool. PLAN §1a lists "the Vagrant Proxmox provider". The launcher's own note says `vagrant-proxmox` is dead (`src/runtime/providers.rs:26`). See open question 5.

### 3.4 Cloud

**`cloud-docker/<cloud>` (container labs) and `cloud-vm/<cloud>` (VM labs)**

| Aspect | Today |
|---|---|
| **Commands** | As Proxmox, with these differences:<br>- Always the host `terraform`.<br>- GCP first ensures the account's labs project (`gcloud projects describe/create`, `billing projects link`, `services enable`, `src/runtime/server/gcp.rs:14-47`).<br>- The player's public IP: two of four HTTPS resolvers must agree (`src/runtime/launch/on_host.rs:126-184`), giving `allowed_cidr=<ip>/32`.<br>- AWS: a budget check through Cost Explorer (`src/runtime/server/budget.rs:15-38`). |
| **Credentials** | Through each CLI's chain, never stored when it can be avoided (`src/cloud.rs:1-2`; `src/runtime/server/contract.rs:65-88`):<br>- AWS: keys, or `AWS_REGION` with `AWS_PROFILE`.<br>- Azure: `ARM_SUBSCRIPTION_ID`.<br>- GCP: ADC.<br>- DigitalOcean: `DIGITALOCEAN_TOKEN`.<br>- Linode: `LINODE_TOKEN`.<br>- OCI: `~/.oci/config`.<br>Variables (`src/runtime/lab/vars.rs:37-74`): `region`, `auto_stop_minutes`, the size under each module's own name, and per-cloud placement. The modules' `expires_at` variable is never set. |
| **Ready** | As Proxmox: the module's `remote-exec` runs the start plan, then writes `ready`. The module also has a `local-exec` that builds the project archive with `tar`, which is sh-only (`examples/supplier-portal-api/.isoloom/docker-vm/proxmox/main.tf:243-245`; the same in the cloud-docker modules). |
| **Expiry** | `run.json` records `expires_at = now + auto_stop_hours×3600` at apply time (`src/runtime/terraform/state.rs:26-37`). The guest powers itself off (`shutdown -h +N`). The reaper runs at start-up and every 300 s; it destroys expired states under a try-lock and re-checks expiry while holding it (`src/runtime/lifecycle.rs:244-318`). It skips labs whose folder is gone, because destroy needs the module. |

**`cloud-services`** (13 labs, all AWS)

| Aspect | Today |
|---|---|
| **Commands** | **L** (`src/runtime/cloud_services.rs:353-390`; `src/runtime/terraform/services.rs`):<br>1. Refuse reserved `cloud.env` names and `cloud.before`.<br>2. Check `cloud.requires` (bare names on PATH, or commands under `sh -c`, or PowerShell on Windows, `src/runtime/lab/mod.rs:312-322`).<br>3. Write `inputs.tfvars.json` 0600 beside the state.<br>4. AWS CLI mode: `aws configure export-credentials --format env-no-export` into a private credentials file for the run.<br>5. `terraform init -input=false -no-color` in the lab's own module.<br>6. `terraform apply … -state=<state>/terraform.tfstate -var-file=.isoloom/cloud-services/terraform.tfvars.json [-var-file=<state>/inputs.tfvars.json]`, with `TF_VAR_expires_at` and `TF_VAR_region`, retried `cloud.apply_retries` times.<br>**I:** `sh up.sh`, `sh down.sh`, `sh outputs.sh` (`crates/isoloom/src/commands/run/bring_up.rs:51`), all sh-only. |
| **Inputs** | `{{ inputs.NAME }}` is filled at any depth of `cloud.vars`. The launcher supports `PLAYER_CIDR`, `PLAYER_IP`, `CTF_API_URL` and `CTF_LAUNCH_TOKEN` (`src/runtime/cloud_services.rs:94-137`). |
| **Outputs** | `cloud.outputs` maps the spec's names to state outputs, `sensitive` from the state. Sensitive values are redacted from the log (`:248-262,490-502`). |
| **Park** | Never: a cloud lab is created or destroyed (`src/runtime/lifecycle.rs:46-51`). |

### 3.5 Cross-cutting machinery the runner inherits

- **Running tools:**
  - Never through a shell; headless on Windows (`CREATE_NO_WINDOW`).
  - Each tool gets its own process group on Unix or a job object on Windows, and `kill_on_drop`, so a cancelled `vagrant` takes its `ruby` with it (`src/exec/mod.rs:147-253`; `src/exec/group.rs`).
  - Lines are decoded lossily, so a non-UTF-8 console can't kill a `terraform apply` (`src/exec/mod.rs:255-314`).
  - Windows `.cmd` tools (`az`, `gcloud`, `oci`) go through `cmd /C`, and arguments cmd.exe would interpret are refused (`src/exec/shim.rs`).
  - Status probes are capped at 25 s (`src/exec/mod.rs:38`).
- **Locks and workers:**
  - One operation per lab: a tokio mutex plus an OS file lock on `deploys/<lab>.lock`, released by the OS when the holder dies (`src/runtime/inflight.rs:98-137`; `src/deploy_worker/lock.rs:1-4`).
  - Long operations run in a detached worker (the same executable, `deploy --job`) with a log, a pidfile and a status file, so quitting the app never cuts an apply short (`src/deploy_worker/mod.rs:1-11,32-114`).
  - A stop during a Terraform run sends SIGINT or a console Ctrl+C, then waits up to 10 min (`src/deploy_worker/process.rs:397-478`).
- **Markers and state:**
  - In the lab folder: `.cyberctf-host`, `.cyberctf-local-vm`, `.cyberctf-parked`, `.cyberctf-instance`, `.cyberctf-ports.yml` (`src/runtime/paths.rs:76-154`).
  - Under `<app data>/deployments/<lab>/<provider>/`: the Terraform state, `run.json`, `runtime`, `host`, `ssh-jump`, `known_hosts` (`src/runtime/paths.rs:38-68`).
  - Isoloom's registry `~/.isoloom/status.yml`, written by both (`src/runtime/lab/registry.rs`; `crates/isoloom-runtime/src/registry.rs:16-169`).
- **Discovery after a crash:** scan Docker for `cyberctf-` projects and the hypervisors for `<lab> · <machine>` VMs (`src/runtime/discover.rs:1-12`).
- **Isoloom's secrets delivery** (to move in, PLAN §0):
  - Stores: `env`, `file`, `vault`, `sops` (`crates/isoloom-runtime/src/secrets.rs:59-128`).
  - VMs get base64 lines into `/var/lib/isoloom/secrets`, then `.delivered`; the guest waits up to 1800 s.
  - Transports: `vagrant ssh`, plain `ssh` (with `-J` through the router), or WinRM through `python3` and pywinrm. Proxmox and ESXi reach WinRM over an SSH tunnel.
  - Retries: a round every 3 s, 120 s per try, and 120 s or 1800 s after the tool ends (agent read of `crates/isoloom/src/secrets/vms.rs:31-36,99-310`, `windows.rs:52-200`, `crates/isoloom/src/commands/run/deliver.rs:22-113`).
- **Isoloom's host readiness:** a probe per target, for example `docker version` and `docker compose version --short` not 1.x; `vagrant plugin list` and the hypervisors; `terraform version -json`; the cloud environment (`crates/isoloom-runtime/src/host.rs:230-427`). Hyper-V elevation is checked with PowerShell (`host/hyperv.rs:8-33`).
- **Isoloom's events:** `step`, `log`, `check` and `result`, tagged by `event`, plus stable `ErrorCode`s (`crates/isoloom-runtime/src/events.rs:45-162`). The runner's event stream evolves from these (runner-extraction.md §3).

### 3.6 What the manifest fixes: today's implicit contract and where it breaks

**Scraped from text today. The manifest declares it instead:**
- the controller's presence, from `config.vm.define "isoloom-controller"` (`src/runtime/vm.rs:29-31`);
- VM names, from `v.name` / `guest_name` / `displayName` lines (`src/runtime/vm/recover.rs:56-86`);
- the topology, from `private_network` lines (`src/runtime/vm/status.rs:78-111`);
- the Compose project, from the `name:` line (`src/runtime/status.rs:100-107`);
- the access stand-ins, from a `"\n  isoloom-access:\n"` substring (`src/runtime/docker/check.rs:45-47`);
- the start plan, from the Compose YAML (`src/runtime/lab/mod.rs:284-286`);
- services, from `isoloom.service.*` / `cyberctf.service.*` labels (`src/runtime/docker/inspect.rs:131-149`).

**Already broken against Isoloom `main`:**
- Isoloom now labels ports `isoloom.port.<name>: http:<port>` (generated `compose.yml`), so the launcher reads no services.
- Proxmox VM labs output `address`, not `ip`, and no `ready_file` (§3.3).
- The ready protocol (`running:`/`failed:` and `/var/log/cyberctf-lab.log`) is launcher legacy (§3.3).
- The modules' `slot` variable defaults to 1 and is never set, so two labs on one Proxmox server collide (agent read of isoloom@50147b3 `generate/proxmox.rs:177-181`).
- `expires_at` is never passed to the cloud modules (§3.4).
- On Vagrant, inputs on a **Windows** machine are read into `INPUTS` but given to no provisioner: a Windows VM never gets its flag (generated `mini-ad` Vagrantfile, `INPUTS` at `:394`, no `inputs` step on `dc01`). Proxmox and ESXi refuse them outright.
- The launcher's 5-minute check cap (`src/runtime/docker/check.rs:54`) is shorter than the checks' own `wait`. 659 corpus checks set one, up to 1500 s, and a runner's waits add up: the generated `flag-portal` networks runner needs 360 s before any margin.

**Guessed today:**
- the "Open" URL, from datastore-looking names (`src/runtime/docker/inspect.rs:22-70`);
- infra containers, by name (`src/runtime/model.rs:43-48`);
- the route keepers (`<m>-routes`) of `internet: false` labs aren't known to the launcher at all, and show as machines.

---

## 4. The format, field by field

Every field is in the schema with its description. This table gives each field's purpose and what it replaces.

| Field | Holds | Replaces |
|---|---|---|
| `manifest_version`, `$schema` | `0`, and the schema URL | (nothing: today's contract is unversioned; `resolved.json` has an internal `resolved_version: 2`, `crates/isoloom-targets/src/resolved.rs:23`) |
| `generator` | `isoloom` and its version | (provenance only) |
| `lab.name`, `lab.instance` | The spec's name, the instance suffix, the instance number | `src/runtime/lab/mod.rs:81-139` (instance), `src/runtime/discover.rs:40` (name read from the YAML) |
| `lab.message` | The `message:` with placeholders rewritten to manifest roots (`${published.web.80}`) | `src/runtime/lab/mod.rs:241-271` (the resolved snapshot plus published ports); `src/runtime/launch.rs:221-231` |
| `target`, `cloud`, `driver` | Which output this is, and which driver runs it | `src/runtime/lab/names.rs:30-69` (folder ↔ target), `src/runtime/launch.rs:185-213` (`LocalRun`), `src/runtime/server/profile.rs:76-87` (`terraform_target`) |
| `hosts` | The host OSes the target runs from | the plan's host-platform rule (PLAN §2) |
| `tools[]` | Each tool, a read-only probe, a minimum version, a pinned container fallback (`container`, `container_hosts`), `hosts` and `providers` filters | `src/runtime/docker/lifecycle.rs:12-22` (engine up), `src/runtime/terraform/runner.rs:16,24,44-48` (Terraform and its image), `src/runtime/providers.rs:330` (`ensure_usable`), `crates/isoloom-runtime/src/host.rs:261-384` (probes) |
| `identity.compose` | Project (`${run.name}`), files, the runner's pinned-ports file | `src/runtime/docker/compose.rs:11-37` (`cyberctf-<id>`, `-p/-f`), `src/runtime/lab/overrides.rs` (the `name:` rewrite is no longer needed: `-p` is always passed), `src/runtime/docker/compose.rs:16-19` (`.cyberctf-ports.yml` moves to the run's state) |
| `identity.vagrant.providers[]` | Each provider with its hosts, plugin, `elevation`, `remote`, and Isoloom's `refusal` | `src/runtime/providers.rs:14-112,186-330`, `src/exec/vagrant.rs:88-96` (Hyper-V elevation), `src/runtime/launch/local.rs:78-95` (`qemu_refusal`) |
| `identity.terraform` | The module folder, `state: runner`, Isoloom's var files | `src/runtime/lab/names.rs:55-91`, `src/runtime/paths.rs:38-51`, `src/runtime/terraform/services.rs` (`-state`, `-var-file`) |
| `params[]` | Non-secret run-time values: name, default, allowed values, `hosts`/`providers` filters, `set_by` (`runner` or `embedder`), deliveries | `ISOLOOM_PUBLISH_ADDRESS/FIXED` (`crates/isoloom-compose/src/docker.rs:22-26`); `CYBERCTF_PORTS` (`src/runtime/docker/lifecycle.rs:70-91`); `ISOLOOM_HYPERV_SWITCH` and `ISOLOOM_SSH_PROXY_COMMAND` (`src/runtime/lab/names.rs:41-44,112-115`); `ESXI_*` (`src/runtime/lab/vars.rs:14-28`); Terraform variables (`src/runtime/lab/vars.rs:37-130`); `allowed_cidr` (`src/runtime/launch/on_host.rs:126-184`) |
| `secrets[]` | Slots: `kind` (`input`, `secret`, `connection`, `generated`), `required`, `sensitive`, `machines`, `providers`, an optional store `source`, deliveries | `src/labs.rs:240-247` (allowed launch names), `src/runtime/lab/mod.rs:333-341` (`inputs_json`), `src/runtime/server/contract.rs:22-88` (connection env), `src/runtime/cloud_services.rs:94-148` (inputs file), `crates/isoloom-runtime/src/secrets.rs` (stores), `crates/isoloom/src/secrets*.rs` (guest delivery) |
| `delivery` | `env` (to named steps), `tf_var` (`string` or a `map_entry` of `inputs`), `state_file` (0600, kept until `down`), `guest` (Isoloom's in-guest delivery over SSH, `vagrant ssh` or WinRM, during a named step) | the `env` arguments threaded through every call today; `src/runtime/terraform/mod.rs:107-111`; `src/runtime/cloud_services.rs:141-148`; `crates/isoloom/src/secrets/vms.rs` |
| `networks[]` | Name, CIDR, `internet`, and the native name (`${run.name}_lab`, `isoloom-<lab>-lab`) | `src/runtime/docker/inspect/networks.rs:6-21` (the network probe), `src/runtime/vm/status.rs:82-111` (Vagrantfile scraping), `src/runtime/lab/names.rs:100-110` |
| `machines[]` | Every machine the target creates, with `role` (`target`, `job`, `infra`, `access`, `tool`, `check`) and `of`; OS, Windows, arch, resources, static addresses; `native` (Compose service, Vagrant define name, VM name per provider); volumes; ports with `http`, `tls` and `publish` (`ephemeral` with a `preferred` number, or `fixed`); `keep_running`; `on_demand` (the learner's access machine, never brought up by the initial `up` — the runner hot-adds it) | `src/runtime/docker/inspect.rs:22-70,95-175` (inspect, labels, datastore guess, serving), `src/runtime/model.rs:43-48` (infra by name), `src/runtime/vm/recover.rs:56-86` (VM names), `src/runtime/vm.rs:25-31` (controller), `src/runtime/launch/local.rs:39-74` (memory, arch), `src/runtime/docker/check.rs:40-47` (stand-ins) |
| `lifecycle.up/down/park/resume/provision` | Ordered steps: `argv` or `action`, `cwd`, non-secret `env`, `stdin`, `timeout_s`, `when` (`if_ok`/`always`), `failure` (`fail`/`tolerate`), `hosts`/`providers` filters, `verdict` | `src/runtime/docker/lifecycle.rs:118-246`, `src/runtime/vm.rs:40-256`, `src/exec/vagrant.rs:14-44` (`Step`, `When`: the shape is lifted from there), `src/runtime/terraform/*`, `crates/isoloom/src/commands/run/bring_up.rs` |
| `lifecycle.status` | The read-only probe: an argv, or an action that reads files (`terraform.read_state`) | `src/runtime/docker/inspect.rs:184-204`, `src/runtime/vm/status.rs:25-76`, `src/runtime/terraform/state.rs:53-146` |
| `ready[]` | `step_ok`, `machines_running`, `ready_file` (grammar `isoloom-ready/1`, timeouts, an optional jump), `tcp` | `--wait` and the job exit codes (`src/runtime/docker/lifecycle.rs:139-149`), `src/runtime/vm/status.rs:31-35`, `src/runtime/terraform/ready.rs:10-100` |
| `outputs[]` | Name, role (`entry`, `ssh`, `address`, `credential`, `info`), `sensitive`, `format`, and the source (`published_port`, `terraform_output`, `static`) | `src/runtime/docker/inspect.rs:57-70` (Open URL), `src/runtime/docker/lifecycle.rs:112-116`, `src/runtime/lab/names.rs:117-127`, `src/runtime/cloud_services.rs:248-262` |
| `checks` | Protocol `isoloom-check/1`; `setup` steps; runners (`from`, a step whose timeout covers its checks' waits, the checks in order with `kind`, `wait_s`, `derived`; `on_timeout`); `teardown` | `src/runtime/docker/check.rs:40-143`, `crates/isoloom/src/commands/test/*`, `crates/isoloom-plan/src/checks/runner.rs:254-278`, `crates/isoloom/src/tools/vagrant.rs:22-91` |
| `recovery` | `stale_markers`, clean-up steps, `retries`; Terraform `apply_retries` | `src/runtime/vm.rs:258-273`, `src/runtime/launch/local.rs:97-124`, `src/runtime/vm/recover.rs:183-202`, `src/runtime/terraform/services.rs:41-50` |
| `expiry` | The Terraform variable the runner passes the computed expiry in, and its unit | `src/runtime/terraform/state.rs:26-37,151-161`, `src/runtime/lifecycle.rs:238-318` |

**What the manifest deliberately leaves to the runner:**
- the run name, the state folder and the lock;
- the event stream;
- which provider and server a run uses;
- credentials and their storage;
- elevation, relays and process trees;
- discovery.

**What it leaves to the embedder:** budgets, the GCP labs project, accounts and the UI (runner-extraction.md §1).

### 4.1 The on-demand access machine

The learner's access machine (their attacker box) is a real machine of the lab, marked `on_demand`
in `machines[]`, that the initial `up` never starts. The runner hot-adds it onto a chosen lab
network when the learner asks, and removes it again. The backends render it so `up` skips it:

- **Compose:** the access service (and its route keeper) sit in the `access` Compose profile, so
  `docker compose up` passes them by. The runner hot-adds with
  `docker compose --profile access up -d --no-deps <service>`, and removes with
  `docker compose --profile access rm -sf <service>`. A lab that authors its own attacker image (a
  `build`) keeps it; otherwise the service's image is `${ISOLOOM_ACCESS_IMAGE:-<default>}`, so the
  runner can override it to the box the learner picks (the `<default>` is the lab's or `kalilinux/kali-rolling`).
- **Vagrant:** the access VM is defined `autostart: false`, so `vagrant up` skips it. The runner
  hot-adds with `vagrant up <name>`, and its box is `ENV.fetch("ISOLOOM_ACCESS_BOX", "<default>")`,
  so the runner can override it to the box the learner picks (`<default>` the lab's or `kalilinux/rolling`).

The profile name and environment-variable names are the shared `isoloom_manifest::access` constants
(`PROFILE`, `IMAGE_ENV`, `BOX_ENV`, `DEFAULT_IMAGE`, `DEFAULT_BOX`).

---

## 5. Placeholders

Syntax: `${root.path}`, with `$$` for a literal `$`.

- **Why not `{{ }}`:** Docker's Go templates (`docker info --format {{.ServerVersion}}`) collide with it.
- **Why not `$NAME`:** a shell-like name would read as the runner doing environment expansion, which it never does.
- **Where they're allowed:** only in `argv` items, step `env` values, `with` values, conditions, outputs' `format`, network and volume native names, and `lab.message`.

| Root | Bound by | Example |
|---|---|---|
| `run.name` | the embedder (default `lab.name`) | `cyberctf-invoice-portal-api` |
| `run.dir` | the runner: the manifest's folder, absolute | |
| `run.state_dir` | the runner: the run's private folder | `…/runs/<name>/` |
| `run.provider` | the embedder's choice among `identity.vagrant.providers` | `virtualbox` |
| `run.ssh_key`, `run.known_hosts` | the runner's key and the run's own `known_hosts` | |
| `params.NAME` | a param's value | |
| `outputs.name` | an output, once it can be read | `${outputs.ip}` |
| `published.<machine>.<port>[.udp]` | the host port the machine's port got | `${published.web.80}` |
| `machine` | a `provision.machine` step | |
| `value` | an output's raw value, in its `format` | |

There is no `secrets.` root: the schema refuses it.

---

## 6. Driver actions: the documented primitives

An action is a step whose commands depend on what a read returned. Its native commands are fixed here. The runner's tests assert them with the fake-tools harness lifted from `src/exec/fake.rs`.

| Action | Native commands | From |
|---|---|---|
| `compose.pin_ports` | Only when the project has no container (`compose ps -a --format json` is empty).<br>1. Write `services: {}` to the pinned-ports file.<br>2. Run `compose config --format json` and pick a free loopback port per ephemeral publish, holding each socket until all are picked.<br>3. With `ISOLOOM_PUBLISH_FIXED`, use the `preferred` port instead, once per protocol.<br>4. Write `ports: !override` per service. | `src/runtime/docker/lifecycle.rs:74-110`, `src/runtime/docker/compose.rs:229-269` |
| `compose.check_ports_free` | Unless a service runs (`compose ps --format json`): each published host port from `compose config --format json` must bind over its protocol on 0.0.0.0 and 127.0.0.1. Otherwise a typed `PortInUse`. | `src/runtime/docker/lifecycle.rs:24-33,125-137` |
| `compose.remove_stale_networks` | `docker network ls --filter label=com.docker.compose.project=P --format {{.Name}}`, minus `compose config`'s networks. Each remaining one: `docker network inspect -f {{range .Containers}}{{.Name}} {{end}}`, then `docker network disconnect -f`, then `docker network rm`. | `src/runtime/docker/lifecycle.rs:186-246` |
| `compose.remove_containers` | `docker ps -aq --filter label=com.docker.compose.project=P [--filter label=com.docker.compose.service=S --filter label=com.docker.compose.oneoff=True]`, then `docker rm -f <ids>`. | `src/runtime/docker/compose.rs:103-134` |
| `compose.remove_networks` | Every project network, unplugged and removed, as above. | `src/runtime/docker/lifecycle.rs:224-231` |
| `vagrant.reconcile_provider` | If `.vagrant/machines/*/<other>/id` exists: `vagrant destroy --force` (bounded 180 s), then remove `.vagrant`. | `src/runtime/vm/recover.rs:6-46` |
| `vagrant.discard_aborted_saved` | For each VirtualBox VM name: `VBoxManage showvminfo <vm> --machinereadable`; on `VMState="aborted-saved"`, `VBoxManage discardstate <vm>`. | `src/runtime/vm.rs:114-126` |
| `vagrant.remove_orphans` | VirtualBox: `VBoxManage list vms`, then for this run's exact names `discardstate`, `controlvm <uuid> poweroff` and `unregistervm <uuid> --delete`; then the leftover folders under `VBoxManage list systemproperties`' machine folder. Parallels: `prlctl list -a --no-header -o name`, then `prlctl stop <vm> --kill` and `prlctl delete <vm>`. | `src/runtime/vm/recover.rs:88-181` |
| `vagrant.reset_state` | Remove `.vagrant` (or the run's `VAGRANT_DOTFILE_PATH`, open question 6). | `src/runtime/vm/recover.rs:201` |
| `vagrant.wake_controller` | 1. `vagrant status isoloom-controller --machine-readable`.<br>2. While it reports running: `vagrant ssh isoloom-controller -c "test -e /run/isoloom-halting"`, up to 60×2 s.<br>3. `vagrant up isoloom-controller --no-provision`. | `crates/isoloom/src/tools/vagrant.rs:22-91` |
| `terraform.clear_lock` | Remove `${run.state_dir}/.terraform.tfstate.lock.info`. Safe because the runner's run lock is the only writer of that state. | `src/runtime/terraform/runner.rs:28-32` |
| `terraform.read_state` | Read `${run.state_dir}/terraform.tfstate`: outputs, resources and taint. No process. | `src/runtime/terraform/state.rs:53-161` |
| `ssh.run` | `ssh -i ${run.ssh_key} -o UserKnownHostsFile=${run.known_hosts} -o StrictHostKeyChecking=accept-new -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 [-J <via>] <user>@<host> <command>`, the step's `stdin` piped. `with`: `host`, `user`, `via`, `command`. | `src/runtime/ssh.rs:121-185`, `crates/isoloom/src/commands/test/remote.rs` |
| `project.archive` | The project as a `.tar.gz` in `${run.state_dir}`, built in Rust (`tar` and `flate2`), excluding `.git`, `.vagrant`, `.terraform` and `.isoloom-project*`. Passed to the module as a `tf_var`. It replaces the generated modules' sh-only `local-exec tar` (open question 4). | `examples/supplier-portal-api/.isoloom/docker-vm/proxmox/main.tf:243-245` |

---

## 7. Runner semantics the manifest relies on

These are fixed by the runner, not repeated in every manifest.

1. **One operation per run.** An in-process mutex plus an OS file lock in the run's state folder. A worker and the app share it, and it is released when the holder dies. **It fails closed** where the launcher fails open (`src/runtime/inflight.rs:111-114`).
2. **`up` on an occupied slot:**
   - Running → `AlreadyRunning`.
   - Parked with something left → `resume` instead (an option the embedder sets).
   - Anything else left over → `down`, then `up`.
   - Recorded on another server → `RunningElsewhere`.
   (`src/runtime/launch.rs:46-129`; `src/runtime/launch/local.rs:9-34`.)
3. **Inputs reach exactly the steps that create things.**
   - On Compose that is `up` and the jobs. `resume` is `start` and never re-creates a container, so a flag written once into a volume stays valid without the value again. `down` removes the volume with the rest.
   - On Vagrant that is `up` and `provision`. The runner can't replay inputs it never kept (open question 2).
4. **Check timeouts follow the checks.** Isoloom computes each runner's `timeout_s` as the sum of its checks' `wait_s`, plus 15 s per check (curl and nc each cap a probe at 5–10 s), plus 60 s to start the runner. Both worked examples use this rule.
5. **Masking.** Every value of a `sensitive` slot is replaced by `***` in `log` events, error tails and the welcome message, before an event leaves the runner (`src/runtime/cloud_services.rs:490-502` does this for outputs today).
6. **Elevation.** For a provider with `elevation: hyperv` on Windows, all the steps of one operation (not `status`) run as one elevated batch, through `src/exec/elevate.rs` as it is.
7. **State.** Pins, the Terraform state, the replay record, `known_hosts`, the inputs file and the parked mark all live in `${run.state_dir}`, outside the lab folder, so a reinstalled lab never orphans a run (`src/runtime/terraform/mod.rs:8-9` gives the reason).

---

## 8. Versioning, determinism and validation

- **Generation:** a function per target in Isoloom (`isoloom_targets::manifest(&Environment, Target, Option<Cloud>) -> Manifest`), written as one more `GeneratedFile`. That way `isoloom check` ("is this file out of date") covers it too.
- **Snapshots:** `insta` per example per target, under Phase 0's characterization net.
- **Validation:**
  - The JSON Schema is generated from the Rust types (schemars, already an Isoloom dependency, `crates/isoloom-spec/Cargo.toml`) and committed. A test fails when it is stale.
  - Every fixture validates against it, in both repositories.
  - The runner also checks what JSON Schema can't express:
    - step ids unique per list;
    - every `deliver.steps` id exists;
    - every `${…}` root is known;
    - `ready.step_ok` names an `up` step.
- **Version policy:** v0 holds until Phase 7. At 1.0, `manifest_version: 1` is frozen. Additive optional fields are allowed in 1.x, and a runner ignores unknown fields only under `x-`. A new required field or a changed meaning is version 2.

---

## 9. Worked example: `docker`

**`flag-portal`**, generated with today's Isoloom (0.10.1) from a synthetic spec shaped like the corpus. Phase 2's lab, invoice-portal-api, has the same shape: an init job, a named volume, inputs, a published HTTP port, and an `http` check plus a `script` check, with the internet left open. The spec:
- one network, `lab` 10.30.0.0/24, with `internet: false`;
- `db` (mysql:8.0);
- `web`, built from `build/web`, with `inputs: [CTF_FLAG_MAIN]`, `init: [ctf/flag.sh]`, volume `ctf: /ctf`, and port 80 `http`, `publish: 8080`;
- checks: `http` (`wait` 60), `tcp` (`wait` 120), `script` (`wait` 180);
- a message pointing at the published port.

It covers what the corpus uses most:

| Corpus construct | Labs | Where in the manifest |
|---|---|---|
| The flag: an input, plus an init script, plus a volume | 330 | The slot `CTF_FLAG_MAIN` (sensitive), delivered to `up` and `job-web-init-1` only. The job `web-init-1` has role `job` and its own step and ready condition. The volume `ctf` is kept by park and resume, removed by `down`. |
| One network with `internet: false` | 327 | `networks[0].internet: false`. The route keepers `db-routes` and `web-routes` are role `infra`, so they are never shown as machines and never counted as down. |
| Published port marked `http` | 368 ports | `publish.mode: ephemeral`, `preferred: 8080`, pinned by `compose.pin_ports`. Output `portal` has role `entry` (the Open button, no guessing), and the message uses `${published.web.80}`. |
| Checks: `http`, `script`, `tcp`, with `wait` | 822 checks, 659 with `wait` | One runner per position; each check with its `kind` and `wait_s`; runner timeouts per §7.4 (the networks runner: 360 + 45 + 60 → 480 s). |

```json
{
  "$schema": "https://www.isoloom.com/schema/run-manifest/v0.json",
  "manifest_version": 0,
  "generator": { "name": "isoloom", "version": "0.10.1" },
  "lab": {
    "name": "flag-portal",
    "instance": null,
    "message": "The portal answers at http://localhost:${published.web.80}/."
  },
  "target": "docker",
  "driver": "compose",
  "hosts": ["linux", "macos", "windows"],
  "tools": [
    { "name": "docker", "probe": ["docker", "info", "--format", "{{.ServerVersion}}"] },
    { "name": "docker-compose", "probe": ["docker", "compose", "version", "--short"], "min_version": "2.17.0" }
  ],
  "identity": {
    "compose": {
      "project": "${run.name}",
      "files": ["compose.yml", "${run.state_dir}/ports.yml"],
      "pinned_ports_file": "${run.state_dir}/ports.yml"
    }
  },
  "params": [
    {
      "name": "ISOLOOM_PUBLISH_ADDRESS",
      "description": "The host address published ports listen on.",
      "default": "127.0.0.1",
      "set_by": "embedder",
      "deliver": [{ "via": "env", "var": "ISOLOOM_PUBLISH_ADDRESS", "steps": ["*"] }]
    },
    {
      "name": "ISOLOOM_PUBLISH_FIXED",
      "description": "Set: each published port takes its `publish:` number instead of an ephemeral one.",
      "default": null,
      "values": ["1"],
      "set_by": "embedder",
      "deliver": [{ "via": "env", "var": "ISOLOOM_PUBLISH_FIXED", "steps": ["*"] }]
    },
    {
      "name": "ISOLOOM_DERIVED",
      "description": "0: the checks derived from the spec (reachability, no internet) are skipped.",
      "default": "1",
      "values": ["0", "1"],
      "set_by": "embedder",
      "deliver": [{ "via": "env", "var": "ISOLOOM_DERIVED", "steps": ["check-networks", "check-db", "check-web"] }]
    }
  ],
  "secrets": [
    {
      "name": "CTF_FLAG_MAIN",
      "kind": "input",
      "required": false,
      "sensitive": true,
      "machines": ["web", "web-init-1"],
      "deliver": [{ "via": "env", "var": "CTF_FLAG_MAIN", "steps": ["up", "job-web-init-1"] }]
    }
  ],
  "networks": [
    { "name": "lab", "cidr": "10.30.0.0/24", "internet": false, "native": "${run.name}_lab" }
  ],
  "machines": [
    {
      "name": "db",
      "role": "target",
      "arch": "amd64",
      "addresses": { "lab": "10.30.0.20" },
      "native": { "service": "db" },
      "ports": [{ "name": "mysql", "port": 3306, "protocol": "tcp", "http": false }]
    },
    { "name": "db-routes", "role": "infra", "of": "db", "addresses": {}, "native": { "service": "db-routes" } },
    {
      "name": "web",
      "role": "target",
      "arch": "amd64",
      "resources": { "cpus": 1, "memory_mb": 512 },
      "addresses": { "lab": "10.30.0.10" },
      "native": { "service": "web" },
      "volumes": [{ "name": "ctf", "path": "/ctf", "native": "${run.name}_web-ctf" }],
      "ports": [
        {
          "name": "portal",
          "port": 80,
          "protocol": "tcp",
          "http": true,
          "publish": { "mode": "ephemeral", "preferred": 8080, "address": "${params.ISOLOOM_PUBLISH_ADDRESS}" }
        }
      ]
    },
    { "name": "web-routes", "role": "infra", "of": "web", "addresses": {}, "native": { "service": "web-routes" } },
    { "name": "web-init-1", "role": "job", "of": "web", "addresses": {}, "native": { "service": "web-init-1" } },
    { "name": "isoloom-probe-amd64", "role": "infra", "addresses": {}, "native": { "service": "isoloom-probe-amd64" } },
    { "name": "isoloom-check", "role": "check", "addresses": {}, "native": { "service": "isoloom-check" } },
    { "name": "isoloom-check-db", "role": "check", "of": "db", "addresses": {}, "native": { "service": "isoloom-check-db" } },
    { "name": "isoloom-check-web", "role": "check", "of": "web", "addresses": {}, "native": { "service": "isoloom-check-web" } }
  ],
  "lifecycle": {
    "up": [
      { "id": "pin-ports", "action": "compose.pin_ports", "note": "Picking the lab's host ports…" },
      { "id": "ports-free", "action": "compose.check_ports_free" },
      { "id": "stale-networks", "action": "compose.remove_stale_networks", "failure": "tolerate" },
      {
        "id": "up",
        "note": "Starting the lab's containers…",
        "argv": [
          "docker", "compose", "--project-name", "${run.name}", "--file", "compose.yml", "--file", "${run.state_dir}/ports.yml",
          "up", "--detach", "--build", "--pull", "missing", "--wait", "--wait-timeout", "600",
          "db", "db-routes", "web", "web-routes", "isoloom-probe-amd64"
        ],
        "timeout_s": 1800
      },
      {
        "id": "job-web-init-1",
        "note": "Running web's set-up script…",
        "argv": [
          "docker", "compose", "--project-name", "${run.name}", "--file", "compose.yml", "--file", "${run.state_dir}/ports.yml",
          "up", "--no-deps", "--exit-code-from", "web-init-1", "web-init-1"
        ],
        "timeout_s": 600
      }
    ],
    "down": [
      {
        "id": "down",
        "argv": [
          "docker", "compose", "--project-name", "${run.name}", "--file", "compose.yml", "--file", "${run.state_dir}/ports.yml",
          "--profile", "check", "down", "--volumes", "--remove-orphans"
        ],
        "timeout_s": 300,
        "when": "always"
      },
      { "id": "containers", "action": "compose.remove_containers", "when": "always", "failure": "tolerate" },
      { "id": "networks", "action": "compose.remove_networks", "when": "always", "failure": "tolerate" }
    ],
    "park": {
      "pause": [
        {
          "id": "stop",
          "argv": ["docker", "compose", "--project-name", "${run.name}", "--file", "compose.yml", "--file", "${run.state_dir}/ports.yml", "stop"],
          "timeout_s": 300
        }
      ],
      "shutdown": [
        {
          "id": "stop",
          "argv": ["docker", "compose", "--project-name", "${run.name}", "--file", "compose.yml", "--file", "${run.state_dir}/ports.yml", "stop"],
          "timeout_s": 300
        }
      ]
    },
    "resume": [
      {
        "id": "start",
        "argv": ["docker", "compose", "--project-name", "${run.name}", "--file", "compose.yml", "--file", "${run.state_dir}/ports.yml", "start"],
        "timeout_s": 600
      }
    ],
    "status": {
      "id": "ps",
      "argv": ["docker", "compose", "--project-name", "${run.name}", "--file", "compose.yml", "--file", "${run.state_dir}/ports.yml", "ps", "--all", "--format", "json"],
      "timeout_s": 25
    }
  },
  "ready": [
    { "kind": "step_ok", "step": "up", "means": "every long-running service is healthy (`--wait`: the TCP probe on each declared port, the route keepers)" },
    { "kind": "step_ok", "step": "job-web-init-1", "means": "web's init script (the flag into the `ctf` volume) exited 0" }
  ],
  "outputs": [
    {
      "name": "portal",
      "description": "web's portal (port 80).",
      "role": "entry",
      "sensitive": false,
      "format": "http://127.0.0.1:${value}/",
      "from": { "kind": "published_port", "machine": "web", "port": 80, "protocol": "tcp" }
    }
  ],
  "checks": {
    "protocol": "isoloom-check/1",
    "setup": [],
    "runners": [
      {
        "id": "isoloom-check",
        "from": "from the environment's networks",
        "checks": [
          { "name": "http://web/", "kind": "http", "wait_s": 60, "derived": false },
          { "name": "db:3306", "kind": "tcp", "wait_s": 120, "derived": false },
          { "name": "build/check/check.sh", "kind": "script", "wait_s": 180, "derived": false }
        ],
        "step": {
          "id": "check-networks",
          "argv": [
            "docker", "compose", "--project-name", "${run.name}", "--file", "compose.yml", "--file", "${run.state_dir}/ports.yml",
            "--profile", "check", "run", "--rm", "--no-deps", "--env", "ISOLOOM_DERIVED", "isoloom-check"
          ],
          "timeout_s": 480
        },
        "on_timeout": [
          { "id": "rm-check-networks", "action": "compose.remove_containers", "with": { "service": "isoloom-check", "one_off": "true" }, "failure": "tolerate" }
        ]
      },
      {
        "id": "isoloom-check-db",
        "from": "from db",
        "checks": [
          { "name": "web:80 from db", "kind": "http", "wait_s": 30, "derived": true },
          { "name": "no internet from db", "kind": "http", "wait_s": 0, "derived": true }
        ],
        "step": {
          "id": "check-db",
          "argv": [
            "docker", "compose", "--project-name", "${run.name}", "--file", "compose.yml", "--file", "${run.state_dir}/ports.yml",
            "--profile", "check", "run", "--rm", "--no-deps", "--env", "ISOLOOM_DERIVED", "isoloom-check-db"
          ],
          "timeout_s": 120
        },
        "on_timeout": [
          { "id": "rm-check-db", "action": "compose.remove_containers", "with": { "service": "isoloom-check-db", "one_off": "true" }, "failure": "tolerate" }
        ]
      },
      {
        "id": "isoloom-check-web",
        "from": "from web",
        "checks": [
          { "name": "db:3306 from web", "kind": "tcp", "wait_s": 30, "derived": true },
          { "name": "no internet from web", "kind": "http", "wait_s": 0, "derived": true }
        ],
        "step": {
          "id": "check-web",
          "argv": [
            "docker", "compose", "--project-name", "${run.name}", "--file", "compose.yml", "--file", "${run.state_dir}/ports.yml",
            "--profile", "check", "run", "--rm", "--no-deps", "--env", "ISOLOOM_DERIVED", "isoloom-check-web"
          ],
          "timeout_s": 120
        },
        "on_timeout": [
          { "id": "rm-check-web", "action": "compose.remove_containers", "with": { "service": "isoloom-check-web", "one_off": "true" }, "failure": "tolerate" }
        ]
      }
    ],
    "teardown": []
  }
}
```

**Notes on the example:**
- **The wait list and the job** are the start plan Isoloom computes today (`crates/isoloom-compose/src/docker.rs:116-155`). `web-init-1` is a leaf job: only the check runners, in the `check` profile, depend on it. The manifest writes the plan out, so nobody parses the YAML again (`src/runtime/lab/mod.rs:284-286` and `bring_up.rs:84-98` go away).
- **`--build`** follows Isoloom. The launcher's `up` has no `--build` (`src/runtime/docker/lifecycle.rs:155`), so a lab updated in place keeps its old image `isoloom/<lab>-web` until it is removed.
- **Pause and shutdown are both `stop`**, as in the launcher today (`src/runtime/lifecycle.rs:31`). `docker compose pause` (freeze) is possible, but it doesn't survive a Docker Desktop restart.
- **The runner never reads the Compose file:** status is `ps --all --format json`, matched to `machines[].native.service`. Addresses and roles come from the manifest, and the published ports from `ps` publishers.
- **The attack box** is not in the manifest. It is a launcher hook around `up`, `down`, `park` and `resume` (runner-extraction.md §3.5).
- **One gap stays open:** the flag also reaches the long-running `web` container's environment (`CTF_FLAG_MAIN: ${CTF_FLAG_MAIN:-}` in the generated Compose file), where `docker inspect` or `/proc/1/environ` shows it. See open question 3.

---

## 10. Worked example: `vagrant`

**`mini-ad`**, generated with today's Isoloom from a synthetic spec shaped like the corpus's VM labs: mostly Windows, and a domain built with Ansible run from a controller. The spec:
- one network, `lab` 10.50.0.0/24;
- `dc01` (`windows-server@2019`, `inputs: [CTF_FLAG_MAIN]`, ports 389 and 445);
- `ws01` (`windows@10`, `depends_on: [dc01]`);
- an environment-level `provision` playbook with `requirements` and `groups`;
- checks: `tcp` (`wait` 300) and `script` (`wait` 600).

**Why the network has internet:** today's Isoloom refuses `internet: false` for a Windows machine on Vagrant ("comes later").

```json
{
  "$schema": "https://www.isoloom.com/schema/run-manifest/v0.json",
  "manifest_version": 0,
  "generator": { "name": "isoloom", "version": "0.10.1" },
  "lab": { "name": "mini-ad", "instance": null, "message": null },
  "target": "vagrant",
  "driver": "vagrant",
  "hosts": ["linux", "macos", "windows"],
  "tools": [
    { "name": "vagrant", "probe": ["vagrant", "--version"] },
    { "name": "virtualbox", "probe": ["VBoxManage", "--version"], "providers": ["virtualbox"] },
    { "name": "vmware", "probe": ["vmrun", "list"], "providers": ["vmware_desktop"] },
    { "name": "parallels", "probe": ["prlctl", "--version"], "providers": ["parallels"], "hosts": ["macos"] },
    { "name": "libvirt", "probe": ["virsh", "--version"], "providers": ["libvirt"], "hosts": ["linux"] }
  ],
  "identity": {
    "vagrant": {
      "providers": [
        { "id": "virtualbox", "hosts": ["linux", "macos", "windows"], "elevation": "none", "refusal": null },
        { "id": "vmware_desktop", "hosts": ["linux", "macos", "windows"], "plugin": "vagrant-vmware-desktop", "elevation": "none", "refusal": null },
        { "id": "hyperv", "hosts": ["windows"], "elevation": "hyperv", "refusal": null },
        { "id": "parallels", "hosts": ["macos"], "plugin": "vagrant-parallels", "elevation": "none", "refusal": null },
        { "id": "libvirt", "hosts": ["linux"], "plugin": "vagrant-libvirt", "elevation": "none", "refusal": null },
        { "id": "qemu", "hosts": ["linux", "macos"], "plugin": "vagrant-qemu", "elevation": "none", "refusal": null },
        { "id": "utm", "hosts": ["macos"], "plugin": "vagrant_utm", "elevation": "none", "refusal": null },
        { "id": "vmware_esxi", "hosts": ["linux", "macos", "windows"], "plugin": "vagrant-vmware-esxi", "elevation": "none", "remote": true, "refusal": null }
      ]
    }
  },
  "params": [
    {
      "name": "ISOLOOM_HYPERV_SWITCH",
      "description": "The Hyper-V virtual switch each VM's first adapter (Vagrant's connection, and the internet) goes on.",
      "default": "Default Switch",
      "hosts": ["windows"],
      "providers": ["hyperv"],
      "set_by": "embedder",
      "deliver": [{ "via": "env", "var": "ISOLOOM_HYPERV_SWITCH", "steps": ["*"] }]
    },
    {
      "name": "ISOLOOM_SSH_PROXY_COMMAND",
      "description": "The command Vagrant's SSH to the VMs goes through when this host can't reach them directly (an ESXi host through the runner's relay on macOS).",
      "default": null,
      "providers": ["vmware_esxi"],
      "set_by": "runner",
      "deliver": [{ "via": "env", "var": "ISOLOOM_SSH_PROXY_COMMAND", "steps": ["*"] }]
    },
    {
      "name": "ISOLOOM_DERIVED",
      "description": "0: the checks derived from the spec are skipped.",
      "default": "1",
      "values": ["0", "1"],
      "set_by": "embedder",
      "deliver": [{ "via": "env", "var": "ISOLOOM_DERIVED", "steps": ["check-controller"] }]
    },
    { "name": "ESXI_HOSTNAME", "description": "The ESXi host.", "required": true, "providers": ["vmware_esxi"], "set_by": "embedder", "deliver": [{ "via": "env", "var": "ESXI_HOSTNAME", "steps": ["*"] }] },
    { "name": "ESXI_HOSTPORT", "description": "Its SSH port.", "default": "22", "providers": ["vmware_esxi"], "set_by": "embedder", "deliver": [{ "via": "env", "var": "ESXI_HOSTPORT", "steps": ["*"] }] },
    { "name": "ESXI_USERNAME", "description": "The ESXi user.", "default": "root", "providers": ["vmware_esxi"], "set_by": "embedder", "deliver": [{ "via": "env", "var": "ESXI_USERNAME", "steps": ["*"] }] },
    { "name": "ESXI_DATASTORE", "description": "The datastore the VMs' disks go on.", "default": null, "providers": ["vmware_esxi"], "set_by": "embedder", "deliver": [{ "via": "env", "var": "ESXI_DATASTORE", "steps": ["*"] }] },
    { "name": "ESXI_VIRTUAL_NETWORK", "description": "Port groups, comma-separated: the management network, then one per lab network (lab).", "default": "VM Network", "providers": ["vmware_esxi"], "set_by": "embedder", "deliver": [{ "via": "env", "var": "ESXI_VIRTUAL_NETWORK", "steps": ["*"] }] }
  ],
  "secrets": [
    {
      "name": "CTF_FLAG_MAIN",
      "kind": "input",
      "required": false,
      "sensitive": true,
      "machines": ["dc01"],
      "deliver": [{ "via": "env", "var": "CTF_FLAG_MAIN", "steps": ["up", "provision-machine"] }]
    },
    {
      "name": "ESXI_PASSWORD",
      "kind": "connection",
      "required": true,
      "sensitive": true,
      "providers": ["vmware_esxi"],
      "deliver": [{ "via": "env", "var": "ESXI_PASSWORD", "steps": ["*"] }]
    }
  ],
  "networks": [
    { "name": "lab", "cidr": "10.50.0.0/24", "internet": true, "native": "isoloom-mini-ad-lab" }
  ],
  "machines": [
    {
      "name": "dc01",
      "role": "target",
      "os": "windows-server-2019",
      "windows": true,
      "arch": "amd64",
      "resources": { "cpus": 2, "memory_mb": 4096 },
      "addresses": { "lab": "10.50.0.10" },
      "native": {
        "vagrant": "dc01",
        "vm_names": {
          "virtualbox": "mini-ad · dc01",
          "vmware_desktop": "mini-ad · dc01",
          "parallels": "mini-ad · dc01",
          "utm": "mini-ad · dc01",
          "hyperv": "mini-ad-dc01",
          "vmware_esxi": "mini-ad-dc01"
        }
      },
      "ports": [
        { "name": "ldap", "port": 389, "protocol": "tcp", "http": false },
        { "name": "smb", "port": 445, "protocol": "tcp", "http": false }
      ]
    },
    {
      "name": "ws01",
      "role": "target",
      "os": "windows-10",
      "windows": true,
      "arch": "amd64",
      "resources": { "cpus": 2, "memory_mb": 4096 },
      "addresses": { "lab": "10.50.0.20" },
      "native": {
        "vagrant": "ws01",
        "vm_names": {
          "virtualbox": "mini-ad · ws01",
          "vmware_desktop": "mini-ad · ws01",
          "parallels": "mini-ad · ws01",
          "utm": "mini-ad · ws01",
          "hyperv": "mini-ad-ws01",
          "vmware_esxi": "mini-ad-ws01"
        }
      },
      "ports": []
    },
    {
      "name": "isoloom-controller",
      "role": "infra",
      "os": "alpine-3.19",
      "windows": false,
      "resources": { "cpus": 1, "memory_mb": 512 },
      "addresses": { "lab": "10.50.0.253" },
      "keep_running": false,
      "native": {
        "vagrant": "isoloom-controller",
        "vm_names": {
          "virtualbox": "mini-ad · controller",
          "vmware_desktop": "mini-ad · controller",
          "parallels": "mini-ad · controller",
          "utm": "mini-ad · controller",
          "hyperv": "mini-ad-controller",
          "vmware_esxi": "mini-ad-controller"
        }
      }
    }
  ],
  "lifecycle": {
    "up": [
      { "id": "reconcile", "action": "vagrant.reconcile_provider", "note": "Clearing the state of a run on another provider, if any…" },
      { "id": "discard-saved", "action": "vagrant.discard_aborted_saved", "providers": ["virtualbox"], "failure": "tolerate" },
      {
        "id": "up",
        "note": "Building the lab's VMs (Windows machines take a while)…",
        "argv": ["vagrant", "up", "--provider", "${run.provider}"],
        "timeout_s": 21600,
        "verdict": "fatal: \\[[^\\]]+\\]: (UNREACHABLE|FAILED)!.*"
      },
      {
        "id": "park-controller",
        "note": "Powering off the setup machine (it comes back for provisioning and checks)…",
        "argv": ["vagrant", "halt", "isoloom-controller"],
        "providers": ["virtualbox", "vmware_desktop", "hyperv", "parallels", "libvirt", "utm", "vmware_esxi"],
        "failure": "tolerate",
        "timeout_s": 300
      }
    ],
    "down": [
      { "id": "destroy", "argv": ["vagrant", "destroy", "--force"], "when": "always", "timeout_s": 1800 },
      { "id": "orphans", "action": "vagrant.remove_orphans", "providers": ["virtualbox", "parallels"], "when": "always", "failure": "tolerate" }
    ],
    "park": {
      "pause": [{ "id": "suspend", "argv": ["vagrant", "suspend"], "timeout_s": 1800 }],
      "shutdown": [{ "id": "halt", "argv": ["vagrant", "halt"], "timeout_s": 1800 }]
    },
    "resume": [
      { "id": "discard-saved", "action": "vagrant.discard_aborted_saved", "providers": ["virtualbox"], "failure": "tolerate" },
      { "id": "up", "argv": ["vagrant", "up", "--no-provision", "--provider", "${run.provider}"], "timeout_s": 3600 },
      {
        "id": "park-controller",
        "argv": ["vagrant", "halt", "isoloom-controller"],
        "providers": ["virtualbox", "vmware_desktop", "hyperv", "parallels", "libvirt", "utm", "vmware_esxi"],
        "failure": "tolerate",
        "timeout_s": 300
      }
    ],
    "provision": {
      "all": [
        { "id": "wake-controller", "argv": ["vagrant", "up", "isoloom-controller", "--no-provision"], "note": "Booting the setup machine…", "timeout_s": 1800 },
        {
          "id": "play",
          "argv": ["vagrant", "provision", "isoloom-controller", "--provision-with", "ansible"],
          "note": "Running the lab's setup play…",
          "timeout_s": 14400,
          "verdict": "fatal: \\[[^\\]]+\\]: (UNREACHABLE|FAILED)!.*"
        },
        { "id": "park-controller", "argv": ["vagrant", "halt", "isoloom-controller"], "when": "always", "failure": "tolerate", "timeout_s": 300 }
      ],
      "machine": [
        { "id": "wake-controller", "argv": ["vagrant", "up", "isoloom-controller", "--no-provision"], "note": "Booting the setup machine…", "timeout_s": 1800 },
        { "id": "provision-machine", "argv": ["vagrant", "provision", "${machine}"], "timeout_s": 7200 },
        {
          "id": "play",
          "argv": ["vagrant", "provision", "isoloom-controller", "--provision-with", "ansible"],
          "note": "Running the lab's setup play…",
          "timeout_s": 14400,
          "verdict": "fatal: \\[[^\\]]+\\]: (UNREACHABLE|FAILED)!.*"
        },
        { "id": "park-controller", "argv": ["vagrant", "halt", "isoloom-controller"], "when": "always", "failure": "tolerate", "timeout_s": 300 }
      ]
    },
    "status": { "id": "status", "argv": ["vagrant", "status", "--machine-readable"], "timeout_s": 25 }
  },
  "ready": [
    { "kind": "step_ok", "step": "up", "means": "every VM booted and ran its provisioners, the controller's Ansible play included" },
    { "kind": "machines_running", "machines": ["dc01", "ws01"] }
  ],
  "outputs": [
    { "name": "dc01", "description": "dc01's address on lab.", "role": "address", "sensitive": false, "from": { "kind": "static", "value": "10.50.0.10" } },
    { "name": "ws01", "description": "ws01's address on lab.", "role": "address", "sensitive": false, "from": { "kind": "static", "value": "10.50.0.20" } }
  ],
  "checks": {
    "protocol": "isoloom-check/1",
    "setup": [
      { "id": "wake-controller", "action": "vagrant.wake_controller", "note": "Booting the setup machine for the checks…", "timeout_s": 1800 }
    ],
    "runners": [
      {
        "id": "isoloom-check",
        "from": "from the environment's networks",
        "checks": [
          { "name": "dc01:389", "kind": "tcp", "wait_s": 300, "derived": false },
          { "name": "checks/kerberoast.sh", "kind": "script", "wait_s": 600, "derived": false },
          { "name": "dc01:389 from the networks", "kind": "tcp", "wait_s": 30, "derived": true },
          { "name": "dc01:445 from the networks", "kind": "tcp", "wait_s": 30, "derived": true }
        ],
        "step": {
          "id": "check-controller",
          "argv": ["vagrant", "provision", "isoloom-controller", "--provision-with", "checks"],
          "timeout_s": 1200
        }
      }
    ],
    "teardown": [
      { "id": "park-controller", "argv": ["vagrant", "halt", "isoloom-controller"], "when": "always", "failure": "tolerate", "timeout_s": 300 }
    ]
  },
  "recovery": {
    "stale_markers": ["verr_already_exists", "could not rename", "is locked", "different provider", "single provider at a time"],
    "steps": [
      { "id": "destroy", "argv": ["vagrant", "destroy", "--force"], "timeout_s": 180, "when": "always", "failure": "tolerate" },
      { "id": "orphans", "action": "vagrant.remove_orphans", "providers": ["virtualbox", "parallels"], "when": "always", "failure": "tolerate" },
      { "id": "reset", "action": "vagrant.reset_state", "when": "always" }
    ],
    "retries": 1
  }
}
```

**Notes on the example:**
- **One manifest for every Vagrant provider**, ESXi included:
  - The run picks `${run.provider}` among `identity.vagrant.providers`, filtered by the host's OS.
  - The `ESXI_*` params and the `ESXI_PASSWORD` slot apply only when the provider is `vmware_esxi`.
  - The launcher's legacy `CYBERCTF_ESXI_*` and `CYBERCTF_REMOTE_*` names go away (`src/runtime/server/contract.rs:22-41`).
- **The controller:**
  - It is role `infra`, so it is never counted in "running".
  - It is halted after `up` and after a re-provision, but not on QEMU, where it is the player's attack position (`src/runtime/vm.rs:33-38,61-73`). Isoloom's controller also powers itself off after its play (`touch /run/isoloom-halting`), so this step is a tolerated belt-and-braces.
  - `vagrant.wake_controller` handles a controller still halting.
- **Checks on VMs**, which the launcher never runs today, come from Isoloom's `test`. With no Linux target machine there is one runner: the controller's `checks` provisioner. Its timeout is 960 s of waits, plus 4×15 s, plus 120 s for the controller → 1200 s.
- **Long steps:** `up` gets a 6-hour cap. On QEMU, emulating x86 can take hours (`src/runtime/launch/local.rs:90-92`), and the runner keeps no shorter limit than the tool would.
- **Gap: Windows inputs.** The input slot names `dc01`, but the generated Vagrantfile reads `CTF_FLAG_MAIN` into `INPUTS` and gives it to no provisioner on a Windows machine. The manifest is right; the Vagrant backend must deliver it (a WinRM `inputs` step, or the controller's Ansible extra-vars). This is Phase 3's "secrets conversion" item, and on the corpus it affects the Windows VM labs that declare inputs.
- **Recovery** is the local one (`src/runtime/launch/local.rs:97-124`). ESXi gets it too, which the launcher doesn't do today. `remove_orphans` only knows VirtualBox and Parallels, as now.

---

## 11. The other Tier 1 targets, in brief

They use the same fields; each gets its own fixture in Phase 3.

| Target | Driver | `up` | Ready | Outputs | Checks | Park |
|---|---|---|---|---|---|---|
| `docker-vm` (local) | vagrant | `vagrant up --provider ${run.provider}`; the guest runs the start plan with `ISOLOOM_PUBLISH_FIXED=1` | `step_ok up` | `published_port` = the forwarded host port, **fixed** at `publish`. Two docker-vm labs publishing one port collide; the runner must check the port is free first (`compose.check_ports_free`'s rule, as a `vagrant.check_forwarded_ports` action) | `vagrant provision --provision-with checks`: the guest loops over the Compose runners | `pause` only (`src/runtime/lifecycle.rs:53-60`) |
| `docker-vm/proxmox`, `cloud-docker/<cloud>` | terraform | `terraform.clear_lock`, `project.archive`, `terraform init … -backend-config=path=${run.state_dir}/terraform.tfstate`, `terraform apply -auto-approve -input=false -no-color`; env `TF_DATA_DIR=${run.state_dir}/.terraform`, `TF_IN_AUTOMATION=1` | `step_ok apply`, then `ready_file` (`${outputs.ip}`, `${outputs.ssh_user}`, `${outputs.ready_file}`, 2400 s, 10 s). **An unreachable host fails** after `unreachable_timeout_s`; it is not reported OK as today (`src/runtime/terraform/ready.rs:86-89`) | `ip` (role `ssh`), `ssh_user`, `ready_file`, `published` | `ssh.run` per runner: `cd /opt/isoloom && sudo docker compose -f .isoloom/docker/compose.yml --profile check run --rm -e ISOLOOM_DERIVED <runner>` | absent |
| `proxmox` (VM labs) | terraform | as above | `step_ok apply` (the module waits in `remote-exec`) | `address` → role `ssh`; `machines`; `checks`. Isoloom should name it `ip` too, or the manifest maps it: the runner reads only the manifest's names | `ssh.run` with `via: isoloom@${outputs.address}`, `stdin: checks/<pos>.sh` | absent |
| `cloud-vm/<cloud>` | terraform | as cloud-docker; params `region`, `allowed_cidr` (`set_by: runner`: the agreed public IP), the size under the module's name; `expiry` → `expires_at` | as above | `machines`, `ssh_users`, `ip`, `ready_file`, `published` | `ssh.run` per position | absent |
| `cloud-services` | terraform | `terraform init`, then `apply -state=${run.state_dir}/terraform.tfstate -var-file=terraform.tfvars.json -var-file=${run.state_dir}/inputs.tfvars.json`; `recovery.apply_retries` from `cloud.apply_retries`. Isoloom's `up.sh`/`down.sh`/`outputs.sh` go (they are sh-only) | `step_ok apply` | `cloud.outputs` → `terraform_output` with `sensitive` | the spec's scripts, as `ssh`-less local steps; they need a host-neutral form (open question 4) | absent |

**Connection slots, per provider:**
- **Proxmox:** `proxmox_endpoint`, `proxmox_username`, `proxmox_insecure`, `node`, `datastore`, `uplink_bridge` and `slot` are params. `proxmox_password` or `proxmox_api_token` is a `connection` slot delivered as a `tf_var` (`src/runtime/lab/vars.rs:77-103`).
- **Clouds:** the provider's own environment, as `connection` slots or params: `AWS_*`, `ARM_SUBSCRIPTION_ID`, `DIGITALOCEAN_TOKEN`, `LINODE_TOKEN` (`src/runtime/server/contract.rs:65-88`).
- **`slot`** must be set per run (a runner-assigned number), or two labs on one Proxmox server collide (§3.6).

---

## 12. What Isoloom must do to write it (generator work)

- **Phase 2:**
  - `isoloom-manifest` (types, schema, fixtures).
  - The `docker` manifest, from `Environment`, the `StartPlan` (computed, not re-parsed), the checks plan (positions, runners, waits) and the published ports.
  - Message placeholders rewritten to manifest roots.
  - Snapshots and `verify` (`check-jsonschema` in the toolchain image).
- **Phase 3, per target:**
  - Vagrant: VM names and providers with their refusals.
  - Terraform: the outputs the manifest names, `expires_at` wired, `slot` exposed.
  - Windows inputs delivered.
  - The `ready_file` grammar written (`running: <step>` before each bootstrap step, `failed: <step>` on error).
  - The sh-only `local-exec` (`tar`, `.isoloom-reach` announcements) replaced by `project.archive` and module outputs.
  - `up.sh`/`down.sh`/`outputs.sh` dropped.

---

## 13. Open questions for the maintainer

1. **The run name and Compose project.** The launcher passes `cyberctf-<lab id>`. Should the runner default `run.name` to `lab.name` (today's Isoloom CLI) or require the embedder to set it? Recommended: the embedder sets it; the CLI defaults to `lab.name`.
2. **Keeping inputs for re-provision.** Today a re-provision gets no inputs (`src/runtime/lifecycle.rs:122-125`). Should the runner keep input values (the flag) in the OS keychain for the run's life, or must the embedder pass them again? Recommended: the embedder passes them again; the runner stores no secret values.
3. **The flag: a first-class form.** 330 labs repeat input + init + volume. Two choices:
   - deliver the value to the init job only (today it also sits in the long-running container's environment);
   - use a Compose `secrets:` file in the run's state folder, so no value is in `docker inspect`.

   Recommended: a first-class `flag:` in the new spec, lowered to a Compose secret and an init step (Phase 2 front end, or Phase 4).
4. **sh-only parts of generated files.** The Terraform modules' `local-exec tar` and `.isoloom-reach` `echo`, the cloud-services `*.sh`, and `cloud.requires` commands all break the host-neutral rule. Confirm the replacements: `project.archive` with a `tf_var`, module outputs, and for `cloud.requires` the per-OS `host_argv` form.
5. **The Vagrant Proxmox provider.** PLAN §1a names it, but the launcher never runs it (Proxmox is Terraform only), and `vagrant-proxmox` is unmaintained. Drop it from Tier 1?
6. **`VAGRANT_DOTFILE_PATH` in the run's state.** It would move Vagrant's `.vagrant/` out of the lab folder, so a reinstalled lab never loses track of its VMs, which today needs `remove_orphans`. Adopt it in Phase 3?
7. **Strictness.** `additionalProperties: false` everywhere in v0, so drift fails fast. Keep it strict until 1.0, then allow only `x-` extensions?
8. **Check runner timeouts.** The proposed rule is waits + 15 s per check + 60 s, computed by Isoloom. Or should Isoloom write only the waits and leave the timeout to the runner?
9. **`docker` `pause`.** Keep `stop` for both park modes (today), or make `pause` a real `docker compose pause`?
