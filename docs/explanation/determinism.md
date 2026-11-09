# Determinism

The same inputs give the same files, byte for byte. The inputs are:

- the `isoloom` version;
- the project folder: `isoloom.yml`, the files it names, `isoloom.defaults.yml`, the profile
  in use;
- your defaults (`~/.isoloom/defaults.yml`), `ISOLOOM_*` defaults variables, `--images`, `-s`.

Nothing else reaches the output: no clock, no random value, no host name, no network lookup, no
file order from the disk. Maps are kept in a fixed order, names are derived from the spec (a hash
of a name where a short id is needed), and the catalogue and image tables are built into the
binary.

## Why it matters

- **Review.** Committed files change only when an input changes, so a diff under `.isoloom/` is
  exactly what a spec change does ([commit the generated files](../guide/commit-generated-files.md)).
- **CI.** `isoloom check` regenerates in memory and compares: it can only fail for a real
  difference.
- **Audit and reproducibility.** Anyone with the same inputs gets the same files, without network
  access ([offline](../guide/offline.md)).

## How it is checked

A test renders every example twice in one process and compares the files; CI's `determinism` job
also generates every example twice with the command line, in the same folder, and compares the
trees.

## What is not deterministic

What happens when the files run: image tags resolve to whatever the registry serves (the
catalogue's images are pinned by digest; your own `app.image` is as pinned as you write it),
package versions installed by steps, cloud IDs and addresses. Terraform modules pin provider
versions to a patch range ([tool versions](../reference/tool-versions.md)), and a run's lock file
(`.terraform.lock.hcl`) is the tool's.
