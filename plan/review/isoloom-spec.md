# Review: isoloom-spec (Phase 1c)

The spec: the format an author writes (`syntax`: parsed into types, every problem coded and
placed), the lowered form the plan builds (`model`) and its validation, the schema and the
reference page read from the types, and what a spec carries beside it (defaults, profiles,
instances, the image table). One line per 3b principle and 3d item: `ok`, or the finding and its
fix commit.

## 3b design rules
- Functional core, imperative shell: ok (parsing, checks, lowering's inputs, validation, the schema and the reference are pure; reading files is `load`, `defaults::load`, `profile::{load, list}` and `validate_files`, each a thin edge; the environment is passed in as a `host::Host`, never read).
- Ports and adapters: ok (no tool, process or network).
- One-way dependencies: ok (no workspace dependency; everything above depends on it).
- Parse, don't validate: ok (see the syntax review below).
- Typed errors: ok (one `Error` in `error.rs` with `Result<T>`; `LoadError`, `os::OsError` and `syntax::Invalid` folded in; no `Result<_, String>` left: the serde conversions return `Error` too, and so do `Cidr` and `Cidr6`'s `FromStr`; spec mistakes are `syntax::Problem`s, data, not errors; a lowered spec's problem carries its `Code` from the check that found it, so the plan never reads a message's wording).
- No hidden state: ok (no `std::env`, clock or global; `Host` carries the environment).
- Determinism: ok (ordered maps; the committed schema and the reference page compared byte for byte).
- One way to do each thing: ok (one `{{ path }}` reader, one group membership walk, one "text or number" visitor, one drive-path parser, one closest-name helper, one `is_default`; a network block's text is `Display` on `validate::Cidr` and `Cidr6`, and plan, render, targets, import and the Terraform backend dropped their own `format!("{}/{}", ..)` copies).

## Syntax types: invalid states unrepresentable where practical
- Names and values parsed once: ok (`os`, port names, input names, services and file modes are newtypes: `OsRef`, `PortName`, `InputName`, `ServiceRef`, `Mode`, also the lowered secret's mode).
- One of several shapes is one enum: ok (a machine is exactly one `Kind`: service, app, bare; an app has one container `Recipe`: image, build, appliance, beside its steps; a file has exactly one `FileSource`; a step is one of five; a port's protocol is one `PortProtocol`; `publish` is one `Publish`; a TPM is one `Tpm`).
- A healthcheck: ok (exactly one `HealthProbe`, a `test` or an `http` page; both or neither is refused when the spec is read).
- Kept as written, by design: ok (rules that span fields need the machine's OS or kind, such as a `.ps1` script, a drive path, a service's port or packages on Windows, so they are checks with coded problems in `syntax::check`; the lab's `name`, network and machine names stay text, because their rules are checked once lowered, where the Kubernetes namespace limit and the clones are known; `EnvValue::Secret` is read only to say "secrets are files" precisely).

## 3b principles
- Names, one vocabulary, no stutter: ok (`isoloom_spec::{Spec, Error, load, parse}`; `syntax`, `model`, `validate`, `load`, `sizing`, `host`, `placeholders` by concept; `Rendered`, `Known`, `Lines` are private).
- Small functions: ok (every walk is bounded: a span's lookup takes one step per separator, the free-block scan is a range stepped by the block's size, so no input or mutant makes one endless).
- Few arguments, no flag arguments: ok (`defaults::load` takes one `Sources`).
- Single responsibility, small modules: ok (every file under 400 lines of code).
- Open/closed, Liskov, interface segregation, dependency inversion: ok.
- DRY: ok (no `dup/clone`).
- KISS and YAGNI: ok (the model's YAML reader, the address split and the OS fill served only tests: they live in `testing`; the lowered machine's `before`, always empty, is gone with its checks).
- Command-query separation, no hidden side effects: ok.
- Law of Demeter: ok.
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic`, indexing, slicing or `as` outside tests).
- Clean boundaries with third-party code: ok (serde, schemars and saphyr only at the types' edges).
- Clean tests (F.I.R.S.T., one concept each): ok.
- Comments say why: ok.
- Formatting: ok.
- No dead code: ok (the schema's "as written" types are the parser's intermediates, so none is unused).

## 3d Rust practices
- Workspace hygiene: ok.
- Common traits: ok.
- Conversions: ok (`FromStr` for every name, `OsRef`, `Mode`; `TryFrom` for the "as written" types; `From<BuildInput> for Build` and friends).
- Types carry meaning, `#[non_exhaustive]`: ok (`Error`, `Code`, `Step`, `PortProtocol`, `ProfileProblem` are non-exhaustive).
- Ownership: ok.
- `#[must_use]`: ok.
- Visibility: ok.
- Library errors with `thiserror`, `#[source]` and a `Result` alias: ok (I/O and YAML errors are sources).
- Error messages: ok (lowercase, no trailing period, the fix named).
- No panics in library code: ok.
- Sync or async, observability: ok (synchronous; logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok.
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (`OsRef`, names, base64, shell quoting, IPv4 blocks, the message's placeholders).
- Snapshots only for whole files: ok (the common mistakes of spec.md §9.2, 17 snapshots in `tests/problems.rs`, as the author reads them and as JSON; the schema is committed and compared whole).
- Fuzzing: ok (300 s each on the landed code (`d7f83f2c`): `spec_syntax` 303,506 runs, `spec_model` 124,741 runs, no crash).
- Mutation testing: ok (before, on the code the unit started from: 2167 mutants, 980 missed, 999 caught, 187 unviable, 1 timeout. After, on the final code, in two passes on friday (the first stopped at 1981 when friday's `/tmp` filled; the second, `--iterate`, took the rest and every mutant whose line moved): 1981 tested, 1761 caught, 219 unviable, 1 missed, 0 timeouts; then 282 tested, 267 caught, 14 unviable, 1 missed, 0 timeouts. The two left were then killed: a closed set's type is tested for each kind of first value, and an override's member check lost a clones arm no parsed spec reaches. Equivalent mutants are listed in `.cargo/mutants.toml` with their reasons: inlined schemas' names, a `visit_unit` that already returns the default, and bits joined where they can't overlap.)
- `#[expect]` only with a reason: ok (two: the lowered `Machine` and `Port` keep the backends' boolean switches one to one until they read the plan, spec.md §7.1 and §7.3 item 2).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok.
- Modules by domain concept: ok (`validate/mod.rs` became `validate.rs`).
- One `Error` in `error.rs` with a `Result` alias: ok.
- Tests next to the code, integration tests in `tests/`, shared helpers in `testing`: ok.
