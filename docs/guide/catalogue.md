# Catalogue services

Instead of describing a service's install and configuration yourself, a machine can name a
packaged **catalogue entry** with `service:`. Isoloom resolves it to a recipe that works in both
forms — an official image (or a build from the OS base image) on container targets, and a fresh
VM set up by the same recipe on VM targets — and keeps the two in parity by construction.

> The catalogue ships **`apache`**, **`mariadb`**, **`mongodb`**, **`mysql`**, **`nginx`**,
> **`openssh`**, **`php-fpm`** and **`postgres`**. A `service:` naming an entry the catalogue doesn't have is refused with `unknown-reference`
> at `machines.<m>.service`.

## Using a service

```yaml
# isoloom.yml
version: 1
name: site
networks:
  lab: { cidr: 10.20.0.0/24 }
machines:
  web:
    os: debian@12
    networks: { lab: 10 }
    service: nginx@1.30.6
    with:
      port: 80
      root: site
    ports: { http: { publish: 8080 } }
checks:
  - name: the page answers
    http: http://web/
    expect: 200
```

- **`service: <entry>[@<version>]`.** A version picks the release it equals, else the newest
  release it is a prefix of on dot boundaries (`postgres@16` → the newest 16.x). With no version,
  the entry's default release is used.
- **`with:`** sets the entry's typed settings. Each is checked against the entry's schema when the
  spec is read: a `string` setting may carry a `pattern` or `values`; an `endpoint` is
  `<machine>:<port>`; a `password` is a literal, `{ secret }` or `{ input }` (its value is
  delivered as a file, never written into a generated file). Unknown or malformed settings are
  reported at `machines.<m>.with.<key>`.
- **Ports** on a service machine are the entry's: `ports.<name>` may only `publish` or mark
  `dormant` one of them. Giving a `port:` number is a conflict (the entry fixes the numbers).
- **OS.** The release must run on the machine's `os`; otherwise it is refused at
  `machines.<m>.os`, listing the OSes the release supports.

The editor-completion JSON schema (`schemas/spec-v1.schema.json`) carries each entry's `with:`
settings, so an editor validates them as you type.

## What ships for an entry

A service machine's forms:

- **Container target** — the entry's official image (pinned by digest) where the release
  publishes one for the machine's OS and architecture, with a thin build of the entry's
  `configure`; otherwise a build from the OS image with the whole recipe.
- **VM target** — a fresh VM of the machine's OS, set up by the recipe.

`resolved.json` lists each service machine's `services`: the entry, the resolved release, the
source (`vendor`, `distro` or `download`) and the official image with its digest (or null when the
container is built).

Some entry recipes are refused on a form the configuration renderers don't carry; the refusal
names the step and the phase. One service per container is the rule (several services share a VM,
but a container runs one).

## See also

- [Spec reference](../spec.md) — `service:`, `with:` and the ports a service machine takes.
- [Targets and providers](../reference/targets.md) — the forms each target gives a service machine.
- [Getting started](getting-started.md).
- Writing an entry: [the contributor guide](../../.github/CONTRIBUTING.md#the-catalogue-contributors).
