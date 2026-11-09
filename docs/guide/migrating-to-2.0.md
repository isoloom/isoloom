# Migrating to 2.0

Isoloom 2.0 keeps the spec format (`version: 1`): most specs validate and generate unchanged. It
is a major release because it refuses a few specs 1.x took, changes the run manifest's contract,
and removes library API. This page lists what you may have to change, by who you are. The full
list is the [CHANGELOG](../../CHANGELOG.md).

## Everyone

1. Install 2.0 ([install](../../README.md#install)): from 2.0 the releases carry signed binaries.
2. Run `isoloom validate` on each spec, and fix what it refuses (below).
3. Run `isoloom generate` and commit the regenerated `.isoloom/` if you commit it. Most generated
   files change; `isoloom check` fails until you do.
4. Upgrade your runner to a release that reads run manifest version 1 (Isothrum 2.0).

## Specs 2.0 refuses

| 1.x spec | 2.0 | Change it to |
|---|---|---|
| `service: dvwa` or `service: juice-shop` | `unknown-reference`: the entries are removed from the catalogue | `app: { image: <the image you used> }` gives the same container |
| A machine name or alias starting with `isoloom-` | refused: those names are Isoloom's own (its controller, the containers' VM, the check stand-in and runners, the route sidecars, the init jobs) | another name |
| A machine using the `openssh` entry and signing in as `ctf` without saying so | the login account defaults to `user` | `with: { username: ctf }` to keep the old account |

## Behavior that changed

- **Published Kubernetes ports** are ClusterIP Services by default (a LoadBalancer before), and
  pods run with a restricted security context. Expose a port outside the cluster in your own
  overlay ([Kubernetes](targets/kubernetes.md)).
- **Images built from the project** are named under `localhost/isoloom/` by default.
- **Terraform**: the generated modules require Terraform 1.11 or later.
- **`.isoloom/.gitignore`** is written by `isoloom generate`, ignoring the tools' state
  ([commit the generated files](commit-generated-files.md)).
- **`isoloom generate` removes the files it wrote before and no longer writes** (listed in
  `.isoloom/.generated`), and `isoloom check` fails on such a stale file.
- **`isoloom check`** checks a profile's pinned target, or the one `--target` names, else every
  target; it has `--quiet` and `--json`.
- **A cloud refuses a machine bigger than its largest size** instead of giving it the largest
  silently, and `cloud-vm` refuses a machine whose user data is over its cloud's cap.
- **An AWS `cloud-vm` instance that shuts itself down at the run's expiry is terminated**, not
  left stopped; its root disk is encrypted and its metadata service needs session tokens (IMDSv2).
  A machine's own AMI must support IMDSv2.
- **An input read as a file** is the slot `<INPUT>` on `cloud-vm` (was `input-<name>`), kept on
  the VM at `/var/lib/isoloom/secrets/<INPUT>`.
- **The Vagrantfile no longer carries a hook for a runner's own ad-hoc VM.** A spec that wants an
  access machine declares it (`access: true`).
- **`resolved.json`** is `resolved_version: 4`: a machine's size in each form (`resources.docker`,
  `resources.vm`).
- The example `pivot-dmz` is now `dmz-jump`.

## Runner authors

- **`manifest_version` is 1**, frozen with a compatibility policy
  ([the run manifest](../reference/run-manifest.md#versions-and-compatibility)). A runner built
  for version 0 refuses 2.0's manifests.
- Values Isoloom never wrote are gone from the schema, and a manifest holding one is refused: the
  `vagrant_ssh` and `winrm` guest-delivery transports, the `dotenv` state-file format, the
  `generated` slot kind, the `ssh` and `credential` output roles, the `static` output source and
  the `minutes_from_now` expiry unit.
- New, optional: each Vagrant VM's `machines[].native.boot_digest` (reload a running VM whose
  digest changed), each lab machine's `machines[].resources`.
- A published port's output is named `<machine>__<port>` (`web__portal`, was `web_portal`),
  and a manifest naming an output twice is refused (`duplicate_output`).
- Isoloom's own Compose services start with `isoloom-`: a route sidecar is
  `isoloom-routes-<machine>` (was `<machine>-routes`), an `init` job `isoloom-init-<machine>-<n>`
  (was `<machine>-init-<n>`) and its step `job-isoloom-init-<machine>-<n>`.
- The controller is `isoloom-controller` in Proxmox's manifests and check output (was
  `controller`).
- The demo secret names in the fixtures are `DEMO_SECRET` and `DEMO_SECRET_<X>`.

## Library users

The supported crates are `isoloom-spec`, `isoloom-targets` and `isoloom-manifest`
([policies](../reference/policies.md#what-is-stable)). Removed or made crate-internal in 2.0,
among others (the CHANGELOG's **Removed** section lists every item):

- `isoloom_spec::defaults::{apply_to_files, set_region, BUILTIN_REGIONS, builtin}`: the lab takes
  the regions (`Lab::with_regions`, from `Defaults::regions`); `isoloom_targets::default_regions`
  gives each cloud's built-in one.
- `isoloom_spec::files` (`GeneratedFile` is re-exported by `isoloom-targets` as before),
  `isoloom_spec::load_with` (use `load_profiled(dir, None, sets)`), `Spec::with_cloud_outputs`,
  `Spec::cloud_message`; `isoloom_spec` no longer re-exports `Declared`, `Dns`, `KNOWN_OS`,
  `Network`, `Port`, `Reach`, `Resources`, `Shape` or `Vlan` at its root (use
  `isoloom_spec::model`).
- `isoloom_targets::resolved::{render_message_at, HostPort, with_published}`,
  `isoloom_targets::decision`, the matrix's `columns`, `rows`, `Row` and `Cell`.
- `isoloom_manifest::access::{DEFAULT_BOX, DEFAULT_ARM64_BOX, DEFAULT_IMAGE}` and
  `isoloom_manifest::SUPPORTED_VERSION` (now crate-internal).
- In the building-block crates (`isoloom-plan`, `isoloom-render`, the backends,
  `isoloom-catalogue`), many items became crate-internal; those crates' API is not a promise.
