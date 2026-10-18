# Runner extraction: carving the Isothrum runner out of the launcher

How the Isothrum runner crate is built from the launcher's runtime (the core, PLAN §0), Isoloom's run part (which moves in), and later Isothrum desktop's runner. It executes the run manifest designed in `run-manifest-v0.md`.

Citation convention as there:
- `src/…` is the launcher (`cyber-ctf-launcher/src-tauri/`).
- `crates/…` is Isoloom.
- `apps/…` and `crates/isothrum-*` are Isothrum.
- Sizes are code lines before the file's test module (the quality bar counts these: at most 400).

---

## 0. Decisions in one place

1. **Two new Isothrum crates.**
   - `isothrum-runner` (library): engine, drivers, store, events.
   - `isothrum` (CLI): a thin `clap` shell; it is also the detached worker from Phase 3.

   The manifest's types live in Isoloom (`isoloom-manifest`, serde only), and both depend on it.
2. **The manifest is the only input.** Drivers never read Isoloom's generated files, `resolved.json` or Isoloom's naming conventions.
3. **De-Tauri-fying is mostly plumbing.** The launcher's runtime already logs through `impl FnMut(String)` and returns `Result`; no file in it calls `emit` or uses `State`. Its `AppHandle` (89 parameters across `src/runtime`) serves two things:
   - the data folder (12 `app_data_dir` calls), which the runner's `Store` replaces;
   - the host profile behind `server::lab_connection`/`connection` (36 calls), which a `RunOptions` built by the embedder replaces.
4. **Drivers are a closed set**, dispatched through an enum (`AnyDriver`): `compose`, `vagrant`, `terraform`, the manifest's `driver` values. Steps are generic and executed by one executor; drivers implement only actions, status, outputs and existence.
5. **The embedder keeps its product.** The attack box, accounts, catalogue, budgets, host profiles, keychain and UI stay in the launcher. The runner offers hooks around lifecycle edges, and takes credentials as slot values, never from a store of its own.
6. **Phase 2 ships** the manifest executor, the store, events and the compose driver, behind a launcher switch, with invoice-portal-api's real-lab test passing through it.

---

## 1. Module-by-module map

**Legend:**
- **Move**: the code moves, imports and paths renamed.
- **De-Tauri**: moves after replacing `AppHandle`, `Channel` or `tauri::async_runtime`.
- **Rewrite on manifest**: the mechanics move, but what they read now comes from the manifest instead of Isoloom's files or names.
- **Stays**: launcher-only (CTF product).
- **Deleted**: the manifest makes it unnecessary.

### 1.1 Process execution (`src/exec/`, platform plumbing)

| Launcher | Size | Tauri | Fate | Notes |
|---|---|---|---|---|
| `src/exec/mod.rs` | 316 | none | **Move** → `runner::exec` | `run`, `run_env_timed`, `stream`, `stream_stdin`, `forward_lines` (lossy decode), `headless`, `status_within`. Replace the two `expect("piped …")` at `:269-270` with typed errors. |
| `src/exec/group.rs` | 185 | none | **Move** → `runner::exec::group` | Unix process groups and Windows job objects; `kill_live_tools` for a stopped worker. 8 `unsafe` blocks (see risk R6). |
| `src/exec/docker.rs` | 84 | none | **Move** → `runner::exec::docker` | Strips `DOCKER_HOST`/`DOCKER_CONTEXT` when the user picked a context. |
| `src/exec/shim.rs` | 31 | none | **Move** | `az`, `gcloud`, `oci` through `cmd /C`, and `check_args` refusing cmd.exe operators. |
| `src/exec/fake.rs` | 129 | none | **Move** (test-only) | The scripted-tools harness every driver test relies on. Lift it first: it makes the rest testable without tools. |
| `src/exec/vagrant.rs` | 165 | none | **Move** → `runner::driver::vagrant::batch` | `Step`/`When`/`Vagrant::batch`. The manifest's step `when` and `failure` are this model, generalised to every driver. |
| `src/exec/elevate.rs` | 745 | none | **Move, split**, Phase 3 | Hyper-V elevation: a PowerShell batch, an env file, one UAC prompt. Over the 400-line bar; split script building / Windows FFI / batch protocol. 5 `unsafe`. |
| `src/platform/console.rs` | 48 | none | **Move**, Phase 3 | Ctrl+C to a Terraform on Windows through a throwaway process. The helper becomes `isothrum interrupt --pid` (or the embedder's exe: open question Q4). |
| `src/platform/env_path.rs` | 167 | none | **Move** → `runner::host::path` | The PATH a GUI app lacks (macOS, Linux), and Windows' registry PATH refresh. The embedder calls `runner::host::path::complete()` at start. |
| `src/lan_relay/*` | 423 | none | **Move**, Phase 3, feature `lan-relay` | macOS keeps third-party tools off the LAN (ESXi through Vagrant): SSH/HTTPS relays, a WinRM proxy, an `ovftool` wrapper. Any GUI embedder on macOS needs it, the Isothrum desktop app included. |
| `src/private_fs.rs`, `src/atomic_file.rs` | 85, 45 | none | **Move** → `runner::store::fs` | Owner-only files and folders; atomic writes. Windows ACLs are a no-op today (R7). |

### 1.2 Lab operations (`src/runtime/`)

| Launcher | Size | Tauri | Fate | Notes |
|---|---|---|---|---|
| `docker/compose.rs` | 270 | none | **Move** → `driver::compose::cli` | `parse_ps` (both formats), `published_from_config`, `pinned_ports`, `remove_one_off`, `remove_all_containers`. `args()` goes: the argv is in the manifest. `project()`'s `cyberctf-` prefix goes to the launcher (its run name). |
| `docker/lifecycle.rs` | 247 | none | **Rewrite on manifest** → `driver::compose::{pins, ports, networks}` | `pin_ports`, `pick_pins`, `port_in_use`, `free_port` and `remove_stale_networks`/`remove_network` become the actions `compose.pin_ports`, `check_ports_free`, `remove_stale_networks`, `remove_networks`. `start`/`stop`/`park`/`resume` disappear into manifest steps. The `exegol::stop/rejoin` calls (`:172,181-182,221`) become launcher hooks. |
| `docker/inspect.rs` + `inspect/networks.rs` | 336 + 78 | none | **Rewrite on manifest** → `driver::compose::status` | Keep `ps` parsing, `down_serving` and the health → `unhealthy` mapping. Roles, addresses and networks come from the manifest. **Delete** `is_datastore`, `declared_services` (label scraping, already broken against Isoloom main) and `first_published_url` (the manifest's `entry` output). `HostProbe` (a remote `sh` script, `:216-223`) moves to the terraform driver's remote status (Phase 3). `subnets_in_use` moves as is: the embedder needs it to pick an instance. |
| `docker/check.rs` | 144 | none | **Rewrite on manifest** → `runner::checks` | Generic over drivers: setup, then runners with a per-runner timeout and `on_timeout`, `record`, `fail_unreported`, teardown. The line grammar comes from `isoloom-plan`'s parser (`crates/isoloom-plan/src/checks/runner.rs:254-278`), moved into `isoloom-manifest` so both sides share it. The 5-minute cap (`:54`) is deleted; the manifest's timeouts replace it. |
| `vm.rs` | 289 | none | **Rewrite on manifest** → `driver::vagrant` | `run_parts`, `ansible_verdict` and `lead_with_verdict` become the executor's `verdict`. `discard_aborted_saved` becomes an action. `start`/`provision`/`park`/`resume`/`stop` become manifest steps. `has_controller` (text scrape) is deleted. `is_stale_state_error` becomes the manifest's `recovery.stale_markers`. |
| `vm/status.rs` | 132 | none | **Rewrite on manifest** | Keep `parse_status` and `provider-name`. Topology comes from the manifest; `vagrant_topology` (Vagrantfile scraping) stays only for the launcher's attack VM. |
| `vm/recover.rs` | 202 | none | **Rewrite on manifest** → actions `vagrant.reconcile_provider`, `remove_orphans`, `reset_state` | `vm_names` (scraping) is deleted: `machines[].native.vm_names`. The exact-match rule and folder candidates stay. |
| `terraform/mod.rs` | 113 | none (`ssh::launcher_key` OnceLock) | **Rewrite on manifest** → `driver::terraform` | `apply`/`destroy` become manifest steps. `with_env` (`TF_VAR_*`, clash rule) becomes delivery. The replay record becomes the runner's `RunRecord`, which keeps **every** non-secret param, so a destroy replays the same region, not the profile's current one (R2). |
| `terraform/runner.rs` | 148 | none | **Move** | Host binary or the pinned container (macOS), mounts, `init_failure`. Whether to use the container becomes `tools[].container_hosts` plus a runner policy decided once per run, not on every call (`:44-48`). |
| `terraform/ready.rs` | 103 | none | **Move** → the `ready_file` condition | Same grammar and polling. **Change**: an unreachable host fails after `unreachable_timeout_s` instead of returning OK (`:67-70,86-89`, R2). |
| `terraform/state.rs` | 162 | none | **Move** → action `terraform.read_state`, `RunRecord.expires_at` | Output names come from the manifest's outputs, not `lab::outputs`. |
| `terraform/services.rs` | 131 | none | **Rewrite on manifest** | `-state`, `-var-file`, `TF_DATA_DIR` and `apply_retries` come from the manifest's steps and `recovery`. 8 parameters (`:27`) go. |
| `cloud_services.rs` | 582 | `AppHandle` ×6 | **Split** | Inputs into `{{ inputs.NAME }}` (`:94-137`) move to Isoloom: written as `tfvars` slots at generation, or filled by the runner from `state_file` delivery. The AWS session export (`:165-239,332-341`) becomes a runner credential helper (generic: any AWS CLI-mode account). Preview, cost and the "Cyber CTF labs" project **stay**. |
| `remote.rs`, `ssh.rs` | 34, 211 | `AppHandle` (key path) | **De-Tauri** → `runner::ssh` | The runner's key lives in `<home>/ssh/`; `ssh::Target`, `exec_timed`, `sudo_bash`. The attack-box shell command lines stay in the launcher. |
| `paths.rs` | 155 | `AppHandle` | **De-Tauri** → `runner::store` | `state_dir`, `deployment_states`, markers (`runtime`, `host`, `.cyberctf-local-vm`, `.cyberctf-parked`) all become `RunRecord` fields. `validate_id` becomes `RunName::new`. |
| `inflight.rs` | 138 | `AppHandle` (lock path) | **De-Tauri** → `runner::store::lock` + `Runner::in_flight` | Process-wide statics become fields of `Runner` (R5). The lock **fails closed** (R2). Remove `expect` at `:89`. |
| `src/deploy_worker/lock.rs` | 82 | none | **Move** | The cross-process OS lock with per-process sharing. Needs `File::lock` (Rust 1.89). |
| `src/deploy_worker/{mod,process,progress,files,worker}.rs` | 183+548+130+83+155 | `AppHandle`, `Channel`, `tauri::Builder` | **De-Tauri**, Phase 3 → `isothrum` CLI `--detach` + `runner::detach` | Detached jobs: pidfile with start time, log, status, follow, Terraform interrupt. The worker is a headless Tauri app only to obtain an `AppHandle` (`worker.rs:13-55`); the CLI needs none. `process.rs` is over the bar: split. In Phase 2 the launcher's worker stays and calls the runner in-process. |
| `lifecycle.rs` | 339 | `AppHandle` | **Split** | Park/resume/provision/stop semantics become `Lab::{park, resume, provision, down}`. The reaper's owner logic (`reap_owner`, `expired_states`, `:238-318`) becomes `Runner::expired()` plus the record's `host_label`; the sweep loop and its 300 s period stay in the launcher. `clear_parked` stays (lab upgrade). |
| `launch.rs`, `launch/local.rs`, `launch/on_host.rs` | 233, 124, 209 | `AppHandle` | **Split** | Moves to `Lab::up`:<br>- the occupied-slot rules (`launch.rs:46-129`, `local.rs:9-34`);<br>- the memory warning (`local.rs:39-57`) and QEMU emulator check (`:59-95`);<br>- the one-retry recovery (`:97-124`).<br>Stays in the launcher: the provider and host choice, the GCP project, the public IP (it becomes the `allowed_cidr` param), the AWS budget, the attack box and the welcome wording. 8- and 9-parameter functions (`launch.rs:35`, `on_host.rs:46`) go. |
| `status.rs` | 176 | `AppHandle` | **Split** | The runner returns `Status`; the launcher maps it to `LabStatus` (`model.rs`) and adds `place`, `host` and `attacker`. The remote probe cache (`:95-138`) moves to the terraform driver. |
| `discover.rs` | 209 | `AppHandle` (labs dir) | **Rewrite** → `Runner::discover()` | Finds runs from the store's records, confirmed by the engines (Compose project label, the manifest's VM names), never by a `cyberctf-` prefix. |
| `providers.rs` | 356 | none | **Split** | `Provider` ids and `ensure_usable` become the manifest's tool probes plus `identity.vagrant.providers`. Detection for the Machine page (`detect`, `kvm_problem`) **stays** (UI). |
| `lab/mod.rs` | 342 | none | **Shrinks**, stays | Keeps: instance choice (`ensure_instance`), generation through the new Isoloom crates (no longer `isoloom-core`, R13), `Observer` (tools). Goes: `start_plan`, `exec_runner`, `topology`, `message_at` (the runner fills the message), `inputs_json` (a slot), `cloud_requirements` (a tool probe). |
| `lab/names.rs` | 128 | none | **Deleted** | Every name it knows is in the manifest. |
| `lab/overrides.rs` | 57 | none | **Deleted** | `--project-name` is always passed. |
| `lab/registry.rs` | 147 | `AppHandle` | **Deleted** | Isoloom's registry goes with Isoloom's runtime; the runner's store is the registry. |
| `lab/vars.rs`, `server/contract.rs` | 131, 208 | `AppHandle` (contract) | **Stay**, rewritten as one adapter | Profile + keychain → `RunOptions{params, secrets, provider}`. The `CYBERCTF_*` legacy names go. |
| `server/{profile,input,store,secrets,commands,setup_window,checks,cloud_checks,reachability,budget,gcp}.rs`, `proxmox.rs`, `proxmox/{check,node_ssh}.rs`, `server_selftest/*` | ~2,350 | yes | **Stay** | Host profiles, the keychain, Test connection, self-tests, budgets, the GCP project. The Proxmox key authorisation through askpass (`node_ssh.rs`) stays a setup step of the launcher's. |
| `attack_box.rs`, `attack_vm.rs`, `exegol.rs`, `shell.rs` | 115, 335, 341, 77 | some | **Stay** | Attached through `Hooks` (§3.5). The attack VM keeps its own Vagrantfile. Its run could become a second manifest later. |
| `commands.rs`, `model.rs` | 182, 177 | yes / ts-rs | **Stay** | The Tauri command surface and the UI's status model. `docker::Check` becomes the runner's `CheckReport`, mapped. |
| `real_labs.rs` | 251 (test) | none | **Stays**, extended | Runs each lab through both paths behind the switch. `cp -R` (`:46`) is replaced with a Rust copy so it runs on Windows. |
| `src/labs.rs`, `src/account/*`, `src/cloud.rs`, `src/operations.rs`, `src/platform/{install,uninstall,steps,scripts,exit_codes}.rs`, `src/machine/*`, `src/terminal.rs`, `src/shared_folder.rs` | | yes | **Stay** | Catalogue download, launch tokens, cloud CLI sign-in, installs, the Machine page, the in-app terminal, the shared folder. |

### 1.3 Isoloom's run part, which moves in rather than being deleted (PLAN §0)

| Isoloom | Fate |
|---|---|
| `crates/isoloom-runtime/src/host.rs`, `host/requirements.rs`, `host/hyperv.rs` | → `runner::doctor`: per-target readiness from the manifest's tools. `requirements` runs commands with `sh -c` or PowerShell; it becomes `host_argv` per OS. |
| `crates/isoloom-runtime/src/secrets.rs` | → `runner::secrets`: the `env`, `file`, `vault` and `sops` stores behind `SecretSource`; `SecretValue` with a redacting `Debug`. |
| `crates/isoloom-runtime/src/events.rs` | → `runner::events`, extended (§3.3). It is still imported by Isoloom's converter-side commands (`failure.rs:5`, `output.rs:11`, `schema.rs:17`, `doctor.rs:20,73`): Phase 1 must keep an error-output type for the converter before deleting the crate. |
| `crates/isoloom-runtime/src/registry.rs` | **Deleted**: the runner's store replaces it. No interop: Compatibility "none", PLAN §2. |
| `crates/isoloom-runtime/src/report.rs` | **Stays in Isoloom** (graph and report are converter-side). |
| `crates/isoloom-runtime/src/tool.rs` | Subsumed by `runner::exec`. |
| `crates/isoloom/src/secrets/{vms,windows}.rs`, `secrets/winrm.py`, `commands/run/deliver.rs` | → `runner::deliver` (Phase 3): the `guest` delivery, its rounds and limits, and WinRM. **Decision needed** on WinRM's Python and pywinrm dependency (Q5). |
| `crates/isoloom/src/tools/vagrant.rs:22-91` | → the action `vagrant.wake_controller`. |
| `crates/isoloom/src/commands/{run,test,down,provision,reset}` | Superseded by the executor plus drivers. The baseline snapshot and reset (`done.rs`, `reset.rs`) become runner 1.x `snapshot`. |
| `crates/isoloom/src/commands/{connect,exec,capture}.rs`, `machine.rs:177-327` | → runner 1.x `console`/`exec`/`capture`, merged with Isothrum desktop's. |

### 1.4 Isothrum desktop's runner (`apps/desktop/src-tauri/src/runner/`, 2,208 lines)

It runs `docker compose` and `vagrant` with blocking `std::process::Command` and adds features the launcher lacks:
- capture (`capture.rs`);
- snapshots (`snapshots.rs`);
- the VNC display (`display.rs`);
- link suspend (`links.rs`, `live.rs`);
- consoles (`machines.rs`).

These become runner 1.x modules once the desktop app switches onto the runner (PLAN Phase 8). None is in the Phase 2 or 3 critical path.

---

## 2. The Isothrum repository and where the runner goes

**Today, local `main` = `origin/main` at `1386934`.** The local checkout is on `editor-isoloom-features`, four commits and 30 files ahead of `main`: a worker must branch from `main` explicitly.

```
isothrum/
  Cargo.toml              workspace: crates/isothrum-core, crates/isothrum-wasm; edition 2024,
                          rust-version 1.88; isoloom-spec/targets pinned by git rev 0111b26; no lints table
  crates/isothrum-core/   the lab-model compiler (drawing → spec)
  crates/isothrum-wasm/   the same, for the browser (release profile opt-level z, LTO)
  apps/desktop/           Tauri app, its own workspace (GTK/WebKit), runner/ as above
  web/pkg/                the built WASM package
  docs/
```

**Target layout:**

```
isothrum/
  Cargo.toml              members += crates/isothrum-runner, crates/isothrum;
                          [workspace.lints] = Isoloom's (pedantic deny, unwrap/expect/panic deny, missing_docs);
                          isoloom-* as path dependencies (PLAN §2, Workflow), no git revs
  crates/isothrum-runner/ the library (below)
  crates/isothrum/        the CLI: up | down | park | resume | provision | status | check | outputs | doctor
                          (Phase 3: --detach, worker, interrupt); --json prints the event stream
  crates/isothrum-core/   unchanged (a rename to isothrum-compiler can wait)
  crates/isothrum-wasm/   unchanged; must never depend on the runner (tokio::process isn't WASM)
  apps/desktop/           unchanged until Phase 8
```

**`crates/isothrum-runner/src/`**, each file under the bar:

```
lib.rs          Runner, Lab, RunOptions, re-exports
error.rs        RunnerError (typed, with ErrorCode)
manifest.rs     load, version check, the checks JSON Schema can't do, placeholder rendering
executor.rs     steps: when/failure/timeout/env/stdin/hosts/providers/verdict; secret delivery; masking
ready.rs        conditions (step_ok, machines_running, ready_file, tcp)
checks.rs       setup, runners, the isoloom-check/1 grammar (from isoloom-manifest), teardown
events.rs       Event, Events, JsonLines
store/          mod.rs (Store, RunRecord), lock.rs, fs.rs (owner-only, atomic)
exec/           mod.rs, group.rs, docker.rs, shim.rs, fake.rs (cfg(test) or a `fake` feature)
driver/         mod.rs (Driver, AnyDriver, Cx); compose/{mod,actions,status,pins}.rs
                Phase 3: vagrant/{mod,actions,batch,elevate/*}.rs, terraform/{mod,state,runner}.rs
ssh.rs          Phase 3
secrets.rs      SecretSource, SecretValue, Isoloom's stores (Phase 3 for vault and sops)
host/path.rs    PATH completion (Phase 3)
```

**Toolchain:** the runner needs Rust ≥ 1.89, because `std::fs::File::lock` is what `deploy_worker/lock.rs` uses. The launcher is on 1.90; Isoloom and Isothrum declare 1.88. Raise the Isothrum workspace to 1.90, or set it per crate.

---

## 3. The runner's public API

### 3.1 Rust

```rust
/// The engine: one per process. Holds the store, the host facts, the in-flight registry and the
/// embedder's hooks. Everything the launcher kept in statics lives here.
pub struct Runner { /* store: Store, host: HostInfo, hooks: Vec<Arc<dyn Hooks>>, in_flight: … */ }

pub struct RunnerConfig {
    /// The runner's home: runs/, ssh/. The launcher passes its app data folder; the CLI uses
    /// `$ISOTHRUM_HOME` or `~/.isothrum`.
    pub home: PathBuf,
    /// Relays for tools macOS keeps off the LAN (feature `lan-relay`, Phase 3).
    pub relay: RelayPolicy,
}

impl Runner {
    pub fn new(config: RunnerConfig) -> Result<Runner, RunnerError>;
    /// Reads and checks the manifest; `name` is the run's name (`${run.name}`).
    pub fn open(&self, manifest: &Path, name: RunName) -> Result<Lab<'_>, RunnerError>;
    pub fn add_hooks(&mut self, hooks: Arc<dyn Hooks>);
    /// Every recorded run (the registry), with its last known state. Reads files only.
    pub fn runs(&self) -> Result<Vec<RunRecord>, RunnerError>;
    /// Runs whose engine still holds something, confirmed by the engines (after a crash).
    pub async fn discover(&self) -> Result<Vec<RunName>, RunnerError>;
    /// Operations in flight in this process: the quit guard and the UI's rehydration.
    pub fn in_flight(&self) -> Vec<(RunName, Op)>;
    /// Recorded runs past their `expires_at` that still hold resources: the reaper's input.
    pub fn expired(&self, now: SystemTime) -> Result<Vec<RunRecord>, RunnerError>;
}

/// One manifest opened under one run name.
pub struct Lab<'r> { /* runner: &'r Runner, manifest: Manifest, dir: PathBuf, name: RunName, driver: AnyDriver */ }

impl Lab<'_> {
    pub fn manifest(&self) -> &Manifest;
    pub async fn up(&self, opts: &RunOptions, events: &dyn Events) -> Result<Run, RunnerError>;
    pub async fn down(&self, opts: &RunOptions, events: &dyn Events) -> Result<(), RunnerError>;
    pub async fn park(&self, mode: Park, events: &dyn Events) -> Result<(), RunnerError>;
    pub async fn resume(&self, opts: &RunOptions, events: &dyn Events) -> Result<Run, RunnerError>;
    pub async fn provision(&self, machine: Option<&str>, opts: &RunOptions, events: &dyn Events) -> Result<(), RunnerError>;
    pub async fn check(&self, opts: &RunOptions, events: &dyn Events) -> Result<CheckReport, RunnerError>;
    /// Read-only, time-bounded, never elevated; cached briefly for remote probes.
    pub async fn status(&self) -> Result<Status, RunnerError>;
    pub async fn outputs(&self) -> Result<Vec<OutputValue>, RunnerError>;
    /// The lab's message, filled; sensitive outputs never substituted.
    pub fn message(&self, outputs: &[OutputValue]) -> Option<String>;
}

pub struct RunOptions {
    /// A Vagrant provider among the manifest's (required for the vagrant driver).
    pub provider: Option<String>,
    pub params: BTreeMap<String, String>,
    pub secrets: Secrets,                       // name → SecretValue; Debug redacts
    /// What the embedder calls the place (a server or account id): recorded, so a later destroy
    /// or the reaper can ask the embedder for the same credentials.
    pub host_label: Option<String>,
    pub expires_in: Option<Duration>,           // terraform runs with `expiry`
    pub occupied: Occupied,                     // Refuse | ResumeParked | Replace
}

pub struct Run { pub record: RunRecord, pub outputs: Vec<OutputValue>, pub message: Option<String> }

pub enum Park { Pause, Shutdown }
pub enum Op { Up, Down, Park, Resume, Provision, Check }
pub enum RunState { Starting, Ready, Failed, Parking, Parked, Resuming, Stopping, Absent }

pub struct Status {
    pub state: RunState,
    pub provider: Option<String>,
    pub machines: Vec<MachineStatus>,          // every manifest machine, with its role
    pub networks: Vec<NetworkView>,             // from the manifest
    pub outputs: Vec<OutputValue>,              // published ports read live
}
pub struct MachineStatus {
    pub name: String,
    pub role: Role,
    pub state: MachineState,                    // Running | Stopped | Exited(i32) | Unhealthy | Absent | Unknown(String)
    pub addresses: BTreeMap<String, String>,
    pub published: Vec<Published>,              // port, protocol, host port
}
pub struct CheckReport { pub ok: bool, pub results: Vec<CheckResult>, pub unfinished: Vec<String> }
```

### 3.2 The driver trait

The trait is internal and statically dispatched. It uses `async fn` in a trait, so no `async-trait` dependency, and is not object-safe by design: drivers are the manifest's closed set.

```rust
pub(crate) trait Driver {
    /// Runs one of this driver's actions (run-manifest-v0.md §6). Argv steps never reach here.
    async fn action(&self, cx: &Cx<'_>, step: &Step) -> Result<(), RunnerError>;
    /// Machine states from the manifest's `lifecycle.status` step, parsed.
    async fn status(&self, cx: &Cx<'_>) -> Result<Status, RunnerError>;
    /// Whether anything of this run exists (containers, a tracked VM, state with resources):
    /// the occupied-slot rules and "nothing left to resume".
    async fn exists(&self, cx: &Cx<'_>) -> Result<bool, RunnerError>;
    /// Values of `outputs` that need the engine (published ports, Terraform outputs).
    async fn outputs(&self, cx: &Cx<'_>) -> Result<Vec<OutputValue>, RunnerError>;
    /// How a batch of steps runs: directly, or as one elevated batch (Hyper-V).
    fn batch(&self, cx: &Cx<'_>, op: Op) -> Batch;
}

pub(crate) enum AnyDriver { Compose(compose::Compose), Vagrant(vagrant::Vagrant), Terraform(terraform::Terraform) }

/// What an operation's steps see: the run, its rendered placeholders, its state folder, the
/// options, the sink, and the tools (real or the test script).
pub(crate) struct Cx<'a> { /* lab: &'a Lab<'a>, opts: &'a RunOptions, events: &'a dyn Events, vars: Vars, … */ }
```

**The executor owns everything generic:**
- step order;
- `when`/`failure`;
- host and provider filters;
- placeholder rendering;
- delivery of params and secrets (env, `tf_var`, `state_file`);
- masking;
- timeouts and process-tree kills;
- `verdict`;
- the recovery retry;
- `ready` conditions;
- events.

A driver is small. In Phase 2, compose is the four actions plus status.

### 3.3 The JSON event stream

**Format:** JSON Lines on the CLI's stdout with `--json`, and the same values through `Events` in Rust. It evolves from Isoloom's events (`crates/isoloom-runtime/src/events.rs:45-162`):
- the same `event` tag and `snake_case`;
- the same `step`/`log`/`check`/`result` shapes;
- `run`, `op`, `seq` and `v` added;
- a `time` on every event.

The schema is generated with schemars and versioned apart from the manifest (`v`).

```json
{"v":0,"seq":1,"time":"2026-10-15T12:00:00Z","run":"cyberctf-flag-portal","op":"up","event":"state","from":"absent","to":"starting"}
{"v":0,"seq":2,"time":"…","run":"…","op":"up","event":"step","step":"up","state":"running","note":"Starting the lab's containers…"}
{"v":0,"seq":3,"time":"…","run":"…","op":"up","event":"log","step":"up","stream":"stderr","line":" Container web  Healthy"}
{"v":0,"seq":4,"time":"…","run":"…","op":"up","event":"step","step":"up","state":"done"}
{"v":0,"seq":5,"time":"…","run":"…","op":"up","event":"ready","condition":"ready_file","state":"waiting","detail":"running: installing Docker"}
{"v":0,"seq":6,"time":"…","run":"…","op":"up","event":"output","name":"portal","role":"entry","value":"http://127.0.0.1:41873/","sensitive":false}
{"v":0,"seq":7,"time":"…","run":"…","op":"check","event":"check","runner":"isoloom-check","from":"from the environment's networks","name":"http://web/","ok":true,"detail":""}
{"v":0,"seq":8,"time":"…","run":"…","op":"up","event":"warning","code":"low_memory","message":"This lab's VMs ask for about 8192 MB, …"}
{"v":0,"seq":9,"time":"…","run":"…","op":"up","event":"result","ok":true,"data":{"state":"ready"}}
{"v":0,"seq":9,"time":"…","run":"…","op":"up","event":"error","code":"port_in_use","message":"host port 8080/tcp is in use","step":"ports-free","hint":"…"}
```

**Rules:**
- A sensitive value never appears: it is masked before the event is built.
- `log` lines are lossily decoded UTF-8.
- An operation ends with exactly one `result` or `error`.
- Error codes are Isoloom's (`tool_missing`, `tool_failed`, `secret_missing`, `not_running`, `machine_unknown`, `unsupported`, `io`, …) plus the runner's: `already_running`, `running_elsewhere`, `port_in_use`, `not_parkable`, `nothing_to_resume`, `locked`, `elevation_declined`, `not_ready`, `timeout`, `manifest_version`, `manifest_invalid`.
- **Text belongs to the embedder:** the runner's messages are plain and technical. The launcher maps codes to its player-facing sentences, which today are inside the runtime (`Error::Invalid("This lab is already running…")`, `src/error.rs:3-17`).

### 3.4 The state store

```
<home>/runs/<run name>/
  record.json        RunRecord: manifest path + sha256, target, driver, cloud, provider, host_label,
                     state, parked, started_at, expires_at, every non-secret param (destroy replay),
                     last operation and its outcome. Atomic writes.
  lock               the OS lock (one operation per run, across processes; released on death)
  events.jsonl       the last operations' events (bounded), so a reattached UI can replay
  ports.yml          Compose pins (`services: {}` when none)
  terraform.tfstate, .terraform/, known_hosts, project.tgz      terraform runs
  inputs.tfvars.json owner-only; removed after a successful down
  vagrant/           VAGRANT_DOTFILE_PATH, if adopted (run-manifest Q6)
  worker.pid, worker.status    detached operations (Phase 3)
<home>/ssh/id_ed25519[.pub]    the runner's key (Terraform hosts, ready_file, ssh.run)
```

**What it replaces:**
- `.cyberctf-local-vm`, `.cyberctf-parked`, `.cyberctf-ports.yml` and `.cyberctf-host` in the lab folder;
- `deployments/<lab>/<provider>/{run.json,runtime,host,ssh-jump}`;
- `deploys/<lab>.{lock,pid,log,status}`;
- Isoloom's `~/.isoloom/status.yml`.

The lab folder then holds only the lab and its generated files, so an upgrade can replace it without stranding a run. That is the problem `src/runtime/launch.rs:66-85` and `status.rs:140-158` guard against today.

`Store` is a struct over the file system, not a trait: one implementation, and tests use a temporary home.

### 3.5 Hooks: the embedder's sidecars

```rust
pub trait Hooks: Send + Sync {
    fn after_up<'a>(&'a self, lab: &'a LabView<'a>, events: &'a dyn Events) -> BoxFuture<'a, Result<(), HookError>>;
    fn before_down<'a>(&'a self, lab: &'a LabView<'a>, events: &'a dyn Events) -> BoxFuture<'a, Result<(), HookError>>;
    fn before_park<'a>(&'a self, lab: &'a LabView<'a>, events: &'a dyn Events) -> BoxFuture<'a, Result<(), HookError>>;
    fn after_resume<'a>(&'a self, lab: &'a LabView<'a>, events: &'a dyn Events) -> BoxFuture<'a, Result<(), HookError>>;
}
```

`BoxFuture` is `Pin<Box<dyn Future + Send>>`, so no extra dependency. `LabView` exposes the run name, the manifest's networks with their native names, the outputs, and an SSH target when there is one.

**The launcher's attack box is these hooks:**
- start or `rejoin` after up and resume;
- `docker stop -t 1` before park;
- remove it before down, so its network is free (`src/runtime/docker/lifecycle.rs:214-221`).

A hook's failure on `after_up` is a warning event, not a failed run, as today (`src/runtime/launch.rs:154-161`).

---

## 4. The Phase 2 slice: the walking skeleton

**Goal (PLAN Phase 2, exit):** invoice-portal-api runs end to end through new spec → Isoloom → manifest → runner → launcher, behind a switch, and its real-lab test passes.

**Isoloom:**
1. `crates/isoloom-manifest`: the serde types, the generated schema, the shared `isoloom-check/1` line parser, and fixtures (`docker` for supplier-portal-api, the public twin of invoice-portal-api, plus a corpus-shaped flag lab with `internet: false`, an init job and a volume).
2. The `docker` target writes `.isoloom/docker/run.json` (run-manifest-v0.md §9, §12). Snapshots and `verify`.

**Isothrum (`isothrum-runner` + `isothrum`):**
3. `exec` lifted first, with `fake` and its tests. Then `store` (record, lock, owner-only atomic files), `events`, `manifest`, `executor`, `ready` (`step_ok` only), `checks`.
4. The compose driver:
   - the five actions;
   - status from `ps`;
   - published ports from `ps` publishers and the pins;
   - `exists`.

   The launcher's unit tests for `docker/{compose,lifecycle,check}` are ported against the fake tools.
5. The CLI: `isothrum up|down|park|resume|status|check|outputs <run.json> [--name] [--param K=V]… [--secret-from-env NAME]… [--json]`.
6. Tests:
   - every fixture parses and plans on every host OS (cross-platform check);
   - a scripted end-to-end through `fake`;
   - an opt-in real-Docker test, matching `real_labs.rs`, that runs the flag fixture: up, status, outputs, checks, park, resume, down, nothing left.

**Launcher:**
7. Move off `isoloom-core@50147b3` onto the new Isoloom crates by path dependency (R13). `runtime::lab::prepare` generates through them.
8. A switch: an `Isothrum runner` setting in Settings > Labs, plus `CYBERCTF_RUNNER=isothrum` for tests, off by default. When on, for **local Docker only**, the following delegate to `Lab`:
   - `launch::start_on_docker`;
   - `docker::{stop, park, resume}`;
   - `docker::check`;
   - `docker::status`.

   Run name: `cyberctf-<lab id>`. Params: `ISOLOOM_PUBLISH_FIXED=1` when the player picked default ports. Secrets: the launch env's inputs. Hooks: the attack box. `LabStatus` is mapped from `Status`; check results from `CheckReport`.
9. The deploy worker stays the launcher's and calls the runner in-process. `real_labs.rs` runs invoice-portal-api and the segmented example through both paths.

**Left out of Phase 2:** Vagrant and Terraform, elevation, relays, guest secrets delivery, detached runs in the runner, discovery, the reaper.

**Rough size:**
- the runner: about 2,500 lines, mostly moved;
- the launcher: about 300 lines of adapter;
- Isoloom: about 600 lines of manifest generation and types.

### 4.1 Phase 3 order for the other drivers

Each step follows PLAN Phase 3: snapshots and `verify` → manifest → driver → the launcher's lab test. The tracks can run in parallel once the executor is stable.

1. **Vagrant, local providers.**
   - VirtualBox and VMware on the Mac first.
   - libvirt and QEMU on `jarvis`.
   - Hyper-V on `karen`, with `elevate.rs` ported and split.

   It brings: the actions `reconcile_provider`, `discard_aborted_saved`, `remove_orphans`, `reset_state` and `wake_controller`; `verdict`; recovery; `machines_running`; VM checks. VM labs are 19 of the corpus's 376, mostly Windows with an Ansible controller, and they are the costliest to get wrong.
2. **`docker-vm` local:** the same driver, plus `check_forwarded_ports` (two labs on one forwarded port).
3. **Terraform: `proxmox` and `docker-vm/proxmox`** on the nested Proxmox in VMware.
   - It brings: `ready_file`, `ssh.run`, `project.archive`, the state, the replay record, the macOS Terraform container, and the `slot` param.
   - Before it, Isoloom fixes the `address`/`ip` and `ready_file` output names and the ready grammar.
4. **Vagrant `vmware_esxi`** on the nested ESXi, with `lan_relay` moved in (macOS).
5. **`cloud-docker` and `cloud-vm` on AWS** when the budget-capped account exists. Then Azure and GCP, then DigitalOcean, Linode and OCI.
   - The runner provides `expiry` and `Runner::expired()`.
   - The launcher keeps the sweep loop, the budget check and the GCP project.
6. **`cloud-services`:** the 13 AWS labs; `apply_retries`, sensitive outputs, `state_file` inputs.
7. **Secrets:** Isoloom's guest delivery and WinRM (Q5), Windows inputs on Vagrant, Compose secrets for the flag (run-manifest Q3).
8. **Detached operations in the runner:** `isothrum up --detach`, the worker, Terraform interrupt, `discover`. Then the launcher's `deploy_worker`, `inflight`, `paths`, `discover`, `lab/registry` and the runtime copies are deleted.

---

## 5. Dependencies the runner brings in

| Crate | Why | Today in |
|---|---|---|
| `isoloom-manifest` (path) | the contract's types and check-line parser | new |
| `tokio` (`process`, `io-util`, `rt-multi-thread`, `macros`, `time`, `sync`, `fs`) | every tool runs async, with timeouts and cancellation | launcher |
| `serde`, `serde_json` | manifest, record, events | both |
| `thiserror` | typed errors | both |
| `libc` (unix) | process groups, signals (`killpg`), the pidfile liveness check | launcher (`exec/group.rs`, `deploy_worker/process.rs`) |
| `windows-sys` (windows; `Win32_System_JobObjects`, `Threading`, `Console`, `Security`, `UI_Shell`, `Registry`) | job objects, UAC `ShellExecuteEx`, Ctrl+C, the registry PATH | launcher `Cargo.toml` |
| `sysinfo` | worker liveness by start time; free memory for the warning | launcher |
| `base64` | the elevated batch's `-EncodedCommand`; guest delivery lines (Phase 3) | launcher, Isoloom |
| `tar`, `flate2` (Phase 3) | `project.archive`, in place of `local-exec tar` | launcher (catalogue), new to the runner |
| `regex` | the steps' `verdict` (or make `verdict` a plain prefix and drop it: minor choice) | new |
| `schemars` (optional) | the event stream's JSON Schema | Isoloom |
| `sha2` | the manifest's hash in the record | launcher |
| `clap` | the CLI crate only | Isoloom |
| dev: `proptest` | the parsers' property tests, which move with them | launcher |

**Deliberately not in the runner:**
- `reqwest`: the public IP and Proxmox API stay in the launcher, passed as params.
- `keyring`: the embedder holds credentials.
- `tauri`, `ts-rs`, `serde_yaml_ng`: the pins file is written as text, as today.
- `async-trait`.
- `rand`: tests use temporary folders.

---

## 6. Risks: what is fragile or untested in the launcher's runtime

| # | Risk | Where | Mitigation in the extraction |
|---|---|---|---|
| R1 | **The implicit contract has already drifted from Isoloom `main`:**<br>- service labels;<br>- `address` vs `ip`;<br>- the ready protocol and log path;<br>- the Proxmox `slot`;<br>- `expires_at` never passed;<br>- Windows inputs dropped on Vagrant. | run-manifest-v0.md §3.6 | The manifest names everything. Fixtures are tested on both sides. |
| R2 | **Paths that report success or act on the wrong thing:**<br>- `wait_ready` returns OK when SSH never answers, or with no key or address (`src/runtime/terraform/ready.rs:67-70,86-89`);<br>- the lab lock fails open (`src/runtime/inflight.rs:111-114,130-133`);<br>- a destroy uses the profile's *current* region and credentials (`src/runtime/terraform/state.rs:26-37`; `src/runtime/server/commands.rs:40-57`);<br>- `inputs` and `ssh_via_node` aren't replayed (`src/runtime/terraform/mod.rs:50-74`). | | `not_ready` after `unreachable_timeout_s`. The lock fails closed. The record keeps every non-secret param, and destroy replays them. |
| R3 | **Panics outside tests:**<br>- `src/runtime/inflight.rs:89` (`expect`);<br>- `src/exec/mod.rs:269-270` (`expect`);<br>- `src/runtime/lifecycle.rs:38,115` (`unreachable!`);<br>- `src/runtime/exegol.rs:19-20` and `src/deploy_worker/worker.rs:53` (stay in the launcher). | | Typed errors; lints at deny. |
| R4 | **Over the quality bar:**<br>- files over 400 code lines: `src/exec/elevate.rs` (745), `src/runtime/cloud_services.rs` (582), `src/deploy_worker/process.rs` (548);<br>- functions with more than 5 parameters: `src/runtime/launch.rs:35`, `launch/on_host.rs:46`, `terraform/services.rs:27`;<br>- stringly errors carrying UX text (`src/error.rs:3-17`). | | Split on move. `RunOptions` and `Cx` carry the parameters. `RunnerError` plus `ErrorCode`, with UX text mapped by the launcher. |
| R5 | **Process-global state in a library:**<br>- the in-flight map and per-lab mutexes (`src/runtime/inflight.rs:27-30,87-91`);<br>- the remote probe cache (`src/runtime/status.rs:97-98`);<br>- the key `OnceLock` (`src/runtime/ssh.rs:26-30`);<br>- the GCP and `server.json` mutexes, which work within one process only, so two worker processes can race (`src/runtime/server/gcp.rs:17`, `store.rs:44`). | | Owned by `Runner`; cross-process guarded by the store's file lock. The GCP and `server.json` races stay launcher issues to fix there. |
| R6 | **`unsafe` FFI:** 8 blocks in `exec/group.rs`, 5 in `elevate.rs`, 5 in `platform/env_path.rs`, 2 in `deploy_worker/process.rs`, 1 in `console.rs`, 1 in `exec/docker.rs`. Isoloom's lints `forbid` `unsafe_code`. | | Decision needed (Q1): allow `unsafe` in one `sys` module (`deny` at the workspace, `allow` there), or use safe wrappers (`process-wrap` for groups and jobs; the elevation and console still need `windows-sys`). |
| R7 | **Platform-specific and only half-tested:**<br>- Unix-only askpass (`src/runtime/proxmox/node_ssh.rs:14-57`), `pgrep`/`kill` (`src/deploy_worker/process.rs:457-469`), and owner-only files that are a no-op on Windows (`src/private_fs.rs`);<br>- `ProxyCommand` built with POSIX quoting, untested under Windows OpenSSH (`src/runtime/ssh.rs:159,263`);<br>- the macOS Terraform container still needs host `terraform` and is re-decided on every call (`src/runtime/terraform/runner.rs:24,44-48`);<br>- `cp -R` in the real-lab test (`src/runtime/real_labs.rs:46`). | | Every fixture runs on the three hosts through the manual cross-platform workflow. Windows ACLs on the state folder. Container use is decided once per run. |
| R8 | **Text heuristics:**<br>- stale-state substrings (`src/runtime/vm.rs:262-273`);<br>- datastore guessing (`src/runtime/docker/inspect.rs:22-55`);<br>- Ansible verdicts (`src/runtime/vm.rs:219-239`);<br>- download-failure detection (`src/runtime/terraform/runner.rs`, `is_download_failure`). | | The markers move into the manifest, where Isoloom owns them. Datastore guessing is deleted (`entry` output). The rest are kept with their tests. |
| R9 | **Races:**<br>- ports are picked, then released before Compose binds them (`src/runtime/docker/lifecycle.rs:96-110`);<br>- the free-port check races another start;<br>- two docker-vm labs forward one fixed host port. | | Pinned ports are re-checked inside the run lock; `check_forwarded_ports` for docker-vm. A residual window remains (Docker binds, not the runner). |
| R10 | **Real coverage is thin:**<br>- real runs only on Linux Docker (`.github/workflows/labs.yml`, `src/runtime/real_labs.rs`);<br>- VM, server and cloud tested with fake tools only;<br>- never run end to end: a Proxmox or ESXi launch with jump and authorise, `wait_ready` against a real SSH host, a real interrupt;<br>- ignored tests: `src/runtime/terraform/mod.rs:206`, `terraform/services.rs:214`, `proxmox/tests.rs:77-79`. | | Each Phase 3 driver ends with a real run on the Track C hosts (Mac, `karen`, `jarvis`, nested Proxmox and ESXi) before it counts. |
| R11 | **The check runner timeout (5 min) is shorter than the labs' `wait`s:** 659 corpus checks have one, up to 1500 s (`src/runtime/docker/check.rs:54`). | | Timeouts from the manifest. |
| R12 | **Deleting Isoloom's runtime breaks converter commands that import it:** `failure.rs:5`, `output.rs:11`, `schema.rs:17`, `report.rs`, `graph.rs`, `targets.rs:42`, `doctor.rs:20,73`. | | Phase 1 keeps the error-output type and `report.rs` converter-side. |
| R13 | **The launcher depends on `isoloom-core@50147b3`**, which exists only on `origin/dev`: not an ancestor of `main`, its facade removed in `a243227b`. The launcher can't take Isoloom `main` without first porting `src/runtime/lab/*` to the layered crates. | | First task of the launcher's Phase 2 unit. |
| R14 | **WinRM delivery needs `python3` and pywinrm on the host** (`crates/isoloom/src/secrets/windows.rs:63-75`). Windows and macOS hosts often lack them. | | Decision Q5. |
| R15 | **Compose-version variance:** `ps` prints an array or NDJSON (`src/runtime/docker/compose.rs:146-154`), and `--wait` fails on a one-shot job exit. | | Parsers kept with their property tests; the start plan written into the manifest; a `min_version` probe. |
| R16 | **The detached worker is the launcher's own executable** (`cyberctf-desktop deploy --job`), recognised by its command line (`src/deploy_worker/process.rs:284`). | | Phase 3: the runner's CLI is the worker, or the embedder passes the command (Q4). |
| R17 | **Existing live deployments:** players' cloud labs recorded under `deployments/<lab>/<provider>/` must still be destroyable after the switch, even with Compatibility "none" for specs. | | Q6. |

---

## 7. Open questions for the maintainer

1. **`unsafe` in the runner.** Allow one `sys` module (workspace `unsafe_code = "deny"`, `#[allow]` there), or require safe wrapper crates (`process-wrap`, while `windows-sys` stays needed for UAC and Ctrl+C)? Recommended: one audited `sys` module.
2. **Crate names and homes.** `isothrum-runner` plus the `isothrum` CLI in `isoloom/isothrum`, with the manifest types as `isoloom-manifest` in Isoloom. Confirm, and confirm the crates.io names to reserve (Track C).
3. **The launcher's switch.** A user setting (off by default, visible) or a hidden developer flag until Phase 3 covers every mode? Recommended: a hidden flag in Phase 2, a setting once Vagrant lands.
4. **Detached runs.** The runner's own `isothrum` binary as the worker, or the embedder's executable with a runner entry point (`cyberctf-desktop deploy --job` calling `isothrum_runner::worker_main`)? The first means shipping a second binary inside the launcher's bundle (signing, notarisation, MSIX). Recommended: the embedder's executable, through a runner entry point.
5. **WinRM delivery.** Keep Python and pywinrm on the host; use Vagrant's own `vagrant winrm`/`vagrant upload` on Vagrant targets plus the cloud secret stores on clouds (PLAN §2, Secrets); or a Rust WinRM client? Recommended: Vagrant's commands and cloud stores, with Python only for Proxmox and ESXi Windows machines until a Rust client is proven.
6. **Live deployments at the switch.** A one-time import of `deployments/` and the lab markers into `RunRecord`s, or keep the launcher's old reaper path until 1.0? Recommended: a one-time import, so the reaper has one code path.
7. **The runner's home.** For the launcher: its app data folder (`runs/` beside `labs/`). For the CLI: `~/.isothrum`. Should the two share one home, so the CLI sees launcher runs, as Isoloom's registry allowed?
8. **The event stream's version.** Its own `v`, independent of `manifest_version` (proposed), or tied to it?
