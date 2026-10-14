# 0005: How coverage is measured

## Status

Accepted; the map is rebuilt as "coverage map v2" in Wave M.

## Context

"Isoloom supports Vagrant" says nothing about which Vagrant settings it writes. Users need to
know what each output can and can't express, and the project needs a number that only goes up
when real support is added, not when a table is edited.

## Decision

**What belongs in the format:** a machine feature the VM targets can do (local VMs, Proxmox,
cloud VMs). Containers are the lightweight option, used when they produce the same machine; a
feature they can't produce makes an environment VM-only rather than staying out. Container
mechanics (capabilities, cgroups) are not machine features.

**Tables:** one `Format` per output (`crates/isoloom-coverage`), listing every feature of that
format from a pinned machine-readable source: the compose-spec schema, Vagrant's config classes,
each Terraform provider's resource list (`crates/isoloom-coverage/coverage/`). Each feature is
classified as `Emitted`, `Partial`, `Equivalent`, `InImage`, `Planned`, `NotPortable` or
`Tooling`. `Unclassified` fails the tests. A spec-side table maps every spec field to what each
output does with it.

**The number:** portable features count (not `Tooling`, not `NotPortable`); done ones count
fully, partial ones half. 100% means every portable feature is produced.

**Truth is tested** (`crates/isoloom-coverage/tests/coverage.rs`): the rows are exactly the
source's features; every feature a generator writes for a committed example is marked as
written, and every row marked as written is written by some example; every spec field has a
row; `docs/COVERAGE.md` is exactly what `isoloom coverage --markdown` prints.

**v2 (Wave M)** adds:

- Depth: nested keys (Compose `deploy.*`, `healthcheck.*`, long-form ports; Vagrant network,
  provisioner, disk and synced folder options; every argument of every Terraform resource emitted).
- Completeness: new tables from pinned sources (Kubernetes OpenAPI per kind, Kustomize, Helm,
  cloud-init, Ansible inventory, Dockerfile, and each new target); the spec table gets a column
  per output.
- Truth: a written-features test against every output using a format, including docker-vm,
  hybrid and every Terraform module; cloud resources classified explicitly, never by default.
- Currency: a refresh script per source, run weekly in CI, opening an issue on upstream changes.

## Consequences

- A feature can't be claimed without an example exercising it.
- Upstream additions show up as unclassified rows and failing tests, not silent gaps.
- The debatable rows (about 22) are re-decided under these rules in Wave M; some become features.
