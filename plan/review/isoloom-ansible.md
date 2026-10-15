# Review: isoloom-ansible (Phase 1c)

The `external` target's backend. One line per 3b principle and 3d item: `ok`, or the finding
and its fix commit.

## 3b design rules
- Functional core, imperative shell: ok (pure rendering of a planned `Environment`; no I/O, clock or environment).
- Ports and adapters: ok (implements the `Backend` port of `isoloom-render`; no tool is run).
- One-way dependencies: ok (spec, plan, render only; no other backend; the dependency test passes).
- Parse, don't validate: ok (reads the plan's typed `Environment`; the endpoint defaults are named constants).
- Typed errors: ok (`Error` in `error.rs`, mapped once into the backend's `RenderError`).
- No hidden state: ok (no globals, no `set_var`, no `unsafe`).
- Determinism: ok (ordered maps throughout; `rendering_twice_gives_the_same_files`).
- One way to do each thing: ok (the inventory, the check scripts and where a probe reaches a machine (`isoloom_render::checks::address_of`) come from `isoloom-render` and `isoloom-plan`; no copy).

## 3b principles
- Names, one vocabulary, no stutter: ok (`ExternalBackend`, `endpoints`, `checks`; the target's words).
- Small functions: ok (all under 60 lines, complexity under 15).
- Few arguments, no flag arguments: ok.
- Single responsibility, small modules: ok (backend, endpoints, check runners, error; each under 120 lines).
- Open/closed, Liskov, interface segregation, dependency inversion: ok (one trait implementation, no downcasts).
- DRY: ok (no `dup/clone`; the tests' environment builder is in `testing`).
- KISS and YAGNI: ok (the public API is `ExternalBackend`, `Error` and `Result`).
- Command-query separation, no hidden side effects: ok (pure functions return files).
- Law of Demeter: ok.
- Errors as types, no null: ok (`Option` and `Result`; no `unwrap`, `expect` or `panic` outside tests).
- Clean boundaries with third-party code: ok (`serde_json` only builds `machines.json`).
- Clean tests (F.I.R.S.T., one concept each): ok (behaviour-named, in-memory, no sleeps).
- Comments say why: ok.
- Formatting: ok (rustfmt).
- No dead code: ok (`unused` at deny; machete clean).

## 3d Rust practices
- Workspace hygiene: ok (inherits the workspace's package, dependencies and lints).
- Common traits: ok (`ExternalBackend`: `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`, `Hash`; `Error`: `Debug`, `Clone`, `PartialEq`, `Eq`).
- Conversions through `From`: ok (`From<Error> for RenderError`).
- Types carry meaning, `#[non_exhaustive]`: ok (`Error` is non-exhaustive).
- Ownership: ok (borrows the environment, returns owned files).
- `#[must_use]`: ok (no pure public function left without it; clippy `must_use_candidate` is clean).
- Visibility: ok (everything but the API is private or `pub(crate)`).
- Library errors with `thiserror` and a `Result` alias: ok (a refusal has no source; its reason is its field).
- Error messages: ok (lowercase, no trailing period: `external: <refusal>`).
- No panics in library code: ok.
- Sync or async, observability: ok (synchronous and pure; it logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok (the crate example renders a home lab).
- Docs build clean: ok (`RUSTDOCFLAGS=-Dwarnings cargo doc`).
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (none here: the inventory's quoting is `isoloom-render`'s).
- Snapshots only for whole files: ok (precise assertions; the CLI's characterization snapshots cover the files).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok.
- Modules by domain concept: ok (`backend`, `endpoints`, `checks`, `error`).
- One `Error` in `error.rs` with a `Result` alias: ok.
- Tests next to the code, shared helpers in `testing`: ok.
