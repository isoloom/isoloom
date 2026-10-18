# Review: isoloom-compose (Phase 1c)

The Docker Compose backend (`docker`, `hosted`, and the Compose side of `docker-vm`,
`cloud-docker` and `hybrid`). One line per 3b principle and 3d item: `ok`, or the finding and its
fix commit.

## 3b design rules
- Functional core, imperative shell: ok (the Compose file, its mounted files and the run manifest are pure functions of a planned `Environment`; no I/O, clock or environment).
- Ports and adapters: ok (implements the `Backend` port of `isoloom-render`; no tool is run).
- One-way dependencies: ok (spec, plan, render, manifest only; no other backend; `isoloom-targets` composes the two-backend targets).
- Parse, don't validate: ok (the file is typed: `Condition`, `Restart`, `NetworkMode`, `LongMount`, `Seconds` and `Ulimit` replace strings; a machine is looked up once, as a `Machine` of name, spec and plan).
- Typed errors: ok (`Error` in `error.rs` with `#[source]`, mapped once into `RenderError`; `start_plan` no longer returns a `String` error).
- No hidden state: ok.
- Determinism: ok (ordered maps throughout; `rendering_twice_gives_the_same_files`).
- One way to do each thing: ok (Compose's `$$` escaping is `interpolation::literal`, once; service names are built in `names`; where a probe reaches a machine is `isoloom_render::checks::address_of`, shared with the VM backends; render's container helpers are used from `isoloom-render`, no longer re-exported).

## 3b principles
- Names, one vocabulary, no stutter: ok (`ComposeBackend` keeps its name beside the `Backend` trait it implements and the registry's other backends; the dissolved `docker` grab-bag became `layout`, `labels`, `names`, `images` and `start`).
- Small functions: ok (all under 60 lines, complexity under 15).
- Few arguments, no flag arguments: ok.
- Single responsibility, small modules: ok (the old `sidecars` is split into `router`, `sidecars`, `check_runners` and `networks`; the file types by concept under `file/`; the largest file has 273 lines of code).
- Open/closed, Liskov, interface segregation, dependency inversion: ok.
- DRY: ok (no `dup/clone`; the own-process spec is a fixture file; container files iterate through the `Machine` view).
- KISS and YAGNI: ok (the public API is the backend, `render`, `hybrid::render`, the start plan, the two labels and the error; the unused `removable_volumes` and the re-exports of render's helpers are gone).
- Command-query separation, no hidden side effects: ok (builders return values; the `Out` collector is local to one render).
- Law of Demeter: ok (the `Machine` view replaces chains of `env.spec().machines[name]`).
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic` or indexing outside tests; a missing network or machine is skipped where the plan guarantees it, and `networks::docker_address` documents its one fallback).
- Clean boundaries with third-party code: ok (`serde_yaml_ng` writes the typed file and reads a file back in `start`).
- Clean tests (F.I.R.S.T., one concept each): ok (unit tests next to the code, shared helpers in `testing`, integration tests return a `Result` instead of allowing `unwrap`).
- Comments say why: ok.
- Formatting: ok (rustfmt).
- No dead code: ok (`unused` at deny; machete clean).

## 3d Rust practices
- Workspace hygiene: ok.
- Common traits: ok (`ComposeBackend`: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`; `StartPlan`: `Debug`, `Clone`, `PartialEq`, `Eq`, `Default`; `Error`: `Debug`).
- Conversions through `From`: ok (`From<Error> for RenderError`, `From<ManifestError> for Error`, `From<String> for Mount`).
- Types carry meaning, `#[non_exhaustive]`: ok (`Error` is non-exhaustive; `StartPlan` is a complete answer, wait and jobs, and stays constructible).
- Ownership: ok (borrows the environment and its machines, returns owned files).
- `#[must_use]`: ok (`capabilities_on_a_vm`; the rest return `Result`).
- Visibility: ok (`pub(crate)` everywhere but the API).
- Library errors with `thiserror`, `#[source]` and a `Result` alias: ok.
- Error messages: ok (lowercase, no trailing period; the backend's messages are unchanged).
- No panics in library code: ok.
- Sync or async, observability: ok (synchronous and pure; it logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok (the crate example renders a lab and reads its start plan back).
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (`interpolation::literal`, `layout::relative_to`, and `start_plan` read back against the typed file).
- Snapshots only for whole files: ok (the six Compose files; the rest are precise assertions).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok.
- Modules by domain concept: ok.
- One `Error` in `error.rs` with a `Result` alias: ok.
- Tests next to the code, integration tests in `tests/`, fixtures in `tests/fixtures/`, shared helpers in `testing`: ok.
