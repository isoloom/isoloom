# Migrating to 1.0

Isoloom 1.0 is a ground-up rebuild. It supports **only** the 1.0 spec format and the
1.0 run-manifest contract.

**There is no backward compatibility.** The project was never past preview, so 1.0 ships
clean:

- No field aliases and no deprecation window.
- **No `isoloom migrate` command.** Old specs are rewritten by hand.
- An old spec fails `isoloom validate` with an error naming the unknown field.

This guide lists every breaking change, with before/after taken from the repository's own
examples. The spec file version itself is unchanged — it was `version: 1` in 0.x and
stays `version: 1` in 1.0, because 1.0 is the first *stable* form of that version.

## 1. One form per machine: no `docker:` / `vm:` blocks

A machine used to carry a `docker:` block and a `vm:` block, each describing how a
platform runs it. In 1.0 a machine declares *what* it is, once, and each target decides
how to deliver it. Use a single `app:` block (a custom image or build recipe) or a
`service:` from the catalogue, plus `os:` for the base, and `setup:` for seed steps.

**Before** (`examples/hello-stack`, 0.x):

```yaml
cache:
  os: debian@12
  networks: { app: 20 }
  ports: [{ port: 6379, name: redis }]
  volumes: { data: /data }
  docker:
    image: "redis:7-alpine"
    init: [build/cache/seed.sh]
  vm:
    provision: [provision/cache.sh]
```

**After** (1.0):

```yaml
cache:
  os: debian@12
  networks: { app: 20 }
  ports: { redis: 6379 }
  volumes: { data: /data }
  app:
    image: "redis:7-alpine"
    steps: [{ run: provision/cache.sh }]
  setup: [build/cache/seed.sh]
```

Mapping:

| 0.x | 1.0 |
|---|---|
| `docker.image` / `docker.build` | `app.image` / `app.build` |
| `docker.init: [...]` (seed once) | `setup: [...]` |
| `vm.provision: [...]` | `app.steps: [{ run: ... }]` |
| `docker:` + `vm:` (two recipes) | one `app:` block, rendered per target |

For a stock service, prefer the catalogue over a raw image:

```yaml
store:
  os: debian@12
  service: redis          # a container image on container targets, a role on VM targets
```

A `service:` can take typed settings through `with:`.

## 2. `ports:` is a name-keyed map

Ports were a list of objects with a `name`. In 1.0 they are a map keyed by that name, and
`http: true` becomes `protocol: http`.

**Before:**

```yaml
ports: [{ port: 80, http: true, publish: 8080 }]
ports: [{ port: 6379, name: redis }]
```

**After:**

```yaml
ports: { web: { port: 80, protocol: http, publish: 8080 } }
ports: { redis: 6379 }
```

A port's name is now used everywhere downstream — Compose labels, Terraform outputs, the
resolved snapshot, the run manifest (`${published.<machine>.<name>}`), and message
placeholders. Message interpolation changes accordingly:

**Before:** `http://localhost:{{ machines.web.ports.0.publish }}/`
**After:**  `{{ machines.web.ports.web.url }}`

## 3. Inputs and secrets: `inputs:` + `files: [{ input }]`

Values given at each start (a flag, a launch token) are declared under `inputs:` and are
**sensitive by default**. A sensitive input reaches a machine as a *file*, never as an
environment variable. A non-secret input can be placed in `environment:` explicitly.
Values never appear in a generated file.

**After** (`examples/supplier-portal-api`):

```yaml
inputs:
  API_URL: { sensitive: false, description: "The API the portal calls." }
  LAUNCH_TOKEN: { description: "A token issued at launch." }

machines:
  database:
    environment:
      API_URL: { input: API_URL }          # not secret → may be an env var
    files:
      - { path: /run/lab/launch-token, input: LAUNCH_TOKEN }   # secret → a file
    setup: [build/database/seed.sh]         # reads /run/lab/launch-token
```

If a 0.x lab delivered a flag or token through the environment, move it to a
`files: [{ path, input }]` entry and read it from that path.

## 4. The egress seal

An `internet: false` network is a real guarantee in 1.0. On cloud targets it is sealed at
the network level after the machines are set up (a `seal` step in the run manifest that
the runner applies, failing closed). Locally, a privileged container can still lift the
seal from inside its namespace, so one is **refused** on an `internet: false` network
unless it opts in:

```yaml
machines:
  jail:
    privileged: true
    seal: best-effort     # acknowledges the seal is advisory for this machine
```

`seal:` defaults to `guaranteed`; set `best-effort` only for a privileged container that
must sit on an offline network.

## 5. The run manifest: `lab:` → `environment:`

If you consume the run manifest (`run.json`) directly, its top-level provenance key was
renamed:

**Before:**

```json
{ "lab": { "name": "goat", "instance": null, "message": null } }
```

**After:**

```json
{ "environment": { "name": "goat", "instance": null, "message": null } }
```

The shape is identical; only the key changed. The manifest is validated strictly
(`additionalProperties: false`), so a manifest still carrying `lab:` is rejected as an
unknown field.

## 6. Removed commands and crates

The runtime left Isoloom; the Isothrum runner runs what Isoloom generates.

- Removed CLI verbs: `run`, `test`, `down`, `status`, `connect`, `exec`, `capture`,
  `reset`, `provision`, plus the registry and run-time secret delivery. Generate the
  files with `isoloom generate` and run them with the Isothrum runner (or the native
  tools directly).
- Removed crate: `isoloom-coverage`. Use `isoloom matrix` (the generated capability
  matrix) and `isoloom verify` (conformance against official validators) instead.
- Renamed crate: `isoloom-external` → `isoloom-ansible`.

The CLI commands in 1.0 are `validate`, `generate`, `check`, `targets`, `resources`,
`defaults`, `graph`, `report`, `message`, `inspect`, `profiles`, `schema`, `import`,
`docs` and `matrix`.

## 7. The Isothrum runner API

If you embed the Isothrum runner, the CTF/courses word "lab" was dropped for a
domain-neutral vocabulary:

| 0.x | 1.0 |
|---|---|
| `Lab<'r>` (from `Runner::open`) | `Running<'r>` |
| `status::Run` (the up/resume result) | `Started` |
| `LabView` (the hook view) | `RunView` |
| compiler `LabFile` | `ProjectFile` |
| CLI `LabArgs` | `TargetArgs` |
| `LabArgs.lab` (its path field) | `TargetArgs.path` |
| the `lab` module | `run` |

The public types are now `Runner` (`open` returns `Running<'r>`), `Running<'r>`,
`Started` and `RunView<'a>`. See the Isothrum CHANGELOG for details. The CLI operations
(`up`, `park`, `resume`, `status`, `check`, `provision`, `down`, `reap`) are unchanged in
shape.

## 8. Engine-compatibility gating (the launcher)

The Cyber CTF launcher only runs labs whose spec format and engine versions match what it
bundles. `isoloom validate --json` reports a lab's `specFormat` and the `isoloomVersion`
it validated against; the publish path stores those alongside the lab, and the launcher
returns only labs whose format is the 1.0 form and whose minimum engine versions are at or
below the launcher's bundled Isoloom and Isothrum. A 0.x-format lab, or one validated
against a newer engine than the launcher bundles, is hidden until it is re-validated and
re-synced. An un-synced lab defaults to legacy and stays hidden until re-synced.
