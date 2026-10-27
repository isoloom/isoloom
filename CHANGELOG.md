# Changelog

All notable changes to Isoloom are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The spec file format has its own version (`version: 1` in `isoloom.yml`), which is
independent of the crate version. 1.0 is the first stable release and the first
stable spec format.

## [1.0.2] — 2026-10-27

### Added

- **A container machine can declare both `app.image` and `app.build`** — the Compose
  `build:`+`image:` combination. The build is tagged with the published image name, so a
  developer builds and tags it locally while a student pulls the pre-built image. The former
  rule that a machine used *either* `image` *or* `build` is relaxed; a machine that still gives
  only one behaves exactly as before.
- **A lab that publishes a built image now pulls it before `up`.** When a machine publishes an
  image it also builds, the Docker run does `docker compose pull --ignore-pull-failures` first
  and `up` drops `--build`: a present published image is used as-is and only a missing one is
  built from its context (pull-if-available-else-build). A lab that only builds locally keeps
  `--build` and gains no pull step, so an author's edits always rebuild. The same treatment
  applies to the Docker-in-VM guest path.
- **libvirt boxes for Windows Server 2016 and 2019**, so Windows VM labs deploy on
  vagrant-libvirt (KVM); the StefanScherer boxes those versions mapped to are
  VirtualBox/VMware-only. (2022/2025/10/11 already had libvirt builds.)

## [1.0.1] — 2026-10-18

### Fixed

- **A lab with an access machine now passes `isothrum check` on Docker.** The Compose check
  runners wrongly depended on the access box (and its routes), which live in the on-demand
  `access` profile; running the checks with just the `check` profile made `docker compose`
  reject the project ("depends on undefined service"). Check runners no longer wait on the
  access box, and a check that runs *from* the access position uses the `check`-profile stand-in
  (an idle container at the box's addresses) rather than the box itself. Affects every target
  that renders Compose (docker, docker-vm, cloud-docker).

## [1.0.0] — 2026-10-18

The first stable release. It is a ground-up rebuild of the 0.x preview: a single,
tool-neutral spec that converts to every supported IaC and configuration-management
tool, with no runtime. The `0.11.x` series on crates.io were the release candidates
for this release.

**Isoloom 1.0 does not read 0.x specs.** There is no compatibility layer and no
`isoloom migrate`: an old spec fails validation with an error naming the unknown
field. See [`docs/migrating-to-1.0.md`](docs/migrating-to-1.0.md) for the field-by-field
changes and before/after examples.

### Added

- **One spec, one form per machine.** A machine declares *what* it is — an `os` plus
  either a `service` from the catalogue (with typed `with:` settings) or a custom `app`
  (an own `image`, or a `build` recipe) — and never *how* a platform runs it. Each
  target decides the delivery: an official or built container image on container targets,
  a fresh VM plus a configuration role on VM targets.
- **Name-keyed `ports:`.** Ports are a map keyed by name (`ports: { web: { port: 80,
  protocol: http, publish: 8080 } }`). The name is used everywhere downstream: Compose
  labels, Terraform outputs, the resolved snapshot, and the run manifest
  (`${published.<machine>.<name>}`), and it fills message placeholders such as
  `{{ machines.web.ports.web.url }}`.
- **`inputs:` and file delivery.** Values given at each start (a flag, a launch token)
  are declared under `inputs:` and are sensitive by default. A sensitive input reaches a
  machine as a file (`files: [{ path, input }]`), never as an environment variable; a
  non-secret input can be placed in `environment:` explicitly. Values never land in a
  generated file.
- **`setup:`** — the once-at-start seed steps for a machine, rendered the same way on the
  container and VM forms.
- **The configuration model.** `app.steps:` carries a typed, tool-neutral resource model
  (packages, users and groups, files and templates, copies, commands with an
  already-done guard, firewall rules and services). It renders to Ansible, cloud-init,
  shell/PowerShell and Dockerfile, with the container and VM forms kept at parity by
  construction.
- **The catalogue.** `isoloom-catalogue`: data-driven service entries (a container form,
  a VM form, and a parity contract), release matching, typed `with:` settings, and
  `cargo xtask catalogue refresh` / `catalogue check` to keep pins current.
- **`isoloom verify`.** Every generated file is run through its format's official
  validator (Compose, Terraform + tflint, Vagrant, kubeconform + Helm, cloud-init,
  hadolint, shellcheck, and more), with a pinned toolchain image and a tracked baseline.
- **The capability matrix and tiers.** `isoloom matrix` shows, per target, what each spec
  field becomes (generated, refused with the reason, or not applicable), worked out from
  the generators themselves, plus each target's tier (how far it has been proven by real
  runs). Generated to `docs/matrix.md`.
- **The egress seal.** An `internet: false` network is a real guarantee. On cloud targets
  it is sealed at the network level by a `seal` step the runner applies once the
  environment is ready, and it fails closed. A machine may acknowledge a best-effort seal
  with `machines.*.seal: best-effort` (needed only for a privileged container on an
  offline network).
- **The run manifest.** Each target emits a versioned `run.json` next to its files: native
  commands and their order, readiness criteria, outputs (addresses, host ports, URLs),
  how to run checks, and secret slots (names and destinations, never values). It is the
  contract the Isothrum runner consumes.
- **Six clouds.** `cloud-vm`, `cloud-docker` and `cloud-services` on AWS, Azure, GCP,
  DigitalOcean, Linode and Oracle Cloud, each through Terraform/OpenTofu with the
  provider's own CLI credentials and its native secret store (with a documented SSH
  guest-delivery fallback where a store doesn't fit).
- **`isoloom import`** drafts an `isoloom.yml` from native files you already have.
- **Cross-platform output.** The CLI ships for macOS, Windows and Linux, and generated
  output runs from all three hosts (no `sh`-only `local-exec`, no baked-in host path
  separators or line endings).

### Changed

- **Rearchitecture into a lowering front end.** The new spec (`isoloom-spec::Spec`) is
  parsed and checked, then *lowered* per target into the plan IR, which carries the
  container and VM forms. The backends (Compose, Kubernetes/Helm/Kustomize, Vagrant,
  Terraform, Ansible) and their 800+ tests keep guarding the output. Crates are grouped
  under `core/`, `backends/`, `tools/`, `targets/` and `cli/`, with a one-way dependency
  direction enforced by a test.
- **`isoloom-external` is now `isoloom-ansible`.** Existing hosts become an Ansible
  inventory plus playbooks.
- **Run manifest: the top-level `lab:` key is now `environment:`** (`name`, `instance`,
  `message`; same shape). The manifest is a general-purpose contract, so the CTF/courses
  term is gone. A manifest still carrying `lab:` is rejected as an unknown field.
- **`Version` ordering is semver-correct.** The internal `Version` newtype now compares its
  numbers numerically (so `2.14.0 > 2.9.0`, and a pre-release sorts before its release)
  instead of lexicographically. Generated output is unchanged.
- **Dev tasks are `cargo xtask` commands**, not shell scripts, so they run the same on
  macOS, Windows and Linux.

### Removed

- **The runtime.** `isoloom-runtime` and the runtime half of the CLI
  (`run`/`test`/`down`/`status`/`connect`/`exec`/`capture`/`reset`/`provision`, the
  registry, JSON lifecycle events, run-time secret delivery and Isoloom-executed checks)
  are gone. Isoloom converts; the Isothrum runner runs.
- **`isoloom-coverage`** (the 30-table format-coverage metric and its drift job), replaced
  by the generated capability matrix and conformance against official validators.
- **Backward compatibility.** No field aliases, no deprecations, no `isoloom migrate`, and
  no tests on old specs. The `docker:` / `vm:` machine blocks and the list-shaped `ports:`
  are gone (see the migration guide).

### Security

- The Docker bootstrap installs from the official apt repository with a key pinned by
  `sha256` and a resolved version (`get.docker.com` kept only as the RPM fallback), and it
  fails closed.
- A privileged container (or `NET_ADMIN` / `NET_RAW`) is refused on an `internet: false`
  or offline network unless it opts in with `seal: best-effort`, so the local seal is a
  real guarantee.
- File `mode` values are quoted in the generated configuration.
- Ansible Galaxy collections are pinned by `sha256`.
- Base cloud OS images (Proxmox, docker-vm, ESXi) are pinned to dated releases with
  checksums.
- Secrets and inputs never appear in a generated file: outputs carry references only, and
  the runner delivers the values over SSH stdin.
