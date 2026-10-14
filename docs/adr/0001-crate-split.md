# 0001: Split isoloom-core into crates by layer and backend

## Status

Accepted (Wave 1, unit 4; merged as #164).

## Context

Until 0.10 everything but the command line lived in one crate, `isoloom-core`: the spec
model, validation, every generator, the importers, the coverage tables and the runtime
registry. Any change rebuilt and retested all of it, module boundaries were conventions only
(a backend could reach into another backend's helpers, and several did), and the crate's
public surface mixed the supported library API with internals the CLI needed.

## Decision

Split by pipeline layer and by backend, with dependencies pointing one way:

- `isoloom-spec`: model, parsing, normalization, validation. Depends on no other crate of ours.
- `isoloom-plan`: the target-neutral plan (checks today, the IR of ADR 0002 next).
- `isoloom-render`: the pieces every backend renders with.
- One crate per backend family: `isoloom-compose`, `isoloom-kubernetes`, `isoloom-vagrant`,
  `isoloom-terraform`, `isoloom-external`.
- `isoloom-targets`: dispatch, refusals, the resolved snapshot.
- `isoloom-import`, `isoloom-coverage`, `isoloom-runtime`: the side concerns.
- `isoloom-core`: a facade that re-exports the supported surface, so tools embedding Isoloom
  keep one dependency.
- `isoloom`: the command line.

The split was mechanical: no logic change, and every example's generated output stayed byte
identical.

## Consequences

- A backend can only use what `isoloom-render` exports: shared helpers are visible, and a
  helper copied between backends shows up in review.
- Crates build and test in parallel; a change to one backend doesn't rebuild the others.
- Lints, coverage floors and docs apply per crate (ADR 0004, CONTRIBUTING.md).
- `isoloom-core` stays the compatibility point for embedders; internal crates may change in
  any release.
- Moving code between crates needs a facade re-export when it was part of the supported surface.

## Update: the facade is gone (Wave 2)

Once every backend rendered the plan, `isoloom-core` re-exported nothing of its own and hid
which crate an item lives in. It was removed: the command line and embedders depend on the
crates themselves (the supported surface is in `isoloom-spec`, `isoloom-plan`'s `checks`,
`isoloom-targets` and `isoloom-runtime`; README, "Using the library"), and its integration tests
moved into the crates they test (the generators over the examples in `isoloom-targets/tests`).
