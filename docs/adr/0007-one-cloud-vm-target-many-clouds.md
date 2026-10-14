# 0007: One `cloud-vm` target, a cloud picked at run time

## Status

Accepted (#196).

## Context

`cloud-vm` writes one Terraform module per cloud (AWS, Azure, Google Cloud, DigitalOcean,
Linode, Oracle Cloud). Each cloud is a backend of its own with its own refusal, but AWS stood
for the target: AWS's refusal refused `cloud-vm`, while another cloud's refusal dropped only
its module. So a feature AWS can't do hid every other cloud's working module. IPv6 (#186) and a
TPM on Windows (#194) are written by the Azure module, yet no example generated it, because AWS
refuses both.

Two ways out were weighed:

1. **A target per cloud** (`aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci`). Each would
   refuse on its own. But it adds six targets to every list (`isoloom targets`, `targets:` in
   the spec, `resolved.json`, the coverage columns, the registry), moves the modules out of
   `.isoloom/cloud-vm/`, and leaves `cloud-docker`, which has the same six clouds, as the odd
   one out. Specs and tools that name `cloud-vm` would break.
2. **`cloud-vm` with a cloud picked at run time.** The modules stay where they are; generating
   writes every cloud that accepts the environment; the runtime already takes `--cloud` (as
   `cloud-docker` does) and records it.

## Decision

Option 2.

- `cloud-vm` is generated when **at least one** cloud accepts the environment. Each cloud that
  accepts gets its module under `.isoloom/cloud-vm/<cloud>/`; a cloud that refuses gets none.
  `cloud_vm::cloud_refusals` says which and why. The target is refused only when every cloud
  refuses it, with each cloud's reason (clouds sharing one named together), or when the plan
  needs what no cloud has.
- `resolved.json` (`targets.clouds.cloud-vm`) and `isoloom targets` list the generated and the
  refused clouds, as they did.
- `run`, `test`, `down`, `exec`, `connect` and `capture` work on one cloud: `--cloud` when
  given; else the one `run` recorded for the environment; else AWS when it accepts the
  environment; else the only cloud that does. When AWS refuses and several clouds accept,
  the command stops and names them (`--cloud gcp, --cloud digitalocean`). `run` with a cloud
  that refuses the environment stops with that cloud's reason.

## Consequences

- A refusal by one cloud never hides another cloud's module; coverage rows written only by
  a cloud other than AWS are proven by the examples (`dual-stack` on Azure, `mixed-office`'s
  Windows TPM on Azure).
- Specs, the registry, `resolved.json`'s shape and the module paths are unchanged. A run
  recorded before the cloud was recorded is still AWS's.
- Picking a cloud is the runtime's job: the default stays AWS where AWS can run the
  environment, so nothing runs on a cloud the user didn't ask for unless it is the only one.
