# Isoloom and Docker Compose

[Docker Compose](https://docs.docker.com/compose/) runs a set of containers from one YAML file.
Isoloom's `docker` target writes a Compose file and runs it with `docker compose`, so the two
work together: Isoloom is a way to get a Compose file, and other targets, from one spec.

## What Compose does better

- **It is the standard.** Everyone with Docker has it, most images document themselves with a
  Compose snippet, and editors, CI services and hosting platforms read it. There is no new tool
  to learn or install.
- **The whole format.** Compose has options Isoloom doesn't write because they are tooling
  rather than the environment's behavior (`include`, `develop` and file watching, `annotations`,
  extension fields). [Coverage](../COVERAGE.md#docker-compose) lists every key and what Isoloom
  does with it.
- **The inner loop.** `docker compose watch`, rebuilding one service, bind-mounting your source
  tree for live reload: Compose is built around a developer editing code. Isoloom builds and runs
  environments; it doesn't sync your working copy into them.
- **Less in the way.** Compose puts containers on the networks you declare and lets them talk.
  Isoloom adds routers between networks to enforce `reach`, and check runners: more containers,
  for a guarantee you may not need.

## What Isoloom adds

- **The same spec beyond containers.** A machine with `vm:` as well as `docker:` also runs as a
  local VM (Vagrant), on Proxmox or ESXi, or as a cloud VM; with `docker:` only, it also runs on
  Kubernetes, on one VM or on one cloud VM. A Windows machine runs as a VM, and the environment
  simply never gets the Compose target. See [Targets](../targets.md).
- **Isolation that is enforced and checked.** `reach` says which network may reach which; on
  `docker` a router container enforces it, and the derived checks prove every port answers from
  where `reach` allows and from nowhere else. Link impairment (`tc`: delay, loss) on a network
  too.
- **Checks run from inside.** `isoloom test docker` runs your checks and the derived ones from
  the machines' own positions, not from the host.
- **Drift detection.** `isoloom check` fails in CI when the committed `.isoloom/` files no
  longer match the spec.
- **Labs that don't collide.** A published port lands on a free host port unless you fix one,
  and `--instance N` runs a second copy with its own names and ports.
- **Secrets from a store at run time,** delivered as files, never written into the Compose file
  ([Secrets](../secrets.md)).

## When to use which

- Use **Compose** for an application's development stack on a developer's machine, especially
  with live reload, and when containers are the only place it will ever run.
- Use **Isoloom** when the same environment must also run as VMs, on a cluster or in the cloud;
  when network isolation is part of what you are testing (a segmented lab, an edge firewall, a
  security exercise); or when you want CI to prove the environment behaves as described.
- **Both:** let Isoloom generate the Compose file, commit it, and run it by hand with
  `docker compose -f .isoloom/docker/compose.yml up -d --wait`. People who only know Compose
  can keep using it.

## Bringing a Compose project in

```sh
isoloom import compose              # reads compose.yaml (or docker-compose.yml) here
isoloom import compose path/to/compose.yml --stdout
```

Each service becomes a machine with `docker:`. The draft carries what the spec can express (the
image or build, networks, ports, environment, volumes, dependencies, CPUs and memory), and lists
every other key with why: it belongs in the image (`command`, `entrypoint`), the same effect
comes another way (`healthcheck`: Isoloom probes every port), it isn't expressible yet, or it is
left out by design. Then:

```sh
isoloom validate && isoloom run docker && isoloom test docker
```

Review the notes marked as changed first (renamed or re-addressed things). The draft is a
starting point: add `reach` and `checks` to say what the old file only implied, and `vm:` to the
machines that should also run as VMs.
