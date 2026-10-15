# Docs facts (for the Phase 7 rewrite)

Every phase appends one line per user-visible fact it changed. The facts kept from before the rebuild follow.

# Facts to get right in the final docs rewrite

## From the website sync
- `docs/comparisons/terraform.md:25` and `pulumi.md:21` say "runs on" Proxmox, ESXi and six clouds. Isoloom generates for them and validates them; the real runs are Docker, VirtualBox, QEMU and kind.
- `docs/comparisons/vagrant.md:44` says VirtualBox is the only provider run for real. QEMU also ran by hand (#187, #199).

## From the docs-truth unit (PR #236)
- **cloud-vm limits in `targets.md`:**
  - It says every cloud refuses `mtu`, but Google Cloud sets it (#236).
  - Every cloud refuses Ansible `.yml` checks and a Windows machine on several networks; neither is listed.
  - The Azure and Google Cloud bullets still say there are no gateways; #220 added them.
  - Linode and DigitalOcean also refuse IPv6 and gateways.
- **AWS and Azure hand runs:** they predate 1.0; the v1 rewrites (#171, #174) applied nothing.
- **VirtualBox:**
  - The TPM run (#194) was on a Debian guest, a copy of tuned-web, not on Windows.
  - Windows Server 2019 ran only with the pre-1.0 generator.
  - The NFS run was redone on v1 (#172).
- **QEMU:** the provider table (~line 200) calls VirtualBox the only provider run for real; QEMU ran in #187 and #199.
- **docker-vm:** it is marked "not run on a real host", but #200 ran it on VirtualBox.

## Real runs recorded on 2026-10-14
- **Docker:** hello-stack, segmented, tuned-web, shared-files, dual-stack and private-names; 32 checks passed.
- **Persistent volumes:** a volume survived `down`/`run`, and `down --volumes` removed it.
- **Owned project files (#221):** 101:101 mode 0400 on docker and hosted.
- **Launcher labs:** pass on Isoloom main.

## Phase 1a: subtract
- **`isoloom coverage` is gone,** with `docs/COVERAGE.md`: no per-format coverage tables any more. The spec-field × target matrix comes back generated from the backends' declared capabilities (Phase 6).
- **`isoloom docs --spec` (docs/spec.md) lists each field from the schema only:** type, default, allowed values, limits, the schema's examples. The per-target "Rendered by / Refused by / Planned for" lines and the probe examples are gone.
- **`isoloom import` notes:** every Compose key, Compose network key, top-level key or Terraform resource type the import doesn't read gets one note, "not read by the import: left out", under "Changed on the way in: check these". The note kind "Written by Isoloom when it generates" is gone.
- **The examples have no committed `.isoloom/` outputs:** run `isoloom generate` to see them. The repository ignores `.isoloom/` and `.isoloom-*/`.
- **No compatibility forms:** the spec reads only the current format; an old form fails to load, naming the field or value.
  - **A machine's `services:` is refused** ("unknown field `services`, expected one of ..."): `ports:` is the only key. `isoloom validate` prints no warnings any more, and its `--json` output has no `warnings` key.
  - **An OS is written `<family>@<version>` or `<family>` only,** in a machine's `os`, `vm.os`, `common:`/group `vm.os`, `-s machines.X.vm.os=...`, and the image table (`--images`, a defaults file's `images:`; its `os:` keys and `access.vm`). Image-table names (`debian-12`, `windows-server-2019`, `almalinux-9`) are refused.
  - **An unknown OS is a load error** naming the field (`machines.web.vm.os: unknown OS ...`), no longer a validation problem listed with the others.
  - **`appliance: cisco-viosl2` is refused:** the spelling is `cisco-vios-l2`.
  - **`isoloom import` drafts write the OS as `<family>@<version>`** (`debian@12`, `windows-server@2019`).
  - **Library:** `isoloom_targets::qemu_refusal` is gone (use `refusal_on(spec, Target::Vagrant, Some("qemu"))`), and so are `isoloom_spec::warnings` and `isoloom_spec::renamed`.
- **The hand-written docs are gone until the Phase 7 rewrite** (the last versions are in git history before this phase): `docs/` getting-started, targets, secrets, storage, esxi, cloud-modules, json, registry, real-targets and stability; the ADRs (`docs/adr/`), the comparisons (`docs/comparisons/`), `rfcs/`, `ARCHITECTURE.md` and `CHANGELOG.md`. `docs/` keeps the generated `spec.md` and `events.schema.json`. CONTRIBUTING, SECURITY and CODE_OF_CONDUCT live in `.github/`.
- **Stale pointers left on purpose for later units:** the lifecycle commands' `--json` help still names `docs/json.md`, the runtime's events doc comments too, and the disabled workflows and `scripts/real-targets/` name `docs/real-targets.md` and `CHANGELOG`; they go with the runtime and the xtask move.

## Phase 1b: Isoloom's structure
- **The repository layout:** the crates sit in groups under `crates/`: `core/` (`isoloom-spec`, `isoloom-plan`, `isoloom-render`, `isoloom-manifest`), `backends/` (`isoloom-compose`, `isoloom-kubernetes`, `isoloom-vagrant`, `isoloom-terraform`, `isoloom-ansible`), `tools/` (`isoloom-import`), `targets/` (`isoloom-targets`) and `cli/` (`isoloom`).
- **The `isoloom-external` crate is now `isoloom-ansible`** (Rust: `isoloom_ansible::ExternalBackend`). The target it renders is still called `external`.
- **`rust-toolchain.toml` pins Rust 1.99.0** with rustfmt, clippy and the Linux and Windows MSVC targets; rustup installs it on first use.
- **Stale pointers left on purpose for the Phase 7 rewrite:** README's crate table still names `isoloom-external`, and `.github/CONTRIBUTING.md` still says `crates/*/src` and `crates/isoloom-spec/tests/code_size.rs`.
- **The runtime is gone from Isoloom** (the last commit holding it: `b6c218c9`; the runner takes what it needs from there with `git show b6c218c9:<path>`). Isoloom converts; it runs nothing:
  - **Removed commands:** `run`, `test`, `down`, `reset`, `provision`, `status`, `connect`, `exec`, `capture`, `tc` (link impairment on a running router), `gc` (Docker leftovers), `doctor` (the host's tools, credentials and secret stores) and `chart` (it ran `helm package`; run `helm package .isoloom/kubernetes/chart` on the generated chart instead).
  - **Removed options:** `isoloom targets --host` (the host's readiness per target), and `isoloom schema --events` with `docs/events.schema.json` (the lifecycle's `--json` events and error documents).
  - **Removed with them:** the registry of running environments (`~/.isoloom/status.yml`), the lifecycle's JSON events, run-time secret delivery (SSH, WinRM and its Python helper), host readiness, and the `isoloom-runtime` crate.
  - **`isoloom report` and `isoloom graph` read the same resolved snapshot:** the tables and diagrams are now `isoloom_targets::report` (a library module, no longer `isoloom_runtime::report`).
  - **`isoloom message`** fills the addresses for the target named with `--target`, else the spec's only target, else the spec's own addresses; it no longer asks a running Docker environment for the host ports it got.
  - **The command line's `about`** now says it gets each tool's own files (Compose, Kubernetes, Vagrant, Terraform, Ansible), no longer that it runs the environment.
  - **Stale on purpose (they change generated output, so they wait for the run manifest work):** the generated files' comments and wait messages still say `isoloom run` delivers secrets or creates Kubernetes Secrets, and README still lists the lifecycle commands.
- **Library: the targets made of two backends are composed in `isoloom-targets`,** so no backend depends on another: `isoloom_targets::composite::{CloudDockerBackend, DockerVmBackend, HybridBackend}` (also re-exported by `isoloom_targets::backends`) replace `isoloom_terraform::cloud_docker::CloudDockerBackend` and `isoloom_vagrant::{DockerVmBackend, HybridBackend}`. The container helpers the backends share (`CHECK_IMAGE`, `PROBE_IMAGE`, `PROBE_DIR`, `UTILITY_IMAGE`, `probe`, `ready_test`, `exec_runner`, `leaf_jobs`, `start_commands`, the project-file variables and `host_environment`) live in `isoloom_render::container` (still re-exported by `isoloom_compose`), and the hybrid containers' Compose file is `isoloom_compose::hybrid::render`. Generated files are unchanged.
