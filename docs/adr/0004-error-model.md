# 0004: Typed errors per library crate

## Status

Accepted; started in Wave 1 unit 5, completed crate by crate through Wave 2.

## Context

Most library functions return `Result<_, String>`, and the command line wraps everything in
`Box<dyn Error>`. A caller can't tell a missing file from an invalid field without matching on
message text, embedders (the launcher) can't react to a specific failure, and messages vary in
whether they name the field, the file or the fix.

## Decision

- Each library crate has its own error enum, derived with `thiserror`.
- A variant carries the data needed to act on it: the file path, the spec field path
  (`machines.web.networks`) and the reason. Underlying errors are kept as `#[source]`, not
  flattened into text.
- Messages are actionable: they say what is wrong and, where it is known, how to fix it
  ("both isoloom.yml and isoloom.yaml exist in DIR; keep one").
- Validation keeps returning every problem at once (`Vec<Problem>`, each with `at` and
  `message`) rather than stopping at the first.
- The command line converts library errors with `?` and `From`, and maps errors to exit
  codes in one place.
- New code adds no `String` errors. `unwrap`, `expect` and `panic` are denied outside tests;
  `expect_used` is tracked until the generators no longer `expect` on a validated spec.

The command line's `--json` output turns any error into one document with a stable code
(`isoloom_runtime::events::ErrorCode`, listed in `docs/json.md`): its own errors carry their
code (`Failure` in `crates/isoloom/src/failure.rs`), the library's get one from their type.

Today: `isoloom_spec::LoadError` (not found, ambiguous, read, YAML, normalization, profile),
`isoloom_spec::Problem` for validation, `isoloom_import::ImportError`, and the runtime's
`RegistryError`, `ReportError` and `HostError`. The remaining `String` errors (defaults,
profiles, the image table, instances, `GenerateError::Unsupported` reasons, the command line's
own helpers) are tracked in issue #106 and converted as their crates are touched.

## Consequences

- Embedders match on variants instead of text.
- Error text stays stable where users see it: converting a crate keeps its messages unless the
  pull request says why they changed.
- Each crate's error type is part of its public API: a new variant needs `#[non_exhaustive]`
  or a major bump (a minor one before 1.0).
