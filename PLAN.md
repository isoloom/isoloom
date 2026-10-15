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
- **The two sites duplicate most of their scaffolding** (`components/ui`, `components/markdown`, `components/seo`, `lib/oembed`, `assets/fonts`, the blog config). DRY across repositories is recommended: one private monorepo, `isoloom/websites`, with `apps/isoloom`, `apps/isothrum` and `packages/site-kit`. Merging keeps each domain's Vercel project, and is the maintainer's call. Without the merge, a private `site-kit` package does the same job.

**When:**
- **Phase 0.7, the engineering standard in both sites:** measure, then lint configs, the ratchet, the folder layout, and driving to zero. It can run alongside other phases, because the sites don't block the converter.
- **Phase 2:** the Isothrum compiler (`isothrum-core`, soon `isothrum-compiler`) and the website editor **write the new spec syntax**. With no old-format support, a drawn lab would otherwise break. They test against Isoloom's spec examples and schema.
- **Until Phase 7:** isoloom.com still describes the old 0.x tool, and some claims are wrong (see the facts kept for the docs rewrite). Recommended: a short "1.0 in progress" banner now, so visitors aren't misled. It needs one push, so it needs the maintainer's OK.
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

**Exit:** green net; a smaller workspace in the target layout; the ratchet in place.

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

**Bugs found in today's code**, fixed by the manifest and runner work (`runner-extraction.md` §6):
- Launcher and Isoloom names have drifted: the port labels, `address` vs `ip`, the ready file.
- The Proxmox `slot` is never set.
- Windows inputs are dropped on Vagrant.
- `wait_ready` reports OK when SSH never answers.
- The lab lock fails open.
- A destroy uses the current region instead of the applied one.
- There are panics outside tests.
- The launcher pins `isoloom-core@50147b3`, which is reachable only on GitHub's object store, since the `dev` branch was deleted; its archive is in `~/code/github-archive/`. The path-dependency switch in Phase 2 removes the pin.

**Still the maintainer's call:** the flag construct in the new syntax (see the syntax design).

### Phase 3: The launcher's modes, wide (L, parallel per target)

Each step means: lowered generation passes the snapshots and `verify`, then a manifest, then a runner driver, then the launcher's lab test.
1. **Local:** `vagrant` (VirtualBox, Hyper-V, VMware, QEMU/libvirt) and `docker-vm`.
2. **Server:** `proxmox` (Terraform) and the Vagrant ESXi provider, with the launcher's `connection_env` contract. The Vagrant Proxmox provider is Tier 2.
3. **Cloud:** `cloud-vm`, `cloud-docker` and `cloud-services`, AWS first and then the other clouds.
   - **Proxmox bugs found by the real smoke test** on `pve-lab` (2026-10-15), to fix in this phase:
     - `destroy` leaves the SDN zone and bridge pending, because the "apply SDN" resource is destroyed before its zone; it needs the provider's finalizer ordering.
     - The module defaults to `node = "pve"`, while the launcher's Test connection uses the first node. A server with another node name passes the test and then fails the launch. Default to the discovered node, or make it required.
     - A router whose guest agent can't install (no DNS or internet) hangs `apply` for 15 minutes. Fail fast with a reason.
4. **Secrets conversion and native checks** for these targets only:
   - Compose secrets;
   - Vagrant host environment;
   - each cloud's secret store, AWS first;
   - checks as native artefacts.
5. **Port the rest of the launcher's runtime into the runner,** driver by driver: locks, park and resume, recovery, reaper, budgets, remote servers, Isoloom's secrets delivery.

**Exit:** every launcher mode runs its test labs (rewritten in the new syntax) through Isoloom and the runner. Local is proven on macOS, plus Windows and Linux through the cross-platform check. Server and cloud are proven where the Track C infrastructure exists.

### Phase 4: The catalogue and the CM model (L)

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
- **Isoloom ecosystem:** import as the front door, the VS Code extension, packages, registry modules and the website rewrite.
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
