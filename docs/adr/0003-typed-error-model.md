# 0003 — Typed, actionable errors

**Status:** Accepted (2026-10).

## Context

A converter is only useful if a rejected spec tells the author exactly what is wrong and how to
fix it. String errors and `Box<dyn Error>` lose the field path and the cause chain, and they can't
be mapped to a stable exit code.

## Decision

- Each library crate has **one `thiserror` enum** with a `pub type Result<T>` alias; variants
  carry their `#[source]` cause, so the chain is never lost. No `anyhow` or `String` errors in
  libraries (`anyhow` only in binaries and `xtask`).
- A spec problem carries a **code** (`unknown-field`, `removed-field`, `missing-field`,
  `invalid-value`, `conflict`, `unknown-reference`, `sensitive-value`, `refused`), the **field
  path as written**, the **line and column**, the **reason** and a **fix**. The CLI prints
  `✗ error[<code>] <path>  (isoloom.yml:<line>:<column>)`, the reason, then `fix: …`; `--json`
  gives `{ code, path, line, column, reason, fix }`.
- **Exit codes** are stable: `0` the answer is yes; `1` the answer is no (the spec has problems,
  files drift, a target refuses the spec, a check failed); `2` the command couldn't do its job (a
  file it can't read, a name it doesn't know, a tool missing, a usage error).
- No `unwrap`, `expect` or `panic` outside tests; an impossible state is an `#[expect(…, reason)]`
  or, better, a type that can't represent it.

## Consequences

- An old-format spec fails with a problem that names the removed field and its new form, rather
  than a vague parse error.
- Tools can branch on the exit code and the JSON problem list.
- The restriction lints (`unwrap_used`, `expect_used`, `panic`, …) are at deny, so the rule is
  enforced, not just intended.
