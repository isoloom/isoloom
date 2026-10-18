# Review: isoloom, the command line (Phase 1c)

The `isoloom` binary: the command line, one module per subcommand, and how an error reads and
exits. One line per 3b principle and 3d item: `ok`, or the finding and its fix.

## 3b design rules
- Functional core, imperative shell: ok (the commands call the pure library crates and write through one `Console`; files are read and written only in the commands).
- Ports and adapters: ok (external tools, `d2`, `dot` and `vagrant`, are run in one place each; a missing tool is `ToolMissing` with its install page).
- One-way dependencies: ok (`isoloom-spec`, `isoloom-targets`, `isoloom-import`; nothing depends on the CLI).
- Parse, don't validate: ok (the Vagrantfile recorder's output is parsed into a `Recording` at the edge; a command's outcome is a `Verdict`, not a bool or an exit number).
- Typed errors: ok (`CliError` in `error.rs`, every cause behind `#[source]` or `transparent`; no `String` errors carrying a formatted cause).
- No hidden state: ok.
- Determinism: ok (the example snapshots are byte-identical).
- One way to do each thing: ok (one error rendering, `diagnostic::render`; one exit-code mapping, `exit::code`; one console; one way to load and check a spec, `spec::checked`).

## 3b principles
- Names, one vocabulary, no stutter: ok.
- Small functions: ok.
- Few arguments, no flag arguments: ok (the shared `Project` and `Listing` arguments are flattened into each command).
- Single responsibility, small modules: ok (`output.rs` split into `console`, `diagnostic` and `exit`; `import` into its sources, `Dest` and `Source`; every file under 400 lines).
- Open/closed, Liskov, interface segregation, dependency inversion: ok.
- DRY: ok (zero duplicate blocks).
- KISS and YAGNI: ok.
- Command-query separation, no hidden side effects: ok (a command returns its `Verdict`; `main` alone prints the error and picks the code).
- Law of Demeter: ok.
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic`, indexing or string slicing outside tests; a closed stdout is an `Output` error, not a panic).
- Clean boundaries with third-party code: ok (clap only in `cli` and the commands' argument types).
- Clean tests (F.I.R.S.T., one concept each): ok (one binary, `tests/cli`, by concept: commands, errors, imports, secrets; helpers in `testing` return a `Result`; the example snapshots are their own binary).
- Comments say why: ok.
- Formatting: ok.
- No dead code: ok.

## 3d Rust practices
- Workspace hygiene: ok.
- Common traits: ok (`Verdict`: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`).
- Conversions through `From`/`TryFrom`: ok (`LoadError`, the targets' `Error`, the import's `Error` and I/O into `CliError`; an invalid spec becomes `InvalidSpec` with its problems).
- Types carry meaning, `#[non_exhaustive]`: ok (the binary's types are private, so none is needed).
- Ownership: ok.
- `#[must_use]`: ok.
- Visibility: ok (66 unreachable `pub` became private or `pub(crate)`).
- Errors with `thiserror`: ok (the binary uses no `anyhow` either).
- Error messages: ok (lowercase, no trailing period, the cause in `source()` and printed as `caused by:`).
- No panics: ok.
- Sync or async, observability: ok (synchronous; it prints only through `Console`).
- Crate docs: ok (a crate doc naming the modules; a binary has no doc examples to run, so the commands are run by the integration tests).
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (the parsing and quoting the CLI relies on is in `isoloom-spec` and `isoloom-import`, with their proptests).
- Snapshots only for whole files: ok (the generated files of every example).

## 1c organization
- `main.rs` holds the module tree and the shell around `run`: ok.
- Modules by concept: ok (`cli`, `commands`, `spec`, `target`, `generated`, `console`, `error`, `diagnostic`, `exit`).
- One error in `error.rs` with a `Result` alias: ok.
- Tests next to the code, integration tests in `tests/`, shared helpers in `testing`: ok.

## Errors and exit codes
- Every error prints `error: <message>`, then one `  caused by: <cause>` per cause in the chain, a cause already in its parent's message skipped: ok (tested for an unreadable file, a non-Compose file and a missing tool).
- An invalid spec prints one `✗ <field path>: <reason>` line per problem, with the fix where there is one, then `error: fix the spec first`: ok (tested on every command that reads a spec).
- Exit codes in one place: 0 passed, 1 failed (a check that failed, an invalid spec), 2 any other error: ok (tested).
