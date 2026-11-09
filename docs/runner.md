# The run manifest and the runner

Isoloom converts a spec into files, and alongside each target's files it writes a **run
manifest** (`.isoloom/<target>/run.json`): a versioned, self-contained description of how to bring
the environment up, check it and tear it down. Isoloom runs nothing itself. The run manifest is
the contract between Isoloom and a runner.

The [Isothrum runner](https://github.com/Isothrum/isothrum) executes it. You can also run each
tool's files by hand (`docker compose up`, `kubectl apply -k`, `vagrant up`, `terraform apply`,
`ansible-playbook`); the manifest exists so a runner doesn't have to know each tool's commands.

## What the manifest holds

The run manifest is one `run.json` per target, holding only documented data — the driver does the
work:

- the **native commands** of the lifecycle and their order;
- **readiness criteria** (a ready file, running machines, a port answering);
- **outputs**: addresses, host ports and URLs, named per machine and port;
- how to run the **checks**, with self-contained timeouts (waits + 15 s per check + 60 s), so a
  long-waiting lab is never cut off by a fixed cap;
- **secret slots**: each secret's and input's name and destination, and an optional `default`,
  **never a value**.

The manifest is strict (`additionalProperties: false`) and carries its own version. Its JSON
schema is committed at [`schemas/run-manifest-v0.schema.json`](../schemas/run-manifest-v0.schema.json),
and `isoloom-manifest` holds the types and the fixtures both repositories test against.

## The lifecycle

The Isothrum runner drives one lifecycle over the manifest:

| Command | Does |
|---|---|
| `up` | Bring the environment up, following the manifest; fails before creating anything if a required tool is missing or too old |
| `check` | Run the checks from inside the environment |
| `status` | Report each machine's state |
| `park` | Stop the environment, keeping its state — `--mode shutdown` (default) or `pause` |
| `resume` | Bring a parked environment back |
| `provision` | Re-run a machine's set-up (VM targets) |
| `down` | Tear it down and remove its state |
| `events` | Stream the run's events |

The runner keeps each run's record, lock, event log and pinned host ports under its own home
(`~/.isothrum` for the CLI), never in the lab folder. A secret or input value reaches only the
steps its delivery names, and every sensitive value is masked (`***`) in events, logs and errors.

## The checks

`check` runs the manifest's `checks` in three phases, and a runner keeps to them:

1. **`checks.setup`**, in order: what the check runners need first, such as the stand-in for an
   access machine the lab brings up on demand (`stand-ins`: the checks that run from its seat run
   from the stand-in's addresses). A failed set-up step fails the checks: no runner runs, but the
   teardown still does.
2. **`checks.runners`**, in order: each runner's step, under its own time limit, prints its
   results in the `checks.protocol` line format. A runner cut off by its limit runs its
   `on_timeout` steps (the one-off container it leaves behind removed); the next runner still
   runs. A check its runner never reported fails.
3. **`checks.teardown`**, after the runners **whatever happened** — every check passed, one
   failed, a runner timed out, or the set-up failed. It takes the stand-ins down
   (`stand-ins-down`), so the access machine they stand in for can take its addresses when it is
   brought up. A teardown step is a clean-up: Isoloom marks it `failure: tolerate`, so its
   failure is logged and fails no check (a step's `failure` is `fail` unless it says so).

Compose in a VM (`docker-vm`, `cloud-docker`) has the same steps, each run in the VM through the
manifest's way in. The docker-vm Vagrantfile's `checks` provisioner, for running the checks without
a runner, does the same by itself: the stand-ins up, the runners, then the stand-ins down whatever
the runners found.

## Secrets and inputs

The spec names secrets and inputs and where each reaches a machine (usually a file); the manifest
turns each into a **slot** — a name and a destination, with an optional `default`, but no value.
At run time the runner reads each value from its store (a cloud secret store, the host
environment, or SSH guest delivery) and delivers it. Values never land in a generated file, the
run's state, an event or a log.

## The egress seal

For a cloud target, an `internet: false` network is sealed at the network level **after** the
machines are set up: the manifest carries an optional `seal` step that the runner applies once the
environment is ready (a second Terraform apply with the module's seal variable on). Egress to the
internet is then cut for those networks, while the runner's SSH and the published ports still
answer. The seal fails closed — a failed seal means the run is not ready — and every later
operation replays it, so a later apply can't silently undo it.

## Running it

```sh
isoloom generate            # writes .isoloom/<target>/{files…, run.json}
isothrum up                 # reads .isoloom/<target>/run.json (or pass --target)
isothrum check
isothrum down
```

The runner's own commands, options, event stream (version 0) and error codes are documented in
the [Isothrum repository](https://github.com/Isothrum/isothrum).

## See also

- [Spec reference](spec.md) — secrets, inputs, `files:` and checks.
- [Targets and tiers](targets.md) — each target's run manifest.
- [Getting started](getting-started.md).
