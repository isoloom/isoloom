# Getting started

This tutorial takes you from installing `isoloom` to running a small environment (a web server
and a cache) with Docker Compose, then with the Isothrum runner. It takes about ten minutes.

You need:

- `isoloom` ([install it](../../README.md#install));
- Docker with Compose 2.17 or later, to run the environment;
- `git`, to fetch the example.

## 1. Get the example

The `hello-stack` example is a complete project: its spec (`isoloom.yml`), the build recipe of
its web server, its set-up scripts and its checks. Take it from the release that matches your
`isoloom`:

```sh
version="$(isoloom --version | cut -d' ' -f2)"
git clone --depth 1 --branch "v$version" https://github.com/isoloom/isoloom.git isoloom-src
cp -R isoloom-src/examples/hello-stack .
cd hello-stack
```

Open `isoloom.yml`. Each machine says what it *is*: an OS, the ports it serves, and an `app:`
(an image, or a build recipe, plus the `steps` that set up the same machine as a VM). It never
says how a platform runs it, so there are no `docker:` or `vm:` blocks. The `checks:` say how the
environment must behave, on every target.

## 2. Validate it

```sh
isoloom validate
```

```text
✓ hello-stack is valid
```

`validate` checks every field and that every file the spec names exists (`build/web`,
`provision/web.sh`, the check scripts). A problem is printed with a code, the field's path, its
line and column, the reason and a fix.

## 3. See where it can run

```sh
isoloom targets
```

It lists the targets this spec can be generated for (`docker`, `kubernetes`, `vagrant`,
`proxmox`, the clouds...), why the others can't take it, and each target's
[tier](../reference/targets.md#tiers-how-far-a-target-is-proven): how far it has been proven by
real runs.

## 4. Generate the Compose files

```sh
isoloom generate --target docker
```

```text
✓ .isoloom/docker/compose.yml
✓ .isoloom/docker/checks/networks.sh
...
✓ .isoloom/docker/run.json
✓ .isoloom/resolved.json
✓ .isoloom/.gitignore
```

Everything is written under `.isoloom/`. `compose.yml` is a plain Compose file: read it, run it
by hand, or commit it. `run.json` is the run manifest a runner reads. `isoloom generate` with no
`--target` writes every target the spec allows.

## 5. Run it with Docker Compose

```sh
docker compose -f .isoloom/docker/compose.yml up -d --wait
docker compose -f .isoloom/docker/compose.yml port web 80
```

The second command prints the address the web server is published on, such as
`127.0.0.1:52673`. Published ports bind to `127.0.0.1` only, on a free port (set
`ISOLOOM_PUBLISH_FIXED=1` to use the spec's `publish: 8080`). Open it:

```sh
curl http://127.0.0.1:52673/
```

Run the environment's checks, from inside it:

```sh
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check
```

```text
isoloom-check: PASS the page greets
...
isoloom-check: END 7 passed, 0 failed
```

Take it down, volumes included:

```sh
docker compose -f .isoloom/docker/compose.yml down -v
```

## 6. Run it with Isothrum

A runner reads `run.json` and does all of the above for you: it checks the tools' versions,
brings the environment up, waits until it is ready, runs every check (from each machine's
position too), and removes everything at the end. [Isothrum](https://github.com/Isothrum/isothrum)
is the reference runner; install it from its releases or with `cargo install --locked
isothrum-cli`.

```sh
isothrum up . --target docker
isothrum check . --target docker
isothrum down . --target docker
```

`isothrum up` prints the published URLs and the spec's `message`. `isothrum check` exits 1 when a
check fails.

## 7. Change it

Edit `isoloom.yml`, for example add a check:

```yaml
  - name: the cache answers
    tcp: cache:6379
```

Then `isoloom validate`, `isoloom generate --target docker` and run it again. In CI,
`isoloom check` fails when the files under `.isoloom/` no longer match the spec
([CI guide](ci.md)).

## Next steps

- Run the same spec as VMs: [Vagrant](targets/vagrant.md); on a server:
  [Proxmox](targets/proxmox.md); in a cloud account: [Clouds](targets/cloud.md).
- Every field: [the spec reference](../spec.md). Every command: [the CLI
  reference](../reference/cli.md).
- Use a packaged service instead of your own image: [catalogue services](catalogue.md).
- Write your own: [the examples](../reference/examples.md) cover networks, reach rules,
  Windows, gateways and more.
