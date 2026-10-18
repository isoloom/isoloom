# Architecture

Isoloom is a converter: it reads one `isoloom.yml` and writes the native files of every
infrastructure and configuration tool, plus a run manifest. It runs nothing. This document is the
map of how the pieces fit; the per-crate docs (`cargo doc`) are the detail.

## The pipeline

```
isoloom.yml ─► isoloom-spec ─► isoloom-plan ─► backends ─► native files
 (one form)     (parse, the     (lower os +     (compose,    (Compose, K8s, Helm,
                 model,          service/app     kubernetes,   Kustomize, Vagrant,
                 validate,       through the     vagrant,      Terraform, cloud-init,
                 the schema)     catalogue, the   terraform,    Ansible) + run.json
                                 environment)     ansible)
```

Supporting the backbone:

- **`isoloom-catalogue`** — packaged services (entries, release matching, typed `with:` settings,
  parity contracts), resolved during lowering.
- **`isoloom-render`** — the `Backend` trait and the builders every backend shares (HCL, Ruby,
  YAML, shell, PowerShell, Dockerfile), plus the configuration renderers and the shared container
  pieces.
- **`isoloom-manifest`** — the run manifest types, its generated JSON schema and the fixtures both
  Isoloom and Isothrum test against.
- **`isoloom-import`** — the reverse direction: a spec drafted from Compose, Vagrant or Terraform
  files.
- **`isoloom-targets`** — every backend behind one call, the composite targets, refusals, the
  resolved snapshot, the capability matrix and the tiers.
- **`isoloom`** — the command line.

## Lowering, not rewriting

The authored spec (`os` + `app:`/`service:`, no `docker:`/`vm:` blocks) is a front end. It lowers
each machine into a target-neutral plan that already carries both the container and the VM form,
and the backends render from that plan. Adding a spec feature means teaching the front end to
lower it; the backends keep working. Parity between a machine's container form and its VM form
holds by construction, and the catalogue's parity contract checks it for real.

## Functional core, imperative shell

Parsing, validation, lowering, rendering and manifest building are pure functions over data — no
I/O, no clock, no environment — so the core is unit-testable without mocks. All I/O sits in a thin
shell at the edge (the CLI, and `cargo xtask`). Output is deterministic: ordered maps everywhere
written, and a render-twice test.

## One-way dependencies

The crates sit in groups under `crates/` — `core/`, `backends/`, `tools/`, `targets/`, `cli/` —
and depend only downward: spec → plan → render → backends → targets → cli. No backend depends on
another backend; the targets made of two backends (`cloud-docker`, `docker-vm`, `hybrid`) are
composed in `isoloom-targets`. A test over `cargo metadata` fails on any upward or cross-backend
edge.

## Parse, don't validate; typed errors

Raw input is converted once into types that can't hold invalid states (newtypes for names, ports,
CIDRs, versions and IDs; enums instead of booleans and strings). Each library crate has one
`thiserror` error enum with a `Result` alias, variants carrying a `#[source]` cause and the field
path. No `unwrap`, `expect` or `panic` outside tests; no `anyhow` in libraries.

## Honest proof

Every generated file passes its format's official validator (`cargo xtask verify`, a pinned
container image). Each target sits in a [tier](docs/matrix.md) worked out from recorded real runs,
never written by hand. The capability matrix (`isoloom matrix`) is generated from the generators
themselves, so it can never claim more than they do.

## The three products

Isoloom is one of three products that ship 1.0 together: Isoloom converts, the **Isothrum runner**
executes the run manifest, and the **Cyber CTF launcher** is the UI. The run manifest is the
contract between them. See the [run manifest guide](docs/runner.md).

## Decisions

Architecture decision records live in [`docs/adr/`](docs/adr/).
