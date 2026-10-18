# Review: isoloom-import (Phase 1c)

Drafting a spec from Compose, Terraform and a recorded Vagrantfile. One line per 3b principle and
3d item: `ok`, or the finding and its fix commit.

## 3b design rules
- Functional core, imperative shell: ok (every import is a pure function of text or a recording; the CLI reads the files and runs the Vagrantfile's recorder).
- Ports and adapters: ok (no tool is run here; the recorder's JSON is parsed once at the edge into `vagrant::Recording`).
- One-way dependencies: ok (`isoloom-spec` only; `isoloom-plan` for the tests' full validation; `isoloom-targets` no longer depends on it).
- Parse, don't validate: ok (the recording is a typed `Recording`, not JSON indexed by string; sizes are an exact `Decimal`, not a float cast; a subnet is a `Declared` enum, not `Option<Option<_>>`; a network keeps its `Cidr`).
- Typed errors: ok (`Error` in `error.rs`, sources carried by `#[source]`, not repeated in the message).
- No hidden state: ok.
- Determinism: ok (ordered maps; names numbered in reading order).
- One way to do each thing: ok (one `label`, one `unique` for every name; one quoting module for every text written; one `block_text`; one memory and CPU reading through `quantity`).

## 3b principles
- Names, one vocabulary, no stutter: ok (`ImportError` became `Error`; `yaml::DraftYaml` became `draft::writer::Writer`; `PortEntry` became `Port`).
- Small functions: ok.
- Few arguments, no flag arguments: ok (the subnet name's flag argument became `Instance::name_or`).
- Single responsibility, small modules: ok (Compose's values split into `text`, `environment`, `ports`, `volumes`, `limits`; Terraform into `instance`, `subnets`, `vms`, `eval`, `placement`; Vagrant into `recording`, `boxes`, `write`; every file under 400 lines).
- Open/closed, Liskov, interface segregation, dependency inversion: ok.
- DRY: ok.
- KISS and YAGNI: ok (the public API is the three `draft` functions, `Recording`, `Draft`, `Note`, `NoteKind` and the error; `label` became crate-private).
- Command-query separation, no hidden side effects: ok.
- Law of Demeter: ok.
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic`, indexing or `as` outside tests; a Compose service named by a number used to panic on `names[""]`).
- Clean boundaries with third-party code: ok (HCL evaluation in `terraform::eval`; YAML reading in `compose::lenient`).
- Clean tests (F.I.R.S.T., one concept each): ok (one test binary, `tests/import`, by source; helpers in its `testing` module return a `Result`).
- Comments say why: ok.
- Formatting: ok.
- No dead code: ok.

## 3d Rust practices
- Workspace hygiene: ok (`hcl-rs` is declared once as `hcl`, so machete matches it with no ignore list).
- Common traits: ok (`Draft`, `Note`: `Debug`, `Clone`, `PartialEq`, `Eq`; `NoteKind` adds `Copy`, `Ord`, `Hash`, `Display`; `Recording`: `Debug`, `Clone`, `Default`, `PartialEq`).
- Conversions through `From`/`TryFrom`/`FromStr`: ok (`Recording: FromStr + TryFrom<serde_json::Value>`; `Decimal: FromStr`).
- Types carry meaning, `#[non_exhaustive]`: ok (`Draft`, `Note`, `NoteKind`, `Error`).
- Ownership: ok.
- `#[must_use]`: ok.
- Visibility: ok (`pub(crate)` and `pub(super)` inside; 14 unreachable `pub` gone).
- Library errors with `thiserror` and a `Result` alias: ok.
- Error messages: ok (lowercase, no trailing period, the cause in `source()`).
- No panics in library code: ok.
- Sync or async, observability: ok (synchronous; it logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok.
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (quoted text and scalars read back as themselves, comments stay comments, labels are machine names, names made unique stay names, decimals scale exactly, shell variables, published ports, memory sizes). They found three bugs: a line break in a quoted value was folded into a space, a label from a digit could run past 63 characters and end on a dash, and a header comment could be ended by the source's file name.
- Snapshots only for whole files: ok (none; drafts are checked field by field and through `isoloom validate`).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok.
- Modules by domain concept: ok.
- One `Error` in `error.rs` with a `Result` alias: ok.
- Tests next to the code, integration tests in `tests/`, fixtures in `tests/fixtures/`, shared helpers in `testing`: ok (every draft of the fixtures passes `isoloom validate`, files included; the Vagrant fixture gained the scripts and playbook it names).

## The new syntax
- Drafts are written in the spec format of `plan/phase2/spec.md` (the spec unit converted them; this unit proves them): named ports, `os`, `app.image` / `app.build` / `app.steps`, `environment` inputs, `overrides.vagrant.machines`.
- Input forms that can't map cleanly, each a note or a validation problem the CLI prints: a mount path with a space or `:`; a service with neither image nor build; host-side Ansible (environment-level `provision:`); DHCP and public networks; inline provisioning scripts; Compose `command`/`entrypoint`, fixed environment values and build settings beyond context, Dockerfile, target and fixed args; shared named volumes; one-shot and profile-only services; Terraform resources other than VMs, their network interfaces and subnets; `for_each` or `count` that needs Terraform to evaluate.
- Not built: Helm and Kubernetes importers (none exists; a 1.x feature, PLAN Phase 8).
