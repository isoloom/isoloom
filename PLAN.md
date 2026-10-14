# Isoloom: the plan to a clean 1.0

Working document for the local rebuild. It is squashed away with the history at the first satisfying version.

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
| `isoloom/isothrum-website` | Proprietary, private | Site, accounts, project store, MCP; uses the UI package and the WASM engine | — |
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
| Compatibility | Fresh start: no aliases or deprecations carried into 1.0 (`services:`, old OS names, `qemu_refusal`...). The new format is **`version: 2`**, because 0.x specs already say `version: 1`, so `isoloom migrate` can tell them apart without guessing. |
| Workflow | Local development on `main` in each repository. No PRs, no releases. The three repositories sit side by side with **path dependencies** (no git revs) until the 1.0 publish. One script runs every repository's checks. At the first satisfying version: squash, re-enable CI, publish. |
| Docs | Written at the end, inside the repo. The website is rewritten from them. |
| Launcher | **Proposed change, needs the maintainer's OK:** integrate it from the walking skeleton (Phase 2) on, instead of after 1.0. It is the reason 1.0 exists, and only it can validate the run manifest. Launcher, runner and Isoloom ship 1.0 together. |
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
                 migrate)         the catalogue,    vagrant,       Terraform, cloud-init,
                                  the environment)  terraform,     Ansible)
                                                    ansible)
                isoloom-catalogue                   isoloom-render (builders: HCL, Ruby, YAML,
                (entries, roles,                     shell, PowerShell, Dockerfile)
                 contracts)
                isoloom-import (native files ──► spec)
                isoloom-verify (official validators on the generated files)
                isoloom-targets (registry, capabilities, the matrix, tiers)
                isoloom (CLI: generate, validate, import, verify, matrix, migrate)
```

- **Lowering, not rewriting:** the `version: 2` spec is a front end. It resolves `os` + `service`/`app` and lowers each machine into today's plan IR (`Environment`), which already carries the container and VM forms. The backends keep working, and their 800+ tests keep guarding them. Internals are reshaped only where a feature needs it.
- **Removed:** `isoloom-runtime`, `isoloom-coverage`, and the runtime parts of the CLI.
- **New:** `isoloom-catalogue` and `isoloom-verify`.
- **Changed:** `isoloom-external` becomes the Ansible output (existing hosts are an inventory plus playbooks).

## 4. Work, in order

**Principles:**
- **The safety net comes before the surgery.** Every change is measured against characterization snapshots, the real-lab corpus and the official validators.
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
- **The CTF lab corpus:** every lab spec from the Cyber-Courses repositories, copied locally as test input.

### Phase 0: The safety net (M)

1. **Characterization snapshots.** Generate every example on every target from today's `main`, and snapshot the output with `insta`. Any later change that alters output must be deliberate and reviewed.
2. **The real-lab corpus** (Track C) runs through `validate` and `generate` in a test, so the CTF labs are the regression suite.
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
5. **Cross-platform check (proposed, needs the maintainer's OK).** A manual-only GitHub workflow that runs `check-all` on macOS, Windows and Linux on demand. No PRs and no automatic runs. Without it, Windows and Linux bugs surface only after the squash.

**Exit:** snapshots, corpus and `verify` are all green or baselined, and `check-all` passes.

### Phase 1: Subtract (S–M)

Now safe, because the net is in place.
1. **Delete the runtime from Isoloom.** It stays on a salvage branch. The runner takes from it what it needs in Phase 2; nothing is copied into Isothrum blindly.
2. **Delete `isoloom-coverage`.** Its schema checks are covered by `verify`, and its capability declarations stay in the backends.
3. **Delete the committed example outputs.** The snapshots replace them.
4. **Delete the compatibility debt** and fix the small debts: the Ruby-dependent test, the dead-code fixtures.

**Exit:** green net; a smaller workspace.

### Phase 2: The walking skeleton (M–L)

One real lab, end to end: `version: 2` spec, Isoloom, run manifest, runner, launcher.
1. **The `version: 2` spec front end:**
   - a machine has `os`, plus `app:` (a custom image or build recipe; CTF labs need this first) or `service:`;
   - the shared fields;
   - `overrides:`.

   It lowers into today's plan IR.
2. **`isoloom migrate` for the corpus.** It turns `docker:`/`vm:` into `app:`. A machine with both forms becomes one `app:` whose recipe both forms derive from. Any lab that can't migrate cleanly is listed, with the reason.
3. **Run manifest v0 for the `docker` target:** commands, readiness, outputs and check runners. Its JSON schema and fixtures live in Isoloom, and both repositories test against them.
4. **The runner skeleton** in `isoloom/isothrum`: the manifest executor and the Docker driver, extracted from the launcher's runtime (its most proven code).
5. **The launcher** runs one real lab (invoice-portal-api) through Isoloom v2 and the runner, behind a switch. Its real-lab test passes.

**Exit:** that lab passes end to end through the new path, and the corpus migrates. The contract is validated by its real consumer.

### Phase 3: The launcher's modes, wide (L, parallel per target)

Each step means: lowered generation passes the snapshots and `verify`, then a manifest, then a runner driver, then the launcher's lab test.
1. **Local:** `vagrant` (VirtualBox, Hyper-V, VMware, QEMU/libvirt) and `docker-vm`.
2. **Server:** `proxmox` (Terraform), plus the Vagrant Proxmox and ESXi providers with the launcher's `connection_env` contract.
3. **Cloud:** `cloud-vm`, `cloud-docker` and `cloud-services`, AWS first and then the other clouds.
4. **Secrets conversion and native checks** for these targets only:
   - Compose secrets;
   - Vagrant host environment;
   - each cloud's secret store, AWS first;
   - checks as native artefacts.
5. **Port the rest of the launcher's runtime into the runner,** driver by driver: locks, park and resume, recovery, reaper, budgets, remote servers, Isoloom's secrets delivery.

**Exit:** every launcher mode runs its labs through Isoloom v2 and the runner. Local is proven on macOS, plus Windows and Linux through the cross-platform check. Server and cloud are proven where the Track C infrastructure exists.

### Phase 4: The catalogue and the CM model (L)

1. **The configuration resource model** (repository, package, file, template, user, group, service, command, firewall rule). `app:` build recipes are written in it, so the container build and the VM install come from one source.
2. **CM backends:** Ansible, cloud-init, shell/PowerShell and Dockerfile, with the pairing rules.
3. **The catalogue engine,** salvaging from `v1/catalogue-engine`. GPL Ansible collections are referenced, never embedded.
4. **Entries, CTF-relevant first:** openssh, nginx, apache, php-fpm, mysql/mariadb, postgres, dvwa, juice-shop. Then the rest: wordpress, mongodb, redis, rabbitmq, iis, ad-ds.

**Exit:** each entry passes its parity contract for real, in both forms, on Docker and one VM provider. Corpus labs that can use catalogue services are migrated to them.

### Phase 5: The other targets (M, parallel)

Kubernetes, Helm, Kustomize, hybrid, hosted, external and Terraform ESXi, onto `version: 2` through lowering. They are Tier 2: snapshots plus `verify`.

### Phase 6: Hardening (M)

1. **Combinations:** property-based random `version: 2` specs; every target's output must pass `verify`.
2. **Cloud logic without clouds:** `terraform test` with mock providers for routing, gateways, firewalls and per-machine ports.
3. **Tiers from evidence,** and `isoloom matrix` generated from the backends' capabilities.
4. **Security review** of secrets and of everything that handles credentials, in Isoloom, the runner and the launcher.

### Phase 7: Release 1.0, together (M)

1. **Minimal docs** in each repository: getting started, the generated spec reference, targets, secrets, migration.
2. **Squash each repository's history,** with the maintainer's confirmation at that moment, and re-enable CI.
3. **Publish to crates.io:** the Isoloom crates, then the Isothrum runner.
4. **Switch the launcher from path dependencies to the published versions,** and release the launcher.

**Exit:** the launcher runs its full catalogue in local, server and cloud mode on published Isoloom 1.0 and the runner.

### Phase 8: After 1.0 (1.x)

- **Isothrum:** `isothrum-desktop` (moved out of `apps/desktop`, dynamic specs) and `isothrum-ui`, shared with the website and the launcher.
- **Isoloom ecosystem:** import as the front door, the VS Code extension, packages, registry modules and the website rewrite.
- **More CM and IaC tools:** Puppet, Chef, Salt, DSC, Pulumi, Podman.
- **Windows on container targets** (dockur), from `v1/windows-in-docker`.
- **Runtime features in Isothrum:** drift detection, upgrades, snapshots and the hub.

## 5. Open decisions

1. **Integrate the launcher from Phase 2** instead of after 1.0. Recommended: it's the only consumer that can validate the contract, and it avoids a late big-bang switch.
2. **The manual cross-platform workflow** (Phase 0.5). Recommended: no PRs, no automatic runs, only `check-all` on demand on macOS, Windows and Linux.
3. **Track C:** which real infrastructure you can provide, and when.
4. **CTF lab content and courses licence.**
5. **Does the launcher use `isothrum-ui` (1.x)?** Recommended: yes.

## 6. Salvage branches (local, not pushed)

| Branch | What may be reused |
|---|---|
| `v1/catalogue-engine` | Catalogue format, loader, nginx entry start, Alpine 3.24 image table entry |
| `v1/windows-in-docker` | dockur container form, KVM refusal, OEM set-up |
| `v1/security-scanning` | generate-everything pipeline, scanner baselines, secure-default generator fixes |
| `v1/cli-output-compat` | spec table as JSON (the matrix may reuse it) |
| `v1/docs-truth-fixes` | Google Cloud MTU capability cell |

They are deleted at the squash.
