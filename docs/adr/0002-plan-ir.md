# 0002: A typed plan IR between the spec and the backends

## Status

Accepted; being built in Wave 1 unit 6, with every backend moved onto it in Wave 2.

## Context

Each generator reads the spec directly and decides on its own how to set up a machine: the
host entries, routes, trunks, volumes, files, the project copy, waiting for dependencies,
egress rules, traffic shaping. The same lifecycle is written once per backend (Vagrant shell
provisioners, cloud-init, Terraform `remote-exec`, WinRM), so a fix lands in one target and
not the others. Whether a target can run a spec is also decided per generator, which makes
refusals inconsistent.

## Decision

Introduce a typed intermediate representation in `isoloom-plan`:

- **`Environment`**: `networks` (lab and Docker blocks, router and controller, gateway,
  offline, traffic control, VLANs and trunks, IPv6, external) and `machines` (shape,
  addresses, names, routes, gateway, published ports, volumes `Own` / `Shared` / `Persistent`,
  files, command, settings, inputs and secrets, `Size`, arch, image and boot, devices), plus
  `start_order`, `checks`, `controller`, `constraints` and `requirements`.
- **`MachineLifecycle = Vec<Step>`** with the steps `Prelude`, `DiskGrow`, `Settings`, `Files`,
  `Hosts`, `Routes`, `Trunks`, `Volumes`, `SharedVolume`, `Project`, `Inputs`, `Before`,
  `WaitFor`, `Run`, `Reboot`, `Egress`, `Tc`, `Ready`. Each step has exactly one Linux and one
  Windows rendering. Backends only choose the transport (Vagrant provisioners, cloud-init,
  `remote-exec`, WinRM).
- **`Backend`**: `capabilities()` plus `render(&Environment) -> Result<Files, RenderError>`.
  Every cloud driver is a backend of its own. A refusal is the plan's requirements minus the
  backend's capabilities, computed without generating anything.
- `resolved.json` v2 is the serialized plan.

Existing code is reused, not rewritten: `checks::plan`, `machine_setup::{linux, windows}`
(they become steps), `constraints::linux_prelude` (the `Prelude` step), the router, trunk and
appliance code, and `cloud_vm::linux_setup_cmds` as the seed of the step list. A
`LegacyBackend` adapter runs the current generators until each one moves.

## Consequences

- Each lifecycle concern exists once; a fix reaches every target.
- New targets are mostly a transport and a capability list.
- Golden output changes as backends migrate: each change is reviewed as a snapshot diff and
  explained in its pull request.
- Two paths coexist during Wave 2; the adapter is removed when the last backend moves.
