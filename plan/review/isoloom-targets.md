# Review: isoloom-targets (Phase 1c)

The composition root above the backends: the registry of each target's backend, the targets made of
two backends, and what spans them (generation, refusals, the resolved snapshot, its reports and
diagram). One line per 3b principle and 3d item: `ok`, or the finding and its fix.

## 3b design rules
- Functional core, imperative shell: ok (every function is pure; only `project_copies` reads the project's files, through `isoloom-kubernetes`).
- Ports and adapters: ok (each backend renders its own files behind `Backend`; this crate only picks and combines them).
- One-way dependencies: ok (spec, plan, render, the backends and manifest; `isoloom-import` is no longer a dev-dependency, and `indexmap` is gone).
- Parse, don't validate: ok (a published port is a `HostPort`, not a `(String, String, u16)`; a message is split into typed pieces with `split_once`, not sliced by index; a cloud target's split is `Clouds { generated, refused }`, not a tuple).
- Typed errors: ok (`Error` in `error.rs`: a refused lab carries a `GenerateError`, why a target wasn't generated, which is also `generate_all`'s list of skipped targets and is built from a backend's `isoloom_render::Error`; a message placeholder, an unclosed `{{`, an unknown report and the project copies each have their own variant, causes behind `#[source]`).
- No hidden state: ok (the registry is a private `static` read through `backend`).
- Determinism: ok (the snapshot's keys are inserted in a fixed order; `generate_all` keeps its file order; the characterization snapshots are byte-identical).
- One way to do each thing: ok (the Docker layout comes from `isoloom_plan::environment::lay_out`, the Vagrant network name from `isoloom_vagrant::network_name`, the Kubernetes member label from `isoloom_kubernetes::network_member`, instead of copies here).

## 3b principles
- Names, one vocabulary, no stutter: ok (`backends::BACKENDS` became the private `registry`; `report::graph_d2`/`graph_dot` became `graph::d2`/`graph::dot`).
- Small functions: ok.
- Few arguments, no flag arguments: ok.
- Single responsibility, small modules: ok (`lib.rs` went from 237 lines of logic to the module tree; generation, refusals, registry and graph each have a module; the snapshot is split into networks, machines, ports, checks, targets, cloud and message; every file under 400 lines).
- Open/closed, Liskov, interface segregation, dependency inversion: ok.
- DRY: ok (zero duplicate blocks; the tests' 14 clones went to the `testing` module).
- KISS and YAGNI: ok (the facade re-exports of backend internals, `GENERATED_TARGETS`, `BACKENDS`, `exec_runner`, `start_plan` and the rest, are gone; a caller uses the backend crate it needs).
- Command-query separation, no hidden side effects: ok.
- Law of Demeter: ok.
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic`, indexing, string slicing or `as` outside tests).
- Clean boundaries with third-party code: ok (JSON only through `serde_json::Value` in the snapshot).
- Clean tests (F.I.R.S.T., one concept each): ok (one integration binary, `tests/targets`, by concept, with `generate/` split into ten; shared helpers in `testing` return a `Result`; the snapshots are their own binary). The time budget that failed under load moved to `cargo xtask bench` (`generate/every-example`, 8 s); `every_example` is its fast, untimed counterpart, every example generated on every target it allows, and `determinism` checks two runs give the same bytes.
- Comments say why: ok.
- Formatting: ok.
- No dead code: ok.

## 3d Rust practices
- Workspace hygiene: ok (no machete ignore left).
- Common traits: ok (`HostPort`, `CloudRefusal`, `Clouds`: `Debug`, `Clone`, `PartialEq`, `Eq`; `HostPort` adds `Hash`).
- Conversions through `From`/`TryFrom`: ok (`isoloom_render::Error` into `GenerateError`, and `GenerateError` into `Error`, by `From`).
- Types carry meaning, `#[non_exhaustive]`: ok (`Error`, `CloudRefusal`, `Clouds`; `HostPort` stays open, since callers build it).
- Ownership: ok.
- `#[must_use]`: ok (every pure query).
- Visibility: ok (`pub(crate)` inside; `pub` only for the facade and `graph`, `report`, `resolved`).
- Library errors with `thiserror` and a `Result` alias: ok.
- Error messages: ok (lowercase, no trailing period, the cause in `source()`).
- No panics in library code: ok.
- Sync or async, observability: ok (synchronous; it logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok (the example generates Docker files and asks Vagrant for its refusal).
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (the message filler: text without a placeholder reads as itself, a placeholder between any texts becomes its value, an open `{{` is an error; every machine name becomes a diagram identifier).
- Snapshots only for whole files: ok (the characterization snapshots and the generated files).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok.
- Modules by domain concept: ok (`registry`, `composite`, `generation`, `refusal`, `resolved`, `report`, `graph`).
- One `Error` in `error.rs` with a `Result` alias: ok.
- Tests next to the code, integration tests in `tests/`, fixtures in `tests/fixtures/`, shared helpers in `testing`: ok.

## Left to other units
- `tests/targets/checks` and `tests/targets/project_copy.rs` test code that lives in `isoloom-plan` (checks), `isoloom-render` (the untar step) and `isoloom-vagrant` (the Ruby it writes); they can move to those crates with their units. The runner-script tests use real servers on 127.0.0.1, with a short sleep for a late greeting.
- The CIDR's text form is written in more than one crate; `impl Display for Cidr` belongs in `isoloom-spec` (the core unit).
