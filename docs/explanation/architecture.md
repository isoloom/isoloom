# Architecture

Isoloom is a converter. It reads one `isoloom.yml`, checks it, lowers each machine into a
target-neutral plan that carries both a container form and a VM form, and renders that plan into
each tool's own files plus a run manifest. It runs nothing: a runner, or you, runs the files.

```text
isoloom.yml ─► parse and validate ─► lower (plan) ─► backends ─► .isoloom/<target>/ + run.json
                (isoloom-spec)       (isoloom-plan,   (Compose, Kubernetes,
                                      the catalogue)   Vagrant, Terraform,
                                                       Ansible)
```

Three ideas shape it:

- **One declaration per machine.** A machine says what it is (an OS plus an `app:` or a catalogue
  `service:`), never how a platform runs it. Each target decides delivery, so the same spec gives
  a container on Compose and a VM on Proxmox, with the same behavior.
- **Behavior is the contract.** The spec's checks, and the ones Isoloom derives from ports,
  `reach` and `internet: false`, run from inside the environment on every target. The generated
  text may change between releases; what the checks prove may not.
- **Pure core, thin shell.** Parsing, validation, lowering and rendering are pure functions: no
  network, no clock, no environment. That makes generation [deterministic](determinism.md) and
  safe to run offline, and keeps every step unit-testable.

The crates, their dependency direction and the rules they keep: [ARCHITECTURE.md](../../ARCHITECTURE.md).
The decisions behind them: [architecture decision records](adr/README.md). The contract with a
runner: [the run manifest](../reference/run-manifest.md).
