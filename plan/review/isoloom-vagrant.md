# Review: isoloom-vagrant (Phase 1c)

The `vagrant` target's backend, and the Vagrant sides of `docker-vm` and `hybrid`. One line per
3b principle and 3d item: `ok`, or the finding and its fix commit.

## 3b design rules
- Functional core, imperative shell: ok (every Vagrantfile is a pure function of a planned `Environment`; the Ruby it writes runs on the host, not here).
- Ports and adapters: ok (implements the `Backend` port of `isoloom-render`; no tool is run).
- One-way dependencies: ok (spec, plan, render only; no other backend; `isoloom-targets` joins the Compose side).
- Parse, don't validate: ok (a provider name is parsed once at the API's edge into `Provider`, so the TPM, nested-virtualization, boot and network rules match on a type, not on strings).
- Typed errors: ok (`Error` in `error.rs`, mapped once into `RenderError`; the hybrid target reports its VMs' refusals through `Error::on`).
- No hidden state: ok.
- Determinism: ok (ordered maps throughout; `rendering_twice_gives_the_same_files`; ports and MAC addresses from one stable hash).
- One way to do each thing: ok (FNV-1a once in `stable_hash`, for the QEMU ports and the Hyper-V and Docker hosts' MACs; the ESXi host's lines once in `vagrantfile::esxi_host`; the containers' host size once in `docker_host`; the QEMU links once in `vms::qemu_links`; where a probe reaches a machine is `isoloom_render::checks::address_of`).

## 3b principles
- Names, one vocabulary, no stutter: ok (the `vagrant` module became `vms`, so the API reads `isoloom_vagrant::VagrantBackend`; `helpers` became `vms::own`, Isoloom's own VMs).
- Small functions: ok (`guest` takes the plan it was looked up by, five arguments).
- Few arguments, no flag arguments: ok.
- Single responsibility, small modules: ok (the backend, the providers' refusals and the error left the target's module; every file under 260 lines of code).
- Open/closed, Liskov, interface segregation, dependency inversion: ok.
- DRY: ok (no `dup/clone` counted to this crate; the one clone still reported beside it is Terraform's capability list, counted there).
- KISS and YAGNI: ok (the public API is the backend, `docker_vm::vagrantfile`, `hybrid::{capabilities, render}`, `provider_refusal`, `disk_refusal` and the error; the Ruby builder is crate-private and lost its unused `Call::when` and `Value::as_str`).
- Command-query separation, no hidden side effects: ok.
- Law of Demeter: ok.
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic`, indexing or `as` outside tests).
- Clean boundaries with third-party code: ok.
- Clean tests (F.I.R.S.T., one concept each): ok (the boot tests split by concept; integration tests return a `Result`; the transport tests' spec validity is a test of its own).
- Comments say why: ok.
- Formatting: ok.
- No dead code: ok.

## 3d Rust practices
- Workspace hygiene: ok.
- Common traits: ok (`VagrantBackend`: `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`, `Hash`; `Error`: `Debug`, `Clone`, `PartialEq`, `Eq`).
- Conversions through `From`: ok (`From<Error> for RenderError`, `From<&str> for Provider`).
- Types carry meaning, `#[non_exhaustive]`: ok (`Error` is non-exhaustive; `Provider` keeps an unknown provider's name, so a refusal still names it).
- Ownership: ok.
- `#[must_use]`: ok (`vagrantfile`, `capabilities`, `provider_refusal`, `disk_refusal`).
- Visibility: ok.
- Library errors with `thiserror` and a `Result` alias: ok (refusals have no source; their reason is their field).
- Error messages: ok (lowercase, no trailing period; unchanged).
- No panics in library code: ok.
- Sync or async, observability: ok (synchronous; it logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok.
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (a heredoc's closing tag is never a line of its script, now for every tag the builder is given, not only `SH`; the Ruby string quoting is `isoloom-render`'s `rb`).
- Snapshots only for whole files: ok (the six Vagrantfiles).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok (the crate-level `router` alias is gone).
- Modules by domain concept: ok (`foo.rs` with `foo/`, never `mod.rs`).
- One `Error` in `error.rs` with a `Result` alias: ok.
- Tests next to the code, integration tests in `tests/`, shared helpers in `testing`: ok.
