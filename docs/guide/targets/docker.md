# Docker Compose: `docker` and `hosted`

The `docker` target turns each machine into a container on your Docker host, with one Docker
network per spec network and the spec's addresses. `hosted` writes the same Compose file for a
hosting service that runs Compose files itself.

Tier: `docker` is Tier 1 (real runs from Linux, macOS and Windows); `hosted` is Tier 2.

## Prerequisites

- Docker Engine or Docker Desktop, with Docker Compose 2.17.0 or later
  ([tool versions](../../reference/tool-versions.md)).
- No credentials. Images are pulled from their registries, or from your mirror
  ([corporate network](../corporate-network.md)).

A machine has a container form when it is built from an image, a build recipe, a catalogue
service, or an OS plus steps a container can run. A Windows machine, a `reboot` or `playbook`
step, or VM-only hardware has none: `isoloom targets` says why `docker` is refused then.

## Generate and run

```sh
isoloom generate --target docker
docker compose -f .isoloom/docker/compose.yml up -d --build --wait
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check
docker compose -f .isoloom/docker/compose.yml down -v
```

That one `up --wait` brings the environment to ready: every service up and healthy, and every
set-up job done. A machine's `setup:` scripts run in a one-shot job once the machine is up;
Compose's `--wait` accepts a job's exit only when another service waits for it to complete, so
the jobs no machine waits for are waited for by `isoloom-jobs-done`, a small idle service of
Isoloom's own: a job that fails fails the `up`.

The checks that run from a machine's own position run as `isoloom-check-<machine>`, and a
machine's `exec` checks are piped into it:

```sh
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check-cache
docker compose -f .isoloom/docker/compose.yml exec -T web sh -s < .isoloom/docker/checks/isoloom-exec-web.sh
```

Each prints a PASS or FAIL line per check and fails when one does; `run -e ISOLOOM_DERIVED=0`
leaves the checks Isoloom derives from the spec out. The access machine is in the `access`
profile: `docker compose -f .isoloom/docker/compose.yml --profile access up -d <machine>` starts
it. The comment at the top of the generated `compose.yml` lists these commands for your spec,
and the variables it reads: each input by its own name (`${NAME}`, empty when unset), each
secret as `ISOLOOM_SECRET_<NAME>`, from Compose's environment (the shell's, else `.env`).

Or let a runner run the same Compose file, which does all of the above and reports each check:

```sh
isothrum up . --target docker
isothrum check . --target docker
isothrum down . --target docker
```

## Published ports

A port with `publish:` is published on the host's loopback only (`127.0.0.1`), on a free port
picked by Docker. `docker compose port <machine> <port>` prints it; a runner prints every
published URL. Two variables change that when you run the file by hand:

| Variable | Effect |
|---|---|
| `ISOLOOM_PUBLISH_FIXED=1` | Each published port takes the spec's `publish` number (`8080`) |
| `ISOLOOM_PUBLISH_ADDRESS=0.0.0.0` | Publishes on every interface of the host: anyone who reaches the host reaches the port |

## Isolation

Each spec network is a Docker network; `internet: false` networks have no route out, which the
derived checks prove ("no internet from web"). Traffic between networks goes through a router
container Isoloom adds when `reach` rules need one, which forwards only what the rules allow; the
derived checks prove that too. A `privileged` container on an offline network is refused unless
the spec accepts a best-effort seal (`machines.<name>.seal: best-effort`): from inside it could
lift the seal.

## Teardown

`docker compose ... down -v` removes the containers, networks and volumes. Every container
carries the labels `isoloom.managed=true` and `isoloom.environment=<name>`, so leftovers are easy
to find:

```sh
docker ps -a --filter label=isoloom.managed=true
```

## Cost

None beyond your host. A machine's size defaults to 1 CPU, 512 MB and 5 GB as a container
(`machines.<name>.resources`); `isoloom resources` adds them up.
