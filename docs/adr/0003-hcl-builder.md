# 0003: Build HCL and Ruby with builders, not format strings

## Status

Accepted. Built in Wave 2 (unit 10): `isoloom_render::hcl_builder`, used by Proxmox and AWS; the
other Terraform backends and Vagrant follow.

Two findings from building it: `hcl-rs` doesn't align attributes as `terraform fmt` does, so the
builder writes the layout itself (leaf expressions are still written by `hcl-rs`); and its
string escaping leaves an already escaped `$${` alone, so the builder escapes strings itself, with
a property test against HCL's template scanning rules.

## Context

The Terraform backends write HCL with `format!` and string pushes, escaping values through
`isoloom_render::hcl`. The Vagrant backends do the same for Ruby, and each of `vagrant.rs`,
`hybrid.rs` and `docker_vm.rs` has its own copy of the Ruby quoting helper `rb`. Text built
this way is easy to get subtly wrong (a missing escape, a brace left open, indentation that
drifts between modules), hard to test except as whole files, and impossible to inspect as
structure.

## Decision

- HCL is built as `hcl-rs` structures (`Body`, `Block`, `Attribute`, expressions) and
  serialized once. Values go in as values, so escaping is the library's job, not ours.
- Ruby for Vagrantfiles goes through one builder in `isoloom-render`: one quoting helper, one
  place that knows how a `config.vm` block nests.
- YAML outputs (Compose, Kubernetes, Helm) are serde structs, as they mostly are today.
- Until a backend migrates, its remaining format strings use the shared quoting helpers only.
  No copy of a helper is added.

## Consequences

- Escaping bugs disappear by construction for migrated outputs; the quoting helpers that
  remain are covered by property tests (round trip and injection safety).
- Generated HCL formatting follows `hcl-rs`. The first migration changes whitespace in the
  committed examples, reviewed once as a snapshot diff.
- Backends can be tested on structure (does this block have that attribute) as well as text.
- The "no `format!`-built HCL or Ruby outside the builders" rule is checked at the end of Wave 2.
