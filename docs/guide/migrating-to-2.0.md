# Migrating to 2.0

Isoloom 2.0 keeps the spec format (`version: 1`): most specs validate and generate unchanged. It
is a major release because it refuses a few specs 1.x took, changes the run manifest's contract,
and removes library API. This page lists what you may have to change, by who you are. The full
list is the [CHANGELOG](../../CHANGELOG.md).

## Everyone

1. Install 2.0 ([install](../../README.md#install)): from 2.0 the releases carry signed binaries.
2. Run `isoloom validate` on each spec, and fix what it refuses (below).
3. Regenerate from scratch once, `rm -rf .isoloom && isoloom generate`, and commit the
   regenerated `.isoloom/` if you commit it. Most generated files change, and `isoloom check`
   fails until you do. From scratch, because 1.x listed none of the files it wrote in
   `.isoloom/.generated`, so 2.0 never removes those it no longer writes; the tools' state under
   `.isoloom/` (a `.terraform/` folder, `.vagrant/`) goes with it, so destroy a running lab first.
4. Upgrade your runner to a release that reads run manifest version 1 (Isothrum 2.0).

## Specs 2.0 refuses

| 1.x spec | 2.0 | Change it to |
|---|---|---|
| `service: dvwa` or `service: juice-shop` | `unknown-reference`: the entries are removed from the catalogue | `app: { image: <the image you used> }` gives the same container |
| A machine name or alias starting with `isoloom-` | refused: those names are Isoloom's own (its controller, router and tool VM, the containers' VM, the check stand-in and runners, the route sidecars, the init jobs) | another name |
| A Vagrant box name with anything but letters, digits, `.`, `_` and `-` after an optional `<org>/` (in `overrides.vagrant` or an image table) | `invalid-value` (an error reading the image table) | the box's name as `vagrant box list` shows it |
| A message naming `.published` or `.url` of a port without `publish:` | `unknown-reference` (`isoloom message` refused it already) | the port's `.port`, or publish the port |
| A machine using the `openssh` entry and signing in as `ctf` without saying so | the login account defaults to `user` | `with: { username: ctf }` to keep the old account |

## Behavior that changed

- **Published Kubernetes ports** are ClusterIP Services by default (a LoadBalancer before), and
  pods run under the Pod Security Standards' `baseline` level, which their namespace enforces. Expose a port outside the cluster in your own
  overlay ([Kubernetes](targets/kubernetes.md)).
- **Images built from the project** are named under `localhost/isoloom/` by default.
- **Proxmox verifies the API's TLS certificate**: `proxmox_insecure` defaults to `false`. A
  server with a self-signed certificate needs `proxmox_insecure=true` (or a certificate its
  clients trust).
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
- **`resolved.json`** is `resolved_version: 5`: a machine's size in each form (`resources.docker`,
  `resources.vm`), and a lab machine's `role` is `member` (was `target`).
- The example `pivot-dmz` is now `dmz-jump`.

## Runner authors

- **A lab machine's role is `member`** (was `target`, a word the manifest's top-level `target`
  uses for something else): `machines[].role` is `member`, `job`, `infra`, `access`, `tool` or
  `check` (`isoloom_manifest::Role::Member`, was `Role::Target`). A runner matching on `target`
  matches `member` instead.

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
  and a manifest naming an output twice is refused.
- A manifest giving one name twice in any list (machines, networks, tools, params, secret slots,
  outputs) is refused with one error, `Error::Duplicate { list, name }` (code `duplicate`), which
  replaces `Error::DuplicateMachine` (`duplicate_machine`).
- Isoloom's own Compose services start with `isoloom-`: a route sidecar is
  `isoloom-routes-<machine>` (was `<machine>-routes`), an `init` job `isoloom-init-<machine>-<n>`
  (was `<machine>-init-<n>`) and its step `job-isoloom-init-<machine>-<n>`.
- The controller is `isoloom-controller` in Proxmox's manifests and check output (was
  `controller`).
- `docker-vm`'s VM is `isoloom-docker-host` in its run manifests (Vagrant's and Proxmox's; was
  `docker`), as `cloud-docker`'s is: read it from `guest.machine` and `machines[]`. On QEMU, the
  VM's SSH port is `ISOLOOM_HOSTPORTS`' `isoloom-docker-host/ssh` key (was `docker/ssh`). The
  Vagrant define stays `default`.
- The demo secret names in the fixtures are `DEMO_SECRET` and `DEMO_SECRET_<X>`.
- On Vagrant (and `hybrid`), the controller's check runner step is `check-isoloom-controller`
  (was `check-controller`) and its runner script `checks/isoloom-controller.sh` (was
  `checks/controller.sh`). Isoloom's own VMs show as `<lab> - isoloom-controller`,
  `<lab> - isoloom-router` and `<lab> - isoloom-tool-shell` (`<lab>-isoloom-…` on Hyper-V and
  ESXi; were `controller`, `router`, `tool shell`), the docker-vm and hybrid hosts as
  `isoloom-docker-host` (were `docker`, `containers`): read them from `machines[].native.vm_names`.
- A Vagrant VM's display name (VirtualBox, VMware, Parallels, UTM) is `<lab> - <machine>`, ASCII
  (was `<lab> · <machine>`): Vagrant renames an existing VM when it next boots it.

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
- `isoloom_spec::images::{VagrantBox, HyperVBox, vagrant, HYPERV_DEBIAN_12, QEMU_DEBIAN_12}` and
  `isoloom_spec::windows::esxi_guest_os` (the backends' own now). `Lab::plan`, `Lab::catalogue`
  and `isoloom_targets::{backend, CloudDockerBackend, DockerVmBackend, HybridBackend}` are
  hidden, outside the supported API: generate with `isoloom_targets::generate`.
- `isoloom_manifest::access::{DEFAULT_BOX, DEFAULT_ARM64_BOX, DEFAULT_IMAGE}` and
  `isoloom_manifest::SUPPORTED_VERSION` (now crate-internal).
- The spec model's growing structs are `#[non_exhaustive]` (`Spec`, `Machine`, `Network`,
  `DockerImpl`, `VmImpl`, `VmImage`, `Constraints`, `Proxy`, `Mirrors`): build them with
  `Spec::new`, `VmImpl::new` or `Default`, then set fields, or parse a spec, and match them
  with `..`.
- In the building-block crates (`isoloom-plan`, `isoloom-render`, the backends,
  `isoloom-catalogue`), many items became crate-internal; those crates' API is not a promise.
