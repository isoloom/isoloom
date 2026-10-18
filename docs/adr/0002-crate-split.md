# 0002 — The crate split and one-way dependencies

**Status:** Accepted (2026-10).

## Context

Isoloom grew a runtime, a coverage crate and a facade that re-exported everything, so crates
reached sideways and upward and the public surface was large and accidental. 1.0 publishes the
crates to crates.io, so the boundaries now matter to consumers.

## Decision

Crates are grouped under `crates/` by role and depend only downward:

```
core/      isoloom-spec, isoloom-manifest, isoloom-catalogue, isoloom-plan, isoloom-render
backends/  isoloom-compose, isoloom-kubernetes, isoloom-vagrant, isoloom-terraform, isoloom-ansible
tools/     isoloom-import
targets/   isoloom-targets
cli/       isoloom
```

The order is spec → plan → render → backends → targets → cli. No backend depends on another
backend; the targets made of two backends (`cloud-docker`, `docker-vm`, `hybrid`) are composed in
`isoloom-targets`. The runtime crate was removed (running is the Isothrum runner's job), and
`isoloom-external` became `isoloom-ansible` (the backend that renders the `external` target).

## Consequences

- A test over `cargo metadata` (`cargo test -p xtask`, module `deps`) fails on any upward or
  cross-backend edge, so the direction can't erode.
- `pub(crate)` is the default (`unreachable_pub`); every `pub` item is deliberate and documented.
- All thirteen crates publish at one workspace version under Apache-2.0, so a consumer can depend
  on exactly the crates it uses.
