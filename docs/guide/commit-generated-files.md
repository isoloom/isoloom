# Commit the generated files

The files under `.isoloom/` are plain Compose files, manifests, Vagrantfiles and Terraform
modules. Committing them lets reviewers see what a spec change does to each target, lets people
without `isoloom` run them, and gives auditors the exact files that ran.

## What to commit, what to ignore

Commit what `isoloom generate` writes: the target folders, `resolved.json` and `.generated` (the
list of files Isoloom owns, so it can remove the ones it no longer writes).

Don't commit what the tools write while running: Terraform's `.terraform/` folders and state,
Vagrant's `.vagrant/`, a runner's working files. `isoloom generate` writes a `.isoloom/.gitignore`
that ignores them, so `git add .isoloom` picks up only the generated files. Keep it committed.

Isoloom owns exactly the files listed in `.isoloom/.generated`: it never deletes anything else in
`.isoloom/`, so the tools' state survives a regeneration. The list is read as untrusted: a line
that isn't a plain path under `.isoloom/` (an absolute path, a `..`), or a file reached through a
symbolic link, stops `generate` and `check` before anything is removed. So does a generated
file whose path goes through a symbolic link (`.isoloom/docker` linked elsewhere), or is one:
Isoloom writes only into the project's own folders, and replaces a file rather than writing into
it.

## Keep them in step with the spec

After changing `isoloom.yml`, a profile or your defaults:

```sh
isoloom generate
git add isoloom.yml .isoloom
```

In CI, `isoloom check` fails when a committed file is out of date or stale (a file Isoloom no
longer writes): [CI guide](ci.md). Generation is deterministic, so a regeneration with the same
inputs changes nothing ([determinism](../explanation/determinism.md)).

The generated files depend on the `isoloom` version, the spec, the profile and the defaults
(regions, images). Pin the `isoloom` version in CI, and keep site-specific defaults in a
[profile](../reference/configuration.md#profiles) or `isoloom.defaults.yml` in the repository
rather than in each person's `~/.isoloom/defaults.yml`, or `isoloom check` will disagree between
machines.

## Don't edit them

Each file starts with a header saying it is generated. A hand edit is lost at the next
`isoloom generate` and fails `isoloom check`. To change the output, change the spec; for what the
spec can't say, layer your own files on top: a Kustomize overlay over `.isoloom/kubernetes`, a
Compose override file (`docker compose -f .isoloom/docker/compose.yml -f compose.override.yml`),
a Terraform `*_override.tf` file beside a module ([remote state](remote-terraform-state.md)).
Isoloom leaves files it didn't write alone.
