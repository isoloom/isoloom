# Catalogue services

Instead of describing a service's install and configuration yourself, a machine can name a
packaged **catalogue entry** with `service:`. Isoloom resolves it to a recipe that works in both
forms — an official image (or a build from the OS base image) on container targets, and a fresh
VM set up by the same recipe on VM targets — and keeps the two in parity by construction.

> At 0.11.0 the catalogue ships one entry, **`nginx`**. More entries (apache, php-fpm,
> mysql/mariadb, postgres, mongodb, redis/valkey, dvwa, juice-shop and others) are planned for
> 1.x. A `service:` naming an entry the catalogue doesn't have is refused with `unknown-reference`
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

Some entry recipes are refused on a form the configuration renderers don't yet carry; the refusal
names the step and the phase. One service per container is the rule (several services share a VM,
but a container runs one).

## Writing an entry

An entry is a folder `catalogue/<entry>/`:

| File | Written by | Holds |
|---|---|---|
| `entry.yml` | by hand | the recipe, the settings schema, the ports, the releases and the OSes each runs on |
| `lock.yml` | `cargo xtask catalogue refresh` | the pinned digests and exact package versions |
| `contract.yml` | by hand | the parity contract: the facts that must read the same in both forms |
| `files/` | by hand | the entry's own templates, each carrying an `SPDX-License-Identifier: Apache-2.0` header |

The catalogue is embedded in the binary at build time (text files only, under 1 MB each);
generating never reads the disk or the network for it. Shared data lives beside the entries:
`catalogue/daemons.yml` (the daemons no entry owns, with their foreground forms) and
`catalogue/collections.yml` (the pinned Ansible Galaxy collections, referenced from Galaxy and
never embedded). The schemas `catalogue/entry.schema.json` and `catalogue/contract.schema.json`
are generated from the types.

An entry is checked when the catalogue loads: its recipe must resolve on every release and OS it
lists, every setting must be used by the recipe, a template's `from` must be a file of the entry,
and the contract must have a default case. These checks run in the test suite, so a malformed
entry fails the build.

### Pinning

```sh
cargo xtask catalogue refresh [<entry>…] [--check] [--update] [--propose]
```

`refresh` pins, through `curl`, each repository signing key (by fingerprint then SHA-256), each
official image (the tag's multi-platform digest and platforms), each vendor package's exact
version per OS, and the Galaxy collections (version, URL and SHA-256), writing `lock.yml`. A pin
that moved upstream fails loudly unless `--update` accepts it; `--check` writes nothing and exits
non-zero on any drift; `--propose` lists newer upstream releases without adding them.

### Proving parity

```sh
cargo xtask catalogue check <entry> [--release V] [--os OS] [--form container|vm|both]
```

`catalogue check` runs the entry's parity contract for real: each case becomes a lab, generated
for `docker` and for `vagrant`, run through the Isothrum CLI (`up`, `check`, `down`), and a case
passes only when every check passes in both forms and every contract fact reads the same in both.

## See also

- [Spec reference](spec.md) — `service:`, `with:` and the ports a service machine takes.
- [Targets and tiers](targets.md) — the forms each target gives a service machine.
- [Getting started](getting-started.md).
