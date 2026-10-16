# 0005 — Coverage is the generated matrix and the tiers

**Status:** Accepted (2026-10).

## Context

Isoloom used to carry `isoloom-coverage` — a 13k-line crate with 30 hand-maintained format
tables, refresh scripts and a drift job, plus a `COVERAGE.md` page. Hand-maintained tables drift
from what the generators actually do, and a doc that claims more than the code is worse than no
doc.

## Decision

Coverage is **generated from the code**, in two parts:

1. **The capability matrix** (`isoloom matrix`, [`docs/matrix.md`](../matrix.md)) — every spec
   field against every target, each cell the generator's own decision: ✓ generated, ✗ refused
   (with the step that refuses and the reason), – not applicable. Tests walk every example on
   every column and require each refusal a generator gives to appear in the matrix with the same
   step and reason, so the matrix can't claim more than the generators.
2. **Tiers** (`tiers.json`, shown in the matrix and by `isoloom targets`) — worked out from
   recorded real runs, never written by hand. See [targets and tiers](../targets.md).

Conformance to each format's official validator is checked separately by `cargo xtask verify` (a
pinned container image) over every generated file.

## Consequences

- There is no `COVERAGE.md` and no `isoloom coverage` command; the matrix replaces both. The
  matrix is the per-field × per-target coverage, generated and stale-tested.
- "Supported" means Tier 1, or Tier 2 with the evidence named — never an unbacked claim.
- The spec-field capability declarations now live in the backends themselves, where the matrix
  reads them.
