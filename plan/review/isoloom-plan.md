# Review: isoloom-plan (Phase 1c)

The target-neutral plan: the authored spec lowered per target (`Lab`, `lower`), the
`Environment` a backend renders, the checks resolved to positions and probes, and the
requirements a backend must meet. One line per 3b principle and 3d item: `ok`, or the finding
and its fix commit.

## 3b design rules
- Functional core, imperative shell: ok (every function is pure over the spec; the one I/O edge is `Lab::problems_with_files`, which looks at the project folder it is given, as the crate docs say).
- Ports and adapters: ok (a target writes a check runner through `checks::Render`, its own way of reaching a host and running a script; nothing here runs a tool).
- One-way dependencies: ok (isoloom-spec only).
- Parse, don't validate: ok for what the plan owns (a machine's forms resolve once into typed `lower::forms::Container` and `Vm`, carrying the OS and its pinned image, read by lowering and the refusals alike, 83e479c4). Two string boundaries remain at isoloom-spec's API, follow-ups for that crate: the model validation's problems carry no code, so `lab::code_of` reads it from the message; `vlans::flatten` fails with a string, so `lower::stage_problem` splits its path off.
- Typed errors: ok (nothing here fails: a spec's problems and a target's refusals are data, `Problem`s at the authored field; so there is no `Error` enum or `Result` alias, by design).
- No hidden state: ok (`Lab`'s memo of each lowering is a cache of pure results, no part of its value).
- Determinism: ok (ordered maps throughout).
- One way to do each thing: ok (a machine's forms through `lower::forms`; the controller's size once, `Controller::of`, 0b9af5d4; each refusal message once, in its concept's module, 83e479c4).

## 3b principles
- Names, one vocabulary, no stutter: ok.
- Small functions: ok (all under 60 lines; `Environment::build` split into `machines` and `all_requirements`).
- Few arguments, no flag arguments: ok.
- Single responsibility, small modules: ok (by concept; the largest file has 389 lines of code; big test modules in `foo/tests.rs`).
- Open/closed, Liskov, interface segregation, dependency inversion: ok.
- DRY: ok (no `dup/clone`; unit tests share `testing`, integration tests `tests/testing`).
- KISS and YAGNI: ok (a machine's `before` scripts, which the spec doesn't have, went with every renderer of them, 5ffae57b; plan-internal functions are `pub(crate)` and an unused constant went, 88d2e0c2; a redundant shared-volume guard went, 2cbd231d).
- Command-query separation, no hidden side effects: ok.
- Law of Demeter: ok.
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic`, indexing or slicing outside tests: a lookup into a validated spec is a `get` with a documented answer; a dependency cycle no longer recurses forever in `start_order`, 83e479c4).
- Clean boundaries with third-party code: ok.
- Clean tests (F.I.R.S.T., one concept each): ok (integration tests return an `Outcome` instead of allowing `unwrap`).
- Comments say why: ok.
- Formatting: ok.
- No dead code: ok.

## 3d Rust practices
- Workspace hygiene: ok (proptest from the workspace).
- Common traits: ok (`Debug`, `Clone`, `PartialEq` on every public type; `Eq`, `Copy`, `Hash` where they hold).
- Conversions through `From`: ok (none needed: the plan's inputs are the spec's types).
- Types carry meaning, `#[non_exhaustive]`: ok (`Feature`, `Requirement`, `Need`, `Hardware`, `Lowered`, `PrivateDns`, `Record`, `Controller`; the check runner's `Line` stays exhaustive: it is the versioned `isoloom-check/1` protocol).
- Ownership: ok.
- `#[must_use]`: ok (every pure public function).
- Visibility: ok (no `unreachable_pub`).
- Library errors with `thiserror`, `#[source]` and a `Result` alias: ok (no error type: see Typed errors).
- Error messages: ok (refusal reasons lowercase, no trailing period; every one tested).
- No panics in library code: ok.
- Sync or async, observability: ok (synchronous and pure; it logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok.
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok (every surviving mutant killed by one: 7ead2341, 56d8ce9e, 95a8d5a1 and after).
- Property tests for parsers and quoting: ok (`checks::parse_line`, `links::megabytes_per_second`, `images::rewrite_image`, `Trail::path`).
- Snapshots only for whole files: ok (the examples' lowered specs, whole).
- Mutation testing: ok (before, 650 mutants: 248 caught, 276 missed, 2 timeouts, 124 unviable; every survivor since killed by a behaviour-named test, but one equivalent mutant listed in `.cargo/mutants.toml` with its reason, 70ebefeb: `|` and `^` in `addressing::address`. The timeouts were test binaries stalled by the machine's load, never a loop: the floor is five minutes, and the plan's one recursion, `start_order`, is bounded by the machines it visits, 83e479c4).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok.
- Modules by domain concept: ok.
- One `Error` in `error.rs` with a `Result` alias: ok (none needed: see Typed errors).
- Tests next to the code, integration tests in `tests/`, shared helpers in `testing`: ok.

## Follow-ups for isoloom-spec (the tmp/1c-spec unit)
- A model validation `Problem` with a typed code, so `lab::code_of` stops reading its message.
- `vlans::flatten`'s error as a type with the field path, so `lower::stage_problem` stops splitting it.
- The model's `Machine.before` field, which nothing sets or reads any more.
