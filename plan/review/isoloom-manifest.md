# Review: isoloom-manifest (Phase 1c)

The run manifest, version 0: the types Isoloom writes next to a target's files and the Isothrum
runner reads, their generated JSON Schema and the shared fixtures. One line per 3b principle and
3d item: `ok`, or the finding and its fix commit.

## 3b design rules
- Functional core, imperative shell: ok (pure: it reads and writes text; no file, process, clock or environment).
- Ports and adapters: ok (a contract crate: no adapters of its own; the runner's drivers implement it).
- One-way dependencies: ok (no workspace dependency; spec, plan, backends and Isothrum depend on it).
- Parse, don't validate: ok (names, step ids, paths, templates, programs, versions, instances and retries are newtypes that only hold valid values; a template can't hold `${secrets.…}`; port names are `PortName` with the spec's rule, so `${published.<machine>.<port>}` reads back as written; a port's `protocol`/`http`/`tls` are one `PortKind`; what spans fields is `Manifest::validate`).
- Typed errors: ok (`Error` in `error.rs`, `Result<T>`, a stable `code()` per variant shared with the runner).
- No hidden state: ok.
- Determinism: ok (keys in the types' order, ordered maps and sets; `to_json` then `parse` gives the same manifest, a property the fuzz target checks).
- One way to do each thing: ok (one `is_default` for skipped fields; one newtype macro; `walk` lists every step list and template once for validation and the runner).

## 3b principles
- Names, one vocabulary, no stutter: ok (`isoloom_manifest::{Manifest, Error}`; modules by concept: `provenance`, `manifest`, `identity`, `lifecycle`, `machine`, `port`, `slots`, `step`, `template`, `path`, `argv`, `names`, `list`, `outputs`, `validate`, `walk`, `schema`).
- Small functions: ok.
- Few arguments, no flag arguments: ok.
- Single responsibility, small modules: ok (the largest file, `template.rs`, has 366 lines with its tests).
- Open/closed, Liskov, interface segregation, dependency inversion: ok.
- DRY: ok (no `dup/clone`).
- KISS and YAGNI: ok.
- Command-query separation, no hidden side effects: ok.
- Law of Demeter: ok.
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic`, indexing or slicing outside tests).
- Clean boundaries with third-party code: ok (serde and schemars only at the types' edges).
- Clean tests (F.I.R.S.T., one concept each): ok (unit tests beside each module; integration tests return a `Result`; the validation tests patch the minimal fixture).
- Comments say why: ok.
- Formatting: ok.
- No dead code: ok.

## 3d Rust practices
- Workspace hygiene: ok.
- Common traits: ok (`Debug`, `Clone`, `PartialEq`, `Eq` on every type; `Hash`/`Ord` on names, paths, templates and enums used in sets; `Display` on every text newtype, `Cidr`, `Template`; `Manifest` and `Error` asserted `Send + Sync`).
- Conversions: ok (`FromStr` and `TryFrom<String>` beside each `parse`, url-style; `From<_> for String`; `TryFrom<u8>`/`From<_> for u8` for `Instance` and `Retries`; `From<Placeholder> for Template`).
- Types carry meaning, `#[non_exhaustive]`: ok (`Error` is non-exhaustive; the contract's own enums are exhaustive on purpose, so a runner handles every action, delivery and condition: the crate docs say why).
- Ownership: ok.
- `#[must_use]`: ok.
- Visibility: ok (every module private; the API is re-exported from `lib.rs`).
- Library errors with `thiserror` and a `Result` alias: ok (contract errors carry no source; serde's are flattened into `Field` with the path).
- Error messages: ok (lowercase, no trailing period, the fix named).
- No panics in library code: ok.
- Sync or async, observability: ok (synchronous; logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok.
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (templates, relative paths, path templates and programs round-trip and agree with the schema; a `${published.<machine>.<port>}` reads back as the machine and port it names, ports drawn from more than a port's name allows).
- Snapshots only for whole files: ok (the committed schema is compared whole; no snapshot of anything else).
- Fuzzing: ok (`manifest`: parse, write, read back; 300 s on the landed code (`d7f83f2c`), 896,135 runs, no crash).
- Mutation testing: ok (before: 314 mutants, 46 missed, 203 caught, 65 unviable, 0 timeouts; after, on the current code with the server unit's additions: 318 mutants, 244 caught, 74 unviable, 0 missed, 0 timeouts; one equivalent listed in `.cargo/mutants.toml`, an inlined schema's name).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok.
- Modules by domain concept: ok.
- One `Error` in `error.rs` with a `Result` alias: ok.
- Tests next to the code, integration tests in `tests/`, shared helpers in `testing`: ok, with one deliberate exception: the fixtures stay in `fixtures/` at the crate's root, because they are the published contract the Isothrum runner tests against (it finds them through `cargo metadata`), not this crate's private test data.
