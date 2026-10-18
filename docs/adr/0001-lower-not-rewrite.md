# 0001 — Lower the new spec, don't rewrite the backends

**Status:** Accepted (2026-10).

## Context

1.0 introduces one spec form — a machine is an `os` plus an `app:` or a catalogue `service:`, with
no `docker:`/`vm:` blocks. The backends (Compose, Kubernetes, Vagrant, Terraform, Ansible) already
render a target-neutral plan that carries both a container and a VM form, guarded by a large test
suite. Rewriting the backends to read the new spec directly would throw that away.

## Decision

The new spec is a **front end**. `isoloom-spec` parses and validates the authored form;
`isoloom-plan` **lowers** each machine into the existing plan, resolving `os` + `service`/`app`
through the catalogue and the environment, with refusals reported at the written field path. The
backends keep rendering the plan unchanged.

## Consequences

- Adding a spec feature means teaching the front end to lower it; the backends and their tests
  keep working.
- A single machine declaration produces both forms, so parity between the container and the VM
  form holds by construction, checked for real by the catalogue's parity contract.
- Refusals are precise: a target with no suitable form names the field that rules it out (for
  example `machines.web.app.build: a Dockerfile builds a container only`).
- The old runtime and the per-format coverage tables are gone; see
  [0005](0005-coverage-methodology.md).
