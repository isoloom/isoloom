# Troubleshooting and FAQ

## `isoloom validate` fails

Each problem names a code, the field's path as written, its line and column, the reason and a
fix. The common ones:

- **A file the spec names doesn't exist** (`build/web`, `provision/web.sh`, a check script): paths
  are relative to the folder holding `isoloom.yml`. `isoloom validate --skip-files` checks the
  spec alone.
- **An unknown field**: the spec format is strict, so a typo is an error, never ignored. A spec
  written for Isoloom 0.x is refused field by field: [migrating to 1.0](migrating-to-1.0.md).
- **`unknown-reference` on `service:`**: the catalogue has no such entry
  ([catalogue](catalogue.md)).

Warnings (`! warning:`) never fail validation; each says what to change.

## A target is missing from `isoloom generate`

`isoloom targets` lists every target with the reason it is refused, naming the field at fault: a
Windows machine has no container form, a TPM needs a VM, a cloud has no image for an OS. A
`targets:` list in the spec, or a profile's `target`, narrows the list too.

## `isoloom check` fails in CI

The committed files under `.isoloom/` don't match what this `isoloom` generates from the spec.
Run `isoloom generate` and commit. If it passes locally but fails in CI, the inputs differ: the
`isoloom` version, your `~/.isoloom/defaults.yml` (a region, an image), `ISOLOOM_*` variables or
the profile. Pin the version in CI and keep shared defaults in the repository
([commit the generated files](commit-generated-files.md)).

## I can't find the published port

Compose publishes on `127.0.0.1` and on a free port: `docker compose -f
.isoloom/docker/compose.yml port <machine> <port>` prints it, and a runner prints every URL. Set
`ISOLOOM_PUBLISH_FIXED=1` to use the spec's `publish` number ([Docker](targets/docker.md)).

## Images or packages don't download

Behind a proxy or TLS inspection, the machines need `constraints.proxy`, mirrors and your CA:
[corporate network](corporate-network.md). The tools on your computer need their own proxy
settings (Docker's, `HTTPS_PROXY`).

## A derived check fails ("no internet from web", "cache is blocked")

The environment doesn't isolate what the spec says. On Kubernetes, the network plugin doesn't
enforce NetworkPolicies ([Kubernetes](targets/kubernetes.md)). On `external`, your network allows
what `reach` doesn't. Elsewhere it is a bug: please report it with the spec.

## A cloud apply fails

- Credentials: [the cloud guide](targets/cloud.md#credentials) lists what each module reads.
- `allowed_cidr` is required.
- A quota or a region without the instance type: change the region
  ([regions](targets/cloud.md#regions)) or the machine's `resources`.
- Leftovers of an interrupted run: `isothrum reap <cloud>` finds them by their tags.

## FAQ

**Does Isoloom need network access, or send data anywhere?** No. Generation reads local files
only, and Isoloom has no telemetry ([offline](offline.md)).

**Does Isoloom need administrator rights?** No: it reads and writes files in the project folder
and `~/.isoloom`. The tools that run the files may (a hypervisor, Docker).

**Is OpenTofu supported?** It isn't tested. The generated modules are plain Terraform; try them
with OpenTofu at your own risk, and tell us what you find.

**Can I edit the generated files?** Layer your own files on top instead: a hand edit is lost at
the next `isoloom generate` ([commit the generated files](commit-generated-files.md#dont-edit-them)).

**Where does Terraform state go?** Locally, in the runner's state folder, or the module's folder
when you apply by hand ([remote state](remote-terraform-state.md)).

**Which runner should I use?** Any program that follows [the run manifest](../reference/run-manifest.md);
[Isothrum](https://github.com/Isothrum/isothrum) is the reference one. Or run each tool's files by
hand.

## Still stuck

[Getting help](../../.github/SUPPORT.md).
