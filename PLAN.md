# Isoloom: the plan to a clean 1.0

Working document for the local rebuild. It is squashed away with the history at the first satisfying version.

## Status (updated 2026-10-15)

| Item | State |
|---|---|
| **Phase 0: safety net** | **Done.** 336 characterization snapshots (`d64255fb`). `verify` with the pinned validator image and 886 baseline failures (`115bb503`). `dev-link`, and `check-all` on macOS, Linux (`jarvis`) and Windows (`karen`) (`ac79b0c6`). |
| **Phase 1a: subtract** | **Done** (`58ae8e6b`). About 195k lines and 943 files removed: coverage, committed outputs, compatibility, hand-written docs, the CHANGELOG. Tests are green on macOS. |
| **Phase 1b: runtime out, layout, xtask, ratchet** | **Done** (`fd001ce6`): layout, runtime out (last at `b6c218c9`), backends independent, `cargo xtask`, the dependency-direction test, and the 1c lint set with a 2,549-finding baseline (terraform 613, spec 475, render 306, targets 247, plan 164, kubernetes 148, vagrant 144, compose 134, cli 119, import 100, manifest 95, ansible 3). The Windows and Linux legs are pending (hosts offline). |
| **Phase 1c: deep organization, zero violations** | **DONE.** All 13 Isoloom crates at 0 findings with closed review checklists (`isoloom-spec` keeps 2 reasoned `#[expect]`s on the lowered `Machine`/`Port` booleans). Mutation testing closed on spec, manifest, plan and render (equivalent mutants listed with reasons); fuzz clean. **Isothrum Phase 1c DONE** (`4c03a3a`): runner, cli, compiler and wasm at 0; every lint at deny; mutants closed on the compiler and the runner's executor/readers; fuzz clean (drawing 4M runs). Open: the wasm32 build isn't verified on this Mac (the target isn't installed). |
| **Phase 2: walking skeleton** | **Done** on `main`, including follow-ups: the spec switch (`e9a9dd66`), the contract fixes (`7b93fef6`), the runner on the new manifest following each slot's `default` (`isothrum@5ec348c`), and the launcher on the new syntax behind the hidden `CYBERCTF_RUNNER=isothrum` switch (`cyber-ctf-launcher@6449daa`). Real runs 6/6. The hand-rewritten `invoice-portal-api` lives privately in `~/code/isoloom-corpus/rewritten/` until the CTF lab rewrite (Phase 7). |
| **Phase 3: launcher modes** | **Vagrant:** run manifest (`17c871da`: Vagrant ≥ 2.4.0, box architecture) and runner driver for every local provider, including Hyper-V elevation (`isothrum@8b369a6`); park_controller (`47106cdd`); real runs on Mac VirtualBox, friday libvirt (full cycle) and karen Hyper-V (full cycle, Tier 1; netsh portproxy, the controller hostname and the suspend-list fixes, `149d4666`); the launcher's VM mode through the runner (`efb13a6`, Lab engine setting, Mac VirtualBox through the launcher passed). **Server mode done:** Proxmox through the runner and the launcher's ServerRun (6 real-server bugs fixed test-first; `ea217edb`, `isothrum@d714009`, `cyber-ctf-launcher@b4ba5c5`); ESXi through the runner and **the launcher** (`f52860ce`, `isothrum@000f08c`, `cyber-ctf-launcher@06db912`, tier record `df281d82`): connection secrets for every operation, a fixed relay session leak that exhausted ESXi's logins, and `park_controller` treating ESXi's already-off as halted. **Still on the launcher's old runtime:** every lab while the Lab engine setting is Built-in (the default); docker-vm (Docker on a local VM, a remote host or a server) and the clouds (cloud VMs, cloud Docker, cloud services); the attack VM and attack box; the server self-test. **Open proofs:** a real Windows-guest lab, and a real UAC prompt (karen has UAC off). Also on the old runtime: the clouds (their project archive and guest SSH steps aren't runner steps yet); the Terraform `esxi` target's guest SSH (Tier 2); file inputs on Proxmox VMs; ESXi in the launcher (the mapping on `VagrantRun` comes after the Vagrant launcher step). Everything is behind `CYBERCTF_RUNNER=isothrum`. |
| **Phase 4: catalogue and CM model** | Design approved. **4.1 done** (`a524ebd5`). **4.3 done** (`ab7d4463`): `isoloom-catalogue` (embedded entries, release matching, typed `with:` settings, `when`/`for_each`, the daemon table, schemas), `service:` lowering with coded refusals, `cargo xtask catalogue refresh` and `catalogue check`, and nginx's entry with a real `lock.yml` (1.30.6/1.31.7). nginx's real parity runs move to 4.6. **4.2 renderers landing** (the recipe carrier `5ad734eb` is in). Decisions: a `with:` setting with neither `required` nor `default` is optional (the nginx example needs it); `alpine` alone stays 3.19 for now; `isoloom-catalogue` joins `cargo xtask mutants`' default list and its run completes in Phase 6. Open for 4.7: `image.start: run` (redis), the password `owner` hand-off, and the service origin in the run manifest (a cross-repo contract change). Follow-through (4.9): the CLI prints `Lab::warnings()` in `validate`. |
| **Phase 5: validator baseline** | **5a done** (`16cd2692`): 896 → 344 entries. shellcheck, kustomize, hadolint and helm-template at 0; examples ship their files; verify is CPU-capped. **5b running:** tflint (272) and cloud-init (70) to 0, `corp-ad-basics`' missing files, and shellcheck for embedded shell (Vagrantfiles, heredocs, cloud-init). |
| **Phase 6: hardening** | **6b done** (`e2c94630`): `isoloom matrix` (96 rows × 24 columns; every cell is the generator's own decision; honesty tests; `docs/matrix.md` generated) and tiers worked out from recorded runs (`tiers.json`, `cargo xtask tiers record`). **Tier 1: `vagrant/libvirt`** (friday), **`vagrant/hyperv`** (karen, `042925e9`; UAC is off on karen, so the elevation prompt itself is unproven) **and `docker`** (`e25e5f96`: Mac, jarvis and karen, three labs each through the full cycle; the flag-file proof passed on Linux and Windows). Tier 2 with Linux/Windows runs pending: vagrant/virtualbox (Mac), proxmox and vagrant/vmware_esxi (Mac against the lab servers). **6a done** (`906f93e0`): `cargo xtask tf-test` (37 suites, 158 runs; all six clouds, cloud-docker, cloud-services, Proxmox and ESXi on mocked providers; `--fast` in check-all, the full set in `--deep`); 19 module bugs recorded as expected failures (the list is under Phase 3). Still to do: combinations (10k random specs), the security review, and making the CLI's tier test use a fixture instead of the real `tiers.json` (it hard-codes docker's tier and breaks on every new record). |
| **Isothrum on the new syntax** | Compiler, WASM (`cargo xtask wasm --website`, npm API unchanged plus `choices_json()`) and desktop app write and read the new syntax (`isothrum@4ed9069`). Compiled drawings pass Isoloom's `Lab::problems()` (property-tested); the crates are at zero findings. Leftovers for Isothrum's own 1c pass: `proptest` declared twice (workspace and runner). |
| **Track C: Proxmox 9.2** | Proven: a real apply, serve, check and destroy. |
| **Track C: ESXi 8.0U3e** | Proven with promiscuous mode on: nested DHCP works, the launcher's Vagrant self-test passes, and Isoloom's `esxi` Terraform module serves and passes its check (finishing from a container). Two fixes are queued for Phase 3 (the controller box on ESXi; guest SSH moving to the runner). |
| **Launcher standard (0.6)** | **Clean outside `runtime/` on Windows, macOS and Linux** (`f5f9452`), with win32 and linux baselines; 12 more lints at deny. The first Windows compile found two real bugs: a stop's Ctrl+C never reached Terraform (the deploy worker's process group ignored it; fixed), and the Hyper-V watcher took 30–90 s to start (now under 0.5 s). **e2e free of real time** (`78b59d7`): axe runs in the page (0.3–0.8 s per screen instead of 3–5 s), one test per screen, and the setup poll on Playwright's clock; 120/120 three times under load. |
| **Test hosts** | **jarvis and karen are back.** jarvis (10.0.0.10) is check-all's main Linux host (8 jobs). karen (10.0.0.40, real Windows 11, **Hyper-V enabled**; host-setup done: Vagrant 2.4.9, Terraform 1.16.5, pwsh 7.6.7 via MSI, since winget's MSIX needs a logged-on user) is the Windows host (6 jobs). The Windows 11 VM is the fallback. friday (Vagrant 2.4.9 + vagrant-libvirt) runs libvirt and the mutants shard. check-all syncs mirrors to every SSH host (`b5f71630`). |
| **Websites** | Kept separate; the isoloom.com rebuild banner is live. Standard done (0 violations, Vitest, typed scripts). **isothrum.com's editor writes the new syntax** with exact parity to Isoloom (`isothrum-website@7fdc123`, local, not pushed). Leftover: the export panel still says `isoloom down` (a removed command); fix it with the runner's command at the docs gate. |

**Queued for when jarvis and karen are back:**
- `check-all --hosts linux,windows` on every repository;
- the Linux host setup script (Rust user-level tools, Terraform, kind, Helm, Ansible, pwsh) and the Windows host setup script (Vagrant, Terraform, pwsh);
- the launcher's `pnpm quality --update` on both, for the Windows and Linux baselines;
- the flag-file proof on Linux and Windows Docker (owner, mode, no flag in `env`);
- the runner's real run on jarvis;
- the Windows leg of Phase 1b's final run;
- **first:** the launcher's Windows-only Rust code moved during the cleanup has never been compiled (`platform::sys::windows`, the elevate split, job objects, registry, winget ids). Run Windows `pnpm check` and clippy on karen. Once that's clean, raise the launcher's zero-count lints from warn to deny.

## Resume point (session usage limit hit, Fri 2026-10-16 about 03:15; reset 08:00 Paris)

Workers stopped mid-step. Their state is safe in their worktrees. To resume, message each by id (SendMessage keeps its context); start a fresh worker from the worktree only if a resume is refused.

| Worker (id) | Worktree / branch | State at stop | Next step |
|---|---|---|---|
| Phase 4.2 renderers (`ab16abd1636a84517`) | `isoloom-4.2-render` (30 changed files) | the recipe carrier landed (`5ad734eb`); rewriting `carry.rs` down to the one remaining refusal (existing hosts), removing the VM copies path; infra containers to stop on SIGTERM (`init`) | finish the shell, PowerShell, Dockerfile and runner renderers plus the conformance tests, land |
| Phase 4.3 catalogue (`a759f864c0984100c`) | — | **done** (`ab7d4463`) | — |
| Phase 3 docker-vm (`aec8376e1618bb59f`) | `isoloom-dvm-manifest` (19 changed files: the `isoloom-manifest` change), `isoloom-dvm` (34), Isothrum `isothrum-wt/dvm`, launcher `cyber-ctf-launcher-wt/dvm` | was on the Vagrantfile side of docker-vm | **land the `isoloom-manifest` change alone first** (`Step.stdin` as a `PathTemplate`, `project.archive` `with: root`, `isoloom_manifest::ignore`, `Manifest.guest`, `vagrant.check_forwarded_ports`); then the runner pieces it **owns** (`project.archive`, stdin from a path template, `ssh.run` `SendEnv`), alone, on Isothrum; then docker-vm |
| Phase 3 clouds (`a781d690309c173fb`) | `isoloom-cloud`, Isothrum `isothrum-wt/cloud`, launcher `cyber-ctf-launcher-wt/cloud` (a helper `a0cb135bbe4435ce4` was on runner cloud support) | design approved (see Phase 3) | the Isoloom cloud modules, secrets layer, cloud-services `run.json`, launcher import; build on docker-vm's runner pieces once they land; `with.wait_s`; reap; credential-guarded real-run |
| Phase 5b verify (`a8fa25ccb1210df50`) | `isoloom-5b-verify` (2 ahead, 1 changed file) | running 6a's `tf-test` suites against its generator changes | finish, land; baseline toward zero |
| Phase 6a, 1c spec wrap-up, Isothrum 1c | — | **all done** (`906f93e0`, `efef9156`, `isothrum@4c03a3a`) | — |

## 0. Three products

| Product | Role | Contains |
|---|---|---|
| **Isoloom** (this repo) | Converter: one spec to every IaC **and configuration management** tool, and back | Spec, catalogue, plan, backends, import, `verify`, and the **run manifest** each target outputs. Apache-2.0. No runtime. |
| **Isothrum runner** (crate and CLI in `isoloom/isothrum`) | Runner: executes what Isoloom generates | One engine with: lifecycle (up, down, park, resume, re-provision); state and locks; a driver per target; secrets delivery; checks; readiness and recovery; reaper and budgets; snapshots, capture and consoles. Shared by both apps below. |
| **Isothrum desktop app** | UI wrapper with **dynamic specs** | The same logic as the launcher, but the spec is drawn or edited live in Isothrum's lab editor (a drawing compiles to a spec) instead of coming from a fixed catalogue. It uses Isoloom to convert and the Isothrum runner to run. |
| **Cyber CTF launcher** | UI wrapper with **fixed specs** | Curated CTF lab catalogue, accounts, attack box and player UX. It uses Isoloom to read and convert labs, and the Isothrum runner to run them. |

**Repositories:**

| Repository | Licence | Contains | Used by |
|---|---|---|---|
| `isoloom/isoloom` | Apache-2.0 | Converter crates and CLI | everything below |
| `isoloom/isothrum` | Apache-2.0 | Libraries only: the runner, the lab-model compiler (drawing to spec, today's `isothrum-core`) and its WASM build | desktop app, website (WASM), launcher (runner) |
| `isoloom/isothrum-desktop` (new) | Apache-2.0 | The Tauri app: brings Isoloom, the runner, the compiler and the UI together, with dynamic specs | users |
| `isoloom/isothrum-ui` (new, optional) | Apache-2.0 | Shared front-end components: editor canvas, lab view, machines, consoles | desktop app, website, possibly the launcher |
| `isoloom/isothrum-website` | Proprietary, private | isothrum.com: marketing site and blog, the in-browser lab editor (`desktop-editor/`, React Flow), accounts (Better Auth), the project store (`db/`), and the MCP server (`/api/mcp`). Uses the WASM engine (committed today under `public/wasm/`) and later `isothrum-ui` | visitors, Isothrum users |
| `isoloom/isoloom-website` | Proprietary, private | www.isoloom.com: marketing site, blog and the docs, synced from the Isoloom repository by `scripts/sync-isoloom.mjs` | visitors, Isoloom users |
| `Cyber-Courses/Cyber-CTF-Launcher` | Apache-2.0 | CTF UI with fixed specs; uses Isoloom and the runner | players |

**Packages, not pinned git commits:**
- **Rust:** Isoloom and Isothrum crates are published to crates.io.
- **JavaScript:** the UI and WASM packages go to npm.
- **Why:** consumers depend on versions, so the Isoloom history squash can't break them, and each one upgrades on its own schedule.

**Contracts:**
- **Isoloom to Isothrum:** the generated files plus a versioned run manifest, holding:
  - native commands and their order;
  - readiness criteria;
  - outputs: addresses, host ports, URLs;
  - how to run checks;
  - secret slots: names and destinations, never values.
- **Isothrum runner to UIs:** a Rust API plus a JSON event stream, used by both apps.
- **The two apps share most of their logic:** lab view, machines, consoles, start/stop, logs, checks. A shared app core (Rust plus UI components) can be extracted once both run on the runner, so a feature is built once.

**Runner code comes from three runtimes, merged into one in Isothrum:**
- **The launcher's** (about 16k lines, the most mature) is the core.
- **Isoloom's run part** moves in rather than being deleted: secrets delivery including Windows, host readiness, the registry and WinRM retries.
- **Isothrum desktop's** switches onto the result.

## 1. What Isoloom 1.0 is

**One spec, converted to every other infrastructure-as-code and configuration-management tool.**

Two layers are converted:
- **Infrastructure (IaC):** networks, VMs, containers, clouds.
- **Configuration (CM):** what is on each machine: packages, services, files, users, config, commands.

The spec describes both once, and the user picks a tool per layer: Terraform + Ansible, Vagrant + Puppet, Compose + Dockerfile, Proxmox + cloud-init.

- **Isoloom is a converter.** It reads one `isoloom.yml` and writes the native files of each tool: Compose, Kubernetes manifests, Helm, Kustomize, Vagrantfiles, Terraform/OpenTofu modules for Proxmox, ESXi and the clouds, cloud-init, and Ansible inventories and playbooks.
- **It also does the reverse:** it imports those formats into a spec (`isoloom import`).
- **It doesn't run anything.** Users, CI and the Cyber CTF launcher run the native tools on the generated files.
- **One declaration per machine.** A machine says what it is: an OS and the services it runs, from a catalogue. It never says how a platform runs it, so there are no `docker:` or `vm:` blocks. Each target decides how to deliver it: an official container image or a built image on container targets; a fresh VM plus an Ansible role on VM targets.
- **Honest proof.** Every generated file passes its format's official validator. Each target sits in a tier that says how far it has been proven.

## 1a. The launcher's promise sets 1.0's priorities

Isoloom exists for the Cyber CTF launcher. Its three modes must work in 1.0, on macOS, Windows and Linux hosts:

| Launcher mode | What runs | Isoloom targets (Tier 1 for 1.0) |
|---|---|---|
| **Local** (`Container`, `LocalVm`) | Docker on the player's machine, or VMs through Vagrant on VirtualBox, Hyper-V, VMware or QEMU/libvirt. Containers can also run inside a VM | `docker`, `vagrant` (every local provider), `docker-vm` |
| **Server** | The player's own Proxmox or ESXi server, through Vagrant (`vagrant-proxmox`, `vagrant-vmware-esxi`, configured through the launcher's `connection_env` contract) or Terraform | `proxmox` (Terraform), the Vagrant Proxmox and ESXi providers |
| **Cloud** | The player's AWS, Azure, GCP, DigitalOcean, Linode or Oracle Cloud account, through Terraform with each provider's CLI credentials | `cloud-vm`, `cloud-docker`, `cloud-services` |

**Rules that follow:**
- **Terraform is core to 1.0** for server and cloud. Only the Terraform provider plugin is dropped.
- **Every phase does the launcher's targets first.** Kubernetes, Helm, Kustomize, hybrid, hosted, external and the Terraform ESXi target follow. They stay ("it's the deal") but aren't Tier 1 for 1.0.
- **Contracts the launcher relies on are kept and tested:** the Vagrant providers reading their connection from the environment; the cloud-services module layout, state and inputs; the check runners.
- **Real proof needs real infrastructure.** Local is proven on this Mac now. Server mode needs a Proxmox or ESXi host, and cloud mode needs budget-capped accounts. Without them, server and cloud ship as generated and validated, not as proven.

## 2. Decisions taken (2026-10-15)

| Topic | Decision |
|---|---|
| Scope | Converter only. The runtime goes: `run`/`test`/`down`/`status`/`connect`/`exec`/`capture`/`reset`/`provision`, the registry, JSON lifecycle events, run-time secret delivery and Isoloom-executed checks. |
| Coverage | `isoloom-coverage` (13k lines, 30 format tables, refresh scripts, drift job) is replaced. What stays: a spec-field × target capability matrix generated from each backend's declared capabilities, and conformance against official validators. |
| Syntax | One form: `os` plus `service`(s) from the catalogue, or a generic custom `app` (own image, or a build recipe). A small per-target `overrides:` handles rare platform specifics. No `docker:`/`vm:` blocks. |
| Catalogue | Data-driven entries: a container form (official image, or a build from the OS base image), a VM form (official Galaxy content only, else our own role), and a parity contract. |
| Breadth | Every current target stays ("it's the deal"), placed in tiers. The launcher's targets (local, server, cloud) come first in every phase and are Tier 1 for 1.0; see 1a. |
| Tiers | **Tier 1:** proven by real runs from every host platform the target exists on (macOS, Windows, Linux). **Tier 2:** generated and validated by the official validators. **Tier 3:** experimental. The tier is shown in the matrix and in the CLI. |
| Host platforms | Every running platform is supported: macOS, Windows and Linux. The CLI ships for all three, and generated output must run from all three hosts: no `sh`-only `local-exec`, no host path separators or line endings baked in, no tool assumed that a host lacks. A target that only exists on some hosts (Hyper-V on Windows, libvirt on Linux, Parallels and UTM on macOS) is proven on those. |
| CM tools in 1.0 | Ansible, cloud-init, shell/PowerShell and Dockerfile. Puppet, Chef, Salt and PowerShell DSC follow in 1.x. |
| IaC tools in 1.0 | The current families: Compose, Kubernetes/Helm/Kustomize, Vagrant (every provider), Terraform/OpenTofu for Proxmox, ESXi and the six clouds. Pulumi and Podman: not yet. |
| Secrets | A conversion layer per tool: the spec names secrets and where they come from, and each output gets that tool's native mechanism. Values never land in a generated file. On clouds that means the cloud's own store: AWS Secrets Manager or SSM, Azure Key Vault, Google Secret Manager, OCI Vault. Where a cloud has none (DigitalOcean, Linode), a documented fallback is used. |
| Terraform provider | Only the provider plugin is dropped for 1.0 (recommended; the maintainer had no preference). Terraform output (Proxmox, the clouds, cloud-docker, cloud-services) is core to 1.0. In a converter, Terraform users point a `module` at the generated folder or use the published registry modules. Revisit later as an Isothrum provider that runs labs. |
| Licences (everything open source, least constraints) | **Isoloom, Isothrum (runner, desktop app, editor) and the Cyber CTF launcher:** Apache-2.0. It is permissive, with a patent grant and patent retaliation, explicit contribution terms, and no trademark rights granted, so forks can't use the Isoloom or Isothrum names. No CLA, only a DCO sign-off. Register the names as trademarks if branding matters. Anyone may build anything on them, closed products included. Official Ansible collections (GPL-3.0) are referenced, never embedded in a binary. The Isothrum website (accounts, project store, MCP) stays private and proprietary. The CTF lab content and courses are a separate decision. |
| Compatibility | **None.** The project isn't even in beta, so 1.0 supports only the new format: no aliases, no deprecations, no `isoloom migrate`, no tests on old specs. Old specs fail validation with an error naming the unknown field. The spec stays at `version: 1`, the first stable format. Examples are rewritten by hand. The CTF lab files are rewritten by hand at the end, before the launcher's release. |
| Workflow | Everything lands directly on each repository's local `main`, fast; parallel workers use scratch worktrees only to avoid clashing, and fast-forward `main` themselves. No PRs, no lingering branches, no releases. The three repositories sit side by side with **path dependencies** (no git revs) until the 1.0 publish. One script runs every repository's checks. At the first satisfying version: squash, re-enable CI, publish. |
| Docs | Written at the end, inside the repo. The website is rewritten from them. |
| Launcher | **Decided:** integrate it from the walking skeleton (Phase 2) on, instead of after 1.0. It is the reason 1.0 exists, and only it can validate the run manifest. Launcher, runner and Isoloom ship 1.0 together. |
| GitHub | Done: PRs and issues closed, releases and tags deleted, workflows disabled. |

## 3. Target architecture

**Configuration model.** A small, typed, tool-neutral resource model sits in the plan:
- `repository`, `package`;
- `file`, `template`;
- `user`, `group`;
- `service`;
- `command`, with an already-done guard;
- `firewall rule`.

Catalogue entries and machine settings are written once in it. Configuration backends render native code from it:
- Ansible tasks and roles;
- cloud-init;
- shell and PowerShell;
- Dockerfile;
- Puppet manifests, Chef recipes, Salt states, PowerShell DSC.

Parity between the container and VM forms holds by construction, and the parity contract still checks it.

**Pairing rules (IaC × CM):**
- Vagrant calls Ansible, Puppet, Chef or Salt as native provisioners.
- Terraform hands off to the chosen CM tool.
- Container targets bake configuration into the image.
- cloud-init configures at first boot.

```
isoloom.yml ──► isoloom-spec ──► isoloom-plan ──► backends ──► native files
 (one form)     (model,          (resolve os +     (compose,      (Compose, K8s, Helm,
                 validate,        service through   kubernetes,    Kustomize, Vagrant,
                 validate)        the catalogue,    vagrant,       Terraform, cloud-init,
                                  the environment)  terraform,     Ansible)
                                                    ansible)
                isoloom-catalogue                   isoloom-render (builders: HCL, Ruby, YAML,
                (entries, roles,                     shell, PowerShell, Dockerfile)
                 contracts)
                isoloom-import (native files ──► spec)
                isoloom-verify (official validators on the generated files)
                isoloom-targets (registry, capabilities, the matrix, tiers)
                isoloom (CLI: generate, validate, import, verify, matrix)
```

- **Lowering, not rewriting:** the new spec is a front end. It resolves `os` + `service`/`app` and lowers each machine into today's plan IR (`Environment`), which already carries the container and VM forms. The backends keep working, and their 800+ tests keep guarding them. Internals are reshaped only where a feature needs it.
- **Removed:** `isoloom-runtime`, `isoloom-coverage`, and the runtime parts of the CLI.
- **New:** `isoloom-catalogue` and `isoloom-verify`.
- **Changed:** `isoloom-external` becomes the Ansible output (existing hosts are an inventory plus playbooks).

## 3b. Engineering standard (every repository, every phase)

**All clean code principles are respected in all three codebases** (maintainer, 2026-10-15). Clean code is built in as we go, not left for a later pass.
- **Enforced by a tool** wherever a tool can check it.
- **Otherwise checked by the review checklist below** at the end of every phase.
- **The target is zero violations in every repository by 1.0,** not just "no worse".

### Design rules
- **Functional core, imperative shell.** Parsing, validation, planning, rendering and manifest building are pure functions over data, with no I/O, no clock and no environment. All I/O (processes, files, network, Tauri) sits in a thin shell at the edge. The pure core is unit-testable without mocks.
- **Ports and adapters** for everything that touches the outside world:
  - **Runner:** drivers (Docker, Vagrant, Terraform, Proxmox, clouds) sit behind a `Driver` trait, and the process runner and the state store are traits too. Tests use fakes, never a real tool.
  - **Launcher:** Tauri commands are thin adapters that call plain Rust services. Domain logic never imports `tauri`.
- **Dependency direction is one-way, and a test checks it.**
  - **Isoloom:** spec → plan → render → backends → targets → cli. No backend depends on another backend.
  - **Isothrum:** compiler and runner, with no UI.
  - **Apps:** depend on libraries, never the reverse.

  Cyclic or upward dependencies fail the build.
- **Parse, don't validate.** Raw input is converted once into types that can't hold invalid states:
  - newtypes for names, ports, CIDRs, versions and IDs;
  - enums instead of booleans and strings;
  - no `Option` soup.
- **Errors are typed and actionable.**
  - One `thiserror` enum per library crate.
  - Messages name the field path, the reason and the fix.
  - `anyhow` only in binaries.
  - No `unwrap`, `expect` or `panic` outside tests.
- **No hidden state:** no globals, no `set_var`, no `unsafe`. Time, randomness, the environment and the filesystem are passed in.
- **Determinism:** ordered maps in all output, and render-twice tests.
- **One way to do each thing.** A helper exists once. A second copy fails review, and a duplicate-code check flags it.

### Size and shape limits (lint-enforced)
- **Rust:**
  - functions ≤ 60 lines, ≤ 5 parameters, cognitive complexity ≤ 15;
  - files ≤ 400 lines of code;
  - clippy `pedantic` at deny, with a short commented allow-list;
  - `missing_docs` on libraries.
- **TypeScript:**
  - `strict`, with no `any`;
  - typescript-eslint `strict-type-checked`;
  - components ≤ 200 lines, functions ≤ 60;
  - no default exports outside framework entry points;
  - Tauri types generated from Rust (`ts_bindings`), never hand-copied.
- **Naming:** one vocabulary across all three repositories (lab, machine, network, target, form, manifest, run, driver). Comments say *why*, never *what*.

### Tests
- **The pyramid:**
  - unit tests for the pure core;
  - `insta` snapshots for every renderer;
  - property tests for parsers, address planning and quoting (HCL, Ruby, shell, PowerShell injection safety);
  - contract tests: Isoloom and Isothrum test against the same run-manifest fixtures;
  - fake-tool tests for drivers;
  - real runs only for Tier 1.
- **Every bug fix starts with a failing test.**
- **Fast and deterministic:** no sleeps, no network in unit tests. The unit suite of each repository runs in under a minute.

### Principles and how each is checked

| Principle | Checked by |
|---|---|
| Meaningful, consistent names; one vocabulary | review checklist, `typos` |
| Small functions that do one thing; one level of abstraction | clippy `too_many_lines` 60, cognitive complexity 15, eslint `max-lines-per-function` |
| Few arguments; no flag arguments | clippy `too_many_arguments` 5, `fn_params_excessive_bools`; review (enums over booleans) |
| Single responsibility, small modules | file ≤ 400 LOC (TS components ≤ 200), the module layout in 3b, review |
| Open/closed, Liskov, interface segregation, dependency inversion | ports-and-adapters traits, small traits, review |
| DRY | duplicate-code check (`jscpd` across Rust and TS), review |
| KISS and YAGNI | review: no speculative abstractions, no unused generality; `cargo machete` and `knip` for unused deps and exports |
| Command-query separation; no hidden side effects | functional core, pure functions, review |
| Law of Demeter | review: no long call chains through other objects' internals |
| Errors as types, not codes or strings; no null | typed errors, `Option`/`Result`, no `unwrap`/`expect`/`panic`, TS `strictNullChecks` |
| Clean boundaries with third-party code | adapters around tools, processes, Tauri, HTTP and SDKs |
| Clean tests (F.I.R.S.T.: fast, independent, repeatable, self-validating, timely); one concept per test | unit suite under a minute, no sleeps or network, review |
| Comments explain *why*; no commented-out code, no noise | review, the eslint and clippy rules for commented-out code |
| Formatting | rustfmt, prettier |
| No dead code | `dead_code` at deny, `cargo machete`, `knip` |

### Ratchet to zero
- Each repository has `quality-baseline.json`: the current count of violations per rule, such as oversized files, long functions, `allow`s, `any`s and duplicates.
- A check fails if any count goes **up**, and every phase drives the counts **down**. The ratchet replaces launcher issue #150.
- **All counts reach zero before the 1.0 release:**
  - Isoloom by the end of Phase 1;
  - Isothrum when the runner lands (Phase 2–3);
  - the launcher's non-runtime code in Phase 0.6;
  - the launcher's runtime as it moves into the runner.
- The only allowed exceptions are local `#[allow]` or `eslint-disable` lines with a written reason, listed in the baseline and reviewed.

### Process
- **Every change:** format, lint, test, the ratchet and the dependency-direction check, all through `check-all`.
- **Every phase ends with a review pass** over its own diff: a code review for bugs, a simplify pass for duplication and dead code, and the principles table as a checklist. Findings are fixed before the next phase starts.
- **Docs stay alignable.** Every phase appends what it changed to `plan/docs-facts.md` (one line per user-visible fact: field, command, target, behaviour, limit). Generated docs (spec reference, targets matrix, catalogue, CLI help, manifest schema) stay current through their stale-file tests. No hand-written docs are written before Phase 7.
- **Dev tasks are `cargo xtask` commands, not shell scripts,** so they run the same on macOS, Windows and Linux. `scripts/*.sh` move into `xtask` in Phase 1.

### Repository layouts
- **Isoloom** (applied in Phase 1, after the snapshots land, so moves are proven output-neutral):

  ```
  crates/core/      isoloom-spec, isoloom-catalogue, isoloom-plan, isoloom-render
  crates/backends/  isoloom-compose, isoloom-kubernetes, isoloom-vagrant, isoloom-terraform,
                    isoloom-ansible (today's isoloom-external), and the CM backends as they arrive
  crates/tools/     isoloom-import, isoloom-verify
  crates/targets/   isoloom-targets (registry, capabilities, tiers)
  crates/cli/       isoloom
  xtask/            check-all, verify, dev-link, snapshots
  catalogue/        data: <service>/{entry.yml, contract.yml, roles/}
  examples/         specs only, no committed outputs
  schemas/          generated JSON Schemas: spec, run manifest
  tools/verify/     the pinned validator image
  .github/          community files (CONTRIBUTING, SECURITY, CODE_OF_CONDUCT) and the disabled workflows
  ```

  - `rfcs/` and the hand-written `docs/` are removed now and rewritten at the end.
  - `install.sh` moves to `packaging/`.
  - `rust-toolchain.toml` pins the toolchain.
- **Isothrum** (applied in Phase 2, when the runner arrives):

  ```
  crates/isothrum-compiler/  today's isothrum-core (drawing → spec)
  crates/isothrum-wasm/
  crates/isothrum-runner/    manifest executor, Driver trait, drivers as modules, state store, events
  crates/isothrum-cli/
  apps/desktop/              until it moves to isothrum-desktop (1.x)
  xtask/
  ```

  The WASM npm package name stays the same, so the private website keeps building.
- **Launcher:**
  - **Now:** reorganize everything except `runtime/`.
    - Group the loose `src-tauri/src/*.rs` modules into domains: `app/` (window, menu, quit, logging, config), `labs/`, `account/`, `terminal/`, `platform/` (fs, atomic files, MSIX data, exec), `cloud/`, and `commands/` (thin Tauri adapters only).
    - Use one module convention (`foo.rs` plus `foo/`, never `mod.rs`).
    - Group the packaging assets (`store/`, MSIX) under `packaging/`, and the attack-box image under `images/attack-box/`.
  - **Phase 2–3:** `runtime/` moves into `isothrum-runner`, and the launcher keeps only CTF-specific code.

## 3d. Rust best practices (Isoloom, Isothrum, the launcher's `src-tauri`)

These go beyond lints. Each one names the tool or review that enforces it.

**Workspace hygiene**
- **Workspace settings:** edition 2024 and resolver 3. Every crate inherits `[workspace.package]`, `[workspace.dependencies]` (one version per dependency, declared once) and `[workspace.lints]` (`lints.workspace = true`). Checked by an `xtask` test over `cargo metadata`.
- **Pinned and declared versions:** `rust-toolchain.toml` pins the toolchain, `rust-version` declares the MSRV, and `cargo msrv verify` (or `cargo hack --rust-version check`) proves it.
- **Features:** additive only, never mutually exclusive. `cargo hack check --feature-powerset` proves every combination builds.
- **Supply chain:** `Cargo.lock` is committed. `cargo deny check` covers advisories, licences (an allow-list compatible with Apache-2.0), duplicate versions (`bans`) and sources (crates.io only, no git dependencies at release). `cargo machete` catches unused dependencies.

**API design** (the Rust API Guidelines checklist, reviewed per public item)
- **Common traits:** public types implement the traits a user expects (`Debug` always; `Clone`, `PartialEq`/`Eq`, `Hash`, `Default`, `Display` where they make sense; `Send`/`Sync` asserted in tests for types that cross threads).
- **Conversions:** through `From`/`TryFrom`/`AsRef`/`FromStr`, not ad-hoc `to_x()`/`from_x()` functions. Constructors are `new` or a builder for more than about three optional fields.
- **Types carry meaning:** newtypes for every identifier and quantity; enums instead of `bool` and stringly modes; `#[non_exhaustive]` on public enums and structs that may grow; sealed traits for traits not meant to be implemented outside the crate.
- **Ownership:** borrow in arguments (`&str`, `&Path`, `impl AsRef<Path>` where it helps), return owned values, and `Cow` only where it measurably saves copies. No `clone()` to satisfy the borrow checker without a reason; clippy `redundant_clone` and review catch it.
- **`#[must_use]`** on pure functions and builders (clippy `must_use_candidate`). Iterators over index loops. No `Rc<RefCell<…>>` in library APIs.
- **Visibility:** `pub(crate)` by default (`unreachable_pub`). Every `pub` item is deliberate, documented and covered by `cargo semver-checks` once the crate is published.

**Errors**
- **Library errors:** one `thiserror` enum per crate, with a `pub type Result<T> = std::result::Result<T, Error>`. Variants carry their `#[source]`, so the cause chain is never lost. No `Box<dyn Error>`, `anyhow` or `String` errors in libraries; `anyhow` is allowed only in binaries and `xtask`.
- **Messages:** lowercase, no trailing period (API guideline C-GOOD-ERR). User-facing text with the field path, reason and fix lives in one rendering layer, not scattered.
- **No panics in library code paths,** enforced by lints. A truly impossible state is an `#[expect]` with its reason, or better, a type that makes it unrepresentable.

**Concurrency and I/O** (the runner especially)
- **Sync or async, chosen once per crate and documented.** In async code: no blocking calls (`spawn_blocking` for processes and files), cancellation-safe steps, timeouts on every external call.
- **Observability:** structured `tracing` spans and fields per run, step and driver in the runner. No `println!` or `eprintln!` outside binaries. The pure core logs nothing.
- **Processes** are spawned only through the `ProcessRunner` port, with argv vectors only, never a shell string.

**Documentation**
- **Crate and item docs:** every crate has crate-level docs (what it is, where it sits, a runnable example). Every public item is documented, with `# Errors` and `# Panics` sections (clippy `missing_errors_doc` and `missing_panics_doc`).
- **Docs build clean:** doctests run and pass; `RUSTDOCFLAGS=-Dwarnings cargo doc` with `broken_intra_doc_links`, `private_intra_doc_links` and `missing_crate_level_docs` at deny.

**Testing depth**
- **`cargo nextest`** is the test runner: each test runs in its own process, with slow-test reporting and retries off, so flaky tests fail loudly.
- **Naming:** tests are named for the behaviour (`refuses_a_service_on_a_container_target_when…`). One concept per test, no `#[should_panic]`, no sleeps, and a fake clock where time matters.
- **Property tests** (`proptest`) for every parser and every quoting or escaping function. **Fuzzing** (`cargo fuzz`) for the spec parser, the manifest parser and the HCL/Ruby/shell/PowerShell quoting: each target runs in `check-all --deep`, and every crash it finds becomes a regression test.
- **Mutation testing** (`cargo mutants`) on the core crates (spec, plan, render, manifest) and the runner's executor. Every surviving mutant is either killed by a new test or listed in `mutants.toml` with its reason (an equivalent mutant). That proves the tests check behaviour, not just run code.
- **Snapshots** (`insta`) only for whole generated files. Never snapshot something a precise assertion should check.
- **`unsafe`:** the single `sys` module (runner and launcher) runs under Miri where the platform allows, and every `unsafe` block carries a `// SAFETY:` comment (clippy `undocumented_unsafe_blocks`).

**Performance budgets**
- Generating every example on every target stays under a fixed time budget, checked by a test, so regressions show.
- Hot paths (spec parsing, lowering, rendering) get `criterion` benchmarks run by `cargo xtask bench`, compared against a stored baseline.

**Where it applies**
- **Isoloom:** Phase 1c (the tools are wired in by the 1c tooling unit, and every crate meets it at 1c's exit).
- **Isothrum:** from its first line (the runner skeleton follows it now).
- **The launcher's `src-tauri`:** as its code is cleaned (non-runtime now, runtime as it moves).

## 3c. The two websites

| | isoloom.com (`isoloom/isoloom-website`) | isothrum.com (`isoloom/isothrum-website`) |
|---|---|---|
| **Stack** | Next.js 16, React 19, Tailwind 4, Base UI, next-md-blog; Vercel | The same, plus React Flow (the editor), Better Auth, `mcp-handler` and a SQL project store; Vercel |
| **Size** | About 7.5k lines of TS/TSX, 43 Markdown pages (42 docs) | About 17k lines of TS/TSX, 50 Markdown pages (blog, glossary) |
| **Depends on** | Isoloom's docs, via `sync:isoloom` | Isothrum's compiler and WASM build, and the Isoloom spec the editor writes |
| **Licence** | Proprietary, private | Proprietary, private |

**Rules:**
- **A push to `main` deploys production** (Vercel): both sites are live. So they follow the same local-`main` workflow, and nothing is pushed until a release, with the maintainer's OK at that moment.
- **The engineering standard (3b) applies in full:** lint levels, the ratchet to zero, `knip` and `jscpd`, components ≤ 200 lines, typed boundaries.
- **The two sites stay in separate repositories** (maintainer, 2026-10-15). Their shared scaffolding is not merged; each repository meets 3b on its own.

**The Cyber CTF website** (`cyberctf-web`, Cyber-Courses/CyberCTF-Web, Next.js 16; labs catalogue from CyberBackend, per-player evidence submission, launcher download, docs, dashboard). Maintainer (2026-10-16): "let's make cyber ctf great as the launcher". It follows the same rules: local `main`, no PRs, no push without the maintainer's OK (a push deploys production).
- **Running now:** consolidating its local branches onto `main` (archived first), the 3b standard to zero, Vitest plus Playwright, and security tests pinning the audit's fixes.
- **Bug found (2026-10-16):** the launcher's `labs::launch_env` dropped every `CTF_FLAG_<KEY>` CyberBackend sends (it kept only `CTF_API_URL` and `CTF_LAUNCH_TOKEN`), so per-player flags never reached any lab on either engine. Being fixed test-first; flags must arrive as files, never in the environment.
- **Decided (2026-10-16): dev's merged PRs #14 (lab objectives) and #17 (lighter catalogue payloads, SEO, cyberctf.org) move onto `main`.** The single evidence form and `submitEvidence` are removed. The lab page submits per objective through `submitObjective(labId, key, answer)`, and a lab without objectives uses the implicit key `main`. Production CyberBackend already serves objectives. Once `main` carries #14 and #17, `dev` can be retired in the GitHub cleanup.
- **Running now:** an end-to-end check of the flag contract, now per objective: CyberBackend's per-player flag for each objective key → the launcher (`CTF_FLAG_<KEY>`) → Isoloom `files: [{input}]` → the runner's slot → the lab → cyberctf-web's `submitObjective` for that key → CyberBackend. Both single-`main` and multi-objective labs are covered, and the key normalisation is pinned by a test.
- **Waiting on the maintainer:** work in CyberBackend (`~/code/cyberbackend`, local main at `cc749ec`). It is test-only plus one exported validator: `isObjectiveKey` shared with `publishLab`; a property test that every objective key maps injectively to a valid Isoloom input name `CTF_FLAG_<KEY>`; a leak test that no flag or wrong answer is stored in the graph or logs; and a local-only e2e backend (a throwaway Neo4j container, keys and secrets generated per run, 127.0.0.1). Local main only, no push, no deploy, no shared database. cyberctf-web's opt-in e2e (`228d055`) is on main.
- **Later:** a sync that shows each launcher mode's real tiers (from Isoloom's `tiers.json` and `docs/matrix.md`), and where each lab runs, after the lab rewrite. The launcher page and docs copy come at the docs gate.

**When:**
- **Phase 0.7, the engineering standard in both sites:** measure, then lint configs, the ratchet, the folder layout, and driving to zero. It can run alongside other phases, because the sites don't block the converter.
- **Phase 2:** the Isothrum compiler (`isothrum-core`, soon `isothrum-compiler`) and the website editor **write the new spec syntax**. The editor's model must accept exactly what Isoloom accepts. For example, a rate with no unit is reported, as Isoloom does; today the editor reads it as bits. With no old-format support, a drawn lab would otherwise break. They test against Isoloom's spec examples and schema.
- **Until Phase 7:** isoloom.com carries a "1.0 is being rebuilt" banner (live since 2026-10-15, commit `60098c4`), because its pages still describe the 0.x preview. The banner is removed in Phase 7.
- **Phase 7, release:** isoloom.com's docs are regenerated from the repository's new docs, and its pages rewritten (features, targets, comparisons) from the 1.0 facts. On isothrum.com: the editor on the 1.0 spec, the WASM from the published npm package instead of the committed `public/wasm/`, and the release announcement. Both are pushed with the 1.0 release.
- **1.x:** `desktop-editor/` and the editor components move into `isothrum-ui`, shared with the desktop app. Accounts (Better Auth, ex-isothrum #30) and the project store follow.

## 4. Work, in order

**Principles:**
- **The safety net comes before the surgery.** Every change is measured against characterization snapshots of the generated output and against the official validators.
- **Vertical first, then wide.** One real lab goes end to end through the new path before any breadth.
- **Lower, don't rewrite.** The new syntax lowers into the existing plan IR.
- **The launcher's needs decide what is in 1.0.** Everything else is 1.x.
- **Parallelize only disjoint work.** Targets and drivers can run in parallel; the spec and the plan core are serial.
- **Close each phase cleanly.** Every phase ends green across all three repositories, with a short review of its own diff.

Sizes: S is about a day of focused work, M a few days, L about a week. There are four tracks; Tracks B and C start now, in parallel with A.

### Track C: start today (no code, long lead times)
- **Real infrastructure for server and cloud mode:**
  - a Proxmox host, and an ESXi one if possible;
  - budget-capped accounts: AWS first, then Azure and GCP.

  Without them, server and cloud mode ship validated but not proven.
- **Reserve names:** the crates.io names `isoloom`, `isoloom-*` and `isothrum*`, and the npm scope.
- **Trademark filing** for Isoloom and Isothrum, if branding matters.
- **The CTF lab specs:** every lab spec from the Cyber-Courses and CyberCTF repositories, copied locally (private) as reference material only, with a report of which features the labs use. It tells the new syntax what to express first; the files are rewritten at the end.

### Phase 0: The safety net (M)

1. **Characterization snapshots.** Generate every example on every target from today's `main`, and snapshot the output with `insta`. Any later change that alters output must be deliberate and reviewed.
2. **(Dropped.)** Old specs are not a regression suite, because the old format isn't supported.
3. **`verify`, the seed version.**
   - **One pinned toolchain image** with every official validator:
     - `docker compose config`;
     - terraform plus tflint;
     - vagrant;
     - kubeconform and helm;
     - cloud-init;
     - hadolint;
     - ansible-lint;
     - shellcheck;
     - pwsh with PSScriptAnalyzer.
   - **Behaviour:** it runs over the snapshots. The image runs the same on macOS, Windows and Linux.
   - **Reuse:** salvage the generate-everything pipeline from `v1/security-scanning`.
   - **Baseline:** failures found now become a baseline of existing problems, fixed in later phases rather than ignored.
4. **Local multi-repository setup.** Clone `isoloom`, `isothrum` and the launcher side by side, with `[patch]` path dependencies. One `check-all` script runs every repository's checks.
5. **Cross-platform check (decided).** A manual-only GitHub workflow that runs `check-all` on macOS, Windows and Linux on demand. No PRs and no automatic runs. Without it, Windows and Linux bugs surface only after the squash.

6. **The engineering standard in the launcher:** lint configs at the 3b levels, the ratchet, the dependency-direction check, `check-all` wiring, the folder reorganization (everything except `runtime/`), and the non-runtime code driven to zero violations.

**Exit:** snapshots and `verify` are green or baselined, and `check-all` passes.

### Phase 1: Subtract (S–M)

Now safe, because the net is in place.
1. **Delete the runtime from Isoloom.** It stays on a salvage branch. The runner takes from it what it needs in Phase 2; nothing is copied into Isothrum blindly.
2. **Delete `isoloom-coverage`.** Its schema checks are covered by `verify`, and its capability declarations stay in the backends.
3. **Delete the committed example outputs.** The snapshots replace them.
4. **Delete the compatibility debt** and fix the small debts: the Ruby-dependent test, the dead-code fixtures.
- **Keep `isoloom targets` (and its refusal reasons) working without the runtime crate.** The characterization snapshots call it (today it reaches into the runtime for `--host`).
5. **Apply the engineering standard to Isoloom:** the repository layout (3b), `xtask` replacing `scripts/*.sh`, the dependency-direction test, `rust-toolchain.toml`, and the ratchet **driven to zero**. Every move is proven output-neutral by the snapshots.

### Phase 1c: Deep organization and clean code, no shortcut (maintainer, 2026-10-15)

Phase 1 is done only when every Isoloom crate is deeply organized and has **zero** violations. Grouping the crates isn't enough.

1. **The strictest lint set, enforced at deny,** across the workspace:
   - **Exceptions need reasons:**
     - `clippy::allow_attributes` and `allow_attributes_without_reason`: every exception is `#[expect(lint, reason = "…")]`, which also fails once it's no longer needed;
     - `#![deny(warnings)]` in CI terms, through `check-all`.
   - **Rust and clippy:**
     - `unreachable_pub`: the public API is minimal, `pub(crate)` by default;
     - `missing_docs` on every public item, and `clippy::missing_errors_doc` / `missing_panics_doc`;
     - clippy `pedantic` at deny.
   - **Clippy restriction rules:**
     - `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` and `dbg_macro`;
     - `print_stdout` and `print_stderr` in libraries;
     - `indexing_slicing` and `string_slice` in libraries;
     - `as_conversions`, `wildcard_imports`, `shadow_unrelated`, `let_underscore_must_use`, `clone_on_ref_ptr` and `rc_buffer`;
     - `str_to_string` and `get_unwrap`.
   - **Limits:** `too_many_lines` 60, `too_many_arguments` 5, `cognitive_complexity` 15, `fn_params_excessive_bools`, `struct_excessive_bools`, and files ≤ 400 LOC.
2. **Organization inside every crate,** reviewed against one template:
   - **Module layout:** `lib.rs` holds only the module tree and the re-exported public API. One concept per module, and folders by domain concept, never by kind (no `utils.rs`, `helpers.rs` or `misc.rs`).
   - **Errors:** one error enum per crate in `error.rs`, with the CLI mapping codes to exit codes in one place.
   - **Tests:** unit tests next to the code (`#[cfg(test)] mod tests` in the same file, or `foo/tests.rs`), integration tests in `tests/`, fixtures in `tests/fixtures/`, snapshots beside their test, and shared test helpers in one `testing` module (`#[cfg(test)]` or a dev-only crate).
   - **Names:** the 3b vocabulary everywhere; no stutter (`isoloom_spec::spec::SpecSpec`); the same word for the same concept in every crate.
   - **Dependencies:** each crate lists only what it uses (machete); the dependency-direction test passes.
3. **The 3d tooling,** wired into `cargo xtask` and `check-all`: nextest, cargo-hack feature powerset, msrv verification, cargo-deny bans and sources, doc lints at deny, fuzz targets, cargo-mutants on core, criterion benchmarks with a stored baseline, and the generation-time budget test.
4. **Ratchet every count to zero, crate by crate,** with real refactors only. This runs in parallel by crate group: backends, then targets, tools and cli, then core, once the spec front end has landed in core.
5. **A principles and 3d review per crate,** using the 3b table and the 3d API, errors, docs and testing items as a checklist: SRP, DRY, KISS/YAGNI, CQS, Demeter, boundaries, F.I.R.S.T. tests and names. The review notes every finding with its fix, and the phase closes when every finding is fixed.

**Exit:** green net (snapshots byte-identical, `verify` with no new failures); `quality-baseline.json` at zero for every crate; every surviving mutant in core killed or listed with its reason; fuzz targets clean; every `#[expect]` carries a reason; each crate's review checklist closed; all of it on three hosts (`check-all`).

### Phase 2: The walking skeleton (M–L)

One real lab, end to end: the new spec, Isoloom, run manifest, runner, launcher.
1. **The new spec front end:**
   - a machine has `os`, plus `app:` (a custom image or build recipe; CTF labs need this first) or `service:`;
   - the shared fields;
   - `overrides:`.

   It lowers into today's plan IR.
- **Design input from the 376 CTF lab specs** (`~/code/isoloom-corpus/corpus-features.md`):
  - **Containers:** 344 labs are container-only, and 409 of 479 container machines build from a local folder. So `app:` with a build recipe comes first.
  - **The flag pattern:** 330 labs repeat it (an input, an init script, a named volume). It deserves a first-class, generic construct, such as an input materialised into a file at start-up.
  - **Networks:** every lab has one network, and 327 have no internet.
  - **Checks:** 822 in all (http, script, tcp, with `wait`).
  - **VMs:** 19 VM labs, mostly Windows and GOAD-style, run with an Ansible controller.
  - **Messages:** 371 reference port fields by path, so the new names must be stable.
2. **Rewrite by hand** the examples and that one lab (invoice-portal-api) in the new syntax. Old `docker:`/`vm:` blocks become `app:`, with one recipe both forms derive from.
3. **Run manifest v0 for the `docker` target:** commands, readiness, outputs and check runners. Its JSON schema and fixtures live in Isoloom, and both repositories test against them.
4. **The runner skeleton** in `isoloom/isothrum`: the manifest executor and the Docker driver, extracted from the launcher's runtime (its most proven code).
5. **The launcher** runs one real lab (invoice-portal-api) through Isoloom and the runner, behind a switch. Its real-lab test passes.

**Exit:** that lab passes end to end through the new path. The contract is validated by its real consumer.

### Phase 2 design, decided (2026-10-15)

The full design is in `plan/phase2/`: `run-manifest-v0.md`, its JSON Schema, `runner-extraction.md`, and two worked manifests. Its open questions are settled as follows.

**Run manifest:**
- **Format:** one `run.json` per target, holding the native commands; drivers hold only documented actions. Strict (`additionalProperties: false`) until 1.0, then only `x-` extensions.
- **Run name:** the embedder sets it (`cyberctf-<id>`); the CLI defaults to `lab.name`.
- **Re-provision:** the embedder passes inputs again; the runner stores no secret values.
- **Generated files are host-neutral:** `local-exec tar` becomes a `project.archive` with a `tf_var`; `.sh` helpers become module outputs or steps; `cloud.requires` uses a per-OS `host_argv`.
- **Check timeouts:** Isoloom computes them (waits + 15 s per check + 60 s), so the manifest is self-contained. This fixes the launcher's 5-minute cap that kills long-waiting labs.
- **Docker park:** stays `stop` (no `pause`, which keeps memory).
- **Vagrant:** `VAGRANT_DOTFILE_PATH` moves into the run's state in Phase 3.
- **Vagrant Proxmox provider:** Tier 2, not Tier 1. The launcher runs Proxmox through Terraform only, and the plugin is unmaintained.

**Runner:**
- **Crates:** `isoloom-manifest` (the types, the generated schema and the fixtures) in Isoloom; `isothrum-runner` (the library) and `isothrum` (the CLI) in Isothrum.
- **`unsafe`:** safe wrapper crates where they exist (`process-wrap`). The rest (UAC elevation, the Windows console, Ctrl+C) sits in one audited `sys` module, the only place `unsafe` is allowed.
- **Detached runs:** through the embedder's own executable, via a runner entry point, so there's no second signed binary.
- **Windows delivery:** Vagrant's own `winrm` and `upload`, plus the cloud secret stores. Python is used only for Proxmox and ESXi Windows machines, until OpenSSH-on-guest or a Rust client removes it. The goal is no host Python by 1.0.
- **Live cloud labs at the switch:** a one-time import into the runner's records, so players' running labs stay destroyable.
- **Homes:** separate for the launcher (its app data) and the CLI (`~/.isothrum`); the CLI can attach with `--home`.
- **Events:** the event stream has its own version.
- **The launcher's switch:** a hidden developer flag in Phase 2, then a visible setting once Vagrant lands.

**Runner follow-ups (decided 2026-10-15):**
- **Check grammar:** the `isoloom-check/1` parser moves into `isoloom-manifest`, so there's one source; the runner drops its copy.
- **Compiler lints:** the compiler and WASM crates take the workspace lints in the compiler-on-new-syntax unit.
- **State files:** the runner is synchronous, so small state files are read and written inline. Documented in the crate docs; that is 3d's "chosen once per crate".
- **Isothrum's 3d tools** (cargo-deny, cargo-hack, msrv, semver-checks, fuzz, mutants) are wired in Isothrum's own 1c-equivalent pass.
- **`cargo xtask dev-link relock`:** updates a consumer's lockfile from the local Isoloom without GitHub, because the pinned revisions aren't pushed.

**Bugs found in today's code**, fixed by the manifest and runner work (`runner-extraction.md` §6):
- Launcher and Isoloom names have drifted: the port labels, `address` vs `ip`, the ready file.
- The Proxmox `slot` is never set.
- Windows inputs are dropped on Vagrant.
- `wait_ready` reports OK when SSH never answers.
- The lab lock fails open.
- A destroy uses the current region instead of the applied one.
- There are panics outside tests.
- The launcher pins `isoloom-core@50147b3`, which is reachable only on GitHub's object store, since the `dev` branch was deleted; its archive is in `~/code/github-archive/`. The path-dependency switch in Phase 2 removes the pin.

**Syntax: approved by the maintainer (2026-10-15),** exactly as designed in `plan/phase2/spec.md`:
- `os` plus one of `service:` or `app:`, with `overrides:` keyed by target;
- `app.steps` is portable, while `app.build` and `app.image` are container-only;
- ports are a map keyed by name, with one `protocol` enum;
- the flag is a value delivered through `files:` with `input:` or `secret:` as its source;
- `setup:` replaces `docker.init`;
- coded errors.

Its §12 open questions take the design's recommendations.

**More decisions (2026-10-15):**
- **Vagrant check timeouts** use the same rule as docker (waits + 15 s per check + 60 s); the controller's boot is a ready condition, not check time.
- **Launcher `config`** moves from `app/` to `platform/`, so domain code never imports the app shell.
- **Launcher `tauri` imports in domain code** reach zero when `runtime/` moves to the runner (Phase 2–3).
- **Launcher `unsafe`** follows the runner's rule: one audited `sys` module.
- **ESXi** server mode is supported only through SSH + ovftool (Vagrant `vmware_esxi`, Terraform `josenk/esxi`); no vSphere API on the free edition. The launcher uses Vagrant for ESXi.
- **ESXi finding:** the Terraform output reports the router's lab-side address when its uplink gets no DHCP. Fix it in Phase 3.
- **`tuned-web` keeps its TPM** (`hardware.tpm: required`), so it is VM-only: the example exists to show hardware tuning.
- **ESXi finding:** ESXi 8 refuses ed25519 keys in FIPS mode. The launcher's guidance must say so, or use RSA.

### Phase 3: The launcher's modes, wide (L, parallel per target)

Each step means: lowered generation passes the snapshots and `verify`, then a manifest, then a runner driver, then the launcher's lab test.
1. **Local:** `vagrant` (VirtualBox, Hyper-V, VMware, QEMU/libvirt) and `docker-vm`.
2. **Server:** `proxmox` (Terraform) and the Vagrant ESXi provider, with the launcher's `connection_env` contract. The Vagrant Proxmox provider is Tier 2.
3. **Cloud:** `cloud-vm`, `cloud-docker` and `cloud-services`, AWS first and then the other clouds.
   - **Proxmox bugs found by the real smoke test** on `pve-lab` (2026-10-15), to fix in this phase:
     - `destroy` leaves the SDN zone and bridge pending, because the "apply SDN" resource is destroyed before its zone; it needs the provider's finalizer ordering.
     - The module defaults to `node = "pve"`, while the launcher's Test connection uses the first node. A server with another node name passes the test and then fails the launch. Default to the discovered node, or make it required.
     - A router whose guest agent can't install (no DNS or internet) hangs `apply` for 15 minutes. Fail fast with a reason.
   - **ESXi findings from the real smoke test** (`esxi-lab`, 2026-10-15), to fix in this phase:
     - **The Vagrant controller on `vmware_esxi` never reports its address,** because the `generic/alpine319` box's VMware Tools don't report one, and `vagrant up` times out. Decided: use `bento/debian-12` for the controller on `vmware_esxi`, as on ARM.
     - **Terraform `esxi` can't finish natively on macOS.** The generated `terraform_data` step makes Terraform SSH to the router's DHCP address, which Local Network Privacy blocks. The container can reach it, but has no Linux `ovftool`. Decided: guest SSH steps move out of Terraform into runner manifest steps (`ssh.run` through the runner's relay or `nc`, as the launcher already does for Vagrant). Terraform keeps only the infrastructure. This also makes the step host-neutral.
     - **With promiscuous mode on,** the router gets its uplink by DHCP. The fallback that reported the lab-side address (10.80.0.254) must still turn into a clear failure, not a wrong output.
   - **Tier 1 for server mode** needs runs from Linux and Windows hosts too. `pve-lab` and `esxi-lab` sit on VMware Fusion's private NAT (172.16.1.0/24), which jarvis and karen can't reach. Track C task: move both lab VMs to **bridged networking on the LAN** (static 10.0.0.x addresses, reserved on the router), so the runner can drive them from jarvis and karen. Then record `proxmox` and `vagrant/vmware_esxi` from both hosts.
   - **`cargo xtask tiers record` doesn't deduplicate:** recording the same run twice lists it twice. Make it idempotent (same entry, host, date and server means an update, not a new row), with a test.
   - **Cloud design (approved 2026-10-16):**
     - Terraform keeps only the infrastructure. Each machine sets itself up from its own user data, and the runner delivers the project over `ssh.run` stdin. This replaces the earlier "`project.archive` with a `tf_var`": Terraform no longer reaches the guest, and user data is capped at 16 KB.
     - `ssh.run` gains `with.wait_s` (retry only while ssh can't connect).
     - **Secrets:** each cloud's store gets one secret per value, through a write-only attribute fed by an ephemeral variable, so the value is in neither a generated file nor the state (Terraform ≥ 1.11). The machine reads its secrets at boot through its identity. DigitalOcean and Linode fall back to guest delivery over SSH.
     - **The runner** strips ambient cloud credentials from Terraform steps (destroy replays the recorded profile and region). It adds `isothrum reap <cloud>` and a credential-guarded `real-run cloud`.
     - **The launcher** gets the one-time import of its old deployment records.
     - **Windows on cloud-vm** sets itself up from its user data (PowerShell), or keeps Terraform's WinRM provisioner, as a reported gap, if that runs long.
   - **Bugs from 6a's `tf-test`** (each recorded as an expected failure, so its fix flips the suite):
     1. **Azure doesn't enforce `reach` between networks** (Azure's default `AllowVnetInBound` lets every subnet reach every port). This is the most serious: segmented labs on Azure aren't segmented. Owner: the cloud worker.
     2. **`internet: false` is enforced only inside the guest,** where root can lift it. **Decided by the maintainer (2026-10-16): "seal after setup."** Machines get internet only during setup. Once ready, the runner applies a seal step at the network level: no public IPs, default routes or NAT for `internet: false` networks; on edge-firewall, `lan` only through `fw`; Proxmox and ESXi routers stop forwarding those networks. Root in a guest can't undo it. Owners: the cloud worker (clouds), then a server follow-up (Proxmox and ESXi routers).
        - **How the seal works (refined 2026-10-16):** the seal is egress-only, enforced at the network level, and applied by the runner (`terraform apply -var sealed=true` after ready). Security group or firewall egress is limited to the lab's ranges, and `internet: false` subnets lose their internet-gateway route. The literal "no public IP" would cut the runner's SSH and the published ports.
        - **Contract (2026-10-16):** run manifest v0 gains an optional `Lifecycle.seal: Vec<Step>`, left out when empty so other manifests stay byte-identical. It runs once up has succeeded and the ready conditions hold. The seal is a second apply with the module variable on, never an out-of-band change, so a later apply can't silently undo it. "Sealed" is recorded in the run's state, so every later up and start applies with it on. It fails closed: a failed seal means the run is not ready. The Proxmox and ESXi routers reuse the same field later.
        - **Residual channel, DNS:** the cloud resolver recurses to public names. It's closed with a DNS allow-list of the lab's own names where cheap (Route 53 Resolver DNS Firewall, GCP response policy, Azure DNS Private Resolver, OCI resolver rules), and documented as a known limitation otherwise.
     3. cloud-docker has no `published` output keyed `<machine>/<port-name>`, on any cloud. Owner: the cloud worker.
     4. Proxmox private-names: a VM's hosts file leaves out its own names. Owner: a server follow-up.
     5. Not plan-testable: ESXi writes only `ready` (no `running:`/`failed:` progress); DigitalOcean can't pin a private address (best effort; document it); Linode private names live only in the hosts file; Terraform 1.16.6 crashes in `terraform test -json` when a failing assertion reads OCI's `route_rules`.
   - **Brought back through the runner** (removed with Isoloom's runtime in Phase 2): the cloud-services `cloud.requires` pre-check (as the runner's tool probes), the launcher's Machine page per-target readiness (from the manifests' `tools`), and Settings' lab list (from the runner's run store).
   - **Retiring the launcher's old Docker path:** its resume still passes empty inputs, a patch that disappears when the runner path becomes the default.
4. **Secrets conversion and native checks** for these targets only:
   - Compose secrets;
   - Vagrant host environment;
   - each cloud's secret store, AWS first;
   - checks as native artefacts.
5. **Port the rest of the launcher's runtime into the runner,** driver by driver: locks, park and resume, recovery, reaper, budgets, remote servers, Isoloom's secrets delivery.

**Exit:** every launcher mode runs its test labs (rewritten in the new syntax) through Isoloom and the runner. Local is proven on macOS, plus Windows and Linux through the cross-platform check. Server and cloud are proven where the Track C infrastructure exists.

### Phase 4: The catalogue and the CM model (L)

**Design approved by the maintainer (2026-10-15),** with all 13 recommendations: `plan/phase4/design.md`, the examples, and the nginx example entry.
- **The model:** eight new step kinds (`repository`, `file`, `template`, `user`, `group`, `service`, `command` with a required guard, `firewall` allow-only) join the approved five. Ordered and guarded; downloads pinned by SHA-256 or digest.
- **Rendering:** one POSIX shell library for Dockerfile, shell and cloud-init; Ansible with official modules only, from the Linux controller; a PowerShell library for Windows. The VM set-up splits after the secrets step (the IaC tool before, the CM tool after). Defaults keep today's output; `--configure` plus a per-target override chooses the CM tool.
- **Services in containers:** one daemon runs as the container's foreground process; several run under the engine's init (tini, Compose `init: true`) with a generated runner; refusal only when a daemon has no known foreground form. No s6-overlay in 1.0.
- **The catalogue:** a hand-written `entry.yml`; a `lock.yml` (digests, exact package versions) written only by `cargo xtask catalogue refresh`. The official image is used only when its OS matches the machine's; otherwise the same recipe builds from the OS image. GPL Galaxy content is referenced, never embedded, and no vendored Galaxy content in 1.0. The folder is `files/`, not `roles/`. Version prefixes (`postgres@16`) resolve to the newest 16.x. Old versions come from the OS that shipped them (MySQL 5.7 on ubuntu@18.04); vulhub-style images stay `app.image`.
- **Entries, wave 1:** openssh, nginx, apache, php-fpm, mysql/mariadb, postgres, **mongodb**, dvwa, juice-shop. Then wordpress, redis **and valkey** (Redis 7.4+ isn't open source), rabbitmq, iis, ad-ds.
- **Not in 1.0:** a `start` phase on `app`; several catalogue services on one container stay refused.
- **Parity:** proven on Docker plus Vagrant/VirtualBox through the Isothrum CLI first, then libvirt on jarvis.

1. **The configuration resource model** (repository, package, file, template, user, group, service, command, firewall rule). `app:` build recipes are written in it, so the container build and the VM install come from one source.
2. **CM backends:** Ansible, cloud-init, shell/PowerShell and Dockerfile, with the pairing rules.
3. **The catalogue engine,** salvaging from `v1/catalogue-engine`. GPL Ansible collections are referenced, never embedded.
4. **Entries, CTF-relevant first:** openssh, nginx, apache, php-fpm, mysql/mariadb, postgres, dvwa, juice-shop. Then the rest: wordpress, mongodb, redis, rabbitmq, iis, ad-ds.

**Exit:** each entry passes its parity contract for real, in both forms, on Docker and one VM provider.

### Phase 5: The other targets (M, parallel)

Kubernetes, Helm, Kustomize, hybrid, hosted, external and Terraform ESXi, onto the new spec through lowering. They are Tier 2: snapshots plus `verify`.

### Phase 6: Hardening (M)

1. **Combinations:** property-based random specs; every target's output must pass `verify`.
2. **Cloud logic without clouds:** `terraform test` with mock providers for routing, gateways, firewalls and per-machine ports.
3. **Tiers from evidence,** and `isoloom matrix` generated from the backends' capabilities.
4. **Security review** of secrets and of everything that handles credentials, in Isoloom, the runner and the launcher.

### Phase 7: Release 1.0, together (M)

1. **Minimal docs** in each repository: getting started, the generated spec reference, targets, secrets. Then the two websites (3c): isoloom.com from the new docs, and isothrum.com on the 1.0 spec and the npm WASM.
2. **The docs alignment gate.** No release until every doc matches the code, in every repository and both websites:
   - **Generated** from the code, with stale-file tests: the spec reference, the targets and capability matrix with tiers, the catalogue, the CLI reference, the manifest schema, and the error codes.
   - **Executed:** every code block in the docs (specs, commands) runs in a test. Specs pass `isoloom validate` and `verify`; commands run against the examples. A doc example that stops working fails the build.
   - **Audited:** every hand-written claim (READMEs, guides, comparisons, both websites' pages, each repository's `AGENTS.md`/`CLAUDE.md` and contributor files) is checked against `plan/docs-facts.md`, the tests and the tier evidence. No claim without proof: "supported" means Tier 1 or Tier 2 with the evidence named.
   - **One source:** isoloom.com's docs come from the repository through `sync:isoloom`; nothing is edited only on the site.
   - **Exit:** an audit checklist covering every doc file, each item closed.
3. **Squash each repository's history,** with the maintainer's confirmation at that moment, and re-enable CI.
4. **Publish to crates.io:** the Isoloom crates, then the Isothrum runner.
5. **Rewrite every CTF lab file in the new syntax** (by hand, with each lab's real run as proof).
6. **Switch the launcher from path dependencies to the published versions,** and release the launcher.

**Exit:** the launcher runs its full catalogue in local, server and cloud mode on published Isoloom 1.0 and the runner.

### Phase 8: After 1.0 (1.x)

- **Isothrum:** `isothrum-desktop` (moved out of `apps/desktop`, dynamic specs) and `isothrum-ui`, shared with the website and the launcher.
- **Isoloom ecosystem:** import as the front door (today only Compose, Vagrant and Terraform import; **Helm and Kubernetes importers don't exist yet** and are built in 1.x), the VS Code extension, packages, registry modules and the website rewrite.
- **More CM and IaC tools:** Puppet, Chef, Salt, DSC, Pulumi, Podman.
- **Windows on container targets** (dockur), from `v1/windows-in-docker`.
- **Runtime features in Isothrum:** drift detection, upgrades, snapshots and the hub.

## 5. Open decisions

1. **CTF lab content and courses licence.**
2. **Does the launcher use `isothrum-ui` (1.x)?** Recommended: yes.

Decided on 2026-10-15:
- The launcher is integrated from Phase 2.
- There is a manual-only cross-platform workflow.
- Track C infrastructure:
  - **Proxmox VE 9.2 (`pve-lab`)** runs in VMware Fusion at `172.16.1.10`. It is proven: a generated lab applied, served and passed its check, then was destroyed.
    - Credentials are in `~/.isoloom-lab/proxmox.env`.
    - Terraform runs in Docker (`~/.isoloom-lab/bin/pve-tf`), because macOS Local Network privacy blocks provider binaries started from the agent session.
    - Lab VMs use the internal NAT bridge `vmbr1`.
    - It's stopped when idle.
  - **ESXi 8.0U3e** runs as a VM in VMware Fusion (installing).
  - **Windows host:** `florianamette@10.0.0.40` (`karen`) over SSH key, with Windows 11, 16 GB RAM, Hyper-V, Rust MSVC, Node 24 and Docker. It gives the Windows host proof and Hyper-V.
  - **Linux host:** `florianamette@10.0.0.10` (`jarvis`) over SSH key, with Debian 13, 46 GB RAM, KVM, libvirt, Docker and Vagrant. It gives the Linux host proof, and real Vagrant runs on libvirt and QEMU.
  - Cloud accounts come later.

## 6. Salvage branches (local, not pushed)

| Branch | What may be reused |
|---|---|
| `v1/catalogue-engine` | Catalogue format, loader, nginx entry start, Alpine 3.24 image table entry |
| `v1/windows-in-docker` | dockur container form, KVM refusal, OEM set-up |
| `v1/security-scanning` | generate-everything pipeline, scanner baselines, secure-default generator fixes |
| `v1/cli-output-compat` | spec table as JSON (the matrix may reuse it) |
| `v1/docs-truth-fixes` | Google Cloud MTU capability cell |

They are deleted at the squash.

## 7. Backlog carried over from GitHub (closed 2026-10-15)

Every issue and PR was closed, and every non-default branch deleted, for the fresh start:
- **Full archive:** `~/code/github-archive/`, one verified git bundle per repository (every branch, tag and PR ref), plus the issue and PR texts as JSON.
- **Kept on purpose:**
  - the launcher's releases and tags, because the installed launchers' updater reads `releases/latest/download/latest.json`;
  - `isoloom/goad`, a fork of GOAD whose branches and tags are upstream's.

| Item | Carry into |
|---|---|
| launcher #227 | Starting a lab always needs sign-in and the backend: allow offline relaunch and/or a practice mode |
| launcher #225 | Windows dev setup: CRLF checkout fails pnpm check and dirties generated types, sh-based tests fail, local build needs the signing key |
| launcher #217 | Windows: in-app terminal can't SSH to remote attack boxes (cmd /c quoting); shell scripts break on non-ASCII user names |
| launcher #178 | Own the Isoloom integration in the launcher |
| launcher #150 | Clean code: split large files and long functions |
| launcher #79 | Isoloom and lab images: QEMU x86 emulation, networking and Windows boxes |
| launcher #78 | Run x86_64 VM labs under QEMU emulation on Apple Silicon |
| launcher #2 | Adopt Isoloom's lifecycle, checks and defaults features |
| isothrum #30 | Accounts with Better Auth: Isothrum website (proprietary), after 1.0 |
| isothrum PR #65 | Isoloom v1 crates in Isothrum: superseded by the path-dependency setup |
| isothrum PR #66, isothrum-website PR #18 | Editor and engine: IPv6 LANs, a machine's environment, files and command. Re-apply on the editor in 1.x, from the archive |

Placement of the launcher items:
- #2, #178, #78 and #79 fold into Phases 2–3: the Isoloom integration, the runner, and QEMU and Windows boxes.
- #150 folds into the engineering standard: the ratchet.
- #217, #225 and #227 are launcher work for Phase 3 and the release.
