# Changelog

All notable changes to Isoloom are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The spec file format has its own version (`version: 1` in `isoloom.yml`), which is
independent of the crate version. 1.0 is the first stable release and the first
stable spec format.

## [Unreleased]

### Added

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

- **Machine names and aliases starting with `isoloom-` are refused.** They are Isoloom's own (its
  controller, the check stand-in, the check runners): a machine named `isoloom-access` collided
  with the stand-in, and `isoloom-web` shared `web`'s check step. `docs/spec.md` says so.
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
  Azure's 65535 bytes of custom data, Google Cloud's 256 KB startup script. The other clouds keep
  their modules (`isoloom targets` says which). What is measured is the text Isoloom writes; the
  values Terraform fills in at apply are left out.
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
  In the library, `isoloom_render::configure::runner` is `configure::supervisor` and
  `ContainerProcess::is_runner` is `is_supervisor`.
- The tier list's evidence notes (`isoloom targets --json`, `isoloom matrix --json`) name no
  particular runner or build of one: "re-validated end to end through a runner".
- **Every Terraform module describes `ssh_public_key` the same way** ("The SSH public key the
  machines let in: whoever holds its private key signs in, as the runner (run.json) does to
  deliver the project and run the checks"), from one definition: the Proxmox and ESXi modules
  each had their own ("Installed for the user `isoloom`..."). It stays required on the clouds and
  ESXi and empty by default on Proxmox.
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
  themselves down (the run's expiry), not left stopped. A machine's own AMI (`vm.image.aws`) must
  support IMDSv2.
- **DigitalOcean's `cloud-docker` firewall lets ICMP out**, as `cloud-vm`'s does: ping and path
  MTU discovery from the containers (every other cloud lets everything out).
- **One rule for what several crates wrote each their own way** (library): a check runner's name
  (`isoloom_render::container::check_runner`, `CHECK_RUNNER_PREFIX`), Isoloom's folder on a Linux
  VM (`isoloom_render::steps::STATE_DIR`, `MAKE_STATE_DIR`) and on a Windows one
  (`isoloom_spec::windows::ISOLOOM_DIR`, `READY_FILE`), the containers' VM in a run manifest
  (`run_manifest::guest::GuestCompose::docker_host`, `docker_host_outputs`), Proxmox's server
  variables, and a name's DNS-label rule (`isoloom_spec::validate::dns_label`, which the catalogue
  shares). The `docker-vm` Proxmox module's variables read as the `proxmox` module's.

### Fixed

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

### Removed

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
  (`schemas/run-manifest-v0.schema.json` regenerated). A manifest holding one is refused.
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
- **`isoloom_manifest::access::DEFAULT_IMAGE`** (Kali's image), which nothing could reach: an
  access machine's Compose service always has its planned image to fall back on.
- **More items only their own crate used**, now crate-internal (nothing outside it, Isoloom's
  consumers included, named them): `isoloom_ansible::configure::role_name`,
  `isoloom_catalogue::{Catalogue::shared_daemons, ContractCheck::runs_in, Release::os_list,
  Release::runs_on, conformance::FACT_PREFIX}`, `isoloom_plan::{DaemonTable::shipped_by,
  configure::unstarted, checks::HTTP_MAX_REDIRECTS}`, `isoloom_manifest::SUPPORTED_VERSION`,
  `isoloom_render::{configure::{rendered_path, uses_library, cloud_init::gz_b64},
  constraints::linux_prelude, inventory::host_value, ipv6::egress_rule, trunks::vm_commands,
  secrets::{DELIVERY_TIMEOUT, PLACE_SERVICE, windows::{BOOTSTRAP, place_first}},
  hcl_builder::WIDTH}`, `isoloom_spec::{Host::text_vars, model::{mapping_id, DEFAULT_FILE_MODE,
  SECRETS_DIR, TOOL_RECIPES, ULIMITS}, syntax::placeholder::Reference::is_run_time, validate::{DOCKER_BLOCK,
  CONTAINER_SYSCTLS}}` and `isoloom_targets::{Matrix::column_tiers,
  resolved::with_published}`; `Row::used_by` is the matrix tests' only. The cloud user-data caps
  (`isoloom_render::configure::cloud_init::{Limit, TooBig}`) moved into `isoloom-terraform`,
  crate-internal.

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
field. See [`docs/migrating-to-1.0.md`](docs/migrating-to-1.0.md) for the field-by-field
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
