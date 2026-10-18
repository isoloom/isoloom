# Getting started

Isoloom converts one environment spec into the native files of every infrastructure and
configuration tool, and emits a run manifest describing how to bring it up and check it. This
guide takes you from installing the CLI to generating and running your first environment.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/main/packaging/install.sh | sh
```

The installer puts the `isoloom` binary on your `PATH`. Release binaries for macOS, Linux and
Windows are also on the [releases page](https://github.com/isoloom/isoloom/releases). The current
release is 0.11.0, the release candidate for 1.0.0.

Check it works:

```sh
isoloom --version
```

## Write a spec

An environment lives in one `isoloom.yml`. Each machine says what it *is* — an OS plus an `app:`
(its own image, a build recipe, or an appliance) or a catalogue `service:` — and which networks
and ports it has. It never says how a platform runs it, so there are no `docker:` or `vm:`
blocks.

```yaml
# isoloom.yml
version: 1
name: hello
networks:
  app: { cidr: 10.60.0.0/24, internet: false }
machines:
  web:
    os: debian@12
    networks: { app: 10 }
    ports: { web: { port: 80, protocol: http, publish: 8080 } }
    app:
      build: build/web
      steps: [{ run: provision/web.sh }]
checks:
  - name: the page answers
    http: http://web/
    expect: 200
message: |
  The page answers at {{ machines.web.ports.web.url }}.
```

The `app:` carries both forms of the machine: on a container target it builds an image from
`build/web`; on a VM target it runs the same `steps:` natively. The checks are run from inside
the environment, on every target.

## Validate

```sh
isoloom validate
```

`validate` parses the spec, checks every field, and confirms every file it names exists
(`build/web`, `provision/web.sh`, the check scripts). Problems are printed with a code, the field
path as written, the line and column, the reason and a fix. See the
[spec reference](spec.md) for every field.

## Generate

```sh
isoloom generate                 # every target the spec can be generated for
isoloom generate --target docker # or just one
```

Each target's files are written under `.isoloom/<target>/`, ready to run by hand, commit, or hand
to the runner. `isoloom targets` lists the targets this spec can be generated for, why the others
can't, and the [tier](targets.md) of each. `isoloom check` fails in CI when the files under
`.isoloom/` drift from the spec.

## Run it

Isoloom runs nothing itself. To bring the environment up, either run each tool's files directly
(for the `docker` target, `docker compose -f .isoloom/docker/compose.yaml up`), or hand the run
manifest to the [Isothrum runner](runner.md):

```sh
isothrum up      # bring it up, following .isoloom/<target>/run.json
isothrum check   # run the checks
isothrum down    # tear it down
```

## Next steps

- [Spec reference](spec.md) — every field, generated from the schema.
- [Targets and tiers](targets.md) — what each target does, and how far each is proven.
- [Catalogue services](catalogue.md) — use a packaged service with `service:`, or write an entry.
- [The runner](runner.md) — the lifecycle Isothrum drives over a run manifest.
- [Architecture](../ARCHITECTURE.md) — how the pipeline and the crates fit together.
