# Docker Compose: `docker` and `hosted`

The `docker` target turns each machine into a container on your Docker host, with one Docker
network per spec network and the spec's addresses. `hosted` writes the same Compose file for a
hosting service that runs Compose files itself, without a run manifest.

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
docker compose -f .isoloom/docker/compose.yml up -d --wait
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check
docker compose -f .isoloom/docker/compose.yml down -v
```

The checks that run from a machine's own position run as `isoloom-check-<machine>`:

```sh
docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check-cache
```

Or hand the run manifest to a runner, which does all of the above and reports each check:

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
derived checks prove that too. A container that needs `privileged`, `NET_ADMIN` or `NET_RAW` on
an offline network is refused unless the spec accepts a best-effort seal
(`machines.<name>.seal: best-effort`).

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
