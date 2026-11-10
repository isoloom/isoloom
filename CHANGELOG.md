# Changelog

All notable changes to Isoloom are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The spec file format has its own version (`version: 1` in `isoloom.yml`), which is
independent of the crate version. 1.0 is the first stable release and the first
stable spec format.

## [Unreleased]

The next release is **2.0.0**: it removes library API and refuses a few specs 1.x took.
[Migrating to 2.0](docs/guide/migrating-to-2.0.md) says what to change.

### Highlights

- **Breaking:** the run manifest is `manifest_version: 1`, frozen with a compatibility policy;
  a runner built for 1.x's version 0 refuses it.
- **Breaking:** the `dvwa` and `juice-shop` catalogue entries are removed, and machine names
  starting with `isoloom-` are refused.
- **Breaking:** Kubernetes publishes a port as a ClusterIP Service by default, and its pods run
  under the Pod Security Standards' `baseline` level, which their namespace enforces.
- **Breaking (library):** many items of every crate are removed or crate-internal; the supported
  API is `isoloom-spec`, `isoloom-targets` and `isoloom-manifest`.
- Signed release binaries for Linux, macOS and Windows, with `SHA256SUMS`, a Sigstore signature,
  build provenance and a CycloneDX SBOM; `install.sh` and the GitHub Action check every download.
- `constraints.ca_certificates` (a corporate CA), a `tags:` map for cost allocation,
  `isoloom check --quiet` and `--json`, and a `.isoloom/.gitignore` for the tools' state.
- `isoloom generate` removes the files it no longer writes, and `isoloom check` reports them.

### Breaking

- **Breaking: what Isoloom names beside a machine is under `isoloom-`, which no machine takes.**
  The check runner of the networks' position is `isoloom-networks` (its script
  `checks/isoloom-networks.sh`, its run-manifest step `check-isoloom-networks`, its Compose
  service `isoloom-check-isoloom-networks`, `resolved.json`'s `checks.positions[].position`; was
  `networks`), a machine's `exec` checks' runner `isoloom-exec-<machine>` (`checks/isoloom-exec-<machine>.sh`,
  step `check-isoloom-exec-<machine>`; was `exec-<machine>`), and a machine's published ports'
  Kubernetes Service `isoloom-<machine>` (was `<machine>-published`). A machine named `networks`,
  `exec-web` or `web-published` took one of those names: `generate` stopped (`wrote … twice`) or
  Kubernetes kept one of two Services silently. A machine's name may now be 55 characters (was
  53), `isoloom_spec::validate::MACHINE_NAME_MAX`.
- **Breaking (runner): `project.archive` leaves Isoloom's outputs out but `with.outputs`.** The
  action's new optional `with.outputs` names the outputs the guest reads (paths from the project,
  separated by spaces; `.isoloom/docker .isoloom/configure` for a Compose VM); the archive follows
  `isoloom_manifest::ignore::Rules::for_copy(<.isoloomignore>, <outputs>)`, which also leaves out
  version control and the tools' state at any depth. A runner that took every output in copied
  other targets' Terraform state and logins into the guest. The tools' state patterns are
  `isoloom_manifest::ignore::{TERRAFORM_STATE, VAGRANT_STATE, RUNNER_STATE}`, and the reach folder
  is `isoloom_manifest::ignore::REACH_DIR` (was `isoloom_render::secrets::REACH_DIR`).
- **Breaking (library): `Manifest::validate` refuses a name twice in any of the manifest's lists**
  (machines, networks, tools, params, secret slots, outputs) with one error,
  `isoloom_manifest::Error::Duplicate { list, name }` (code `duplicate`), which replaces
  `Error::DuplicateMachine` (`duplicate_machine`). Two networks, tools, params or secret slots of
  one name were taken.
- **Breaking: a lab machine's role in the run manifest is `member`** (was `target`, which the
  manifest's top-level `target` names a target with): `machines[].role: member`,
  `isoloom_manifest::Role::Member` (was `Role::Target`). `resolved.json` says `member` too, and
  is `resolved_version: 5`. The schema's title says version 1 (it said 0).
- **Breaking: Isoloom's own Compose services all start with `isoloom-`**: a machine's route
  sidecar is `isoloom-routes-<machine>` (was `<machine>-routes`) and its `init` jobs
  `isoloom-init-<machine>-<n>` (was `<machine>-init-<n>`; their run-manifest steps
  `job-isoloom-init-<machine>-<n>`). A machine named `web-routes` or `web-init-1` replaced `web`'s
  sidecar or job in the Compose file, silently; the prefix is the one a lab's machines can't take.
- **Breaking: a published port's output is named `<machine>__<port>`** (`web__portal`, was
  `web_portal`), in snake case. Joined by one `_`, two ports could take one name (`web-admin`'s
  `ui` and `web`'s `admin-ui` were both `web_admin_ui`) or a Terraform output's (machine `ssh`'s
  port `user` was `ssh_user`), and the run manifest listed both. A machine's name has no `--`, so
  the names can't meet now, and `Manifest::validate` refuses an output named twice
  (`Error::Duplicate`, `duplicate`).
- **Breaking: the run manifest is version 1, and stable.** `manifest_version` is `1`; a reader
  refuses `0`. The schema is `schemas/run-manifest-v1.schema.json`, its `$id` (and each
  manifest's `$schema`) the committed file on `main`
  (`https://raw.githubusercontent.com/isoloom/isoloom/main/schemas/run-manifest-v1.schema.json`).
  The schema and the crate state the compatibility policy (`isoloom_manifest::COMPATIBILITY`):
  within a version only optional fields are added; removing or renaming a field, changing its
  meaning, or adding a value a runner must act on bumps the version.
- **Breaking: images the project builds are tagged `localhost/isoloom/<environment>-<machine>`**
  (was `isoloom/<environment>-<machine>`, a name in someone else's Docker Hub namespace)
  (`isoloom_plan::images::BUILT_IMAGE_PREFIX`).
- **Breaking: a Kubernetes published port is a cluster-internal Service** (`<machine>-published`),
  reached with the `kubectl port-forward` the manifests' header and the chart's notes give; no
  public `LoadBalancer` is opened. Pods run under the Pod Security Standards' `baseline` level,
  which the namespace enforces (`privileged` when a machine is privileged or sets a sysctl
  outside the safe set): the runtime's seccomp filter, no privilege escalation, capabilities
  dropped but those a root process of a stock image needs (no `NET_RAW`), Isoloom's busybox
  helpers as `nobody`. Each machine's CPU and memory are limited to its size (a container's
  default, 1 CPU and 512 MiB, when it declares none), and it reserves what it declares, else
  100m and 128 MiB.
- **Breaking: `proxmox_insecure` defaults to `false`**: the Proxmox API's certificate is
  verified unless the operator passes `proxmox_insecure=true`.
- **Breaking: renamed examples and fixtures, neutral words.** `examples/pivot-dmz` is
  `examples/dmz-jump` (its user machine Debian 12, not Kali); the run-manifest fixtures'
  `flag-portal` and `flag-vm` are `web-portal` and `web-vm`, their `CTF_FLAG_MAIN` input
  `DEMO_SECRET`; `fixtures/invalid/version-1` is `version-0`, beside a new `version-2`.
- **Breaking (library): the model's growing structs are `#[non_exhaustive]`** (`Spec`, `Machine`,
  `Network`, `DockerImpl`, `VmImpl`, `VmImage`, `Constraints`, `Proxy`, `Mirrors`), so a new
  spec key is a minor change: build them with `Spec::new`, `VmImpl::new` or `Default`, then set
  fields. `isoloom_render::configure::Configured::env` is new.
- **Breaking (repository): the development tasks that span repositories moved out**:
  `cargo xtask check-all`, `dev-link` and `host-setup` are the workspace repository's. The
  baselines are under `xtask/baselines/`, the validator image under `xtask/src/verify/`, the
  design examples under `crates/core/isoloom-plan/tests/spec-examples/`, the spelling list is
  `.typos.toml`.
- **Breaking: Isoloom's own Vagrant VMs are named after their define names**, under the
  `isoloom-` prefix a lab machine can't take: their display names (`<lab> - isoloom-controller`,
  `isoloom-router`, `isoloom-tool-shell`, `isoloom-docker-host` for the docker-vm and hybrid
  hosts; were `controller`, `router`, `tool shell`, `docker`, `containers`), the controller's
  check step (`check-isoloom-controller`, was `check-controller`) and its runner
  (`checks/isoloom-controller.sh`). A lab machine named `controller` with checks failed `generate`
  (one step id twice in the Vagrant run manifest), on `hybrid` its runner and the controller's were one file, the
  second replacing the first, and a machine named `router` or `containers` shared its VM name
  with Isoloom's. A backend writing two files at one path is now an error of the target.
- **Breaking: tags one cloud would merge or refuse are refused.** A key must start with a letter
  (Google Cloud's labels do: `9lives` failed at apply there); two keys equal but for case
  (`Team`, `team`: one tag on Azure) or one label once lowercased (`Cost.Center` and
  `cost_center`: one label on Google Cloud and Kubernetes, the second silently lost) are
  refused, and so are Isoloom's own keys in any case (`Managed-By`). Library: the tags' forms
  are `isoloom_spec::tags` (was `isoloom_render::tags`).

### Added

- **`constraints.ca_certificates`**: the site's certificate authorities (PEM files in the
  project), trusted by every Linux VM (`update-ca-certificates`, `update-ca-trust`, else the
  OpenSSL bundle) and Windows VM (its root store) once the project is in, by every image
  Isoloom builds (before its steps), and mounted into every container at
  `/usr/local/share/ca-certificates/isoloom-<n>.crt`.
- **`tags:`** in the spec and the defaults (a file, `ISOLOOM_TAGS__<KEY>`,
  `-s defaults.tags.<key>`), each value text, or a number or `true`/`false` read as its text:
  cost tags on every cloud resource (AWS default tags, Azure tags, Oracle Cloud free-form tags,
  Google Cloud labels, its boot disks and private DNS zone included, DigitalOcean and Linode
  `key:value` tags) and every Kubernetes object (labels: the manifests, the checks' Jobs, the
  chart's templates and tests, and the `ConfigMap`s the kustomizations generate, which carry
  `app.kubernetes.io/managed-by: isoloom` too).
- **`.isoloom/.gitignore`**, written and listed like every generated file: Terraform's state,
  backups, cache, lock info and crash logs, Vagrant's machine folders and the runner's reach
  files stay out of version control; `.terraform.lock.hcl` is meant to be committed.
- **`isoloom check --quiet`** (only what fails) **and `--json`** (one result per file, then a
  summary).
- **`isoloom docs --cli`**: the command-line reference, `docs/reference/cli.md`, kept current by
  a test.
- **`isoloom schema` prints the committed schema**, the catalogue's `with:` rules included.
- **Tier evidence records the Isoloom version** whose files ran (`cargo xtask tiers record
  --isoloom`, this checkout's by default; `Run::isoloom`).
- Each published crate ships `LICENSE` and `NOTICE`; `cargo binstall isoloom` finds the release
  archives; the crates' homepage is the docs on GitHub.
- **Release binaries, signed.** A `vX.Y.Z` tag builds `isoloom` for `x86_64` and `aarch64` Linux
  (static, musl), `x86_64` and `aarch64` macOS and `x86_64` Windows, after the test gate and only
  when the tag is the workspace version. The release carries the archives (with the licences,
  `NOTICE` and the third-party licences), `SHA256SUMS`, a keyless cosign signature of it, a build
  provenance attestation of every file and a CycloneDX SBOM, with the CHANGELOG's section as its
  notes; then the published release is installed with `install.sh` and the Action and run. 1.x
  releases had no binaries. [Verify a release](docs/guide/verify-a-release.md).
- **Documentation by purpose** under `docs/`: guides (getting started, one per target, corporate
  networks, golden images, offline, CI, committing `.isoloom/`, remote Terraform state,
  troubleshooting), reference (configuration, tool versions, policies, examples), explanation
  (architecture, security considerations, determinism) and maintainer pages; `SUPPORT.md`,
  `GOVERNANCE.md`, `CODEOWNERS` and a Dependabot configuration.

- **`isoloom generate` removes the files it no longer writes, and `isoloom check` reports them.**
  A lab that stopped allowing a target (a Debian machine made Windows: `docker`, `docker-vm`,
  `cloud-docker`, `kubernetes` and four clouds' `cloud-vm`) kept that target's old files and
  `run.json`, and `check` passed. `generate` now lists what it wrote in `.isoloom/.generated` and
  removes a listed file it no longer writes (every target's under `.isoloom/`; with `--target`,
  that target's folder only), then a folder that leaves empty; `check` fails on such a file.
  Isoloom owns exactly the listed files: what a runner or its tools write there
  (`.terraform.lock.hcl`, `.isoloom-reach/`, `.vagrant/`) stays, and so does its folder. A list
  line that isn't a plain path under `.isoloom/` (absolute, or with `..`), or a file reached
  through a symbolic link, is an error before anything is removed, so a removal never leaves
  `.isoloom/`.
- **The library takes the person's regions like the image table** (`Lab::with_regions`, from
  `Defaults::regions`; `Environment::regions`), and says each cloud's built-in one
  (`isoloom_targets::default_regions`). Also: `isoloom_spec::MachineSize` (a machine's size in a
  form), `isoloom_render::run_manifest::{machine, resources, secret_source}`,
  `MachinePlan::{reads_input, reads_input_file}`, `isoloom_plan::routing::{forwarding,
  routes_ipv6}`, `isoloom_render::esxi::guest_os`, `isoloom_terraform::hcl_quote` and
  `isoloom_targets::Error::Instance`.
- **`isoloom validate` shows the spec's warnings**: a daemon a recipe installs with no `service`
  step for it, a `seal: best-effort` machine. Each reads `! warning: <field>` with its reason and
  fix, and `--json` lists them under `warnings`; they never fail validation. They were worked out
  (`Lab::warnings`) but shown nowhere.
- **The run manifest says what each Vagrant VM takes only when it boots** (`machines[].native.boot_digest`):
  a digest of the VM's Vagrantfile configuration but its provisioners (synced folders and
  `host_folders`, networks, forwarded ports, size, provider settings), on `vagrant` and on
  `docker-vm`'s VM. Vagrant applies those only when a VM boots or reloads, so a runner updating a
  running run compares it with the digest the VM booted with and reloads the VM when it changed: a
  host folder added to a running `docker-vm` lab is then mounted, not an empty folder. Optional; an
  older manifest without it still reads.
- **A machine can mount a folder of the computer that runs the lab** (`machines.<name>.host_folders:
  [{ from, path, read_only }]`), both ways: what the machine writes there lands on that computer.
  `from` is relative to the project folder (it must exist), in the home folder (`~/work`) or
  absolute (the running computer's, unchecked, created when missing); `path` is an absolute path
  in a Linux machine, not inside Isoloom's own folders or the kernel's. On `docker`, a bind mount;
  on `vagrant`, each provider's own two-way shared folder (VirtualBox `virtualbox`, VMware
  `vmware`, Parallels `parallels`, UTM `utm`, QEMU `qemu_virtiofs` with `virtiofsd` on the host,
  libvirt `virtiofs`); on `docker-vm`, shared into its VM the same way, then bound into the
  container. Hyper-V (its only two-way folder, SMB, asks for a password at every `vagrant up`)
  and ESXi refuse it, QEMU a read-only one (whichever VM has it), and so does `docker-vm`'s Proxmox server, whose module
  isn't written then. Every target whose machines run elsewhere (`hosted`, `cloud-docker`,
  `kubernetes`, `proxmox`, `esxi`, `cloud-vm`, `external`) and `hybrid` refuse it
  (`host-folders` in the capability matrix). The Vagrantfile stops `vagrant up` on any provider
  that doesn't share the folders, one Isoloom doesn't know included, before creating the VM.
- **Kali on AWS `cloud-vm`.** `os: kali` boots Kali's own image from the AWS Marketplace, pinned
  to its product (the name alone could match a look-alike), logging in as `kali`, like any other
  OS of AWS's image table. An account may have to accept the image's terms on the Marketplace once.
- **Each `cloud-vm` machine has its `ssh_user_<machine>` output.** The runner signs in to a
  machine as `ssh_user_<machine>` (then `ssh_user`, then root); the modules only gave an
  `ssh_users` map, so on AWS a secret's delivery signed in as root and was refused.

### Changed

- **CI builds the Isothrum runner only at a pinned ref**: `ISOTHRUM_REF` is a release tag or a
  commit SHA, never a branch (`.github/scripts/isothrum-ref.sh`); it no longer falls back to
  `main`. Unset, CI's Compose real runs are skipped (the jobs that need `isoloom` alone still
  run) and the real-host workflows fail before building.
- **`isoloom_manifest::COMPATIBILITY` is the document's**, and the crate says what it means for
  its Rust types: they mirror the document and stay exhaustive, so a new optional field (a minor
  release) is a new struct field; building them by hand isn't part of the library promise, as
  for the spec model ([policies](docs/reference/policies.md#what-is-stable)).
- **Step timeouts count time without progress**: the manifest's `timeout_s` and
  `provider_timeout_s` say so (a step stops after that long without new output, or at six
  hours, or the limit when longer), which a runner follows.
- **Helper images pinned by digest**: `curlimages/curl:8.22.0`, `busybox:1.37.0-musl`, and
  `alpine:3.24` in place of the unsupported 3.20.
- The ESXi module's header takes the password from `TF_VAR_esxi_password`, never a command line.
- Examples, fixtures and tests use neutral words (`DEMO_SECRET`, `web-portal`, `admin`).
- **`packaging/install.sh` and the GitHub Action check the archive against the release's
  `SHA256SUMS`**, the Action on Windows too (it checked nothing there); the archives hold an
  `isoloom-<target>/` folder. CI runs on every push to `main` and every pull request, with
  SemVer checked against crates.io, the MSRV checked with Rust 1.88 itself, and the links between
  the docs checked. Contributions no longer ask for a DCO sign-off, which nothing checked.

- **A machine's size follows one rule everywhere**: its `resources`, each field left out the
  default of the form it runs as (a container 1 CPU, 512 MB, 5 GB; a VM 1 CPU, 1024 MB, 20 GB).
  Every lab machine of a run manifest carries `machines[].resources` as it runs on that target
  (Compose set it only when the spec did, Vagrant always with the VM defaults, Terraform never),
  and `resolved.json` gives a machine's size in each of its forms, `resources.docker` and
  `resources.vm` (it gave every machine the VM defaults): **`resolved_version` is 4** (5 since a
  lab machine's role is `member`). The spec's
  docs say so.
- **An input read as a file is the input's own slot on every target**: on `cloud-vm` it was a
  secret slot named `input-<name>`; it is the slot `<INPUT>` of kind `input`, delivered over SSH to
  the machines reading it as a file (and an entry of the module's `inputs` for those reading it in
  their environment). A VM keeps its value under the input's name
  (`/var/lib/isoloom/secrets/<INPUT>`, was `input-<name>`). A `cloud-vm` secret slot names its
  store and is required, as on `docker` and `vagrant`.
- **Every Kubernetes object carries `app.kubernetes.io/managed-by: isoloom`**, as Compose's
  containers carry `isoloom.managed` and the clouds' resources `managed-by`. Google Cloud's project
  of its own and Linode's firewalls carry the environment's labels and tags.
- **The controller is `isoloom-controller` in Proxmox's run manifests and in Proxmox's and ESXi's
  `checks` output** (was `controller`), as on the clouds.
- **`docker-vm`'s `up` provisions `nat-route` first** (`--provision-with nat-route,...`),
  and every Linux VM of the Vagrant target has that provisioner first: see Fixed.
- The run manifest's `entry` output role is "the address a user opens first", and a dormant port
  "one nothing listens on until something in the machine starts it" (schemas regenerated).
- CI's `check` job runs the quality ratchet, as the release gate does; its `terraform` job
  validates the ESXi modules and the cloud-services module too. `publish-crates.yml` pins
  `actions/checkout` and `rust-lang/crates-io-auth-action` to commit SHAs.
- **`isoloom check` checks what `isoloom generate` writes**: a profile's pinned target, or the one
  `--target` names (new), else every target. A target asked for that refuses the lab fails.
- **A cloud refuses a machine bigger than its largest size**, naming the machine, the memory it
  asks for and that size (DigitalOcean 8 GB; AWS, Google Cloud and Linode 16 GB; AWS and Google
  Cloud 32 GB for a machine that nests; Azure 128 GB). It used to get the largest size silently.
- **The controller's disk is the spec's `controller.resources.disk_gb`, else 10 GB, on every
  target** (AWS ignored the spec's; Proxmox gave 8 GB, AWS and Google Cloud the image's). Azure
  keeps its image's 30 GB floor. ESXi's controller installs the SSH client, as the others do.
- **`terraform init` may take 900 s on every target** (Proxmox's run manifests gave it 600 s).
- **`cargo xtask catalogue check` requires `--runner`**: there is no default runner.
- **Machine names and aliases starting with `isoloom-` are refused.** They are Isoloom's own (its
  controller, the containers' VM, the check stand-in and runners, the route sidecars, the init
  jobs): a machine named `isoloom-access` collided with the stand-in, and `isoloom-web` shared
  `web`'s check step. `docs/spec.md` says so.
- **Compose's start removes the services the lab no longer has** (`up --remove-orphans`), on this
  computer and in a VM: a machine taken out of a lab, then the run brought up to date, goes.
  Services behind a profile that isn't started are still the file's, so they stay.
- **A `cloud-vm` machine sets itself up once, from its first boot's user data.** Its instance
  ignores later changes to its user data (`lifecycle { ignore_changes }`): a later apply without
  the launch's values, which a run never keeps, no longer stops and restarts it to change them
  (AWS) or replaces it with a new VM (Azure's `custom_data`, GCP's `metadata_startup_script`,
  DigitalOcean's `user_data`, Oracle Cloud's `metadata["user_data"]`, Linode's
  `metadata[0].user_data`, which its provider replaces an instance for too). The controller
  (Ansible) does the same, AWS's included.
- **`cloud-vm` refuses a machine whose user data is over its cloud's cap**, naming the machine
  (the controller included), instead of letting the apply fail: AWS's 16 KB counted after gzip,
  Azure's 65535 bytes of custom data, Google Cloud's 256 KB startup script, DigitalOcean's 64 KB,
  Linode's 65535 bytes, and Oracle Cloud's 32000 bytes of metadata (the user data's base64 and
  1 KB for the SSH key). The other clouds keep their modules (`isoloom targets` says which).
  What is measured is the text Isoloom writes; the values Terraform fills in at apply are left
  out.
- **A `cloud-docker` host keeps its first boot's expiry** the same way: a later apply with another
  `expires_at` no longer stops and starts the AWS host or replaces the Azure, DigitalOcean, Linode
  or Oracle Cloud one. GCP's startup script is a `metadata` entry, which the provider updates in
  place, and the next boot schedules the new expiry.
- **Generated files and messages name no particular runner.** Usage comments, the check
  runners' headers, a set-up's waits for its secrets and its project, and the `ssh_public_key`
  variable's description say "the runner (run.json)"; a refusal no longer names the removed
  `isoloom run`.
- **A `cloud-vm` Linux machine is named after itself**, as on every other target, not after its
  address or its image (Kali's names itself `kali`); kept through a reboot.
- **A refusal for an OS AWS has no image of lists the ones it has from its image table**, Kali
  among them (it was left out); every cloud's list, and whether it has an image at all, is read
  from its own image table, so they can't drift (`Windows Server 2016/2019/2022/2025`; Oracle
  Cloud's `Debian 12/13 on Ubuntu 24.04` from the Ubuntu each OS runs on there).
- **A containers' host VM has at least 40 GB of disk** (`HostSize::MIN_DISK_GB`) on every cloud
  and on QEMU: room for a large image pulled on it (a full tools image unpacks to about 12 GB,
  and an access machine's may be one the runner is given) beside the lab's own. GCP had 30 GB fixed, Azure the image's default,
  and docker-vm on QEMU its own 40 GB whatever the containers needed; the size follows the
  containers beyond that, from the spec's defaults.
  `docker-vm` on Proxmox now follows it too (its VM's disk was 30 GB whatever the containers
  asked), and every containers' host has at least 2 GB of memory (`HostSize::MIN_MEMORY_MB`), as
  only `docker-vm`'s Vagrant VM had: a small lab's host on Proxmox and the clouds' run manifests
  had 1.5 GB.
- The run manifests' and the spec's docs no longer name a particular program running them.
- **The process Isoloom gives a container that runs services is its "supervisor"**, not its
  "runner", which is what runs a run manifest: the generated script's header ("run by Isoloom's
  supervisor"), the shell library's message for a service left to it, and the docs say so. The
  script's paths (`.isoloom/configure/<machine>/run`, `/usr/local/bin/isoloom-run`) are unchanged.
  In the library, `ContainerProcess::is_runner` is `is_supervisor` (the configure module is
  crate-internal, below).
- The tier list's evidence notes (`isoloom targets --json`, `isoloom matrix --json`) name no
  particular runner or build of one: "re-validated end to end through a runner".
- **Every Terraform module describes `ssh_public_key` from one definition** ("The SSH public key
  the machines let in: whoever holds its private key signs in, ..."), which ends with who does:
  the runner (run.json), to deliver the project and run the checks, on the clouds and Proxmox;
  Terraform itself, to set the machines up, on ESXi, which writes no run manifest. The Proxmox
  and ESXi modules each had their own ("Installed for the user `isoloom`..."). It stays required
  on the clouds and ESXi and empty by default on Proxmox.
- **`esxi` refuses a secret's or an input's value as a file** (`files: [{ path, secret }]`,
  `files: [{ path, input }]`): its machine waited for a runner to deliver the value, and ESXi
  writes no run manifest yet, so the apply failed after half an hour. The capability matrix says
  ✗ for `files[].secret`, `files[].input` and `files[].uid` on `esxi`.
- **The files of a target without a run manifest point at the by-hand way, not the runner.**
  Kubernetes' checks Job and Helm chart say how to pipe an `exec` check into its machine (`kubectl
  exec -i deploy/<machine> -- sh -s`) and its Secret's usage how to create it; ESXi's
  `ssh_public_key` says Terraform signs in with it. A check runner's header says what it prints
  ("each prints a PASS or FAIL line"), not who runs it.
- **The `openssh` catalogue entry's login account defaults to `user`** (was `ctf`). A lab that
  relied on the default and signs in as `ctf` sets `with: { username: ctf }`.
- **Generated text, schemas and the tier list name no particular kind of lab or host.** The run
  manifest's name hints are `my-lab` and `APP_TOKEN`, a runner-set param is "an operator's IP",
  an input "an access token, a password"; Vagrant's `ISOLOOM_SSH_PROXY_COMMAND` describes the
  param itself; the spec's examples, messages and the matrix say token where they said flag. The
  tier list's hosts are `linux-1`, `linux-2`, `macos-arm64` and `windows-11`, and its notes
  (`isoloom targets --json`, `isoloom matrix`) name no private lab or app.
- **A cloud's `cloud-docker` and `cloud-vm` modules share one source** (its usage, variables,
  provider, images and their users), so they no longer drift:
  - AWS's `cloud-docker` tags every resource as `cloud-vm` does (`isoloom-environment`,
    `isoloom-instance`; was `isoloom:environment` without the instance).
  - `cloud-docker`'s usage says what its cloud needs (`-var billing_account` on Google Cloud,
    `-var compartment_id` on Oracle Cloud, the `# Auth:` line on DigitalOcean, Linode and Oracle
    Cloud), its Oracle Cloud variables have their descriptions, and DigitalOcean and Linode have
    their `provider` block.
  - The Docker host's image is its cloud's image of Debian 12 from the image table (Ubuntu 24.04
    on Oracle Cloud), AWS's limited to x86-64; on Google Cloud its user is the image's, `admin`
    (was `isoloom`), as on `cloud-vm`.
  - Azure's Docker host takes `cloud-vm`'s OS disk and image, and every one of its resources the
    tags.
- **AWS `cloud-vm` instances are hardened as the `cloud-docker` host is**: an encrypted gp3 root
  disk, instance metadata by session token only (IMDSv2), and terminated when they shut
  themselves down (the run's expiry), not left stopped. A machine's own AMI
  (`overrides.aws.machines.<machine>.image`) must support IMDSv2.
- **DigitalOcean's `cloud-docker` firewall lets ICMP out**, as `cloud-vm`'s does: ping and path
  MTU discovery from the containers (every other cloud lets everything out).
- **One rule for what several crates wrote each their own way** (library): a check runner's name
  (`isoloom_render::container::check_runner`, `CHECK_RUNNER_PREFIX`), Isoloom's folder on a Linux
  VM (`isoloom_spec::linux::STATE_DIR`, every path under it derived from it) and on a Windows
  one (`isoloom_spec::windows::ISOLOOM_DIR`, `READY_FILE` derived from it), the containers' VM in a run manifest
  (`run_manifest::guest::GuestCompose::docker_host`, `docker_host_outputs`), Proxmox's server
  variables, a name's DNS-label rule (`isoloom_spec::validate::dns_label`, which the catalogue
  shares), and a step file's kind by its extension (`isoloom_spec::syntax::has_extension`;
  `isoloom_plan::lifecycle::has_extension` is gone). The `docker-vm` Proxmox module's variables read as the `proxmox` module's.
- **Every Terraform module and its run manifest name Terraform 1.6.0 as the oldest** (one rule,
  `required_version = ">= 1.6.0"`): the cloud modules' run manifests said 1.11.0, for
  write-only attributes the modules never used, and Terraform 1.6.6 validates every cloud and
  server module. Proxmox on macOS runs Terraform from `hashicorp/terraform:1.16.7`, pinned by
  digest (was 1.9.8).

### Fixed

- **`isoloom generate` never writes through a symbolic link.** A clone holding
  `.isoloom/docker/run.json` (or `.isoloom/docker`) as a link elsewhere had the generated file
  written where the link led, outside the project. A generated file whose path goes through a
  link, or is one, now stops `generate` and `check` before anything is written, and each file is
  written beside its place and renamed into it.
- **`docker-vm` and hybrid's containers' host never run a value given at launch.** Their
  Vagrantfiles handed the inputs to the `environment`, `checks` and `containers` provisioners as
  they were, and Vagrant writes a provisioner's environment into a double-quoted shell string: a
  `$(...)` or a backquote in a value ran in the VM, as root. The values now travel in base64, as
  the other VMs' always did, and each script decodes them first.
- **A cloud machine's set-up script is readable by root alone.** `cloud-vm`'s user data wrote
  `/var/lib/isoloom/setup.sh` under the default umask, readable by every account on the machine,
  and it holds the sensitive inputs in base64 (and, on the controller, its private key and the
  inventory with the Windows password). It is now a fresh file written under umask 077.
- **No copy of the project takes the tools' state, nor outputs its machine doesn't read.**
  `docker-vm`'s project step, the runner's `project.archive` (`cloud-docker`, `docker-vm` on
  Proxmox, `cloud-vm`) and ESXi's archive copied every one of Isoloom's outputs into the machines,
  other targets' folders included, with what the tools write there: a Terraform state (the secrets
  Terraform saw) and a cloud machine's login (`.isoloom-reach/`); ESXi's also took a state of the
  project's own. Every copy now follows one rule, `isoloom_manifest::ignore::copy_rules`, read
  after the project's `.isoloomignore` so it can't bring any of it back: Isoloom's outputs out but
  those the machine reads (a Compose VM, the `docker` target's folder and `.isoloom/configure/`;
  hybrid's host, its Compose file), version control, Terraform's cache, state, lock and crash
  logs, Vagrant's machine folders and the runner's logins out at any depth. The Vagrant project
  step takes `rules:` (the lines to follow) in place of `generated:` and `extra:`.
- **Ansible: a machine named with a `-` works.** Its role's variables were
  `isoloom_machine_web-front_file`, which Jinja reads as a subtraction, so every task naming a
  project file failed. A role's name, and the variables it registers, now take `_` for `-`
  (`isoloom_machine_web_front`).
- **A message naming a cloud output reaches the run manifest.** `{{ cloud.outputs.<name> }}`
  validated, but the run manifest's `message` was `null` and `isoloom message` failed (the
  `cloud-bucket` example). The message is read by the spec's one placeholder grammar
  (`isoloom_spec::syntax::placeholder::read`): a cloud output is `${outputs.<name>}` in the run
  manifest, and stays as written in `isoloom message` and `resolved.json`, since only the runner
  knows it. Library: `isoloom_targets::Error::Placeholder` says `why` (was `hint`), `fill` takes
  the message's paths only, `isoloom_spec::placeholders` (the `{{ }}` scanner) is public, and
  `isoloom_spec::syntax::placeholder::Part` is exhaustive.
- **A template filled in for two places generates.** Two `template:` steps of one machine
  reading the same file stopped `generate` (`wrote … rendered/conf/site.conf twice`); the filled
  file is written once per source.
- **Every command reads the defaults' tags as `generate` does, by the spec's rules.**
  `validate`, `targets` and `resources` read the spec without the defaults, so a tag the defaults
  add (`ISOLOOM_TAGS__TEAM=dev` beside the spec's `Team`) passed `validate` and then failed
  `generate`, blamed on the spec's line. Every command now builds the lab with the defaults
  applied, and a tag of the defaults no cloud takes, or clashing with the spec's, is refused where
  the defaults are loaded, naming the layer that set it (`defaults (environment
  ISOLOOM_TAGS__TEAM): tag `team`: …`). Library: `isoloom_spec::Error::DefaultTag`;
  `Resolved::tags_under` (was `Defaults::tags_under`) returns a `Result`.
- **A tag's rules are the strictest cloud's, as documented.** A value was allowed 255 bytes, but
  Linode takes a `key:value` tag of 3 to 50 characters (`x:` was too short, a long one too long),
  and the length was counted in bytes against a message saying characters. A tag is now refused
  unless `key:value` is 3 to 50 characters, its value on one line; the rules are
  `isoloom_spec::tags::{key_refusal, value_refusal}`, which the defaults' tags follow too.
- **A project path with a space works on every target.** A spec takes `provision/my setup.sh`,
  but the Dockerfiles wrote `COPY provision/my setup.sh provision/my setup.sh` and the set-up and
  check commands `sh provision/my setup.sh`, two words. Every `COPY` is now in its JSON form
  (`COPY ["provision/my setup.sh", …]`), and every shell command quotes the project path it runs
  (a VM's set-up and playbook, a check script on every target, the Vagrant check refresh).
- **The message's `.published` and `.url` name a published port**: validation took them on a
  port without `publish:`, which `isoloom message` then refused, and its fix suggested one; a
  fix is now a placeholder that resolves (a published port's `.published`, else a port's `.port`,
  else an address).
- **The Ansible project fails when a firewall step's collection has no pin** (library:
  `isoloom_ansible::Error::Collection`) instead of leaving `requirements.yml` out silently.
- **A check or `provision:` file is a playbook by its extension in any case**, as a step's file
  is (`checks/site.YML` ran as a shell script): one rule, `isoloom_spec::syntax::is_yaml_file`.
- **A tag from the environment or a `-s` is its text as written**: `ISOLOOM_TAGS__ACCOUNT=0x1F`
  was read as YAML, the tag `31` (`1e3` was `1000`).
- **Docker: a network appliance joins the existing networks it is on** (`overrides.docker.networks.<n>.external`),
  at its fixed address and after its own interfaces (the next `ethN`); its interfaces replaced
  every network, so it was left off them silently.
- **Kubernetes: a project script's `ConfigMap` key is valid and its own.** A key was the path
  with `/` as `__`: a space (`setup: [build/cache/my seed.sh]`) gave an invalid key and an
  unquoted `sh` command, and `a/b__c` and `a__b/c` shared one. Any byte but a letter, digit, `-`
  or `.` is now `_` and its two hex digits (`/` stays `__`), so every path has its own key, a
  single word.
- **`isoloom generate` refuses a bad `.isoloom/.generated` before it writes anything**: it wrote
  every file, then refused the list (a path outside `.isoloom/`, one reached through a link),
  leaving a half-done generation.
- **`isoloom check` checks `.isoloom/.generated`** like any generated file: a list deleted or
  edited passed, and `generate` then removed nothing it should. Files a 1.x `generate` wrote are
  in no list, so they are never removed: regenerate once from scratch
  ([migrating to 2.0](docs/guide/migrating-to-2.0.md)).
- **The Ansible renderer's multi-line strings read back as written.** A block scalar had no
  indentation indicator and its lines were trimmed: a command or file whose first line starts
  with spaces made YAML that doesn't parse, and trailing spaces and extra final newlines were lost (the
  shell renderer kept them). A block scalar now carries its indentation (`|2`) and keeps every
  final newline (`|2+`); text a block can't hold exactly (a line ending in a space, a control
  character) is double-quoted with escapes.
- **Security: the controller's inventory is owner-only.** On the clouds, Proxmox and ESXi the
  controller wrote `/etc/isoloom/inventory.ini`, which holds the Windows machines' generated
  password, world-readable (`0644`); it is `0600` like the controller's key, and only root (the
  playbooks, the checks) reads it.
- **Security: a box name never runs as Ruby.** The access machine's box was written into the
  Vagrantfile with Rust's debug quoting, which leaves Ruby's `#{…}` live: an image table giving
  `vagrant: 'corp/deb#{`id`}'` ran `id` on the host whenever Vagrant read the file. It is a
  Ruby string literal like every other now, and a box name (`vagrant`, `qemu`, `hyperv` in the
  image table or `vm.image`, `box` and `boxes` under `overrides.vagrant`) must be letters,
  digits, `.`, `_` and `-` after an optional `<org>/`; anything else is refused (`invalid-value`
  in a spec, an error reading the image table).
- **Security: Ansible never evaluates Jinja in the spec's text.** The Ansible renderer passed a
  step's command, path, name, unit or file contents to Ansible as plain strings, which Ansible
  templates: a `{{ lookup('pipe', …) }}` in a `command:` ran on the machine running the play
  instead of reaching the host as text. Every such string holding `{{`, `{%` or `{#` is now
  written `!unsafe`, and a project file (`copy`, `run`, `playbook`) is named through a task
  variable (`isoloom_machine_<m>_file`) beside the play's `isoloom_project`, so the shell renderer
  and the Ansible one install the same literal text.
- **A service whose recipe the machine's OS can't carry is a problem of the spec** (`conflict`
  at `machines.<m>.service`), as an app's own steps are a target's refusal: the plan dropped
  such a configuration and set the machine up without it. No entry of the built-in catalogue
  offers such an OS today; one that did would have been left unconfigured, silently.
- **`docker-vm`'s Vagrantfile runs the run manifest's commands, with their failure policy.** Its
  `environment` provisioner chained the start steps with `&&`, so a tolerated one (the pull of
  the lab's published images) stopped the start when it failed; each step now ends the script
  on failure unless the manifest tolerates it. Its `checks` provisioner runs the manifest's own
  stand-in set-up, runners and tear-down (one runner per position, in the Compose project the
  start made) instead of rebuilding them, and the Vagrant run manifest refuses to omit a VM's
  boot digest (`NoBootDigest`, a bug) instead of writing none. Library:
  `isoloom_render::run_manifest::guest::GuestCommand::in_vm_shell`,
  `isoloom_manifest::Placeholder::root`.
- **A Proxmox LXC container is a container's size** (1 CPU, 512 MB, 5 GB unless its `resources`
  say), as its run manifest said; `main.tf` gave it a VM's (1024 MB, 20 GB). Every backend sizes a
  lab machine by one rule now (library: `isoloom_plan::MachinePlan::size`, its `resources` the
  spec's; `isoloom_plan::Size` and `Size::effective` are removed).
- **`constraints.ca_certificates` reaches Isoloom's own VMs**: the containers' Docker host
  (`docker-vm` on Vagrant and Proxmox, `cloud-docker`, the hybrid host), the controller (Vagrant,
  Proxmox, the clouds, ESXi), the router and the `shell` tool's VM trust the site's authorities
  from the project before they install Docker, Ansible or packages; behind a TLS-inspecting proxy
  their downloads failed. The Docker host gets the project first now: `docker-vm`'s and
  `cloud-docker`'s run manifests copy it before the `docker` step (`archive`, `project`,
  `docker`), and the `docker-vm` Vagrantfile provisions `project` (then `ca-certificates`)
  before `docker`.
- **A Kubernetes pod keeps `SYS_CHROOT`**, the one capability of the runtime's default set
  besides `NET_RAW` it dropped, though `baseline` allows it: sshd's privilege separation needs it.
- **A network appliance keeps the spec's `volumes`, `host_folders` and `environment`** on the
  Docker targets: its own mounts and variables (configuration files, `IOL_PID`) replaced them,
  silently. They are added to the spec's now.
- **A Proxmox router waits for its uplink address, as its check says.** The provider's wait for an
  address ended at the first IPv4 address the guest agent reported, whichever card had it: the
  router's environment addresses, set statically, came first, so on an uplink whose DHCP was slow
  or absent the check failed after about a minute instead of waiting up to 8. The router now
  starts its agent once its uplink has an IPv4 address (or after 7 minutes), and the failure says
  that an IPv6 address alone isn't enough.

- **A default region reached `main.tf` but not the run**: `isoloom generate` rewrote a cloud
  module's text for the region in the person's defaults while `run.json`'s `region` param kept the
  built-in one, which a runner hands Terraform. The lab now writes the region into the module's
  variable and the param alike.
- **Proxmox refuses secrets and inputs as files**: its machine waited half an hour for a delivery
  the run manifest had no slot for, then failed. A Proxmox machine sits behind its router, which
  guest delivery doesn't go through, so nothing could deliver them (as on ESXi). An input in a
  machine's environment still reaches it.
- **Proxmox and ESXi run checks from a Linux VM or the controller**: a position's checks ran on
  its machine whatever its OS (mixed-office's piped `sh -s` into a Windows Server), and the
  networks' from the first VM, Windows or not. The rule is the clouds': the machine when it is a
  Linux VM, else the controller, else the first Linux VM.
- **DigitalOcean and Linode pick a size that fits the disk too** (a 50 GB disk got a 25 GB
  droplet), and refuse a disk larger than their largest size's, naming the machine and the size.
- **`cloud-docker` refuses containers that need more memory than the largest Docker host**
  (16 GB), naming the container that asks for the most; they got that host all the same.
- **On UTM, a Linux VM's internet takes the network that reaches it**: vagrant_utm gives a VM
  UTM's shared network and the plugin's emulated one (QEMU user mode, `10.0.2.0/24`), both with a
  DHCP default route at the same metric, and Linux picked the emulated one, which was about 12
  times slower one day (an image pull of a gigabyte took over half an hour in `docker-vm`'s VM)
  and fast the next, when the shared network reached nothing. A `nat-route` provisioner, first and
  at every `up`, gives the shared card's default route metric 50 when that network reaches a
  public resolver, 200 otherwise, now and in that card's own network configuration
  (NetworkManager, systemd-networkd or ifupdown). It never changes the emulated card, which
  carries Vagrant's SSH session and the forwarded ports: applying a configuration to it renewed
  its lease and cut that session. It does nothing on the other providers or without a second
  default route.
- **A gateway forwards IPv6 when the lab routes it**, on every VM target (it forwarded IPv4 only
  outside Compose); its forwarding is a plan step every VM backend renders, and routers and
  gateways decide IPv6 by one rule.
- **`isoloom_targets::resolved::resolve_with` fails for an instance the lab can't be** (library):
  it returned the lab as written while its snapshot said `"instance": n`.
- **`isoloom-catalogue` fails to build without its catalogue folder** instead of embedding an
  empty catalogue.
- **A cloud VM keeps its set-up across every reboot**: its set-up unit stays enabled, and at
  every boot it schedules the run's expiry again and brings back what a restart undoes (extra
  interfaces, routes, the inputs file); it used to stop first once the set-up was done. A
  cloud-docker host schedules its expiry from a boot unit too (its user data runs once).
- **Published ports moved inside a cloud VM survive a reboot**: the redirects are an nftables
  ruleset loaded at every boot, nft installed with the machine's package manager (no `sudo`
  or `apt-get` in the root set-up).
- **On `docker`, a gateway machine keeps the spec's `sysctls`** beside the forwarding ones; they
  were replaced.
- **Oracle Cloud's cloud-docker host gets the disk its containers need**
  (`boot_volume_size_in_gbs`, 50 GB at least).
- **Data volumes carry the environment's tags** on Google Cloud (labels), DigitalOcean and Linode,
  so a reaper finds them.
- **`docker-vm`'s run manifest names its VM only under the providers it offers** (no `hyperv`
  entry in `native.vm_names`).
- **Chart.yaml names the real packaging command** (`helm package .isoloom/kubernetes/chart
  --version <v>`).
- **`isoloom import vagrant` says what it changed in a private network**: an address that isn't
  IPv4 was dropped silently, and a netmask wider than /24 (`255.255.0.0`) narrowed to the
  address's /24 silently; each is a note now ("isn't a /24 to /29; re-addressed to ..."), as the
  Compose and Terraform imports say.
- **`isoloom generate` and `isoloom check` report every target whose backend failed**, each with
  its cause, not only the first.
- **A lab machine named `docker` no longer stops `docker-vm`** (and with it every `isoloom
  generate`: "`docker` appears twice"). The VM the containers run in is `isoloom-docker-host`
  everywhere, under the prefix a lab's machines can't take: `docker-vm`'s run manifests (Vagrant's
  and Proxmox's) named it `docker`, `cloud-docker`'s `isoloom-docker-host` and the hybrid
  Vagrantfile `isoloom-docker`. A runner reads the new name from the manifest (`guest.machine`,
  `machines[]`); on QEMU, the VM's SSH port it chooses is `ISOLOOM_HOSTPORTS`'
  `isoloom-docker-host/ssh` (was `docker/ssh`). The Vagrant define stays `default`. In the
  library, `isoloom_render::container::DOCKER_HOST` names it.
- **`isoloom generate` and `isoloom check` fail when a backend fails**, instead of leaving its
  target out as if the target refused the lab. A failure (a run manifest value the manifest's own
  types refuse, a file a writer couldn't write: a bug) printed a `·` line under `generate` and
  nothing under `check`, which passed while that target's committed files went stale. Only a
  refusal of the lab (the target can't run it, as `isoloom targets` says) leaves a target out
  now; anything else stops both commands with the cause. The failure names the target being
  generated (`docker-vm`, `cloud-docker`), not the backend's own (`vagrant`, `docker`).
  `isoloom check` reports a file it can't read with the system's reason, not as missing. In the
  library: `isoloom_render::Error::Internal` and `isoloom_targets::GenerateError::Internal` keep
  the cause, `GenerateError::is_refusal` tells them apart, `isoloom_targets::Error::Refused` is
  `Error::Generate`, and `isoloom_render::Error::reason` is `Error::refusal` (`None` for a
  failure).
- **A VM's boot digest changes when its ESXi port groups move.** A Vagrantfile picked each VM's
  port groups by its networks' places in a list above `Vagrant.configure` (`LAB_NETWORKS`), which
  the digest didn't cover: a network inserted ahead of another moved a VM to other port groups
  with its digest unchanged, so a runner didn't reload it. Each VM names its port groups by their
  index in `ESXI_VIRTUAL_NETWORK` now (`esxi_networks([1, 2])`), in its own `config.vm.define`,
  and `LAB_NETWORKS` is gone. Boot digests change.
- **Compose in a VM tolerates a failed pull of the lab's published images**, as on this
  computer: an unreachable registry falls back to building (docker-vm, cloud-docker), instead of
  failing the start. Both describe the start once now.
- **A lab's own access machine can start after its checks ran.** The checks that run from the
  access machine's seat run from a stand-in at its addresses (`isoloom-access`, check profile),
  which nothing took down: once checked, the lab's access machine couldn't come up (Docker:
  "Address already in use"). Every container target's run manifest now takes the stand-ins down
  in `checks.teardown` (`stand-ins-down`), after the checks whatever happened: the docker target,
  and Compose in a VM (docker-vm on Vagrant and on Proxmox, cloud-docker). Taking them down is a
  clean-up: its failure is logged and fails no check (`failure: tolerate`). The docker-vm
  Vagrantfile's `checks` provisioner takes them down after its runners too.
- **An AWS machine that asks for a TPM plans again.** Its instance had two `lifecycle` blocks (the
  NitroTPM precondition and the set-up once), which Terraform refuses ("Duplicate lifecycle
  block"); they are one block now.
- **The cloud-docker run manifest lists the lab's networks**, by their Compose names in the VM, as
  docker-vm's does: it listed none, so a runner reported none.
- **The access machine keeps its role on the Terraform targets** (`access` in the run manifest,
  not `target`), and **an AWS machine's root disk is at least its image's** (Kali's snapshot is
  25 GB; AWS refuses a smaller root volume), or the spec's when it asks for more.
- **A cloud machine gets a size with its CPUs and an interface per network.** `cloud-vm` picked
  a size by memory (and, on DigitalOcean and Linode, disk) alone: `cpus: 8, memory_mb: 1024`
  got AWS's `t3.micro` (2 vCPUs) while the run manifest said 8 CPUs, and AWS never counted a
  machine's network interfaces, so a machine on three networks, or the controller on every one,
  got a type that takes two and the apply failed. Every cloud's size table now lists each size's
  vCPUs, memory, disk where it fixes it, and interfaces (AWS: and addresses per interface), and
  one rule picks the smallest that takes all of them, for a machine and for the controller; more
  than the largest has is refused, naming what is short. Oracle Cloud's shape gets an OCPU per
  two CPUs or per 8 GB, whichever is more (at most 64 OCPUs and 1024 GB). Google Cloud's sizes
  carry their real memory (`e2-small` 2 GB, `e2-medium` 4 GB) and go up to `e2-standard-8`.
- **An AWS Windows machine answers WinRM with NTLM, not Basic.** Its first boot turned on Basic
  authentication, which sends the password with every request over plain HTTP; Terraform's
  set-up and the controller's Ansible sign in with NTLM now, as on Azure, and Basic stays off.
  The service still takes unencrypted messages, which Terraform's WinRM client needs.
- **The Proxmox and ESXi routers trust the site's certificate authorities** before they install
  their packages (cloud-init's `ca_certs`, each file read from the project), as the CHANGELOG's
  entry on Isoloom's own VMs said: only Vagrant's router did.
- **Isoloom's own router and controller take Isoloom's names on Proxmox, ESXi and the clouds.**
  A lab machine named `router` or `controller` wrote a second `proxmox_virtual_environment_file`
  of the same name (invalid Terraform), and on ESXi and the clouds a second VM, snippet or cloud
  resource of the same name. Their snippets, VMs and cloud resources are named
  `isoloom-router`/`isoloom-controller` now (`iso<slot>-isoloom-router`,
  `<env>-isoloom-controller`; Terraform labels `isoloom_router`, `isoloom_controller`), as their
  run-manifest machines already were, and the `checks` output names the controller
  `isoloom-controller`.

- **QEMU's hub ports are one block per environment.** Each network had its own hashed base
  port, its VMs the ports after it, so two networks could hand one port to two hubs (`n9` and
  `n235` in a lab named `lab`: 38221 twice), and one hub couldn't start. The environment's VMs
  now take consecutive ports from one base, never the same twice; past 20 000 linked VMs QEMU
  refuses the environment.

- **`isoloom import vagrant`'s help says the Vagrantfile's code runs**: it said nothing is
  created, but Ruby runs the whole file on this computer (only `Vagrant` is a stand-in), so
  import only a Vagrantfile you trust.

- **A Vagrant VM's display name is ASCII**: `<lab> - <machine>` (was `<lab> · <machine>`) on
  VirtualBox, VMware, Parallels and UTM, and the docker-vm and hybrid hosts. Vagrant interpolates
  the name into its own messages, and `vagrant reload` failed on the `·` (`incompatible character
  encodings: UTF-8 and ASCII-8BIT`, seen on UTM) after a first `up` had passed.

### Removed

- **The Ansible renderer's `ansible.windows.win_feature` task**, which no lab reached: the lab
  refuses a `packages` step on a Windows host that already exists. The renderer writes
  `ansible.builtin` and `ansible.posix` tasks only, as its docs say; reaching that step is an
  error (`isoloom_ansible::Error::WindowsPackages`, and `configure::role_files` returns a
  `Result`). `isoloom_plan::configure::Resource` is exhaustive, so a renderer handles every kind
  of step rather than dropping a new one.
- **The supported crates hold only the supported API.** `isoloom_spec::images::{VagrantBox,
  HyperVBox, vagrant, HYPERV_DEBIAN_12, QEMU_DEBIAN_12}`, the Vagrant boxes each OS boots, are the
  Vagrant backend's own; `isoloom_spec::windows::esxi_guest_os` is
  `isoloom_render::esxi::guest_os`, which the Vagrant and ESXi backends share. Items of the
  supported crates that hand out a building-block crate's types are `#[doc(hidden)]`, so not a
  promise: `Lab::plan` and `Lab::catalogue`, and `isoloom_targets::{backend,
  CloudDockerBackend, DockerVmBackend, HybridBackend}` (`isoloom_targets::generate` renders a
  target).
- **Library items replaced or nothing outside their crate used**:
  `isoloom_spec::defaults::{apply_to_files, set_region, BUILTIN_REGIONS, builtin}` (the lab takes
  the regions: `Lab::with_regions`) and `isoloom_spec::files` (`GeneratedFile` is
  `isoloom_render`'s, re-exported by `isoloom-targets` as before);
  `isoloom_targets::resolved::{render_message_at, HostPort}`. `isoloom_terraform::hcl_builder`
  (use `hcl_quote`), `isoloom_targets::tiers::os_id` and the `isoloom_render::more_addresses`
  re-export are crate-internal.
- **Library items nothing outside their crate used**: the HCL builder moved from
  `isoloom-render` into `isoloom-terraform`, crate-internal (`isoloom_terraform::hcl_quote`
  quotes a string), Ruby's quoting
  into `isoloom-vagrant` (`isoloom_vagrant::ruby_quote`); `isoloom_render::configure::{powershell,
  shell}`, `boot::{service, Kind}` and further render, plan, spec, ansible and targets functions
  are crate-internal; `isoloom_plan`'s `lab`, `outside`, `requirements` and `storage` modules
  and `isoloom_spec`'s `clones` and `error` modules are private (their types stay at the crate
  root); `isoloom_spec` no longer re-exports `Declared`, `Dns`, `KNOWN_OS`, `Network`, `Port`,
  `Reach`, `Resources`, `Shape` or `Vlan` at its root (`isoloom_spec::model` has them).
- **The dvwa and juice-shop catalogue entries** (security-training apps: a lab names their images
  itself). A spec with `service: dvwa` or `service: juice-shop` is refused with
  `unknown-reference`; `app: { image: ... }` gives the same container.
- **The Vagrantfile's hook for the runner's ad-hoc VM.** A Vagrantfile no longer carries
  `IsoloomAdHoc` (its `ADHOC_*` constants and the loader of `<VAGRANT_DOTFILE_PATH>/adhoc/*.rb`),
  and on QEMU a network's hub no longer keeps a spare port for that VM (`isoloom-access` is gone
  from `QEMU_HUBS`). The runner no longer stands up a VM of its own on a lab that declares no
  `access: true` machine: a lab's own access machine is unchanged, and a program that wants
  another machine adds it to the spec as an ordinary lab machine.
  `isoloom_manifest::access::DEFAULT_BOX` and `DEFAULT_ARM64_BOX`, which only that VM used, are
  removed too. The unreleased `hostname:` and `image:` of `IsoloomAdHoc.define` go with it.
- **`Spec::with_cloud_outputs`, `Spec::cloud_message` and `Copy::is_folder`**, which nothing in
  Isoloom called but their own tests, and the public `secrets::windows::command_line` (now
  test-only). `GuestCompose::empty` left the API: tests of the crates above build an empty
  guest with `isoloom_render::test_support::empty_guest`, behind the `test-support` feature.
- **`isoloom_plan::configure::{Configure, Handoff, OutPath}` and the plan's `Error::UnknownTool`
  and `Error::Unpaired`.** They described choosing a CM tool per target (`--configure`,
  `overrides.<target>.configure`), which no command or spec key ever offered: each target renders
  its machines' configuration its own way.
- **Run manifest values Isoloom never writes**: the `vagrant_ssh` and `winrm` transports of a
  guest delivery, the `dotenv` state-file format, the `generated` slot kind, the `ssh` and
  `credential` output roles, the `static` output source and the `minutes_from_now` expiry unit
  (`schemas/run-manifest-v1.schema.json` regenerated). A manifest holding one is refused.
- **`isoloom_compose::start_plan`**, which read a Compose file's start plan back from its text
  for a parity test, and `isoloom_compose::Error::Unreadable` with it; `StartPlan` is internal
  now (the run manifest's `up` is how to start the file). Also out of the public API, as nothing
  outside their crate used them: `isoloom_compose::{ENVIRONMENT_LABEL, MANAGED_LABEL}`,
  `isoloom_kubernetes::SECRETS`, `isoloom_catalogue::fill_contract`, `isoloom_spec::load_with`
  (`load_profiled(dir, None, sets)` does the same), `isoloom_plan::problems_in` (tests compose
  `Lab::parse` and `Lab::problems`), and `isoloom_render::steps::{DOCKER_VERSION,
  COMPOSE_VERSION, DOCKER_APT_KEY_SHA256}`, whose `INSTALL_DOCKER` is now `install_docker()`,
  built from them. Items only their own file used are private: `inventory::{CONTROLLER_KEY,
  winrm_vars, python_literal, host_vars_suffix}`, `container::host_folders::VM_ROOT`,
  `secrets::windows::{PLACE_SCRIPT, PLACE_TASK}`, `isoloom_spec::windows::expects_tpm`,
  `profile::valid_name`, `syntax::placeholder::Reference::allowed_in` and `FileSource::is_value`.
- **`isoloom_targets::matrix::Row::is_field`**, which only the matrix's own tests called.
- **`isoloom_manifest::access::DEFAULT_IMAGE`** (Kali's image), which nothing read: an access
  machine's Compose service runs its planned image (its OS's or `--images`'), and one with
  neither (no `os`) has no image at all.
- **More items only their own crate used**, now crate-internal (nothing outside it, Isoloom's
  consumers included, named them): `isoloom_ansible::configure::role_name`,
  `isoloom_catalogue::{Catalogue::shared_daemons, ContractCheck::runs_in, Release::os_list,
  Release::runs_on, conformance::FACT_PREFIX}`, `isoloom_plan::{DaemonTable::shipped_by,
  configure::unstarted, checks::HTTP_MAX_REDIRECTS}`, `isoloom_manifest::SUPPORTED_VERSION`,
  `isoloom_render::{configure::{rendered_path, uses_library, cloud_init::gz_b64},
  constraints::linux_prelude, inventory::host_value, ipv6::egress_rule, trunks::vm_commands,
  secrets::{DELIVERY_TIMEOUT, PLACE_SERVICE, windows::{BOOTSTRAP, place_first}}}`, `isoloom_spec::{Host::text_vars, model::{mapping_id, DEFAULT_FILE_MODE,
  SECRETS_DIR, TOOL_RECIPES, ULIMITS}, syntax::placeholder::Reference::is_run_time, validate::{DOCKER_BLOCK,
  CONTAINER_SYSCTLS}}` and `isoloom_targets::{Matrix::column_tiers,
  resolved::with_published}`; `Row::used_by` is the matrix tests' only. The cloud user-data caps
  (`isoloom_render::configure::cloud_init::{Limit, TooBig}`) moved into `isoloom-terraform`,
  crate-internal.
- **Library items nothing outside their crate used** (no dependent crate either):
  `isoloom_spec::syntax::reboots`, `Problem::section` (both the syntax's and
  validation's) are gone; `Lab::lowered`, the runner's side of secret delivery
  (`isoloom_render::secrets::receive`, `secrets::windows::{receive, powershell_args, input}`) and
  the matrix's `Authored` and `Matrix::cell` are test-only; `isoloom_targets::decision`, the
  matrix's `columns`, `rows`, `Row` and `Cell`, `isoloom_render::configure::supervisor`,
  `isoloom_plan::ansible`, `PortKind::tls`, `isoloom_render::secrets::KEPT_DIR` (and Windows'),
  the configure libraries' `LIBRARY`, `LIBRARY_PATH` and `library_file`,
  `network::trunks::vms` (the Linux VMs' set-up reads it)
  are crate-internal; `isoloom_spec::derive`, `isoloom_spec::model::after_drive`,
  `isoloom_catalogue::value_file` and `isoloom_plan::configure::codename` leave their re-exports.

## [1.0.9] — 2026-11-08

### Added

- **The runner can add a machine to a running `vagrant` environment.** As GNS3 adds a node to a
  live topology: every Vagrantfile now carries `IsoloomAdHoc`, with what a VM needs to join each
  lab network on each provider (its name on the hypervisor, its netmask, the lab machines' names
  for the VM's hosts file, this host's default box), and loads the definitions the runner writes
  under the run's state folder (`<VAGRANT_DOTFILE_PATH>/adhoc/*.rb`) at the end of
  `Vagrant.configure`. A definition is one `IsoloomAdHoc.define(config, name, network:, ip:,
  box:)` line, `autostart: false`: `vagrant up` never starts it, the runner boots it alone while
  the lab runs and destroys it alone. This is how a VM lab whose spec declares no `access: true`
  machine gets its attack VM (Isothrum 1.0.5). Its default box follows the host's CPU as OS images
  do: Kali's `kalilinux/rolling` on x86, the Debian cloud image on Arm (Kali's box has no arm64
  build). Hyper-V, ESXi and UTM give a late VM no lab network this way.

### Changed

- **On QEMU every lab network links its VMs through a hub.** Two VMs on a network were a socket
  pair (one listening, the other connecting), which left a VM added later no way in: the hub now
  serves every network, with one port kept for the runner's ad-hoc VM (`isoloom-access` in
  `QEMU_HUBS`), reserved when the Vagrantfile is written.

### Fixed

- **A container lab in a VM boots on QEMU.** The docker-vm helper VM kept `generic/debian12` on
  the QEMU provider, whose libvirt build never comes up under vagrant-qemu (the VM starts, SSH
  never answers, `vagrant up` gives up after the boot timeout). On QEMU the helper now boots the
  Debian cloud image the Vagrant target's VMs use there (`cloud-image/debian-12`), with its 10 GB
  disk grown to 40 GB for Docker's images and the lab's volumes.
- **A VM's fixed forwarded ports no longer fail as collisions on macOS 27.** Vagrant's own check
  of a host port before forwarding it (`Socket.tcp` with a timeout) reads every port as in use on
  macOS 27 (Ruby bug #22223: a refused connection hands back a socket), so a docker-vm or VM lab
  with a fixed published port failed at "forwarded port … is already in use" before its VM
  booted, on UTM, VirtualBox, Parallels and VMware alike. Every generated Vagrantfile now
  corrects that check on a Mac, reading the socket's own error.
- **A Docker host on an Arm CPU runs a lab's x86 images.** A lab's images are built for x86
  unless its spec says otherwise, and a docker-vm helper VM on an Apple Silicon Mac (UTM, QEMU)
  or an Arm cloud VM had no way to run them: the lab's probe failed with `exit 255` right after
  the images were built. The Docker host's set-up now installs QEMU's user-mode emulation
  (`qemu-user-static`, registered with binfmt) on a non-x86 Debian or Ubuntu host, as Docker
  Desktop provides on its own.

## [1.0.8] — 2026-11-07

### Fixed

- **Cloud-docker creates the `sshd` drop-in directory before writing to it.** The run manifest
  wrote the `AcceptEnv` drop-in to `/etc/ssh/sshd_config.d/` without first ensuring the directory
  exists, so a bare VM without it failed the setup. The directory is now created before the
  drop-in is written.
- **The QEMU docker-vm helper VM's SSH forward takes a chosen free host port.** The forwarded SSH
  port was left to collide; it is now bound to a free host port picked ahead of time, matching how
  the other providers forward it.

### Changed

- **Isothrum references point at the new `Isothrum` GitHub org.** The runner moved to its own
  organisation, and the references across the workspace follow it.
- **Release tags now gate on tests and the quality ratchet before publishing.** Pushing a release
  tag runs the full test suite and the quality ratchet first, so a red tree can no longer publish.

## [1.0.7] — 2026-11-06

### Fixed

- **docker-vm runs on QEMU and Hyper-V.** The helper VM's box was `bento/debian-12`, which ships
  only VirtualBox, VMware and Parallels builds, so running a container lab in a VM on QEMU or
  Hyper-V could not find the box. It is now `generic/debian12`, which has a build for every
  provider the docker-vm supports (VirtualBox, VMware, Parallels, libvirt, QEMU and Hyper-V); UTM,
  which has none, takes `utm/bookworm`.

## [1.0.6] — 2026-11-06

### Fixed

- **A container lab whose every published port is `fixed:` can start with the embedder's "pin the
  ports" option.** The Compose manifest declared `ISOLOOM_PUBLISH_FIXED` only when a lab had a
  random (ephemeral) port, so passing it to an all-fixed lab failed with "isn't a param of this
  manifest". It is now declared whenever a lab publishes any port (a no-op where a port is already
  pinned to its `publish:` number).
- **Cloud-docker delivers the launch's input values to the containers.** The VM a cloud-docker run
  creates is set up bare over SSH, so its `sshd` dropped the variables the runner forwards
  (`SendEnv`), and a container that reads an input (e.g. a launch token) failed to start with the
  variable "is not set". The run manifest now tells the VM's `sshd` to accept them (`AcceptEnv`)
  before the containers start, as docker-vm on Proxmox already does through cloud-init.

## [1.0.5] — 2026-11-05

### Added

- **A local VM lab's published ports get a free host port, picked by the runner** (paired with
  Isothrum 1.0.3). A privileged port (80/443) no longer needs administrator rights to forward, and
  two labs that publish the same port no longer collide. Opt back into the fixed numbers with
  `ISOLOOM_PUBLISH_FIXED`.

## [1.0.4] — 2026-11-04

### Changed

- A Compose setup job is given the project at `/opt/isoloom`, as the VMs have it.

## [1.0.3] — 2026-11-03

### Added

- **Per-target resource totals** — `isoloom_spec::totals_for(spec, target)` sizes each machine the
  way the chosen target runs it (a container reserves less memory and disk than a VM), so the same
  spec reads lighter on the Docker targets than on the VM ones. The existing `totals` keeps
  reporting the all-VM figure (the heaviest form). Lets a consumer show a lab's minimum to run as a
  Docker-to-VM range.

## [1.0.2] — 2026-10-27

### Added

- **A container machine can declare both `app.image` and `app.build`** — the Compose
  `build:`+`image:` combination. The build is tagged with the published image name, so a
  developer builds and tags it locally while a student pulls the pre-built image. The former
  rule that a machine used *either* `image` *or* `build` is relaxed; a machine that still gives
  only one behaves exactly as before.
- **A lab that publishes a built image now pulls it before `up`.** When a machine publishes an
  image it also builds, the Docker run does `docker compose pull --ignore-pull-failures` first
  and `up` drops `--build`: a present published image is used as-is and only a missing one is
  built from its context (pull-if-available-else-build). A lab that only builds locally keeps
  `--build` and gains no pull step, so an author's edits always rebuild. The same treatment
  applies to the Docker-in-VM guest path.
- **libvirt boxes for Windows Server 2016 and 2019**, so Windows VM labs deploy on
  vagrant-libvirt (KVM); the StefanScherer boxes those versions mapped to are
  VirtualBox/VMware-only. (2022/2025/10/11 already had libvirt builds.)

## [1.0.1] — 2026-10-18

### Fixed

- **A lab with an access machine now passes `isothrum check` on Docker.** The Compose check
  runners wrongly depended on the access box (and its routes), which live in the on-demand
  `access` profile; running the checks with just the `check` profile made `docker compose`
  reject the project ("depends on undefined service"). Check runners no longer wait on the
  access box, and a check that runs *from* the access position uses the `check`-profile stand-in
  (an idle container at the box's addresses) rather than the box itself. Affects every target
  that renders Compose (docker, docker-vm, cloud-docker).

## [1.0.0] — 2026-10-18

The first stable release. It is a ground-up rebuild of the 0.x preview: a single,
tool-neutral spec that converts to every supported IaC and configuration-management
tool, with no runtime. The `0.11.x` series on crates.io were the release candidates
for this release.

**Isoloom 1.0 does not read 0.x specs.** There is no compatibility layer and no
`isoloom migrate`: an old spec fails validation with an error naming the unknown
field. See [`docs/guide/migrating-to-1.0.md`](docs/guide/migrating-to-1.0.md) for the field-by-field
changes and before/after examples.

### Added

- **One spec, one form per machine.** A machine declares *what* it is — an `os` plus
  either a `service` from the catalogue (with typed `with:` settings) or a custom `app`
  (an own `image`, or a `build` recipe) — and never *how* a platform runs it. Each
  target decides the delivery: an official or built container image on container targets,
  a fresh VM plus a configuration role on VM targets.
- **Name-keyed `ports:`.** Ports are a map keyed by name (`ports: { web: { port: 80,
  protocol: http, publish: 8080 } }`). The name is used everywhere downstream: Compose
  labels, Terraform outputs, the resolved snapshot, and the run manifest
  (`${published.<machine>.<name>}`), and it fills message placeholders such as
  `{{ machines.web.ports.web.url }}`.
- **`inputs:` and file delivery.** Values given at each start (a flag, a launch token)
  are declared under `inputs:` and are sensitive by default. A sensitive input reaches a
  machine as a file (`files: [{ path, input }]`), never as an environment variable; a
  non-secret input can be placed in `environment:` explicitly. Values never land in a
  generated file.
- **`setup:`** — the once-at-start seed steps for a machine, rendered the same way on the
  container and VM forms.
- **The configuration model.** `app.steps:` carries a typed, tool-neutral resource model
  (packages, users and groups, files and templates, copies, commands with an
  already-done guard, firewall rules and services). It renders to Ansible, cloud-init,
  shell/PowerShell and Dockerfile, with the container and VM forms kept at parity by
  construction.
- **The catalogue.** `isoloom-catalogue`: data-driven service entries (a container form,
  a VM form, and a parity contract), release matching, typed `with:` settings, and
  `cargo xtask catalogue refresh` / `catalogue check` to keep pins current.
- **`isoloom verify`.** Every generated file is run through its format's official
  validator (Compose, Terraform + tflint, Vagrant, kubeconform + Helm, cloud-init,
  hadolint, shellcheck, and more), with a pinned toolchain image and a tracked baseline.
- **The capability matrix and tiers.** `isoloom matrix` shows, per target, what each spec
  field becomes (generated, refused with the reason, or not applicable), worked out from
  the generators themselves, plus each target's tier (how far it has been proven by real
  runs). Generated to `docs/matrix.md`.
- **The egress seal.** An `internet: false` network is a real guarantee. On cloud targets
  it is sealed at the network level by a `seal` step the runner applies once the
  environment is ready, and it fails closed. A machine may acknowledge a best-effort seal
  with `machines.*.seal: best-effort` (needed only for a privileged container on an
  offline network).
- **The run manifest.** Each target emits a versioned `run.json` next to its files: native
  commands and their order, readiness criteria, outputs (addresses, host ports, URLs),
  how to run checks, and secret slots (names and destinations, never values). It is the
  contract the Isothrum runner consumes.
- **Six clouds.** `cloud-vm`, `cloud-docker` and `cloud-services` on AWS, Azure, GCP,
  DigitalOcean, Linode and Oracle Cloud, each through Terraform/OpenTofu with the
  provider's own CLI credentials and its native secret store (with a documented SSH
  guest-delivery fallback where a store doesn't fit).
- **`isoloom import`** drafts an `isoloom.yml` from native files you already have.
- **Cross-platform output.** The CLI ships for macOS, Windows and Linux, and generated
  output runs from all three hosts (no `sh`-only `local-exec`, no baked-in host path
  separators or line endings).

### Changed

- **Rearchitecture into a lowering front end.** The new spec (`isoloom-spec::Spec`) is
  parsed and checked, then *lowered* per target into the plan IR, which carries the
  container and VM forms. The backends (Compose, Kubernetes/Helm/Kustomize, Vagrant,
  Terraform, Ansible) and their 800+ tests keep guarding the output. Crates are grouped
  under `core/`, `backends/`, `tools/`, `targets/` and `cli/`, with a one-way dependency
  direction enforced by a test.
- **`isoloom-external` is now `isoloom-ansible`.** Existing hosts become an Ansible
  inventory plus playbooks.
- **Run manifest: the top-level `lab:` key is now `environment:`** (`name`, `instance`,
  `message`; same shape). The manifest is a general-purpose contract, so the CTF/courses
  term is gone. A manifest still carrying `lab:` is rejected as an unknown field.
- **`Version` ordering is semver-correct.** The internal `Version` newtype now compares its
  numbers numerically (so `2.14.0 > 2.9.0`, and a pre-release sorts before its release)
  instead of lexicographically. Generated output is unchanged.
- **Dev tasks are `cargo xtask` commands**, not shell scripts, so they run the same on
  macOS, Windows and Linux.

### Removed

- **The runtime.** `isoloom-runtime` and the runtime half of the CLI
  (`run`/`test`/`down`/`status`/`connect`/`exec`/`capture`/`reset`/`provision`, the
  registry, JSON lifecycle events, run-time secret delivery and Isoloom-executed checks)
  are gone. Isoloom converts; the Isothrum runner runs.
- **`isoloom-coverage`** (the 30-table format-coverage metric and its drift job), replaced
  by the generated capability matrix and conformance against official validators.
- **Backward compatibility.** No field aliases, no deprecations, no `isoloom migrate`, and
  no tests on old specs. The `docker:` / `vm:` machine blocks and the list-shaped `ports:`
  are gone (see the migration guide).

### Security

- The Docker bootstrap installs from the official apt repository with a key pinned by
  `sha256` and a resolved version (`get.docker.com` kept only as the RPM fallback), and it
  fails closed.
- A privileged container (or `NET_ADMIN` / `NET_RAW`) is refused on an `internet: false`
  or offline network unless it opts in with `seal: best-effort`, so the local seal is a
  real guarantee.
- File `mode` values are quoted in the generated configuration.
- Ansible Galaxy collections are pinned by `sha256`.
- Base cloud OS images (Proxmox, docker-vm, ESXi) are pinned to dated releases with
  checksums.
- Secrets and inputs never appear in a generated file: outputs carry references only, and
  the runner delivers the values over SSH stdin.
