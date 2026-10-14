# Stability

What you can rely on when you upgrade Isoloom: which changes a minor release may bring, which
wait for a major one, and how a field on its way out is retired.

The promise starts with Isoloom 1.0. Until then (0.x), the spec format can still change in a
minor release, and each change is listed in the [changelog](../CHANGELOG.md).

## What the promise covers

A spec written for `version: 1` (the first line of every `isoloom.yml`) that validates on one
1.x release:

- still validates on every later 1.x release, with at most warnings;
- still produces the same external behavior on each target that accepted it: the same
  machines, the same networks and addresses, the same services answering from where `reach`
  allows and from nowhere else, and the same derived checks passing.

The command line's names, arguments and exit codes, the JSON output ([JSON output](json.md)),
the registry ([`~/.isoloom/status.yml`](registry.md)) and the supported library surface
([README](../README.md#using-the-library)) follow the same rules.

`version: 1` is the only spec format so far: `isoloom validate` refuses any other with
"unsupported version", so a spec never silently changes meaning because a later release reads it
differently.

## What a minor release may change

Additions only:

- **New fields and values.** A new field is optional, and leaving it out keeps the behavior it
  had before. A new value in an existing list (an OS, a cloud, a Vagrant provider) doesn't
  change what the existing values mean.
- **A target producing more.** A field a target refused can become one it renders. A spec that
  ran keeps running.
- **New targets, commands and options,** and new derived checks for behavior the spec already
  promised.
- **Deprecations** (below): an old spelling keeps working, with a warning.
- **Bug fixes.** When a target's output didn't do what the spec says, fixing it changes
  behavior toward the spec. Such fixes are listed in the changelog. A target may also start
  refusing a spec it accepted, when what it produced was wrong (a `reach` it didn't actually
  enforce, say): a clear refusal is better than an environment that looks right and isn't.

Validation is strict: an unknown field is an error, not something to ignore. So a spec that uses
a field added in 1.4 is refused by 1.3, with the field's path. Upgrading is safe; downgrading
is not promised. If a team shares a spec, pin the version they use (the release binaries, or
`uses: isoloom/isoloom@vX.Y.Z` in GitHub Actions).

## What only a major release changes

- Removing a field, a value, a target, a command or an option.
- Changing what an existing field means, or its default.
- Changing a documented format in a way its readers notice: a JSON event's field removed or
  changing meaning, the registry's `version` going up.
- A change to the supported library surface that isn't additive.

A major release that changes the spec format bumps the spec's `version:` as well, so a spec says
which rules it was written for.

## Deprecation

When a field is renamed or retired:

1. A minor release adds the replacement. The old field keeps working as before.
2. `isoloom validate` warns about the old field, with its path and what to write instead. A
   warning doesn't stop a command: the spec still validates.
3. The old field keeps working with that warning for **at least one minor release** after the
   one that deprecated it, and is removed only in a **major** release.
4. The changelog lists each deprecation when it starts, and each removal in the major release
   notes.

The first rename planned under this policy is a machine's `services:`, becoming `ports:`.

## What isn't covered

- **The generated files' layout.** What Isoloom writes under `.isoloom/` (file names, folders,
  resource and service names inside a Compose file, manifests, Vagrantfiles or Terraform
  modules, the order of things, comments) can change in any release. The promise is about the
  behavior those files produce, not their text. After an upgrade, regenerate and commit
  (`isoloom generate`), and let `isoloom check` catch the drift in CI. Changes that need care
  on an environment that is already running (a Terraform resource renamed, for example) are
  marked "Breaking (generated modules)" in the changelog, with the steps to follow. The
  [cloud modules](cloud-modules.md)' variables and outputs, and the resolved snapshot
  (`.isoloom/resolved.json`, which carries its own `version`), are documented formats: they
  follow the rules above.
- **Internal crates and items.** Only the supported surface listed in the
  [README](../README.md#using-the-library) is covered. Every other public item, and the crates
  outside that list (`isoloom-render`, the backends, `isoloom-import`, `isoloom-coverage`), is
  public for the command line only and may change in any release.
- **Human-readable output.** Messages, tables and notes printed without `--json` may be
  reworded. Scripts should use `--json` and exit codes.
- **Drafts from `isoloom import`.** An import is a starting point: a later release may draft a
  different spec from the same file, usually a fuller one.
- **The tools Isoloom drives.** A new Docker, Kubernetes, Vagrant or Terraform provider version
  can change what a target needs from the host. Isoloom follows them; which versions each
  target needs is in [Targets](targets.md).
