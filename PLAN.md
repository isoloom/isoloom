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
| `isoloom/isoloom` | MIT OR Apache-2.0 | Converter crates and CLI | everything below |
| `isoloom/isothrum` | MIT OR Apache-2.0 | Libraries only: the runner, the lab-model compiler (drawing to spec, today's `isothrum-core`) and its WASM build | desktop app, website (WASM), launcher (runner) |
| `isoloom/isothrum-desktop` (new) | MIT OR Apache-2.0 | The Tauri app: brings Isoloom, the runner, the compiler and the UI together, with dynamic specs | users |
| `isoloom/isothrum-ui` (new, optional) | MIT OR Apache-2.0 | Shared front-end components: editor canvas, lab view, machines, consoles | desktop app, website, possibly the launcher |
| `isoloom/isothrum-website` | Proprietary, private | Site, accounts, project store, MCP; uses the UI package and the WASM engine | — |
| `Cyber-Courses/Cyber-CTF-Launcher` | MIT OR Apache-2.0 | CTF UI with fixed specs; uses Isoloom and the runner | players |

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
| Licences (everything open source, least constraints) | **Isoloom, Isothrum (runner, desktop app, editor) and the Cyber CTF launcher:** dual-licensed `MIT OR Apache-2.0`, the Rust ecosystem's standard. Each user picks either. MIT has the fewest conditions; Apache-2.0 adds a patent grant. No CLA, only a DCO sign-off. Anyone may build anything on them, closed products included. Official Ansible collections (GPL-3.0) are referenced, never embedded in a binary. The Isothrum website (accounts, project store, MCP) stays private and proprietary. The CTF lab content and courses are a separate decision. |
| Compatibility | Fresh start: no aliases or deprecations carried into 1.0 (`services:`, old OS names, `qemu_refusal`...). Old specs move with a one-time `isoloom migrate`. |
| Workflow | Local development on `main`. No PRs, no CI (all workflows disabled), no releases. The checks run locally. At the first satisfying version: squash the history, re-enable CI, tag 1.0.0. |
| Docs | Written at the end, inside the repo. The website is rewritten from them. |
| Launcher | Reworked onto the new crates after the clean 1.0. Until then, no launcher changes. |
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

- **Removed:** `isoloom-runtime`, `isoloom-coverage`, and the runtime parts of the CLI.
- **New:** `isoloom-catalogue` and `isoloom-verify`.
- **Changed:** `isoloom-external` becomes the Ansible output (existing hosts are an inventory plus playbooks).

## 4. Work, in order

Each phase ends with its exit check, run locally. One phase at a time. Within a phase, a few units may run in parallel in local worktrees, merged into `main` locally.

### Phase 1: Subtract

Make the codebase smaller before building on it.

1. **Move the runtime out.**
   - The CLI lifecycle commands, the registry, the events, host readiness, secret delivery, check execution and their tests go to the Isothrum repository as the runner's seed, rather than being deleted.
   - What generation needs from them, for example the check definitions, moves into the plan.
2. **Remove the coverage crate.**
   - Its tables, refresh scripts and `docs/COVERAGE.md`, the coverage-floor tooling, and the drift workflow.
   - Keep the capability declarations in the backends. The matrix is rebuilt in Phase 3.
3. **Remove the committed example outputs** (745 files, 5.7 MB). Keep `insta` snapshots for a representative subset of examples × targets.
4. **Remove compatibility debt:** deprecated APIs, renamed-key aliases, old OS names and dead fixtures.
5. **Fix the small debts:**
   - the macOS-only test that depends on the system Ruby;
   - `#[allow(dead_code)]` in the test fixtures;
   - leftovers found along the way.

**Exit:** the workspace builds, the remaining tests pass, and every crate left has a clear converter role.

### Phase 2: The one declaration

1. **New spec model.**
   - A machine has an `os` (family@version), plus either `service:` (one or a list, from the catalogue) or `app:` (a custom image or build recipe), plus the shared fields: networks and addresses, ports, files, secrets, volumes and disks, resources, command, healthcheck, hardware and DNS.
   - A per-target `overrides:` block holds rare platform specifics, such as a Vagrant box or an instance type.
2. **`isoloom-catalogue`.**
   - The data format: `entry.yml`, `contract.yml` and roles.
   - Official Galaxy collections are GPL-3.0, so the permissively licensed Isoloom only **references** them (a pinned `requirements.yml` with checksums) and never embeds them in its binary. The runner fetches them at run time. Isoloom's own roles are MIT OR Apache-2.0.
   - The loader, validator and resolution.
   - Salvage from the stopped `v1/catalogue-engine` branch: format, loader, and the start of the nginx entry.
3. **The configuration resource model** in the plan. Catalogue entries are written in it, not as raw roles. The plan's current set-up steps (raw shell and PowerShell) move onto it.
4. **Plan resolution.**
   - `os` + `service`/`app` becomes the container form and the VM form of each machine.
   - Checks (from the parity contract and the spec) become target-neutral check definitions.
5. **`isoloom migrate`:** rewrites a 0.x spec into the new form, with tests on every old example and on the Cyber CTF labs' specs.

**Exit:** every example is rewritten in the new syntax, every old spec migrates, and validation errors name the fix.

### Phase 3: Backends on the new model

0. **Configuration backends** next to the infrastructure ones: Ansible, cloud-init, shell/PowerShell and Dockerfile (plus Puppet, Chef, Salt and DSC per the scope decision), with the pairing rules.
1. **Renders forms, not blocks.** Each backend renders the resolved forms instead of reading `docker:`/`vm:`.
2. **The secrets conversion layer.**
   - **In the spec:** secrets are declared by name, source and destination, never by value.
   - **In each output:** the tool's native mechanism.
     - Compose: `secrets:`.
     - Kubernetes: Secret references, or External Secrets.
     - Vagrant: host environment plus a provisioner.
     - Ansible: Ansible Vault references.
     - Terraform on each cloud: the cloud's own store (AWS Secrets Manager or SSM, Azure Key Vault, Google Secret Manager, OCI Vault), read by the machine at boot through its identity.
     - Proxmox and ESXi: a provisioner from the host environment.
     - DigitalOcean and Linode: a documented fallback, since they have no store.
     - Later, the Puppet, Chef and Salt equivalents.
   - **The run manifest** lists the secret slots so a runner can fill them.
   - **A leak test** proves no value reaches any generated file, on every target.

3. **Checks become native artefacts:** Compose healthchecks plus a test service, Helm test pods or a Kubernetes Job, Terraform `check` blocks or a script, an Ansible checks playbook for VMs, and a portable `checks/` bundle.
4. **The run manifest.** Each target also writes a versioned manifest, the contract with Isothrum: commands and order, readiness, outputs, check runners and secret slots. It is pure conversion: deterministic and snapshot-tested. An optional thin `isoloom up/down` may read it and call the native tools, with no state of its own.
5. **The capability matrix and tiers.**
   - The spec-field × target matrix is generated from the backends' declared capabilities and refusals.
   - Tiers come from a single list in `isoloom-targets`.
   - `isoloom matrix` prints the matrix (text and JSON).
6. **Windows on container targets** (dockur, opt-in, KVM). Salvage from `v1/windows-in-docker`.

**Exit:** every example generates on every target, or is refused with a reason, and the snapshots are reviewed.

### Phase 4: Catalogue content

- **Entries:** nginx, apache, php-fpm, wordpress, postgres, mysql, mariadb, mongodb, redis, rabbitmq, openssh, iis, ad-ds, dvwa and juice-shop.
- **Per entry:** versions, OS families, container form (official image or build), VM form (role) and a parity contract.

**Exit:** each entry generates on each supported OS and target. Its contract passes for real in both forms locally, on Docker and on one VM provider.

### Phase 5: Proof

1. **`isoloom-verify` / `isoloom verify <target>`.** It runs the official validator on every generated file:
   - `docker compose config`;
   - kubeconform, `helm lint` and `kustomize build`;
   - `terraform validate`, plus tflint with the provider rulesets;
   - `vagrant validate`;
   - `cloud-init schema`;
   - `docker buildx build --check` and hadolint;
   - `ansible-inventory` and ansible-lint;
   - shellcheck and PSScriptAnalyzer;
   - CM validators: `ansible-lint`, `puppet parser validate`, `cookstyle`, `salt-call --local state.show_sls`, the DSC compiler;
   - plus the security scanners: Checkov, Trivy and kube-score.

   Salvage the generate-everything pipeline from `v1/security-scanning`.
2. **Combinations.** Property-based generation of random valid specs; every target's output must pass `verify`.
3. **Cloud semantics without clouds.** `terraform test` with mock providers asserts the routing, gateway, firewall and per-machine port logic.
4. **Tier 1 real runs, using the native tools on the generated files.**
   - Locally on macOS now: Docker, kind, VirtualBox, QEMU.
   - Windows and Linux hosts once CI is re-enabled after the squash. CI runs a host matrix of macOS, Windows and Linux, plus KVM for libvirt and Hyper-V on Windows.
5. **Host neutrality checks:** generated output is scanned for host-specific assumptions (shell-only `local-exec`, path separators, line endings). Tests run the CLI's generation on all three hosts in CI.

**Exit:** `verify` is green on all examples and on 10,000 random specs. The Tier 1 real runs pass on macOS locally, and the tier list is set from evidence. The Windows and Linux host proof completes once CI is back.

### Phase 6: The overall review pass

Read the whole codebase end to end for coherence:
- one idiom per concern;
- duplication removed;
- module boundaries and the public API checked;
- error messages consistent.

Then the security review of secrets handling and of every place that writes credentials.

**Exit:** a review checklist with every item closed.

### Phase 7: Converter-aligned ecosystem

- **`isoloom import` as the front door:** Compose, Vagrant, Terraform, Helm and Kubernetes into a spec. For CM, 1.0 imports only what maps cleanly: recognised catalogue roles and modules, plain packages, files and services. Arbitrary playbooks don't map back. Recognise catalogue images and propose `service:`. Lossless round trips where the formats allow.
- **VS Code extension** updated to the new syntax, `matrix` and `verify`.
- **Packages:** Homebrew, winget, Scoop, apt and rpm, and a CLI container image.
- **Terraform/OpenTofu registry modules** published from the generated output.
- **Terraform provider:** dropped for 1.0. Keep the `isoloom/terraform-provider-isoloom` repository archived and private. Revisit as an Isothrum provider.
- **Out of Isoloom (runtime work):** drift detection, upgrades, snapshots, persistence checks, the deployment hub, the cloud reaper, PR preview environments and the test library. They belong to the launcher or a later product, and are not part of 1.0.

### Phase 8: Docs, squash, release

1. Write the docs in the repo: getting started, the spec reference (generated), targets, the catalogue (generated), secrets, migration and comparisons.
2. Rewrite the website from them.
3. **Squash the history into one commit.** This is a force-push, and needs the maintainer's confirmation at that moment. Then re-enable CI.
4. Sign the release, with an SBOM, provenance and a pinned toolchain, and tag 1.0.0.
5. Publish the Isoloom crates to crates.io at 1.0.0, so the runner and the apps depend on versions.

### Phase 9: The Isothrum runner

1. **Create the runner engine** in `isoloom/isothrum` (a crate plus a CLI), under MIT OR Apache-2.0. It executes Isoloom run manifests, on macOS, Windows and Linux hosts.
2. **Port into it:**
   - the launcher's runtime: lifecycle, locks, deploy worker, drivers, recovery, reaper, budgets, remote servers;
   - Isoloom's moved run part: secrets delivery, host readiness, registry, WinRM retries.
3. **Tests:**
   - a fake-tool harness for unit tests;
   - one real-lab job per target family (Docker, kind, and Vagrant on a KVM runner), the Tier 1 proof.
4. **Isothrum's desktop app** becomes a UI over the editor, Isoloom and the runner, with the same logic as the launcher but dynamic specs.

### Phase 9b: Isothrum desktop and shared UI

1. **Create `isoloom/isothrum-desktop`.** Move `apps/desktop` out of `isoloom/isothrum`, so that repository holds only libraries. Rebuild the app as a UI over Isoloom, the runner and the compiler, with dynamic specs.
2. **Optionally create `isoloom/isothrum-ui`:** the editor canvas, lab view, machines and consoles, published to npm and used by the desktop app and the private website. Decide whether the launcher uses it too.
3. **Publish** the Isothrum crates to crates.io and the WASM build to npm.

### Phase 10: The launcher as a UI

1. **Replace the launcher's runtime module** with the Isothrum runner, and its spec handling with the Isoloom crates. Depend on published versions, not git commits.
2. **Keep only CTF-specific code:** lab catalogue, accounts, attack box UX, player flows.
3. **Move the labs** with `isoloom migrate`.
4. **Order matters:** the rev the launcher pins today stops resolving after the Isoloom squash, so this phase follows right after.

## 5. Open decisions

0. **Real infrastructure for the launcher's server and cloud modes:** a Proxmox and/or ESXi host, and budget-capped cloud accounts (which clouds first?), so those modes are proven before 1.0 rather than only validated.

1. **CTF lab content and courses licence:** proprietary, or a content licence such as Creative Commons. Separate from the code.
2. **Pulumi and Podman:** revisit after 1.0.
3. **Does the launcher use `isothrum-ui`?** Recommended: yes, since the lab view, machines and consoles are the same in both apps.

## 6. Salvage branches (local, not pushed)

| Branch | What may be reused |
|---|---|
| `v1/catalogue-engine` | Catalogue format, loader, nginx entry start, Alpine 3.24 image table entry |
| `v1/windows-in-docker` | dockur container form, KVM refusal, OEM set-up |
| `v1/security-scanning` | generate-everything pipeline, scanner baselines, secure-default generator fixes |
| `v1/cli-output-compat` | spec table as JSON (the matrix may reuse it) |
| `v1/docs-truth-fixes` | Google Cloud MTU capability cell |

They are deleted at the squash.
