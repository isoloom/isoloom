# Review: isoloom-kubernetes (Phase 1c)

The `kubernetes` target's backend: plain manifests with their kustomizations, the checks' Jobs,
and a Helm chart of the same objects. One line per 3b principle and 3d item: `ok`, or the finding
and its fix commit.

## 3b design rules
- Functional core, imperative shell: ok (rendering is pure; `project_copies` is the one function that reads the project folder, documented as the shell).
- Ports and adapters: ok (implements the `Backend` port of `isoloom-render`; no tool is run).
- One-way dependencies: ok (spec, plan, render only; no other backend).
- Parse, don't validate: ok (typed objects with typed Helm values, `Text` and `Image`; a resolver option converts `From<&str>`; a machine is a `Machine` view of name, spec, plan and image).
- Typed errors: ok (`Error` in `error.rs` with `#[source]`, mapped once into `RenderError`; it replaces `ProjectCopyError` and the scattered `RenderError` builders, the messages unchanged).
- No hidden state: ok.
- Determinism: ok (ordered maps throughout; `rendering_twice_gives_the_same_files`).
- One way to do each thing: ok (the YAML writers are `render::document` and `documents`; the target's paths are `layout`; the GPU refusal iterates the `Machine` view instead of copying Terraform's loop).

## 3b principles
- Names, one vocabulary, no stutter: ok (`KubernetesBackend` keeps its name beside the `Backend` trait and the registry's other backends).
- Small functions: ok.
- Few arguments, no flag arguments: ok.
- Single responsibility, small modules: ok (the backend and its capabilities, the error, the layout and the project copies left `lib.rs`; every file is under 300 lines of code).
- Open/closed, Liskov, interface segregation, dependency inversion: ok.
- DRY: ok (no `dup/clone`; the integration tests no longer repeat the capability list, and the own-process spec is a fixture file).
- KISS and YAGNI: ok (the public API is the backend, `project_copies`, the `SECRETS` name the runner creates, and the error; `reason`, `render` and every object type became crate-private).
- Command-query separation, no hidden side effects: ok.
- Law of Demeter: ok.
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic`, indexing or string slicing outside tests).
- Clean boundaries with third-party code: ok (`serde_yaml_ng` and `serde_json` write the typed objects and the values schema).
- Clean tests (F.I.R.S.T., one concept each): ok (the DNS tests split by concept; integration tests return a `Result` instead of allowing `unwrap`).
- Comments say why: ok.
- Formatting: ok.
- No dead code: ok.

## 3d Rust practices
- Workspace hygiene: ok (the crate's own `#![deny]` list is gone: the workspace's lint table covers it).
- Common traits: ok (`KubernetesBackend`: `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`, `Hash`; `Error`: `Debug`).
- Conversions through `From`: ok (`From<Error> for RenderError`, `From<&str> for DnsOption`).
- Types carry meaning, `#[non_exhaustive]`: ok (`Error` is non-exhaustive).
- Ownership: ok.
- `#[must_use]`: ok (the public functions return `Result`).
- Visibility: ok (`pub(crate)` everywhere but the API).
- Library errors with `thiserror`, `#[source]` and a `Result` alias: ok (the project-copy variants keep their cause in their message too, because the command line prints only the top message).
- Error messages: ok (lowercase, no trailing period).
- No panics in library code: ok.
- Sync or async, observability: ok (synchronous; it logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok.
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (`go_string`, now total: a control character is an escape, never a raw line break; Helm's brace escaping; the size quantities; the subdomain label; the kustomization's scripts read back).
- Snapshots only for whole files: ok (the manifests and the chart; the refusal's message).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok.
- Modules by domain concept: ok.
- One `Error` in `error.rs` with a `Result` alias: ok.
- Tests next to the code, integration tests in `tests/`, fixtures in `tests/fixtures/`, shared helpers in `testing`: ok.
