# Isoloom: the plan to a clean 1.0

Working document for the local rebuild. It is squashed away with the history at the first satisfying version.

## 0. Three products

| Product | Role | Contains |
|---|---|---|
| **Isoloom** (this repo) | Converter: one spec to every IaC tool, and back | Spec, catalogue, plan, backends, import, `verify`, and the **run manifest** each target outputs. Apache-2.0. No runtime. |
| **Isothrum** (`isoloom/isothrum`) | Runner: executes what Isoloom generates | One runner engine (crate and CLI) with: lifecycle (up, down, park, resume, re-provision); state and locks; a driver per target; secrets delivery; checks; readiness and recovery; reaper and budgets; snapshots, capture and consoles. Its lab editor (a drawing becomes a spec) stays an authoring tool. |
| **Cyber CTF launcher** | UI wrapper for CTF labs | Lab catalogue, accounts, attack box and player UX. It uses Isoloom to read and convert labs, and Isothrum to run them. |

**Contracts:**
- **Isoloom to Isothrum:** the generated files plus a versioned run manifest, holding:
  - native commands and their order;
  - readiness criteria;
  - outputs: addresses, host ports, URLs;
  - how to run checks;
  - secret slots: names and destinations, never values.
- **Isothrum to UIs:** a Rust API plus a JSON event stream, used by the launcher and by Isothrum's desktop app.

**Runner code comes from three runtimes, merged into one in Isothrum:**
- **The launcher's** (about 16k lines, the most mature) is the core.
- **Isoloom's run part** moves in rather than being deleted: secrets delivery including Windows, host readiness, the registry and WinRM retries.
- **Isothrum desktop's** switches onto the result.

## 1. What Isoloom 1.0 is

**One spec, converted to every other infrastructure-as-code tool.**

- **Isoloom is a converter.** It reads one `isoloom.yml` and writes the native files of each tool: Compose, Kubernetes manifests, Helm, Kustomize, Vagrantfiles, Terraform/OpenTofu modules for Proxmox, ESXi and the clouds, cloud-init, and Ansible inventories and playbooks.
- **It also does the reverse:** it imports those formats into a spec (`isoloom import`).
- **It doesn't run anything.** Users, CI and the Cyber CTF launcher run the native tools on the generated files.
- **One declaration per machine.** A machine says what it is: an OS and the services it runs, from a catalogue. It never says how a platform runs it, so there are no `docker:` or `vm:` blocks. Each target decides how to deliver it: an official container image or a built image on container targets; a fresh VM plus an Ansible role on VM targets.
- **Honest proof.** Every generated file passes its format's official validator. Each target sits in a tier that says how far it has been proven.

## 2. Decisions taken (2026-10-15)

| Topic | Decision |
|---|---|
| Scope | Converter only. The runtime goes: `run`/`test`/`down`/`status`/`connect`/`exec`/`capture`/`reset`/`provision`, the registry, JSON lifecycle events, run-time secret delivery and Isoloom-executed checks. |
| Coverage | `isoloom-coverage` (13k lines, 30 format tables, refresh scripts, drift job) is replaced. What stays: a spec-field × target capability matrix generated from each backend's declared capabilities, and conformance against official validators. |
| Syntax | One form: `os` plus `service`(s) from the catalogue, or a generic custom `app` (own image, or a build recipe). A small per-target `overrides:` handles rare platform specifics. No `docker:`/`vm:` blocks. |
| Catalogue | Data-driven entries: a container form (official image, or a build from the OS base image), a VM form (official Galaxy content only, else our own role), and a parity contract. |
| Breadth | Every current target stays ("it's the deal"), placed in tiers. |
| Tiers | **Tier 1:** proven by real runs (locally now, in CI later). **Tier 2:** generated and validated by the official validators. **Tier 3:** experimental. The tier is shown in the matrix and in the CLI. |
| Compatibility | Fresh start: no aliases or deprecations carried into 1.0 (`services:`, old OS names, `qemu_refusal`...). Old specs move with a one-time `isoloom migrate`. |
| Workflow | Local development on `main`. No PRs, no CI (all workflows disabled), no releases. The checks run locally. At the first satisfying version: squash the history, re-enable CI, tag 1.0.0. |
| Docs | Written at the end, inside the repo. The website is rewritten from them. |
| Launcher | Reworked onto the new crates after the clean 1.0. Until then, no launcher changes. |
| GitHub | Done: PRs and issues closed, releases and tags deleted, workflows disabled. |

## 3. Target architecture

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
   - The data format: `entry.yml`, `contract.yml`, roles and the vendored official Galaxy content.
   - The loader, validator and resolution.
   - Salvage from the stopped `v1/catalogue-engine` branch: format, loader, and the start of the nginx entry.
3. **Plan resolution.**
   - `os` + `service`/`app` becomes the container form and the VM form of each machine.
   - Checks (from the parity contract and the spec) become target-neutral check definitions.
4. **`isoloom migrate`:** rewrites a 0.x spec into the new form, with tests on every old example and on the Cyber CTF labs' specs.

**Exit:** every example is rewritten in the new syntax, every old spec migrates, and validation errors name the fix.

### Phase 3: Backends on the new model

1. **Renders forms, not blocks.** Each backend renders the resolved forms instead of reading `docker:`/`vm:`.
2. **Secrets map to each tool's native mechanism.** No value is ever written into a generated file.
   - Compose: `secrets:` from the environment or files.
   - Kubernetes: Secret references with a documented creation step.
   - Terraform: sensitive variables, and the cloud secret stores where a target has one.
   - Vagrant: host environment plus a provisioner.

   Each trade-off is stated.
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
   - plus the security scanners: Checkov, Trivy and kube-score.

   Salvage the generate-everything pipeline from `v1/security-scanning`.
2. **Combinations.** Property-based generation of random valid specs; every target's output must pass `verify`.
3. **Cloud semantics without clouds.** `terraform test` with mock providers asserts the routing, gateway, firewall and per-machine port logic.
4. **Tier 1 real runs, locally:** Docker, kind, VirtualBox and QEMU, using the native tools on the generated files.

**Exit:** `verify` is green on all examples and on 10,000 random specs. The Tier 1 real runs pass, and the tier list is set from evidence.

### Phase 6: The overall review pass

Read the whole codebase end to end for coherence:
- one idiom per concern;
- duplication removed;
- module boundaries and the public API checked;
- error messages consistent.

Then the security review of secrets handling and of every place that writes credentials.

**Exit:** a review checklist with every item closed.

### Phase 7: Converter-aligned ecosystem

- **`isoloom import` as the front door:** Compose, Vagrant, Terraform, Helm and Kubernetes into a spec. Recognise catalogue images and propose `service:`. Lossless round trips where the formats allow.
- **VS Code extension** updated to the new syntax, `matrix` and `verify`.
- **Packages:** Homebrew, winget, Scoop, apt and rpm, and a CLI container image.
- **Terraform/OpenTofu registry modules** published from the generated output.
- **To decide:** the Terraform provider. Today it wraps the runtime. Either reshape it into a data source that returns generated files, or drop it.
- **Out of Isoloom (runtime work):** drift detection, upgrades, snapshots, persistence checks, the deployment hub, the cloud reaper, PR preview environments and the test library. They belong to the launcher or a later product, and are not part of 1.0.

### Phase 8: Docs, squash, release

1. Write the docs in the repo: getting started, the spec reference (generated), targets, the catalogue (generated), secrets, migration and comparisons.
2. Rewrite the website from them.
3. **Squash the history into one commit.** This is a force-push, and needs the maintainer's confirmation at that moment. Then re-enable CI.
4. Sign the release, with an SBOM, provenance and a pinned toolchain, and tag 1.0.0.

### Phase 9: The Isothrum runner

1. **Create the runner engine** in `isoloom/isothrum` (a crate plus a CLI). It executes Isoloom run manifests.
2. **Port into it:**
   - the launcher's runtime: lifecycle, locks, deploy worker, drivers, recovery, reaper, budgets, remote servers;
   - Isoloom's moved run part: secrets delivery, host readiness, registry, WinRM retries.
3. **Tests:**
   - a fake-tool harness for unit tests;
   - one real-lab job per target family (Docker, kind, and Vagrant on a KVM runner), the Tier 1 proof.
4. **Isothrum's desktop app** moves onto the engine.

### Phase 10: The launcher as a UI

1. **Replace the launcher's runtime module** with the Isothrum runner, and its spec handling with the Isoloom crates.
2. **Keep only CTF-specific code:** lab catalogue, accounts, attack box UX, player flows.
3. **Move the labs** with `isoloom migrate`.
4. **Order matters:** the rev the launcher pins today stops resolving after the Isoloom squash, so this phase follows right after.

## 5. Open decisions

0. **Isothrum's lab editor:** it stays in Isothrum next to the runner (a "studio": draw, run, inspect), or it becomes its own tool. Also decide whether the Isothrum runner is open source like Isoloom.

1. **The tool list for 1.0.** "Every other IaC tool": confirm the current families, and whether to add Pulumi, Podman (Compose and Quadlet) or others for 1.0 or 1.x.
2. **The Terraform provider:** reshape it into a files data source, or drop it.
3. **Secrets on clouds:** sensitive Terraform variables (simple, but values sit in Terraform state), or the cloud secret stores (safer, more work).
4. **Tier 1 membership** beyond Docker, kind, VirtualBox and QEMU. Proxmox and the clouds need real infrastructure, so they stay Tier 2 until then.

## 6. Salvage branches (local, not pushed)

| Branch | What may be reused |
|---|---|
| `v1/catalogue-engine` | Catalogue format, loader, nginx entry start, Alpine 3.24 image table entry |
| `v1/windows-in-docker` | dockur container form, KVM refusal, OEM set-up |
| `v1/security-scanning` | generate-everything pipeline, scanner baselines, secure-default generator fixes |
| `v1/cli-output-compat` | spec table as JSON (the matrix may reuse it) |
| `v1/docs-truth-fixes` | Google Cloud MTU capability cell |

They are deleted at the squash.
