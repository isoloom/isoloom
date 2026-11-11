# Policies

What Isoloom promises between releases, how each promise is versioned, and how something is
deprecated and removed.

## Versioning

All published crates and the `isoloom` binary share one version (`[workspace.package].version`)
and follow [Semantic Versioning](https://semver.org/). A release that breaks anything under
[What is stable](#what-is-stable) is a new major version; one that only adds is a minor; one that
only fixes is a patch. Every change is recorded in the [CHANGELOG](../../CHANGELOG.md), breaking
ones marked **Breaking**.

## What is stable

| Surface | Promise within a major | How it is versioned and checked |
|---|---|---|
| **The spec format** (`isoloom.yml`, `version: 1`) | Strict. A spec that validates on one release validates on every later release of the same major, and means the same: the same machines, networks, reach and checks. New fields and new allowed values come in minor releases. | The spec's own `version:` (1) names the format; a major release of Isoloom may refuse specs an earlier major took, listed in its migration guide ([2.0](../guide/migrating-to-2.0.md)). The docs test validates every spec in the docs, and CI validates every example. |
| **The command line** | Strict: commands, flags, their meanings, exit codes (0 success, 1 the answer is no, 2 could not run) and `--json` outputs. New commands and flags come in minor releases. | The crate version. Text meant for people (without `--json`) is not stable: don't parse it. |
| **The run manifest** (`run.json`) | Versioned: `manifest_version: 1`. Within version 1, a minor release may add an optional field. The reader is strict: it refuses a field or value it doesn't know, so a manifest using an addition needs a runner built with the `isoloom-manifest` release that knows it; a runner pins the `isoloom-manifest` it reads. Removing, renaming or changing a field, making one required, or adding a value a runner must act on raises `manifest_version`, and that is a major release of Isoloom (`isoloom_manifest::COMPATIBILITY`). | `manifest_version` and the committed JSON schema under `schemas/`. [The run manifest](run-manifest.md). |
| **The contract's names outside `run.json`** (`isoloom_manifest::contract`: the output folder, the reserved prefix, the controller and its halting mark, the cloud tags, the Vagrantfile's variables, the published ports' params, the derived checks' switch `ISOLOOM_DERIVED`, the variable each secret is handed over in `ISOLOOM_SECRET_<NAME>`, the access machine's image and box variables `ISOLOOM_ACCESS_IMAGE` and `ISOLOOM_ACCESS_BOX`, the secrets folder and its mark) | Strict: a runner uses them as they are, and the generated files are written from them. Renaming or removing one, or changing what it means, is a major release; a new one comes in a minor release. | The `isoloom-manifest` crate version, with its library API above. |
| **The resolved snapshot** (`.isoloom/resolved.json`) | Versioned by its `resolved_version`, raised when its shape changes. | `resolved_version`. |
| **Catalogue entries** (`service: <entry>`) | An entry, or a release of one, can be removed with notice: it is marked deprecated in the CHANGELOG at least one minor release before, and removed no earlier than three months later. A removal that makes specs fail is listed under **Breaking** in the release that removes it. | The CHANGELOG. Pins (`lock.yml`) move in any release. |
| **The library API** | Supported: the public items of `isoloom-spec` (parsing, loading, validation and the spec model under `isoloom_spec::model`), `isoloom-targets` (`Lab`, generation, refusals, the resolved snapshot, the matrix and tiers) and `isoloom-manifest` (the run manifest's types and parsing). Not a promise: `#[doc(hidden)]` items, test-support features, building the spec model's structs by hand (parse a spec instead: a new spec key adds a field) or the run manifest's (parse one instead: a new optional field of the document, which `isoloom_manifest::COMPATIBILITY` allows in a minor release, adds a field), and the other published crates (`isoloom-plan`, `isoloom-render`, `isoloom-catalogue`, the backends, `isoloom-import`, the `isoloom` crate's library items), which exist so the supported crates can be built from crates.io. | The crate version. CI runs `cargo semver-checks` on the three supported crates against their latest release on crates.io. |
| **The generated files** | Their behavior is the contract: the environment the spec describes, which its checks prove. Their text is not stable: a release may change any generated file to fix or improve it. `isoloom check` tells you when your committed files need regenerating. | Snapshots in the test suite; the CHANGELOG says what changed. |

## Deprecation and removal

- **Spec fields and CLI flags** are removed or changed in meaning only in a major release, listed
  in that release's migration guide. Within a major, a field or flag kept for compatibility is
  marked deprecated in the CHANGELOG and, where `isoloom validate` can tell, warned about.
- **Library items** of the supported crates are removed only in a major release. They are
  removed outright, not kept as deprecated shims; the CHANGELOG names each one and its
  replacement.
- **Catalogue entries** follow the notice above.
- **Targets, providers and clouds** are removed only in a major release. A target can move down
  a tier at any time: tiers state evidence, not a promise ([tiers](targets.md#tiers-how-far-a-target-is-proven)).

## Supported releases

| Release | Fixes |
|---|---|
| The latest 2.x | Bug and security fixes, as new patch or minor releases |
| The last 1.x (1.0.9) | Security fixes for three months after 2.0.0 is released |
| Anything older | None: upgrade |

Fixes are not backported to earlier minor releases: upgrade within the major. How to report a
vulnerability, and the response times: [SECURITY.md](../../.github/SECURITY.md).

## Minimum supported Rust version (MSRV)

- The crates declare `rust-version = "1.88"`; CI checks that the published crates build with Rust
  1.88.
- Development, CI and the release binaries use the toolchain pinned in `rust-toolchain.toml`,
  which is newer: the MSRV is what a crate's users need, the pin is what we build with.
- The MSRV is raised only when a dependency or a language feature needs it, to a Rust release at
  least six months old. Raising it is a minor release, never a patch, and the CHANGELOG says so.

## Platforms

The release binaries are built for Linux (x86_64 and aarch64, static), macOS (x86_64 and arm64)
and Windows (x86_64). CI runs the tests on Linux, macOS and Windows. The generated files run from
all three; each target's [tier](targets.md#tiers-how-far-a-target-is-proven) says from which host
OSes it has been proven.
