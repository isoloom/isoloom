# Changelog

## 1.0.0 (2026-10-14)

The first stable release, and everything since 0.10.1. Isoloom was rebuilt around one typed plan
that every target renders, and the coverage map was rebuilt to measure every output in depth.
Upgrading from 0.10: [docs/migration-1.0.md](docs/migration-1.0.md).
Numbers in brackets are the pull requests and issues on GitHub.

### Highlights

- **One plan, every target.** A spec becomes one typed `Environment` (networks, machines,
  steps, requirements) that each backend renders, with no text templates or post-passes left.
  Refusals come from what each backend declares it can render, without generating
  ([#165], [#168], [#169], [#171], [#172], [#173], [#174], [#176]).
- **New targets and outputs:** `esxi` (Terraform on a standalone ESXi host) [#209], a Helm chart
  beside the Kubernetes manifests with `isoloom chart` [#154], and each cloud of `cloud-vm` on its
  own, picked with `--cloud` [#202].
- **Secrets** from the environment, files, Vault or SOPS, delivered as files on every target,
  Windows included, and never written in a generated file ([#189], [#200], [#208]).
- **Big spec additions:** IPv6 networks [#186], machine files [#192], a machine's own command
  and healthcheck [#193], custom images and boots [#197], GPUs, devices and nested
  virtualization [#195], existing and bridged networks [#190], persistent volumes and data
  disks [#212], private DNS [#210], cloud networking with gateways and several networks
  ([#211], [#220]), several addresses per machine [#222], Proxmox firewall, VLANs, arm64 and LXC containers [#207], Windows guests on every
  target that can run them [#194], UDP services [#124], profiles [#152] and constraints [#153].
- **Library:** one crate per layer and backend, typed errors, every lint denied, and a supported
  surface with a stability promise; the `isoloom-core` facade is gone ([#164], [#166], [#178]).
- **Coverage map v2:** every output (Compose, Kubernetes, Helm, Kustomize, Vagrant, every
  Terraform provider, cloud-init, the Ansible inventory, the Dockerfile) measured key by key
  against its pinned upstream schema, tested against what Isoloom writes, with a weekly drift
  job ([#180] to [#185]). **All 30 format tables are at 100%** of their portable features
  ([#217] to [#222]): every entry is written, equivalent to what Isoloom writes, or decided not
  portable with its reason. The spec table, read from the spec's side, is not at 100% everywhere:
  a field a target can't render is refused there with a reason, and a few cells are still
  planned (external 16, hybrid 7, each cloud 6 or 7, Proxmox and ESXi 3, Kubernetes 1).
- **Machine-readable runs:** `--json` events on every lifecycle command [#170], and workflows
  for real targets in CI, waiting for their runners and secrets [#203].
- **Documentation:** a spec reference generated from the schema, and guides tested against the
  code [#213].

### Breaking changes

#### Spec and behavior

- Nested virtualization comes only from `machines.*.nested_virtualization` (Vagrant's
  providers, Proxmox, AWS, Google Cloud, Oracle Cloud): `privileged` doesn't turn it on, as it
  briefly did on Hyper-V and VMware during the 1.0 work ([#177], [#187]). existing-hosts'
  Vagrantfile loses `enable_virtualization_extensions` [#195].
- A Linux VM's secret file defaults to mode `0400` (owner root or the spec's `uid`), since a VM
  has other users; containers keep `0444` (Docker, Kubernetes) [#208].
- `test --json` prints events (a `check` per result, the summary as the `result` event's data)
  instead of one pretty document [#170].
- `cloud-vm`: AWS no longer stands for the target. A cloud's refusal drops only its module; the
  target is refused only when every cloud refuses it. Without `--cloud`, a command no longer
  assumes AWS: it picks a cloud (see Runtime and CLI) [#202].
- Google Cloud `cloud-vm` modules give each network a VPC network of its own:
  `google_compute_network.env` is one `google_compute_network.<network>` per network, and each
  machine's `<machine>_net` firewall rule is `<machine>_net_<network>`. An environment applied
  with an older module is destroyed with it before applying 1.0's, or its state moved
  ([docs/cloud-modules.md](docs/cloud-modules.md#upgrading-an-existing-google-cloud-environment))
  [#220] ([#214]).
- Oracle Cloud refuses an OS other than Debian and Ubuntu (Kali, Rocky...) instead of running
  Ubuntu in its place; a `vm.image.oci` of its own still runs it [#202].
- Generated modules pin each provider to the version the coverage tables were measured against
  (`~>` on the patch level): Linode `cloud-vm` had `~> 2.0` and is now `~> 4.7.0`, Oracle Cloud
  `~> 6.0` is now `~> 9.8.0`, docker-vm's Proxmox `~> 0.84` is now `~> 0.115.0` [#185].
- The controller that runs `provision:` playbooks defaults to 1 CPU and 512 MB (was 1 GB); on
  local VMs its box is `generic/alpine319`, and it halts once provisioning is done
  (`controller: { keep_running: true }` keeps it up) ([#29], [#30]).
- Waits for a dependency allow 900 s on every VM target (Vagrant had 300 s, Proxmox 600 s)
  ([#171], [#172]).
- Without `follow`, an `http` check compares the first answer on every runner: a wget-only
  runner used to compare the last one past redirects, so such a check now says `got 301` as curl
  did [#122].
- Kubernetes volume names are at most 58 characters [#120]. On Kubernetes, search domains and
  resolver options without servers of the machine's own are added to the cluster's resolver
  instead of replacing it [#187].
- Validation is stricter wherever a field gained rules (listed with each field below); a spec
  that relied on an unchecked value may now fail `validate`.

#### Library

- **`isoloom-core` removed** [#178]. Depend on the crates that hold what you use: `isoloom-spec`
  (`load`, `parse`, `validate`, `derive`, `effective`, `totals`, the model, `instance`,
  `defaults`, `profile`), `isoloom-plan` (`checks`), `isoloom-targets` (`generate`,
  `generate_all`, `generate_instance`, `refusal`, `refusal_on`, `resolved`) and
  `isoloom-runtime` (`registry`, `host`, `events`, `tool`, `report`). Each item keeps its name:
  `isoloom_core::generate::X` is `isoloom_targets::X`, `isoloom_core::{coverage, import}` are
  `isoloom_coverage` and `isoloom_import`, `isoloom_core::ignore` is `isoloom_render::ignore`.
  The README's "Using the library" lists the supported surface; CI's API report covers
  `isoloom-spec`, `isoloom-plan`, `isoloom-targets` and `isoloom-runtime`.
- **Typed errors** with `thiserror` [#166]. `LoadError` gains `Yaml`, `Expand { stage, reason }`
  and `Profile { problems }` in place of `Parse(String)`, and `Read` becomes `Read { path, source }`;
  `Problem` is an error type; isoloom-import returns `ImportError`; the registry returns
  `RegistryError`, `report` a `ReportError` and `run_limited` a `HostError`. Messages are unchanged.
- **No longer public** [#121]: `checks::sq`, `vlans::vlan_network`, `groups::RESERVED`,
  `registry::rfc3339` and `defaults::at_path`.
- **Generators replaced by backends:** `vagrant::generate`, `forwarded_protocol`,
  `constraints_step` and `windows_step` (use `VagrantBackend`, `vagrant::render`, `render_in` or
  `generate_in`) [#172]; `isoloom_vagrant::docker_vm::generate` and `hybrid::generate` (use the
  backends, or `isoloom_targets::generate`) [#168]; `proxmox::generate` and `cloud_vm::generate`
  (`ProxmoxBackend`, `CloudVmBackend`) [#171]; `isoloom_kubernetes::kubernetes::generate`
  (`KubernetesBackend`, or `isoloom_kubernetes::render(&Environment)`) [#169];
  `isoloom_external::generate` (`isoloom_external::render(&Environment)` and `ExternalBackend`)
  [#167]; `cloud_docker::generate`, `other_in`, `PROXMOX`, `cpus`, `memory_mb` and
  `cloud_services::generate` (the backends, `cloud_docker::proxmox::render` and
  `cloud_docker::host::Sizing`) [#176]; Azure's and Google Cloud's `build` [#174].
- **Removed:** `isoloom_targets::LegacyBackend`, `constraints::terraform_prelude` and
  `constraints::rewrite_images` (`constraints::rewrite_image` stays) [#176];
  `cloud_vm_others::DRIVERS` and `cloud_vm::legacy` (`linux_setup_cmds`, `remote_exec`,
  `checks_output`, `hcl_cmd`...) [#174]; `isoloom_render::RETRIES` is `playbooks::helpers()`
  (`ansible_runs` and `provision_script` are kept) [#176]; `Feature::WindowsSecrets`,
  `Requirement::WindowsSecrets` and `requirements::windows_secrets` [#208]; `spec::PROXMOX` in
  isoloom-coverage [#184].
- **Plan types gained fields and variants** (struct literals must set them; get the model's
  types by parsing a spec):
  - `MachinePlan`: `os`, `winrm`, `external`, `host_vars` [#167]; `process` [#193]; `secrets`
    [#189]; `vm_image`, `boot` [#197]; `tpm`, `winssh` [#194]; `addresses6`, `routes6` [#186];
    `hardware` [#195]; `outside` [#190]; `rate_limit` [#207]; `disks`, and `names` with the
    private DNS names [#210], [#212]. `Environment` gains `inventory_groups` [#167], `outside`
    [#190], `router6` [#186], `private_dns` [#210]; `NetworkPlan` gains `ipv6` [#186] and `mtu`
    [#207]; `Volume` gains `subpath` and `lifetime` [#212].
  - `Step` gains `Before` and `Command` [#193], `Secrets` [#200], `Ipv6` [#186], `Disks` [#212];
    the `Hosts` step carries the private DNS names [#210].
  - `Requirement` and `Feature` gain `MachineAliases`, `Ulimits`, `TrafficControl` [#169];
    `Gateway`, `VolumeAcrossShapes`, `VmDependsOnContainer` and more [#168]; `MachineProcess`
    [#193]; `Secrets`, `SecretOwner` / `SecretOwners` [#189]; `OwnedProjectFile` [#192];
    `OwnBoot` [#197]; `Tpm` [#194]; `Ipv6`, `Ipv6Only`, `Ipv6Auto` [#186]; `ExternalNetwork`,
    `BridgedNetwork` [#190]; `Requirement::Hardware { machine, need }` with `ContainerGpus`,
    `ContainerDevices`, `ContainerNestedVirtualization`, `VmGpus`, `VmDevices`,
    `NestedVirtualization` [#195]; `NetworkMtu`, `RateLimit` [#207]; `PrivateDns` [#210];
    `PersistentVolume`, `ExternalVolume`, `DataDisks`, `RawDisks`, `TmpfsModes` [#212].
    `Shape::VmOrLxc` (Proxmox's) [#207].
- **Spec model:** `VmImage` gains `hyperv`, `CloudServices` gains `before`, `env`,
  `apply_retries` and `requires`, and `images::VagrantBox` gains `hyperv: HyperVBox` [#177];
  `Spec::dns` (`EnvironmentDns`) [#210]; `Machine::volumes` holds `Mount`s and `Machine::tmpfs`
  `Tmpfs` entries [#212].
- **Rendering crates:** Compose `ComposeFile::secrets` and `Service::secrets`, Kubernetes
  `VolumeMount::sub_path` and `VolumeSource::Secret` [#189]; the Compose structs gain
  `Network::external`, `Network::name` and `Attachment::gw_priority` [#168]; the Kubernetes
  `Probe` object's `exec` is optional [#193]; `project_copies` returns a `ProjectCopyError` and
  `isoloom_kubernetes::reason` words a refusal as Kubernetes reports it [#169];
  `isoloom_render::boot::at_boot` keeps a step's effect across restarts, and the shared offline
  step loads its rules with it, keeps the loopback open and installs nftables with the machine's
  own package manager [#172]; `isoloom_render::secrets`, the Terraform core's
  `Provisioner::announce`, `isoloom_runtime::secrets::delivery_input`, and
  `capabilities_on_a_vm` with `Secrets` and `SecretOwners` [#200].
- **Terraform crate:** `cloud_vm::linux_setup_cmds` took the spec and the machine's name before
  it went [#171]; `vm::cloud` gains what AWS, Azure and Google Cloud share (interfaces by MAC, the
  Windows password and refusals, `tags`, `resource`), `cloud::Reach::controller` carries the
  controller's SSH user, `controller::Ssh` gains `user`, `windows::Machine` gains `platform`
  (`windows::Platform`) and `Connection::WinRm` gains `ntlm` [#174]; `CloudVmBackend`'s refusal is
  `cloud_vm::refusal` (every cloud refuses), and `isoloom_targets::cloud_refusals` can list AWS
  [#202]; `cloud_vm_others::single_network` is `shared`, with `Networks` replacing
  `single_machine`, `shared::limits` takes the cloud's gateway reason and
  `shared::published_ports` takes one machine [#211]; Proxmox VMs' `cpu`, NICs and VNets come
  from `proxmox::arch` and `proxmox::links`, and `isoloom_plan::links` is new [#207];
  `machine_setup::linux` takes the machine only, and `bring_up` takes a `Way` [#212].
- **Several addresses** [#222]: `Machine::more_addresses`, `MachinePlan::more_addresses`,
  `Step::MoreAddresses`, `Feature::{VmMoreAddresses, ContainerMoreAddresses}`,
  `Requirement::MoreAddresses`, `Stage::Addresses`; the Hosts step lists the further addresses
  after every primary one.
- **Later cloud and Compose changes:** `shared::limits` takes the cloud's IPv6 reason as an
  `Option` [#217]; `SharedRole::HostFolder` [#219]; Compose's `Config` has an `environment` field
  [#221]; the Compose backends (and hybrid's) declare `Feature::OwnedProjectFiles`, so
  `Requirement::OwnedProjectFile` no longer refuses them [#221]; new:
  `isoloom_compose::render::files` (`project_variables`, `host_environment`),
  `isoloom_targets::compose_project_variables` [#221], `vm::limits::derived_ipv6` [#217],
  `vm::controller::provisioner_waiting` [#218].
- **Coverage crate:** `isoloom_coverage::spec::{Row, Output}` changed shape (cells per output;
  `Output::all()`), and `spec::score` returns a `Score` [#184].
- **Other:** `isoloom_spec::shell::base64` (moved from the runtime), `isoloom_spec::model::is_drive_path`,
  `isoloom_render::secrets::windows`, and a cloud's WinRM announcement ends with ` winrm` or
  ` winrm-ntlm` [#208].

### New spec fields

Each is validated with the field's name and the fix, rendered where the targets can, and
refused with a reason where they can't (`isoloom targets`, `refusal`). The spec reference,
[docs/spec.md](docs/spec.md), lists every field with the targets that render or refuse it.

#### Machines

- **`command`, `working_dir`, `before`, `healthcheck`** [#193] ([#126]): `command` is a text run
  by the shell or a list run as is; `working_dir` is absolute; `before` lists scripts of the
  project run inside the machine before its command, at every start, with its inputs and
  volumes; `healthcheck` takes `test` (a command) or `http: { port, path }` (a page), with
  `interval`, `timeout`, `retries` and `start_period`. Validated: one test, durations like
  `1m30s`, `.sh` steps (`.ps1` on Windows), no process on an appliance or an `idle` machine. The
  healthcheck is checked beside the port probe, never instead of it: a machine is ready when its
  services' ports answer and its own check passes; machines without one keep the probe
  unchanged. On a container, `before` replaces the image's entrypoint with a shell that runs the
  steps then `exec`s the command, so a container machine with `before` restates its whole
  process in `command`.
- **`files`** [#192]: files placed at a path on the machine, on every target, each with a `path`,
  a `source` (a file of the project) or a `content` (the text), and a `uid`, `gid` and `mode`
  (default root and `0644`; Linux machines only). Validated: the path (absolute, a file, once
  per machine, outside the machine's volumes), one text source, an octal mode, no owner or mode
  on Windows, no files on an appliance, a `source` that is a file of the project.
- **`secrets`** [#189] ([#147]): the secrets a machine gets, as files, never environment
  variables: a name (`/run/secrets/<name>`), or `{ source, target, uid, gid, mode }`. The default
  mode is `0444` in a container and `0400` on a Linux VM [#208]. On Windows, a path from `/` is
  on drive C: (`/run/secrets/db` is `C:\run\secrets\db`), a `target` may be a drive path, and
  `uid`, `gid` and `mode` are refused [#208].
- **`environment`, `sysctls`, `ulimits`** [#157]: fixed variables, kernel parameters and process
  limits (one value, or `{ soft, hard }`), on a machine or shared through `common:` and groups.
  Validated: variable names; kernel parameter names; a machine with `docker:` may only set the
  sysctls a container can (`net.*`, `kernel.shm*`, `kernel.msg*`, `kernel.sem`, `fs.mqueue.*`),
  so leaving `docker:` out makes it VM-only; known limit names, soft not above hard; no sysctls
  or ulimits on Windows.
- **`gpus`, `devices`, `nested_virtualization`** [#195]: `gpus` is a count, `all`, or
  `{ count, ids, driver, capabilities, options }` (driver `nvidia` by default); `devices` is
  `/dev/fuse`, `host:guest[:permissions]`, or `{ host, guest, permissions }`. Validated: `count`
  and `ids` together, a zero count, duplicate ids, permissions other than `r`, `w`, `m`, a device
  path that isn't absolute, a device given twice at one path, nested virtualization on a machine
  without `vm:`.
- **`disks`** [#212]: data disks, `{ <name>: { size_gb, fs, mount } }`, with `fs` `ext4` by
  default on Linux, `xfs`, `ntfs` on Windows, or `raw`.
- **`volumes` entries `{ path, subpath }`** [#212]: one folder of a shared volume, created when
  missing (the owner holds the whole volume). **`tmpfs` entries `{ path, size, mode }`** [#212].
- **`rate_limit`** [#207]: a `tc` rate such as `10mbit`, on each of the machine's links (Proxmox).
- **`docker.lxc`** [#207]: opt-in; on Proxmox, a machine with no `vm:` runs as an LXC container.
  Validated: a published `image`, no appliance and no `vm:`.
- **`docker.target`** [#187]: the stage of a multi-stage Dockerfile to build.
- **`vm.image`** [#197]: a machine's own image on every VM target: `box_url` (a URL, or a `.box`
  file in the project, checked by `validate`) and `box_server_url` with the `vagrant` box name;
  `proxmox` (a cloud image's URL, downloaded for the machine, or a template VM's id, cloned with
  its disk grown to `disk_gb`); `aws` (an AMI), `azure` (an image's resource id), `gcp` (an image
  path), `digitalocean`, `linode` and `oci` (an image OCID). A machine with its own image no
  longer needs a built-in image for its OS on that target; `validate` checks each is in its
  target's form; the controller's image stays a Vagrant box. `vm.image.hyperv` names a Hyper-V
  box [#177]. `vm.image.winssh` reaches a Windows box over OpenSSH instead of WinRM; not with
  `winrm`, and only on Windows [#194].
- **`vm.boot`** [#197]: `kernel`, `initrd`, `cmdline`, `dtb`, `firmware` (`bios`, `uefi`),
  `loader`, `nvram`, `firmware_format`, `order` (`hd`, `network`, `cdrom`) and `network`
  (`tftp_root`, `file`, `server`, served on the machine's first network with DHCP on, the same
  for every machine on it).
- **`vm.tpm`** [#194]: a virtual TPM 2.0. Unset, a Windows 11 machine gets one wherever the target
  can give one and runs without where it can't; `true` requires one (a target that gives none
  refuses the environment); `false` gives none.
- **`vm.provision` gains `reboot`** [#80] ([#62]): `[kernel.sh, reboot, app.sh]` restarts a Linux
  machine and goes on once it's back. Windows machines are refused with the reason: they restart
  from the environment's playbooks (`ansible.windows.win_reboot`).

#### Services and checks

- **`protocol: udp`** [#124] ([#85]): DNS, SNMP, TFTP, IKE, syslog labs; a port served over both
  protocols is two services. Validated: no `http` or `tls` on UDP, a port listed twice per
  protocol, a host port published once per protocol; `depends_on` a machine whose services are
  all UDP is refused.
- **`dormant: true`** [#79] ([#69]): a port that only answers once something wakes it (the bind
  shell an exploit opens). Declared, published and opened by `reach` like any service, but
  nothing waits for it and no derived check expects it to answer (the `blocked` ones still
  apply). `depends_on` a machine whose services are all dormant is refused.
- **`http` checks: `follow: true`** [#122] ([#86]): follows up to 5 redirects and compares the
  final answer; a POST answered with a 301, 302 or 303 goes on as a GET, other methods are sent
  again to each hop. Validated: with `http`, not with `expect: blocked`.
- **`tcp` checks: `contains` and `send`** [#78] ([#73]): text the bytes read in the first 5
  seconds must contain, and a line written first (a newline added; end it with `\r` for CRLF),
  retried for `wait` like any check.

#### Networks

- **`ipv6`** [#186]: `networks.<name>.ipv6: { cidr: fd00:20::/64 }` makes a network dual-stack;
  `only: true` makes it IPv6-only (no IPv4 on the wire; no `reach`, `gateway`, `tc` or internet
  on it); `addresses: auto` leaves the addresses to the target. Blocks are /64, unique local or
  global, never shared between networks. A machine's IPv6 address reads like its IPv4 one (the
  octet's digits as the last group: 10.20.0.10 is fd00:20::10); Isoloom's router gets one too
  (`::254` in a /24). A LAN split into `vlans` and a network with a `gateway` machine stay IPv4
  for now (validation says so). Instances shift a network's IPv6 block by the instance number
  (third group).
- **`external: <name>` and `bridged: { interface, dhcp, default_route, vlan }`** [#190]: an
  existing network the machines join by name and Isoloom never creates or deletes (a Docker
  network, a Proxmox bridge), or a network bridged to an interface of the host so the machines
  sit on its LAN (fixed addresses in `cidr`, or DHCP: the machine lists the network with `0`).
  Validated: one kind per network; no `gateway`, `internet: false`, `tc`, `docker` or `switch`
  on it; no `reach` rule names it; a machine's first network is one of the environment's own;
  `0` only on a DHCP network, and a DHCP network takes `0`; `default_route` needs `dhcp`. An
  outside network keeps its own block on Docker.
- **Several addresses on a network** [#222]: a machine's address on a network can be a list,
  `networks: { lan: [10, 11, 12] }`; the first is its primary address (the one its name
  resolves to), the others are on the same interface. Validated: an address written twice, one
  outside the network's usable range, one another machine has, and a list on an outside
  (`external`, `bridged`) network. `count:` clones move every address along. The schema takes a
  number or a list.
- **`mtu`** [#207]: 576 to 9216 bytes; a LAN's VLANs carry the LAN's.
- **`switch: <machine>`** [#72]: on a LAN split into `vlans`, a switch appliance of the
  environment (`docker.appliance: cisco-iol-l2` or `cisco-vios-l2`) switches the LAN instead of
  Isoloom's Linux switch container. Validated: a switch kind, no `networks`, `services` or
  `aliases` of its own, one LAN per switch, no `count`, no machine on the LAN directly. Docker
  targets only, like every appliance.

#### Top level

- **`secrets:`** [#189] ([#147]): where each secret's value lives: `{ env: VAR }` (the runner's
  environment, or the profile's `inputs`), `{ file: path }` (absolute, `~/`, or relative to the
  project), `{ vault: '<path>#<field>' }` (HashiCorp Vault, through `vault kv get`) or
  `{ sops: '<file>#<dotted.key>' }` (through `sops --decrypt --extract`). Validated: kebab-case
  names, each store's reference, the paths, every secret declared and used.
- **`volumes:`** [#159]: volumes several machines mount, at each machine's own path. `owner` says
  which machine holds the data (default: the first that mounts it); the others start after it.
  Validated: mounted, its owner one of its machines, each user sharing a network with the owner,
  Linux machines only. A volume can be `persistent: true` (kept by `isoloom down`, removed by
  `isoloom down --volumes`) or `external: <name>` (an existing Docker volume or Kubernetes claim,
  never created or removed) [#212]. A volume takes `virtiofs: { mapping, cache, direct_io,
  xattr, acl }` (`virtiofs: {}` for the defaults): on Proxmox it is then a folder of the server
  shared into each VM that mounts it, instead of NFS from its owner; every other target ignores
  it. `validate` checks the mapping id, and names the missing virtio-fs driver and WinFsp for a
  Windows VM [#219].
- **`dns: { private: true, domain: lab.internal }`** [#210]: every machine's name and `aliases`
  also resolve under the domain (`web.lab.internal`), served by the platform's own DNS where it
  has one. Off by default (a cloud zone is billed); the domain defaults to `<name>.internal`.
  Validated: a domain of two labels or more, and `private: true` with a domain.
- **`dns.options`** (`ndots:2`, `timeout:1`, `rotate`, validated) and **`dns.domain`** on more
  targets [#187].
- **`constraints:`** [#153] ([#133]): what the network an environment runs in imposes, usually set
  by a profile: `registries` (a registry to its mirror), `proxy` (`http`, `https`, `no_proxy`),
  `mirrors` (`debian`, `ubuntu`, `pypi`, `npm`) and `ntp`. Validated: a proxy or mirror that isn't
  a URL, a registry key that isn't a host, a mirror given with a scheme. Without `constraints:`,
  every generated file is unchanged.
- **`controller:`** [#29]: the controller like a machine: `image: { vagrant, vagrant_version }`
  for a box of your own (Alpine or Debian), `resources: { cpus, memory_mb, disk_gb }` (disk on
  Proxmox), and `keep_running: true` [#30].
- **`provision[].host_vars`** [#25]: a machine's name to its variables, written on its line of the
  inventory, each as a Python literal so it keeps its type. Validated: every key names a machine,
  every variable is a valid name. With `groups:` and `vars:`, a spec can hold a whole Ansible
  inventory.
- **`provision[].vars` takes any YAML value** [#24]: numbers, booleans, lists and maps, passed to
  `ansible-playbook` as JSON (`-e`) with their types; `isoloom run`'s own playbook runs pass them
  as JSON too (they split values on spaces before).
- **`cloud.before`, `cloud.env`, `cloud.apply_retries`, `cloud.requires`** [#177] ([#98]): host
  steps in the module's copy before `init`; variables exported by every script (the scripts' own
  names and the shell's refused); 0 to 5 more applies after a failed one (the last failure's code
  kept); host tools, checked by `isoloom doctor` and `isoloom run`. `{{ inputs.NAME }}` in
  `cloud.vars` works in any string at any depth. `cloud.hourly_usd` is documented as the author's
  estimate of the hourly cost; `isoloom run cloud-services` says it before applying, and the
  resolved snapshot carries it [#123] ([#107]).

### Targets

#### Every target

- **Rendered from the plan** ([#164], [#165], [#167] to [#176]): one crate per layer and backend,
  then a typed `Environment` built from the validated spec and a target (networks, machines,
  start order, checks, controller, constraints, requirements). A VM's set-up is a list of plan
  steps (`Prelude` to `Ready`), each rendered once as shell (and PowerShell where Windows has it)
  by `isoloom_render::steps`, the same scripts on every VM target. `LegacyBackend` and the text
  post-passes are gone; Terraform HCL is built with `hcl-rs` (`isoloom_render::hcl_builder`) and
  Vagrantfiles with one Ruby builder (`isoloom_vagrant::ruby`). Every Terraform module passes
  `terraform fmt -check` and `terraform validate`. The crate split itself changed no generated
  file.
- **One Ansible inventory** (`isoloom_render::inventory`) [#167], with each target's way of
  reaching its hosts as an `Access` (Vagrant, Proxmox, a cloud, external). Proxmox's controller
  inventory gains the spec's `groups:`. A machine with `external:` is planned as what it is (a VM,
  a container) on every target but `external`.
- **The controller's playbooks retry** what failed only because of the connection [#176] ([#20]):
  unreachable hosts (a Windows host dropping WinRM after a domain join or an update reboot) and
  hosts whose task was cut by a dropped connection (the list is `playbooks::TRANSIENT`). After
  60 s (4 × `ISOLOOM_RETRY_PAUSE`; 30 s when no host was unreachable, only cut), the play runs
  again on those hosts only (`--limit @<playbook>.retry`, retry files in /tmp/isoloom-retry), up
  to 4 runs in all. A task that failed on its own, or a run with no recap, stops the setup at
  once. Each wait and run is logged (`isoloom: site.yml: unreachable: dc01; waiting 60 s, then
  running it again on them (2/4)`, `still unreachable after 4 runs: dc01; giving up`).
- **Inputs** are single-quoted (`KEY='value'`) so `sh` takes each as it is; AWS and the other
  clouds used `jsonencode`, which let `sh` expand `$`, backticks and `$(...)` and turned `&`
  into `\u0026` ([#171], [#174], [#176]). They are read from `/tmp/isoloom-inputs.env`, kept in
  `/var/lib/isoloom` when a reboot comes.
- **Boot-time services:** routes, trunks, `tc` and the offline egress rules are reloaded at every
  boot (`isoloom_render::boot`, OpenRC on Alpine) on every VM target; the egress rule drops new
  connections leaving the environment's networks [#172].
- **Constraints** [#153]: every image of the Compose file and the Kubernetes manifests goes
  through the `registries` mirrors (Isoloom's router and check images included, and the `FROM`
  lines of the Dockerfiles Isoloom writes; images built from the project stay as they are); the
  proxy reaches the containers as environment variables and their builds as Docker's predefined
  build arguments, never Isoloom's checks, with the environment's own names and blocks reached
  directly. Linux VMs run an idempotent preamble as root first (the proxy for every shell,
  systemd, sudo, apt, dnf/yum and Docker's daemon; apt sources rewritten to the mirrors, with
  `debian-security` left alone; `/etc/pip.conf`, `/etc/npmrc`; chrony or systemd-timesyncd), as
  each VM's first Vagrant provisioner, `bootcmd` on Proxmox, and first after cloud-init on every
  cloud. Windows machines aren't covered yet.
- **A machine's environment, sysctls and ulimits** [#157]: a container gets them from its runtime
  (on Kubernetes `ulimits` are refused); a VM from one script run as root before its steps
  (`/etc/environment`, systemd's `DefaultEnvironment`, `/etc/sysctl.d`, `limits.d`, systemd's
  `DefaultLimit*`, then systemd re-executed); on Windows the environment is set machine-wide.
- **Machine files** [#192]: a `Files` step on VMs (shell and PowerShell) after the project is
  unpacked; a machine whose files copy from the project gets the project even with no scripts.
- **A machine's own process** [#193]: VMs get the plan steps `Before` (a one-shot service run now
  and at every boot) and `Command` (a service restarted when it fails; OpenRC on Alpine; a startup
  task on Windows), through `boot::service`. The healthcheck is tried in the `Ready` step.
- **Shared volumes** [#159]: one named volume on Docker; one claim on Kubernetes, seeded by the
  owner, its other users on the owner's node; on the VM targets the owner serves its directory
  over NFS and the others mount it before their steps and at boot, waiting up to 15 minutes.
- **UDP services** [#124]: published as UDP on Compose, Vagrant, Kubernetes, the Proxmox router
  and every cloud (including Windows firewall openings); nothing waits for a UDP port, and the
  derived checks leave it out. A Windows cloud VM can't publish a UDP service on another port
  (refused). Outputs name a UDP service `<name>-udp` when a TCP service of the machine has the
  same name. vagrant-libvirt skips UDP forwarded ports: the Vagrantfile says so, and `isoloom run`
  warns.
- **Reboot steps** [#80] ([#62]): Vagrant restarts the VM itself (the shell provisioner's
  `reboot: true`); on Proxmox cloud-init restarts the machine once it has run the steps before
  (`power_state`) and a unit runs the rest at the next boot; on the clouds the set-up splits into
  one `remote-exec` per part, the next part checking the boot changed, bringing extra interfaces
  up again and scheduling the auto-stop again. Example: existing-hosts' cache turns transparent
  hugepages off, restarts, then installs Redis.
- **Cisco switch** [#72]: the switch's data ports are an access port on each VLAN's network (in
  VLAN order), then a trunk port per trunk machine; Isoloom's startup configuration declares the
  VLANs (`vtp mode transparent`, `vlan <id>`) and sets each port, and the machine's own `config`
  follows; trunk machines' sidecars wait for the switch to start. Example: cisco-switch.
- **Dormant services** [#79] are left out of the Docker healthcheck, the Kubernetes readiness
  probe and init-job wait, and the dependency waits on the VM targets.
- **Derived checks in Windows-only environments** [#75] ([#60]): when no Linux machine can run
  them, every service is checked at each of its addresses from the environment's networks: the
  controller on Vagrant, AWS and Azure (created for it when needed), the check runner on every
  network on Docker and Kubernetes, the controller on hybrid. Nothing is asserted where `reach`
  blocks a service or a network is offline.
- **The project goes into VMs as an archive** [#76] ([#67], [#63]): a tar.gz written by
  Vagrant's own Ruby when the step runs (no `tar` on the host), symbolic links kept as links,
  unpacked in /opt/isoloom; `.git`, `.vagrant` and `.terraform` stay out at any depth.
  `.isoloomignore` [#81] ([#61]) leaves paths out of every copy (gitignore-style: `#`, `!`,
  trailing `/`, anchoring `/`, `*`, `?`, `[abc]`, `**`; the last match decides).
- **Windows steps use the project's files** [#82] ([#59]): the project goes to `C:\isoloom` once,
  and each `.ps1` step runs from there; the archive is unpacked by PowerShell over .NET's
  GZipStream (PowerShell 2.0 and later), symbolic links left out and names Windows can't take
  skipped with a warning.
- **Several addresses** [#222]: on every VM target a set-up step (`Step::MoreAddresses`) puts each
  further address on the interface holding the primary one (`ip addr replace`, now and at every
  boot and link change on Vagrant; `New-NetIPAddress -SkipAsSource` on Windows); every hosts file
  names them after their machine, and the derived checks probe each one (`web:80 at 10.86.0.11
  from client`). Containers (Compose, Kubernetes, and the containers of `hybrid`, `docker-vm` and
  `cloud-docker`: Docker gives an endpoint one IPv4 address) and existing hosts refuse it.
- **Private DNS** [#210]: the Docker targets give the names to Docker's DNS (network aliases);
  every VM's hosts file carries them too; Kubernetes and existing hosts refuse it.
- **Secrets** ([#189], [#200], [#208]): on `docker` and `hosted` the Compose file declares each
  secret with `environment: ISOLOOM_SECRET_<NAME>`, a variable only `docker compose up` gets, and
  the containers (their init jobs too) see files with their owner and mode. On `kubernetes` the
  pods mount the keys of an `isoloom-secrets` Secret, which `run` applies server-side before the
  manifests (no copy in a `last-applied` annotation); a secret with `uid` or `gid` is copied by
  the pod's `isoloom-secrets-owned` init container into a memory-backed emptyDir with its owner
  and mode (it was refused until [#221]; the Secret volume's own owner fields are alpha behind a
  feature gate in Kubernetes 1.37, so Isoloom doesn't write them).
  On `vagrant`, `hybrid`, `docker-vm`, `proxmox`, `cloud-vm`, `cloud-docker` and `external`,
  `isoloom run` writes them on each machine over its own connection: the values travel on
  standard input, base64 per line, to a script with none in it, which keeps each root-only under
  `/var/lib/isoloom/secrets` and puts it in place. A `Secrets` plan step waits (up to 30 minutes)
  for the delivery, and a boot service (`isoloom-apply-secrets`) puts the files back at every
  boot. Docker hosts get the containers' secrets the same way. Windows machines get them over
  WinRM (python3 with pywinrm on the runner), or OpenSSH where `vm.image.winssh` is set, kept in
  `C:\ProgramData\isoloom\secrets` with an access list of SYSTEM and Administrators only, and put
  back by a startup task. No value is in a generated file, the resolved snapshot, a log,
  cloud-init user data, Terraform's state, a Vagrant provisioner or a command line; a test
  generates every target with the values set and finds none.

#### Docker, hosted (Compose)

- Compose rendered from typed structs [#165]. New keys: `configs` for machine files [#192]. A
  project file with a `uid`, `gid` or `mode` is an `environment` config
  (`ISOLOOM_FILE_<MACHINE>_<N>`) copied in with its owner and mode, since Compose outside Swarm
  ignores both for a mounted file: `isoloom run` reads the file from the project into that
  variable for `up`, a Docker host VM (`docker-vm`, `cloud-docker`, hybrid's) reads it from the
  project it unpacked, and the Compose file's header names the variables for a run by hand
  (it was refused until [#221]); `command`, `working_dir`, the `before` entrypoint (steps mounted read-only), the
  healthcheck test after the port probe, and `depends_on` waiting for a machine with only a
  healthcheck [#193]; `enable_ipv6`, an IPv6 subnet (Docker at `::1`, its own addresses in
  `<block>::1:0/112`), `ipv6_address`, `enable_ipv4: false` on an IPv6-only network (Docker Engine
  28 or later) [#186]; `external: true` networks at the machine's fixed address [#190]; GPU
  requests in `deploy.resources.reservations.devices` and long-form `devices` (`permissions`
  always written, `rwm` by default) [#195]; `dns_opt` and `build.target` [#187]; persistent
  volumes labelled `isoloom.persistent`, existing ones `external: true`, `volume.subpath` folders
  made by the one-shot `isoloom-volume-folders` job, sized tmpfs mounts, a named volume per disk
  [#212]; `deploy.resources.reservations` for its one portable use [#221].
- Refused on Compose: bridged networks (name a macvlan network made beforehand with `external`)
  [#190]; raw disks [#212]; several addresses on a network [#222].
- Proven for real: `dual-stack` over IPv6 and an IPv6-only network with `auto` addresses [#186];
  machine files [#192]; host devices [#195]; secrets from the environment and a file on
  hello-stack (14 checks) [#189]. Before the release, hello-stack, segmented, tuned-web,
  shared-files, dual-stack and private-names passed `run`, `test` and `down` on Docker (32
  checks); a persistent volume survived `down` and `run`, and `down --volumes` removed it; an
  owned project file landed as 101:101, mode 0400, on `docker` and `hosted`.

#### Kubernetes, Helm, Kustomize

- Typed objects (Deployment, Service, NetworkPolicy, PersistentVolumeClaim, Job, Pod, ConfigMap,
  Secret, Namespace, Kustomization), the manifests and the Helm chart from the same ones; the
  plain manifests are byte-identical, refusals come from the plan's requirements, and every image
  comes from the plan's mirrors [#169].
- **A Helm chart** [#154] ([#134]) in `.isoloom/kubernetes/chart/`: the namespace is the release's,
  each machine's image a value (`images.<machine>`), the inputs a Secret from `inputs.<NAME>`, the
  checks `helm test` Pods (the `exec` ones stay with `isoloom test kubernetes`), `NOTES.txt` the
  message. The chart works away from the project: `generate`, `run` and `check` copy the scripts
  its pods run into `chart/files/project/`, so `check` fails when a copy is stale. Its templates
  are YAML before Helm renders them, and a `{{` in a value is kept as `` {{ `{{` }} `` [#169]. A
  `values.schema.json` from the machines and inputs makes `helm install` reject an unknown or
  empty image, an input it doesn't read, or any other value [#187].
- The kustomizations are serialized as YAML [#169].
- **Volumes start with the image's files** [#120] ([#102]): an init container of the machine's
  image (`seed-<volume>`, Isoloom's static busybox) copies the image's files at the path into an
  empty claim, as Docker seeds a named volume; a failed copy is cleared and tried again (a
  `.isoloom-seeding` marker).
- New: `args`, `workingDir`, the `before` shell, a command healthcheck in the `exec` readiness
  probe, a page as an `httpGet` readiness probe with the port probe as the `startupProbe` [#193];
  machine files through a `<machine>-files` ConfigMap and an `isoloom-files` init container
  [#192]; `ipFamilyPolicy: PreferDualStack` (or `SingleStack` IPv6) and a published port open to
  `::/0` [#186]; GPU counts as the device plugin's resource (`nvidia.com/gpu`, `amd.com/gpu`,
  `gpu.intel.com/i915`), with `all`, `ids`, `options` and host devices refused [#195]; `subdomain`
  from `dns.domain`, `dnsConfig.options` [#187]; persistent claims labelled for `down` to keep and
  annotated `helm.sh/resource-policy: keep`, existing claims by name, `subPath`, a tmpfs
  `sizeLimit`, a claim per disk with `volumeMode` (`Block` and `volumeDevices` at
  `/dev/isoloom-<disk>` for a raw one), a tmpfs `mode` refused [#212].
- Refused: existing and bridged networks [#190], private DNS [#210].
- Validated with `kubeconform` and `helm lint`; CI installs the chart on kind for hello-stack and
  segmented. Not run on another cluster.

#### Vagrant (`vagrant`, `docker-vm`, `hybrid`)

- Rendered from the plan by `VagrantBackend`, `DockerVmBackend` and `HybridBackend`, each VM's
  set-up its planned steps run as provisioners ([#168], [#172]). Output changes: hosts lines carry
  aliases on Windows too; the project is copied to every VM first; each VM ends with a `ready`
  step; a machine behind a gateway no longer blocks its NAT interface; a reboot step checks the
  machine restarted; playbooks run with `ansible-playbook -c local -i <machine>,` from a shell
  step; a gateway's forwarding sysctl has its own file; a Windows machine may have routes
  (`route -p`), waits for its dependencies, and grows its disk. In hybrid's `compose.yml`,
  `extra_hosts` moves among the service's keys and the publishing bridge is `isoloom-publish: {}`.
  docker-vm reports its refusals as `docker-vm: ...`.
- **Hyper-V** [#177] ([#92]): a `hyperv` provider block on every VM, a first adapter on
  `ISOLOOM_HYPERV_SWITCH` (else the Default Switch), a private switch `isoloom-<env>-<network>`
  per network (removed with its last VM), an adapter per network with a fixed MAC, the address set
  in the guest before provisioning. Published ports aren't forwarded on Hyper-V (`vagrant up`
  warns, `isoloom report services` says so). Boxes: the roboxes `generic/*` boxes,
  gusztavvargadr's ubuntu-24.04, StefanScherer's windows-server-2019; debian-13, ubuntu-14.04 and
  fedora-42 are refused on Hyper-V until a box exists.
- **Disk size on every provider** [#156]: `resources.disk_gb` sizes the disk (Vagrant's `vm.disk`
  on VirtualBox, VMware, Hyper-V; libvirt's `machine_virtual_size`; ESXi's
  `guest_boot_disk_size`), as a minimum, and a `disk` step grows the root filesystem (the last
  partition, LVM, ext4 or XFS; `Resize-Partition` on Windows). QEMU is left out on purpose: a
  smaller overlay would truncate the box.
- **libvirt** sets `machine_arch` from the machine's `arch` [#158]; it renders `vm.boot`
  (`v.kernel`, `v.cmd_line`, `v.loader`, `v.boot`, `libvirt__tftp_root`, the DHCP boot file and
  server) [#197], an emulated TPM [#194], and the IPv6 block [#186].
- **QEMU**: a network of other than two VMs gets a hub, one QEMU process with a socket port per VM
  (no root; socket multicast doesn't send on a Mac) [#187]; the hub stops with the network's last
  VM [#199] ([#188]); `disk_gb` becomes `disk_resize` only above the box's own size [#187];
  `firmware_format` [#197]. **UTM** refuses VMs on a network (vagrant_utm 0.1.6 has no private
  networks) [#187].
- **TPM** [#194]: VirtualBox 7 (`--tpm-type 2.0`), libvirt, and Hyper-V (`Enable-VMTPM`);
  `tpm: true` is refused on QEMU, VMware, Parallels, UTM and ESXi. Run for real on VirtualBox 7.2.
- **Windows over OpenSSH** (`config.winssh`, `ansible_connection=ssh`,
  `ansible_shell_type=powershell`) [#194]; ESXi's `guest_guestos` from the OS [#194].
- **Nested virtualization** per provider (VirtualBox `--nested-hw-virt`, VMware and ESXi
  `vhv.enable`, Parallels `--nested-virt`, libvirt `nested`, Hyper-V
  `enable_virtualization_extensions`); QEMU and UTM stop `vagrant up` with the reason; GPUs and
  devices refused [#195].
- **Bridged networks** as a `public_network` (`bridge`, `ip`,
  `use_dhcp_assigned_default_route`); refused with a VLAN tag, a fixed address outside a /24,
  Windows guests, and on QEMU, Hyper-V and libvirt [#190].
- **IPv6** through `Step::Ipv6` on Linux and Windows, again at every boot; IPv6-only networks and
  `auto` addresses refused [#186]. **Data disks** with `config.vm.disk` (VirtualBox, VMware,
  Hyper-V), libvirt's `storage`, ESXi's `guest_storage`; QEMU, Parallels and UTM refuse them
  [#212].
- **The controller** halts after provisioning on local VMs, and `isoloom test` boots it for the
  checks [#30]; it is `generic/alpine319` with 512 MB [#29].
- Not run on a real host beyond VirtualBox: VMware, Parallels, libvirt, Hyper-V, UTM, QEMU and
  ESXi are generated and tested only.

#### Proxmox

- Rendered from the plan through the Terraform VM core [#171]: a network's `tc` is applied on the
  machines (it was silently dropped), routes to the networks behind the router are added, inputs
  are written to `/var/lib/isoloom` first, a machine depending on one that isn't a VM there no
  longer points at a missing resource, and machine names that are HCL keywords are quoted.
- **Windows machines** [#194]: a full clone of a template per OS (`template_<os>`: cloudbase-init
  and VirtIO drivers, UEFI on Q35), set up by a cloudbase-init script, then WinRM over HTTP;
  `windows-hello` and `mixed-office` generate for Proxmox. Steps other than `.ps1`, gateways,
  `internet: false`, volumes and inputs on Windows are refused there for now.
- **Firewall, pool, VLANs, MTU, rate limits, arm64, LXC** [#207]: each machine's devices have the
  Proxmox firewall on, dropping what comes in by default, with rules for its own networks, each
  `reach` rule and its published ports, each network an IP set of the datacenter
  (`iso<slot>n<i>`); the rules take effect where the datacenter's firewall is on. The environment
  is in a pool (`iso<slot>`). A LAN's `vlans` are one VLAN-aware VNet with tags on the devices.
  `mtu` on every device, `rate_limit` in MB/s. An `arch: arm64` machine is an aarch64 VM QEMU
  emulates (UEFI, the `max` CPU, Debian 11 to 13 or Ubuntu 20.04 to 24.04 arm64 images, or its
  own); only `root@pam` may set a VM's architecture, and the server needs
  `pve-edk2-firmware-aarch64`. A `docker.lxc` machine is a container from its image pulled as an
  OCI template (Proxmox VE 9.1 or later, `template_datastore`), with the router answering its
  names; arm64, volumes, files, secrets, inputs, sysctls, ulimits, `read_only`, tmpfs,
  `shm_size`, `docker.init`, a process of its own, GPUs, devices, IPv6, outside networks and
  checks run from it are refused on a container.
- Also: own images (a cloud image's URL or a template VM's id) and `firmware`/`order` from
  `vm.boot` (Proxmox refuses the host files, the network boot and `cdrom`) [#197]; `tpm_state`
  [#194]; IPv6 addresses through the set-up step [#186]; existing and bridged networks with
  `vlan_id`, a fixed address or `dhcp` [#190]; GPUs by `gpus.ids` (`hostpci`, PCIe, q35; a count
  alone refused) and nesting from the `host` CPU type [#195]; cloud-init's `fqdn` from
  `dns.domain` [#187]; the private DNS domain as the search domain (its SDN DNS needs a PowerDNS
  server outside the environment) [#210]; a `disk` per data disk with cloud-init's `disk_setup`,
  `fs_setup` and `mounts` [#212]; `.vagrant` and `.terraform` left out of the copy [#81].
- **Shared volumes over virtiofs** [#219]: a `virtiofs` device on each VM that mounts the volume
  (`mapping`, `cache` (`auto` by default), `direct_io`, `expose_xattr`, `expose_acl`), and a
  `proxmox_hardware_mapping_dir` per volume that names no mapping, for
  `<virtiofs_root>/<environment>/<volume>` on the node (new variable `virtiofs_root`,
  `/srv/isoloom` by default; make the folder before `apply`). Each VM mounts the folder by its
  tag from `/etc/fstab`; a `subpath` binds its folder from the whole one. Refused: an LXC
  container mounting one, a Windows VM, more than 8 on a VM. The plan's
  `SharedRole::HostFolder` takes the place of the NFS roles.
- Not run on a Proxmox server: generated, `terraform validate`d and unit-tested.

#### ESXi

- **New target `esxi`** [#209]: `.isoloom/esxi/main.tf` for the josenk/esxi provider (1.10.x),
  through the Terraform VM core. Each network gets a vSwitch with no uplink and one port group; a
  router VM (Ubuntu 24.04) sits on the host's `uplink_port_group` and every network; each machine
  is a VM from an OVA (Ubuntu's cloud OVAs by default, or your own with cloud-init and
  open-vm-tools), addressed by cloud-init's VMware data source; Terraform runs its steps over SSH
  through the router, and the controller runs the playbooks with Proxmox's inventory. `run`,
  `down`, `test`, `connect`, `exec` and `status` work as on Proxmox; `isoloom targets --host`
  checks for `ovftool`, `TF_VAR_esxi_hostname` and `TF_VAR_esxi_password`.
- **Windows, data disks, firmware, MTU, outside networks, IPv6 and secrets** [#218]: a Windows
  machine is a VM from the OVA given in `images` (VMware Tools and cloudbase-init), with its guest
  OS type (`guestos`), set up by cloudbase-init from `guestinfo` with Proxmox's script, and the
  controller waits for its WinRM ready marker; Proxmox's Windows limits apply. A machine's
  `disks` are an `esxi_virtual_disk` each (thin, on `disk_store`), attached by SCSI slot
  (`virtual_disks`, 14 at most); `vm.boot.firmware` is `boot_firmware` (`bios`, `efi`), the rest
  of `vm.boot` refused with why; a network's `mtu` is its vSwitch's and each guest interface's.
  `external` networks join an existing port group; `bridged` ones are a port group on the host's
  vSwitch (`uplink_vswitch`), or on a vSwitch of their own on the NIC they name (`uplink`), tagged
  with their `vlan`. IPv6 networks give the router its IPv6 address. Secrets are delivered
  through the router (SSH with `via` on Linux, WinRM on Windows). Several addresses on a network
  work as on the other VM targets [#222].
- Still refused: arm64, `gateway` networks, TPM, GPUs, devices, nesting, rate limits, the boot
  settings other than `firmware`, and more than nine networks.
  [docs/esxi.md](docs/esxi.md) says when to use it rather than Vagrant's ESXi provider. Not run
  on a real host.

#### Clouds (`cloud-vm`, `cloud-docker`, `cloud-services`)

- **Each cloud a backend** in `cloud_vm::CLOUDS` (AWS, Azure, Google Cloud, DigitalOcean, Linode,
  Oracle Cloud), rendered from the plan ([#171], [#173], [#174]); `cloud-docker` and
  `cloud-services` are backends of their own (`CloudDockerBackend`, `CloudServicesBackend`), the
  Docker host set up through the core's `remote-exec` transport, its project archive
  `.isoloom-project-environment.tgz` [#176]. Every cloud takes machine aliases and `ulimits`
  (DigitalOcean, Linode and Oracle Cloud gain `tuned-web`) [#174]. Firewalls list their ports as
  values [#176].
- **Each cloud on its own** [#202] ([#196]): a cloud's refusal drops only its module, and the
  target is refused only when every cloud refuses, with each cloud's reason (ADR 0007 says why
  one target with a cloud picked at run time). `dual-stack` generates `cloud-vm` (Azure, and Oracle Cloud since [#217]), and
  `mixed-office`'s Windows file server asks for a TPM (its AWS module is gone).
- **Networking** [#211] ([#206]): AWS routes a network through its `gateway:` machine (the gateway
  at the network's router address, source/destination checks off, the VPC's block holding every
  network within one /16, a route table per network); `edge-firewall` generates `cloud-vm` (AWS,
  Oracle Cloud), its firewall moving from .2 to .5 on `outside`. Oracle Cloud takes several
  networks (a subnet each in one VCN, five at most; a VNIC per network found by its MAC; a
  security list per network enforcing `reach` both ways) and routes through a `gateway:` machine.
  Linode takes several networks (a machine on one), routes between them at boot, and a firewall
  per machine enforcing `reach`. Azure subnets of an offline network have no default way out.
  Gateways are refused on Linode and DigitalOcean (no route tables); DigitalOcean stays one
  machine on one network.
- **Azure and Google Cloud gateways** [#220] ([#214]): Azure routes a network through its
  `gateway:` machine with a route table per network (`azurerm_route_table`, attached by
  `azurerm_subnet_route_table_association`) and an `azurerm_route` to each network it reaches
  through a gateway (next hop `VirtualAppliance` at the gateway's private address there, else its
  first), with IP forwarding on the gateway's interfaces; Azure keeps .1 to .3, so the gateway
  sits at the network's router address, as on AWS. Google Cloud gives each network a VPC network
  of its own, so a machine can sit on several (an interface in each, 8 at most, IP forwarding on,
  its machine type bumped to an E2 type with a vCPU per interface; a GPU or nesting type with too
  few is refused) and the controller spans every network. The networks no machine routes are
  peered with each other both ways (`google_compute_network_peering`, exchanging custom routes);
  firewall rules sit in the VPC of the network they open; the private zone is visible from every
  VPC; each network takes its own `mtu` (Ethernet's 1500 by default). A routed network is peered
  with none: its VPC sends every other network to the gateway's address (`google_compute_route`,
  `next_hop_ip`), and each other VPC the gateway is on sends the routed network to its address
  there. Google keeps the second to last address too, so the gateway takes the highest free one
  (`vm::gateways::free_address`); secondary interfaces come up by the MAC Google derives from the
  address. A gateway's relocated address follows into the private zone too
  (`vm::gateways::relocated`). `edge-firewall` generates `cloud-vm` on Azure and Google Cloud,
  and `vlan-office` on Google Cloud. The Google Cloud module's resources are renamed (see
  Breaking changes).
- **Published ports per machine**: Oracle Cloud opens them on the machine's own VNIC through a
  network security group [#205]; Linode's and DigitalOcean's firewalls open only that machine's
  own ports [#211].
- **Private DNS** [#210]: a Route 53 private hosted zone (AWS), a private DNS zone linked to the
  virtual network (Azure), a Cloud DNS private managed zone (Google Cloud), a private view on the
  VCN's resolver (Oracle Cloud); DigitalOcean and Linode use the hosts files. Azure also writes an
  `azurerm_private_dns_aaaa_record` per name whose machine's first network is IPv6, and a reverse
  zone per /24 holding a machine, linked to the virtual network, with an
  `azurerm_private_dns_ptr_record` naming each machine at its first address [#220].
- **Data disks** [#212]: AWS's `ebs_block_device`, Oracle Cloud's `launch_volume_attachments`,
  Google Cloud's `google_compute_disk`, DigitalOcean's `digitalocean_volume`, Azure's managed disk
  and attachment, Linode's `linode_volume`; each cloud refuses a disk below its smallest volume or
  more disks than a VM takes. `docker-vm` and `cloud-docker` refuse persistent and existing
  volumes.
- **Windows** [#194]: NitroTPM on AWS (Windows on AWS is refused for now), trusted launch on Azure,
  shielded VMs on Google Cloud and Oracle Cloud; DigitalOcean and Linode refuse a TPM. Azure's
  Windows hosts use NTLM in the controller's inventory [#194] ([#175]) and run the core's set-up
  script, keeping their name from creation [#174].
- **GPUs and nesting** [#195]: a GPU count picks the cloud's GPU type (AWS g4dn, Azure NCasT4_v3,
  Google Cloud g2, DigitalOcean H100, Linode RTX 6000; Oracle Cloud refuses); nesting on AWS m8i,
  Google Cloud n2 and Oracle Cloud's AMD shape (Azure, DigitalOcean and Linode refuse); no cloud
  takes GPUs and nesting on one machine.
- **IPv6** [#186]: Azure writes the spec's blocks; AWS, Google Cloud, Linode and DigitalOcean
  refuse with their reason. Oracle Cloud takes dual-stack networks [#217]: the VCN holds each
  network's IPv6 block as a private prefix (`is_ipv6enabled`, `ipv6private_cidr_blocks`, no
  Oracle-allocated public prefix: `is_oracle_gua_allocation_enabled = false`), each subnet its
  network's block (`ipv6cidr_block`), and each VNIC asks for the machine's derived address
  (`assign_ipv6ip`, `ipv6address_ipv6subnet_cidr_pair_details`), which the IPv6 set-up step puts
  on the interface. With `ipv6.addresses: auto` Oracle Cloud picks the address and the set-up
  reads it back from Terraform at every boot; Azure refuses `auto` with its own reason, and the
  plan no longer refuses `auto` on `cloud-vm` as a whole. Security lists mirror the IPv4 rules on
  dual-stack networks (ICMPv6 "packet too big" both ways); no IPv6 route leaves the VCN. Refused
  on Oracle Cloud: a machine whose derived IPv6 address is one of the first four of a prefix, and
  `ipv6.only` (machines are set up over their public IPv4 address).
- **Several addresses** [#222]: AWS writes `secondary_private_ips`, or a network interface's
  ordered `private_ip_list` and `ipv4_prefixes` for a whole aligned /28, and picks a larger
  instance type when the interface needs more addresses; Azure an `ip_configuration` per
  address; Google Cloud an `alias_ip_range` each; Oracle Cloud an `oci_core_private_ip` each;
  Linode the VPC interface's `ip_ranges`. DigitalOcean refuses it (a Droplet has one VPC
  address).
- **Own images** (`aws`, `azure` as `source_image_id`, `gcp`, `digitalocean`, `linode`, `oci`)
  [#197]; `vm.boot` is refused on every cloud.
- **Refusals** [#174]: Azure subnets smaller than /29 and machines at .1 to .3; Google Cloud
  subnetworks smaller than /29 and machines at .1; arm64 with each cloud's reason. Existing and
  bridged networks are refused on every cloud [#190].
- **Provider pins** [#185]: `~> 6.67.0` AWS, `~> 5.8.0` azurerm, `~> 8.5.0` google, `~> 2.103.0`
  digitalocean, `~> 4.7.0` linode, `~> 9.8.0` oci, `~> 0.115.0` proxmox, tls `~> 4.4.1`, random
  `~> 3.9.1`, and `local` (`~> 2.9.1`), `random` and `time` declared where used; the versions are
  in `isoloom_terraform::vm::versions`.
- **cloud-services** [#177] ([#98]): Terraform runs on a copy of the module,
  `.isoloom/cloud-services/module/`, refreshed at every `run`; each deployment gets a name
  (`isoloom-instance`, `TF_VAR_isoloom_instance`), and after destroying, `isoloom down` on AWS
  lists what is still tagged with it, deleting nothing.
- Not run on a real cloud in this release's CI: generated and `terraform validate`d.

#### External

- Rendered from the plan (`ExternalBackend`), byte for byte as before [#167]. Refused: a machine
  process [#193], machine files [#192], private DNS [#210]. `isoloom run external` restarts a
  machine for a `reboot` step and waits for a new boot id [#80]; secrets are delivered over SSH,
  WinRM or OpenSSH ([#200], [#208]).

#### Examples

- New: `shared-files` (a writer stores a file, the reader finds it at its own path; run on
  Docker, kind and two VirtualBox VMs over NFS) [#159], `tuned-web` (nginx with `APP_MODE`, a
  4096-deep accept queue and 8192/16384 open files, read back by its checks; run on Docker and
  VirtualBox) [#157], `udp-dns` (dnsmasq across a router that lets only port 53 through) [#124],
  `windows-hello` [#75], `cisco-switch` [#72], `dual-stack` [#186], `private-names` [#210],
  `several-addresses` [#222].
- Changed: hello-stack's `/hello` answers 301 [#122] and its cache answers `PING` [#78];
  segmented's web server has a dormant 4444 [#79]; ansible-pair's cache gets a `maxmemory`
  host variable [#25], its `tools: [curl, jq]` and `cache_port: 6379` typed variables [#24] and a
  768 MB controller [#29]; existing-hosts' cache restarts for a kernel option [#80] and is
  `privileged` [#177]; windows-hello sets `vm.image.hyperv` [#177]; `tuned-web` asks for a TPM
  [#194] and, with `shared-files`, has a data disk checked as mounted [#212]; `arm-vm` generates
  `proxmox` [#207]; `windows-hello` and `mixed-office` generate for Proxmox [#194]; mixed-office's
  hybrid output checks files01 and the intranet [#75]; `edge-firewall` generates `cloud-vm`
  ([#211], [#220]); `vlan-office` generates for Google Cloud [#220]; hello-stack and segmented's `/data` claims are seeded on Kubernetes [#120].

### Coverage

- **Tables v2.** Every output is measured key by key against a pinned upstream source, with a
  refresh script (`scripts/refresh-*`, `ISOLOOM_REFRESH=latest` for the newest release) and a
  written-features test that reads back what Isoloom writes for every example and fixture: every
  entry written is marked as written, and every row marked as written is written somewhere.
  - Compose [#181]: every nested key path of the pinned compose-spec schema (347 rows, from 118),
    `scripts/refresh-compose-schema.sh`, split by section under `isoloom-coverage/src/compose/`.
  - cloud-init (26.2's schema plus the `users`, `write_files` and `power_state` items), the
    Ansible inventory (ansible-core v2.21.5 and pywinrm v0.5.0's `ansible_winrm_*`) and the
    Dockerfile (BuildKit dockerfile/1.28.0); the Compose table's "in image" rows land there [#180].
  - Kubernetes (the v1.37.0 OpenAPI, the version CI's kind runs), Kustomize and Helm
    (SchemaStore's kustomization.json and chart.json, Helm's chart files and hooks),
    `scripts/refresh-kubernetes.py` [#182].
  - Vagrant [#183]: the options of `config.vm.network`, `config.vm.disk`, `synced_folder`, every
    provisioner by its real name, `config.ssh.*`, `config.winrm.*`, `config.winssh.*`,
    `config.push` and Vagrant's Docker provider, from Vagrant v2.4.9 and each plugin's tag
    (`scripts/refresh-vagrant.py`, was `vagrant-settings.py`).
  - Terraform [#185]: besides every resource type, each provider's settings and every argument
    and nested block of what Isoloom writes, from `terraform providers schema -json`
    (`coverage/terraform/<provider>-schema.txt`), and a table for the utility providers (built-in,
    random, tls, local, time, null, cloudinit, http). `scripts/refresh-terraform.sh` (was
    `terraform-resources.sh`). Every entry is classified by a reviewed family rule (one module per
    provider under `isoloom-coverage/src/terraform/`): an unclassified entry or a rule naming
    nothing fails the tests. The 63 types left "with the cloud generator" are each decided
    (equivalent, not portable, tooling, or moved to their feature) [#205]. The ESXi table comes
    from the provider's schema (`esxi-schema.txt`) [#209].
- **Classification ADR** ([docs/adr/0006-coverage-classification.md](docs/adr/0006-coverage-classification.md))
  [#181]: rows re-decided, e.g. `configs`, `working_dir`, `dns_opt`, `secrets`, `gpus` and
  `devices` Planned, `command` and `healthcheck` Partial, `models` Not portable; custom cloud
  images and private DNS Planned [#185] and since done. Kubernetes `PodSpec.os` and
  `windowsOptions` are Not portable (Windows containers only) [#194].
- **The spec table for every output** [#184]: a column per output (docker, hosted, kubernetes,
  helm, kustomize, vagrant, docker-vm, hybrid, proxmox, each cloud of cloud-vm, cloud-docker,
  cloud-services, external, and esxi [#209]), rendered in `isoloom coverage` (`--all` lists what
  each output doesn't handle yet) and in docs/COVERAGE.md. A cell starts from what its rendering
  states and becomes a refusal when the backend refuses the field's probe spec. Tests: every
  rendering states every field, every claim names an example or probe that generates, every
  probe is valid. Stale Proxmox rows fixed. Every new field since has its rows.
- **Weekly drift job** [#184]: `.github/workflows/coverage-drift.yml` runs every
  `scripts/refresh-*` against the newest upstream releases and opens or updates one issue (label
  `coverage-drift`); `scripts/coverage-drift.sh --dry-run` prints it.
- **Line coverage floors** per crate (cargo-llvm-cov, `.github/coverage-floors.txt`) [#166]; the
  const row helpers are run at run time in tests so llvm-cov sees them [#198].
- **Every format table at 100%** ([#217] to [#222]): Compose, the fourteen Vagrant tables, the
  nine Terraform tables (Proxmox, ESXi, AWS, Azure, Google Cloud, DigitalOcean, Linode, Oracle
  Cloud, the utility providers), cloud-init, the Ansible inventory, the Dockerfile, Kubernetes,
  Kustomize and Helm: 30 tables, every portable feature done. The last rows:
  - Oracle Cloud's IPv6 rows are written, or re-decided in ADR 0006 (`oci_core_ipv6`, `ipv6id`
    and the subnet's `ipv6cidr_blocks` equivalent, bring-your-own IPv6 not portable); the spec
    table's `networks.*.ipv6.cidr` is done on Azure and Oracle Cloud, `ipv6.addresses` on Oracle
    Cloud [#217].
  - Terraform: ESXi reaches 100%, and the spec table's `esxi` column flips IPv6, MTU, `external`,
    `bridged`, secrets, disks and firmware [#218].
  - The six `virtiofs` rows and `proxmox_hardware_mapping_dir` are written (Terraform: Proxmox at
    100%); a written Terraform argument's own rule may carry its note [#219].
  - Azure 96% to 100%, with its AAAA and PTR records; Google `scheduling` and
    `scheduling.on_host_maintenance` written (TERMINATE for GPUs, MIGRATE otherwise), spot,
    preemption and sole tenancy Tooling, restart and stop policies the cloud's own [#220].
  - Compose `configs[].uid`, `gid`, `mode` and `configs.*.environment`, `deploy.resources.reservations`
    and Kubernetes `PodSpec: subdomain` written; Kubernetes `defaultUser` and `KeyToPath: user`
    not portable (alpha, gated); QEMU `vmnet_interface` not portable (vagrant-qemu's one advanced
    NIC is always the lab network's) [#221].
  - AWS's four several-address rows written or equivalent, Google Cloud's `secondary_ip_range`
    equivalent to its alias ranges, Oracle Cloud's `oci_core_private_ip` written, and a
    `machines.*.networks[]` row in the spec table [#222].
  - The schemas of the new resource types are refreshed into `coverage/terraform/`.
- **The spec table** reads the other way, from each spec field to each output. It is not at
  100%: a field an output can't render is refused there with its reason, and a few cells are
  still planned (external 16, hybrid 7, each cloud 6 or 7, Proxmox and ESXi 3, Kubernetes, Helm
  and Kustomize 1). [docs/COVERAGE.md](docs/COVERAGE.md) lists each.
- **Scores along the way** (each from the feature's pull request): Compose 92% to 94% (shared
  volumes, [#159]), 88% to 92% ([#157]), 68% of 148 portable features after v2 (was 95% of 59)
  [#181], 68% to 70% ([#186]), 71% to 78% of 150 ([#189]), 84% to 85% ([#193]); Vagrant 83% to
  88% ([#156]), 90% to 95% ([#194]), Docker 92% to 100% ([#193]); libvirt 15% to 30% ([#156]), 30%
  to 35% ([#158]), 35% to 52% ([#194]); ESXi (Vagrant) 47% to 57% ([#156]), 89% to 100% ([#194]);
  Vagrant networks 73% to 81% ([#186]); Ansible inventory 95% to 100% ([#194]); Kubernetes 88%
  to 89% ([#186]), 91% of 201 ([#189]), 92% to 93% ([#194]), 93% to 96% ([#193]); Terraform:
  Proxmox 54% to 57% ([#186]), 57% to 64% ([#194]), 76% to 93% of 181 ([#207]); AWS 58% to 69%,
  Google Cloud 55% to 64%, Linode 48% to 59%, DigitalOcean 71% to 75% ([#186]); Azure 74% to 75%,
  Google Cloud 64% to 67%, Oracle Cloud 45% to 48% ([#194]); Terraform ESXi 0% of 4 to 77% of
  35, spec table `esxi` 67% of 111 ([#209]). The current figures are in
  [docs/COVERAGE.md](docs/COVERAGE.md).

### Runtime and CLI

- **`--json`** on `run`, `down`, `reset`, `provision`, `test`, `capture`, `status`, `connect`,
  `exec`, `tc` and `gc` [#170] ([#146]): newline-delimited `step`, `log`, `check` and `result`
  events for the long commands, one document for the one-shot ones (`connect --json` prints the
  command that opens the shell, without running it), and a final
  `{ "error": { "code", "message", "machine", "step", "hint", "problems" } }` with a stable code on
  failure. Described in [docs/json.md](docs/json.md); the schema is docs/events.schema.json and
  `isoloom schema --events`. Without `--json` every command prints what it printed before. New
  error code `secret_missing` [#189].
- **Picking the cloud of `cloud-vm`** [#202]: `--cloud` (on `run`, `down`, `reset` and `test`);
  else the cloud `run` recorded (what `exec`, `connect` and `capture` use); else AWS when it can
  run the environment; else the only cloud that can. When AWS can't and several can, the command names them; `run --cloud <cloud>`
  with a cloud that refuses stops with its reason. The cloud is recorded and in the `result`
  event.
- **`--profile <name>`** on every command (or `ISOLOOM_PROFILE`) [#152] ([#132]): reads
  `profiles/<name>.yml` (`target:`, `spec:`, `defaults:`, `inputs:`); a profile naming an unknown
  machine, network or input is an error. `isoloom profiles` lists and checks them. `run` records
  the profile in `~/.isoloom/status.yml`, and `status` shows it.
- **New commands:** `isoloom provision [machine...] [--target t]` re-runs provisioning on a running
  local-VM environment, limited to the named machines [#31]; `isoloom chart [DIR] --version 4.2.1
  [-o DIR]` writes and packages the Helm chart [#154]; `isoloom profiles` [#152]; `isoloom docs
  --spec` prints the spec reference [#213]; `isoloom down --volumes` also removes persistent volumes,
  which `down` and `reset` keep [#212].
- **`isoloom run`** reads every secret before anything starts and stops with `secret_missing`
  naming each it can't read [#189]; delivers secrets while the target's tool runs, trying each
  waiting machine every few seconds [#200]; checks `cloud.requires` and runs its shell commands
  [#177]; asks `refusal_on` for the provider in the defaults and refuses up front (QEMU, UTM,
  Hyper-V) ([#123], [#177], [#187]); checks pywinrm before the tool starts for Windows secrets
  [#208].
- **`isoloom doctor`** lists the spec's secrets and where each comes from [#189], looks up
  `cloud.requires` [#177], and counts Hyper-V on Windows when its service runs and the shell is
  elevated [#177]. `cloud.requires` on Windows looks a command up on the PATH with `PATHEXT`, and
  runs a shell command in PowerShell [#177] ([#150]).
- **`isoloom targets`** lists the refused clouds under `cloud-vm`, one indented line each [#178],
  and validates the spec instead of panicking on an invalid one [#116] ([#105]).
- **`isoloom status`** counts the spec's machines only on Vagrant outputs (a halted controller is
  never a machine down) [#30], and says `deploying` while a cloud-services apply runs [#177].
- **`isoloom test`** boots a halted controller for the checks and halts it again [#30].
- **`isoloom coverage`** renders the spec table's per-output scores [#184].
- **Checks:** `http` `follow` uses curl `-L --max-redirs 5`; wget, which always follows, has each
  answer's status read, the last of at most 6 compared; bash's fallback doesn't follow [#122].
  `tcp` `contains`/`send` use `nc` (busybox in the runner image, the VM's own), else bash's
  `/dev/tcp`, stdin kept open and nc stopped after 5 seconds [#78]. hello-stack's `/hello` answers
  301, and its cache gets a `PING`.
- **Import:** `import compose` reads `53:53/udp` and `protocol: udp`, notes a port of another
  protocol (`/sctp`) drafted as TCP, and keeps `build.target`; `import vagrant` reads a forwarded
  port's `protocol: "udp"` and `reboot: true` ([#124], [#187], [#80]). A key set twice in one
  mapping keeps the last value, as Compose does, and the draft's notes say which [#74] ([#68]).
- **The resolved snapshot** (`.isoloom/resolved.json`, still version 1): `targets` gains `clouds`
  (generated and refused clouds of `cloud-vm`) [#178]; a network's `ipv6` and a machine's
  `ipv6_addresses` [#186]; `protocol: udp` [#124], `dormant: true` [#79], `follow: true` [#122];
  each machine's `role` (`target`, or `infra` for the controller with `keep_running`) [#30];
  `cloud.hourly_usd` [#123]. No secret value is ever in it [#189].
- **`~/.isoloom/status.yml`** starts with `version: 1` (`registry::VERSION`), written on every
  save; a file without it reads as version 1 [#121] ([#108]). It records the profile [#152] and
  the cloud picked [#202].
- **Real targets in CI** [#203] ([#145]): `real-vagrant.yml` (runners labelled `virtualbox` and
  `libvirt`: solo-web, segmented, windows-hello where allowed), `real-proxmox.yml` (a `proxmox`
  runner: hello-stack, segmented) and `real-cloud.yml` (solo-web on `cloud-docker`, one job per
  cloud), weekly or by hand, never on pull requests; a gate job skips each job when its runner or
  secrets are missing. Budget guard: the smallest sizes, an `isoloom-expires-at` tag, VMs that
  stop on their own, timeouts, `isoloom down` in an `always()` step, and
  `scripts/real-targets/reap.sh`, run before every job and daily. Each job uploads a record
  (`isoloom-real-target/1`, `scripts/real-targets/record.sh`). [docs/real-targets.md](docs/real-targets.md)
  lists the runners, secrets, variables and budget caps the maintainer provides.
- **Host probes** don't open console windows on Windows (`CREATE_NO_WINDOW`) [#101].
- **No `unsafe`:** a profile's inputs reach every tool through `Command::envs` instead of the
  command line setting its own environment [#170].

### Library

- **One crate per layer and backend** [#164]: `isoloom-spec`, `isoloom-plan`, `isoloom-render`,
  `isoloom-compose`, `isoloom-kubernetes`, `isoloom-terraform`, `isoloom-vagrant`,
  `isoloom-external`, `isoloom-targets`, `isoloom-import`, `isoloom-coverage`, `isoloom-runtime`
  and the command line, `isoloom`. Integration tests live in the crates they test [#178].
- **A stability policy** [#121] ([#108]): the README's "Using the library" names the supported
  surface; a breaking change to it bumps the major version from 1.0 on, and gets a "Breaking
  (library)" line here. CI runs `cargo semver-checks` on every pull request against its base and
  the last tag, and reports it in the job summary.
- **New public items:** `refusal_on(spec, target, provider)` (`qemu_refusal` is deprecated, a thin
  wrapper) [#123] ([#107]); `isoloom_targets::cloud_refusals` and `CloudRefusal` [#178];
  `isoloom_runtime::events` (the events, `ErrorCode`, `Emitter` with a callback or a channel) and
  `tool::run_logged`; `host::check_in` and `host::run_limited_in` take extra environment variables
  [#170]; `host::requirements` and `host::requirements_with`, `CloudServices::inputs_in` and
  `fill_inputs`, `report::hyperv_ports_note`, `isoloom_vagrant::vagrant::hyperv` and
  `hyperv_refusal` [#177]; `profile` (load, list, check, pin), `load_profiled` and
  `defaults::load_profiled` [#152]; `project_copies(files, dir)` [#154]; `Problem::new` and
  `Problem::section`, with validation in one validator per section [#170];
  `isoloom_plan::boot`, `Feature::OwnBoot`, `isoloom_vagrant::vagrant::provider_refusal`,
  `VmImage: Default` [#197]; `isoloom_spec::windows` (`Tpm`, `tpm`, `winssh`), `VmImpl::tpm`,
  `VmImage::winssh`, `inventory::WinrmAuth`, `Access::proxmox(windows_password)`,
  `Access::cloud(.., auth)`, `vm::controller::inventory_template` [#194];
  `isoloom_spec::model::{Ipv6, Ipv6Addresses}`, `validate::{Cidr6, IPV6_PREFIX}`,
  `isoloom_plan::ipv6`, `Environment::probe_address`, `isoloom_render::ipv6`,
  `vm::limits::Limits::ipv6` [#186]; `isoloom_plan::hardware::Need` [#195];
  `isoloom_plan::secrets::VM_DEFAULT_MODE` [#208]; each secret store is an
  `isoloom_runtime::secrets::SecretStore` [#189]; `support::PUBLIC_IP` [#205]; `Target::Esxi`,
  `isoloom_terraform::esxi::EsxiBackend`, `Connection::SshVia`, `Provisioner::via`,
  `Variable::typed` and `Variable::validated` [#209]; `vm::gateways`, `vm::reach` [#211];
  `Lifetime::{Environment, Persistent, External}`, `DataDisk` [#212]; `Registry::default()`
  [#121].
- **Code quality** ([#106], [#166], [#179]): every workspace lint at `deny` with no tracked
  exception (`missing_docs`, `expect_used`, `too_many_lines` at 60, `cognitive_complexity` at 15,
  clippy pedantic, no `unwrap`, `panic`, `dbg!` or `todo!`, at most 5 parameters), `unsafe_code`
  forbidden, a 400-line limit per source file enforced by a test; property tests for IPv4 blocks
  and the shell, HCL, Ruby and PowerShell quoting; snapshot tests of every example's normalized
  spec; a golden test over every example [#119]; CI jobs for line coverage, API docs, unused
  dependencies (cargo-machete), `cargo deny`, spelling (typos), determinism (every example
  rendered twice), MSRV 1.88 and tests on macOS and Windows [#112], and the examples driven
  through `run`, `status`, `test`, `reset`, `down` and `gc` [#114]. ARCHITECTURE.md,
  CONTRIBUTING.md and the decision records (docs/adr/).
- **Documentation** [#213]: [docs/spec.md](docs/spec.md) is generated from the schema and the spec
  table (a test fails when it is stale); new guides [docs/targets.md](docs/targets.md),
  [docs/secrets.md](docs/secrets.md), [docs/storage.md](docs/storage.md) and
  [docs/getting-started.md](docs/getting-started.md); every `isoloom.yml` block in the docs
  validates and every link between them resolves. The cloud modules' interface and
  `status.yml` are documented ([#117]).

### Fixes

- Ruby strings escape `#$` and `#@` as well as `#{`; PowerShell strings double the typographic
  single quotes [#166].
- `config.winrm.retry_limit` isn't a Vagrant 2.4.9 setting (Vagrant rejected the Vagrantfile):
  the generator writes `max_tries` [#183].
- QEMU: a network's hub stops with its last VM, not its first [#199] ([#188]).
- A container publishing a fixed port (`fixed: true`) gets a valid `host:port` mapping and
  forward in hybrid [#168].
- Proxmox applies a network's `tc` on the machines (it was silently dropped), and a dependency
  without ports no longer fails the wait [#171].
- AWS declares the TLS provider whenever there is a controller (a Windows-only environment with
  checks used it undeclared); the Windows ready marker is `C:\isoloom\ready` everywhere [#171].
- The Google Cloud labels and some Azure blocks now pass `terraform fmt -check`; the Google Cloud
  controller carries the environment's labels; Azure's Windows `reach` rules are named like
  Linux's [#174].
- An Isoloom container whose image happened to be a machine's (udp-dns's `wait-dns`) keeps its own
  image in the Helm chart [#169].
- The controller's hosts file keeps the aliases [#174]; a play cut by a dropped connection runs
  again only on the hosts it was cut on [#176].
- Linode's firewall opened every published port on every machine, and Oracle Cloud's security
  list did too; each now opens a machine's own ports only ([#205], [#211]).
- `isoloom targets` no longer panics on an invalid spec, `refusal` refuses an invalid spec with
  its first problem, and `checks::plan` no longer indexes an undeclared network [#116] ([#105]).
- Windows host probes no longer flash console windows from a windowed app [#101].
- The project copy into VMs no longer follows symbolic links (a link loop failed it and Vagrant
  destroyed the VM), nor fails on an entry deleted during `vagrant up` [#76] ([#67], [#63]).
- A volume mounted over a path the image has no longer hides the image's files on Kubernetes
  [#120] ([#102]).
- The secrets test compares generated paths by component, so it passes on Windows [#215].
- The cargo audit ignores the unsafe-libyaml unmaintained notice (RUSTSEC-2024-0449) [#130].

[#20]: https://github.com/isoloom/isoloom/issues/20
[#24]: https://github.com/isoloom/isoloom/issues/24
[#25]: https://github.com/isoloom/isoloom/issues/25
[#29]: https://github.com/isoloom/isoloom/issues/29
[#30]: https://github.com/isoloom/isoloom/issues/30
[#31]: https://github.com/isoloom/isoloom/issues/31
[#59]: https://github.com/isoloom/isoloom/issues/59
[#60]: https://github.com/isoloom/isoloom/issues/60
[#61]: https://github.com/isoloom/isoloom/issues/61
[#62]: https://github.com/isoloom/isoloom/issues/62
[#63]: https://github.com/isoloom/isoloom/issues/63
[#67]: https://github.com/isoloom/isoloom/issues/67
[#68]: https://github.com/isoloom/isoloom/issues/68
[#69]: https://github.com/isoloom/isoloom/issues/69
[#72]: https://github.com/isoloom/isoloom/issues/72
[#73]: https://github.com/isoloom/isoloom/issues/73
[#74]: https://github.com/isoloom/isoloom/issues/74
[#75]: https://github.com/isoloom/isoloom/issues/75
[#76]: https://github.com/isoloom/isoloom/issues/76
[#78]: https://github.com/isoloom/isoloom/issues/78
[#79]: https://github.com/isoloom/isoloom/issues/79
[#80]: https://github.com/isoloom/isoloom/issues/80
[#81]: https://github.com/isoloom/isoloom/issues/81
[#82]: https://github.com/isoloom/isoloom/issues/82
[#85]: https://github.com/isoloom/isoloom/issues/85
[#86]: https://github.com/isoloom/isoloom/issues/86
[#92]: https://github.com/isoloom/isoloom/issues/92
[#98]: https://github.com/isoloom/isoloom/issues/98
[#101]: https://github.com/isoloom/isoloom/issues/101
[#102]: https://github.com/isoloom/isoloom/issues/102
[#105]: https://github.com/isoloom/isoloom/issues/105
[#106]: https://github.com/isoloom/isoloom/issues/106
[#107]: https://github.com/isoloom/isoloom/issues/107
[#108]: https://github.com/isoloom/isoloom/issues/108
[#112]: https://github.com/isoloom/isoloom/issues/112
[#114]: https://github.com/isoloom/isoloom/issues/114
[#116]: https://github.com/isoloom/isoloom/issues/116
[#117]: https://github.com/isoloom/isoloom/issues/117
[#119]: https://github.com/isoloom/isoloom/issues/119
[#120]: https://github.com/isoloom/isoloom/issues/120
[#121]: https://github.com/isoloom/isoloom/issues/121
[#122]: https://github.com/isoloom/isoloom/issues/122
[#123]: https://github.com/isoloom/isoloom/issues/123
[#124]: https://github.com/isoloom/isoloom/issues/124
[#126]: https://github.com/isoloom/isoloom/issues/126
[#130]: https://github.com/isoloom/isoloom/issues/130
[#132]: https://github.com/isoloom/isoloom/issues/132
[#133]: https://github.com/isoloom/isoloom/issues/133
[#134]: https://github.com/isoloom/isoloom/issues/134
[#145]: https://github.com/isoloom/isoloom/issues/145
[#146]: https://github.com/isoloom/isoloom/issues/146
[#147]: https://github.com/isoloom/isoloom/issues/147
[#150]: https://github.com/isoloom/isoloom/issues/150
[#152]: https://github.com/isoloom/isoloom/issues/152
[#153]: https://github.com/isoloom/isoloom/issues/153
[#154]: https://github.com/isoloom/isoloom/issues/154
[#156]: https://github.com/isoloom/isoloom/issues/156
[#157]: https://github.com/isoloom/isoloom/issues/157
[#158]: https://github.com/isoloom/isoloom/issues/158
[#159]: https://github.com/isoloom/isoloom/issues/159
[#164]: https://github.com/isoloom/isoloom/issues/164
[#165]: https://github.com/isoloom/isoloom/issues/165
[#166]: https://github.com/isoloom/isoloom/issues/166
[#167]: https://github.com/isoloom/isoloom/issues/167
[#168]: https://github.com/isoloom/isoloom/issues/168
[#169]: https://github.com/isoloom/isoloom/issues/169
[#170]: https://github.com/isoloom/isoloom/issues/170
[#171]: https://github.com/isoloom/isoloom/issues/171
[#172]: https://github.com/isoloom/isoloom/issues/172
[#173]: https://github.com/isoloom/isoloom/issues/173
[#174]: https://github.com/isoloom/isoloom/issues/174
[#175]: https://github.com/isoloom/isoloom/issues/175
[#176]: https://github.com/isoloom/isoloom/issues/176
[#177]: https://github.com/isoloom/isoloom/issues/177
[#178]: https://github.com/isoloom/isoloom/issues/178
[#179]: https://github.com/isoloom/isoloom/issues/179
[#180]: https://github.com/isoloom/isoloom/issues/180
[#181]: https://github.com/isoloom/isoloom/issues/181
[#182]: https://github.com/isoloom/isoloom/issues/182
[#183]: https://github.com/isoloom/isoloom/issues/183
[#184]: https://github.com/isoloom/isoloom/issues/184
[#185]: https://github.com/isoloom/isoloom/issues/185
[#186]: https://github.com/isoloom/isoloom/issues/186
[#187]: https://github.com/isoloom/isoloom/issues/187
[#188]: https://github.com/isoloom/isoloom/issues/188
[#189]: https://github.com/isoloom/isoloom/issues/189
[#190]: https://github.com/isoloom/isoloom/issues/190
[#192]: https://github.com/isoloom/isoloom/issues/192
[#193]: https://github.com/isoloom/isoloom/issues/193
[#194]: https://github.com/isoloom/isoloom/issues/194
[#195]: https://github.com/isoloom/isoloom/issues/195
[#196]: https://github.com/isoloom/isoloom/issues/196
[#197]: https://github.com/isoloom/isoloom/issues/197
[#198]: https://github.com/isoloom/isoloom/issues/198
[#199]: https://github.com/isoloom/isoloom/issues/199
[#200]: https://github.com/isoloom/isoloom/issues/200
[#202]: https://github.com/isoloom/isoloom/issues/202
[#203]: https://github.com/isoloom/isoloom/issues/203
[#205]: https://github.com/isoloom/isoloom/issues/205
[#206]: https://github.com/isoloom/isoloom/issues/206
[#207]: https://github.com/isoloom/isoloom/issues/207
[#208]: https://github.com/isoloom/isoloom/issues/208
[#209]: https://github.com/isoloom/isoloom/issues/209
[#210]: https://github.com/isoloom/isoloom/issues/210
[#211]: https://github.com/isoloom/isoloom/issues/211
[#212]: https://github.com/isoloom/isoloom/issues/212
[#213]: https://github.com/isoloom/isoloom/issues/213
[#214]: https://github.com/isoloom/isoloom/issues/214
[#215]: https://github.com/isoloom/isoloom/issues/215
[#217]: https://github.com/isoloom/isoloom/issues/217
[#218]: https://github.com/isoloom/isoloom/issues/218
[#219]: https://github.com/isoloom/isoloom/issues/219
[#220]: https://github.com/isoloom/isoloom/issues/220
[#221]: https://github.com/isoloom/isoloom/issues/221
[#222]: https://github.com/isoloom/isoloom/issues/222

## 0.10.1

### Google Cloud: long environment names
- The project Isoloom creates on Google Cloud is named `isoloom-<environment>` cut to 30 characters, Google's limit: an environment named over 22 characters failed at plan time (`terraform validate` too).

## 0.10.0

### Cloud services: typed variables, launch-time inputs
- `cloud.vars` take any value (strings, numbers, booleans, lists, maps), and a variable whose value is exactly `{{ inputs.NAME }}` takes the launch-time input `NAME` (declared in `inputs`): `run` reads it from the environment (and refuses without it), writes it next to the state (mode 0600) and `down` destroys with the same values. For a player's IP in an allow-list (CloudGoat). (#98)

### Cloud services
- An environment can be cloud services instead of machines: `cloud: { provider, terraform, vars, outputs, hourly_usd }` names a Terraform root module in the project, applied into the user's own AWS, Azure or Google Cloud account by the new `cloud-services` target. `run` and `down` apply and destroy it (state and plugins under `.isoloom/cloud-services/`, the module untouched), `test` fills `{{ cloud.outputs.<name> }}` into the checks and runs them from here (scripts get `ISOLOOM_OUTPUT_<NAME>`), the message too, and `status` counts the deployed resources. Example: cloud-bucket (a public S3 website). Run against a real AWS account: cloud-bucket and 13 AWS labs (AWSGoat, CloudGoat scenarios, CloudFoxable, iam-vulnerable, sadcloud) applied, tested and destroyed. (#96)

### Cloud resources say which instance they are, and when they end
- Every cloud module takes `expires_at` (Unix seconds; empty by default) and tags what it creates with the environment, the instance (`isoloom-instance` = the module's unique name; DigitalOcean and Linode: an extra tag) and `isoloom-expires-at` (AWS cloud-vm through the provider's default tags), so a reaper can find what to destroy after a crash or a lost state. Proxmox VMs keep their environment-named tags. (#88)

### Emulated machines on one vCPU
- On QEMU, a machine emulating another CPU (x86 on an Apple Silicon Mac) gets one virtual CPU. QEMU emulates another architecture on a single host thread, so a second vCPU only took turns with the first, and could see code the other was rewriting half-done: Windows' PowerShell died now and then jumping into the data bytes of a .NET call stub being patched (illegal instruction `push ds`), which cut WinRM in the middle of GOAD-Mini's setup.

### Setup that survives a flaky connection
- The environment's playbooks run again when a dropped connection cut them (WinRM's shell crashing mid-task, an SSH reset, a host briefly unreachable), up to 4 runs in all: the setup is idempotent, so a run picks up where the last stopped. A task that fails on its own still stops the setup at once. Under x86 emulation on an Apple Silicon Mac, Windows' PowerShell host crashes now and then (an illegal-instruction fault in .NET), which stopped GOAD-Mini's setup twice in one build.
- Role downloads (`ansible-galaxy install`, from GitHub) are tried 5 times with a growing pause (`ISOLOOM_RETRY_PAUSE`, 15 s by default).

### x86 machines on an Apple Silicon Mac, with QEMU
- On Vagrant's QEMU provider (vagrant-qemu), each machine runs with its own CPU (`v.arch` from `arch`), emulated when the host's is another: an x86 Windows DC boots on an Apple Silicon Mac, slowly, so emulated machines get an hour to boot and a longer WinRM budget. A QEMU box can be given per image (`vm.image.qemu`, libvirt format); `windows-server-2019` and `debian-12` have one built in, and Isoloom's own VMs (controller, router, tool shell) use `cloud-image/debian-12` there.
- Machines start one at a time, in dependency order, on every provider (`VAGRANT_NO_PARALLEL` in the Vagrantfile): QEMU and libvirt declare themselves parallel, so the controller ran its play before the machines were up.
- Private networks on QEMU without root: a network of exactly two VMs is a `socket` listen/connect pair on a loopback port. A Windows machine's lab address is set from PowerShell (no cloud-init). `qemu_refusal` says when a spec doesn't fit (more than two VMs on a network, a VM on several), so a launcher can decline QEMU up front.


### `isoloom reset`
- `isoloom reset <target>`: back to the environment as it came up, what was done in it since gone (files dropped, users added, databases changed). Docker: torn down with its volumes and run again from the same images (init jobs run again). Vagrant: `isoloom run vagrant` saves a baseline snapshot of every VM once provisioned (`isoloom-baseline`; `ISOLOOM_NO_BASELINE=1` skips it), and `reset` restores it without provisioning; without one, down and run. Other targets: down and run. The Vagrant snapshot path is not run end to end yet. (#89)

### Labels on everything, and `isoloom gc`
- On Docker, every container, built image, network and volume an environment creates carries `isoloom.managed=true` and `isoloom.environment=<name>`.
- `isoloom gc` lists Docker leftovers of environments: labelled networks and volumes whose Compose project has no container left (a crashed run, a killed launcher), and with `--images` the built images no container uses; `--yes` removes them. It never touches containers (a parked environment keeps its volumes) or anything unlabelled. Cloud tags follow. (#88)

## 0.9.0

### Fixed published ports
- `fixed: true` on a service keeps its `publish` port as the host port on local Docker too (normally a free one, so labs never collide), for apps whose pages call `localhost:<port>` themselves. Launchers that pin ports keep a fixed one as is. (#70)

### Routes survive a machine's restart
- On Docker, a machine whose container restarted got a new network namespace with Docker's default route back (internet regained, router routes lost), its route sidecar left in the old one. The sidecar now exits once its namespace has no addresses left, and its restart sets the routes in the new one. segmented passes all its checks after every machine is restarted. (#65)

### Older systems, fresh check scripts on VMs
- OS names `ubuntu-14.04`, `ubuntu-16.04`, `ubuntu-18.04`, `windows-server-2008r2` and `windows-server-2012r2`, for labs about older systems (Metasploitable 3): built-in Vagrant boxes, Proxmox images for 16.04 and 18.04; targets without an image decline them, saying why. (#57)
- `isoloom test` on Vagrant runs the check scripts as they are in the project now: the `checks` provisioner writes them over the VM's copy before the runner. (#58)

### Inputs, TLS services, aliases
- `inputs` take any environment variable name (DVLA reads `model_name`); `import compose` keeps the name as written. (#53)
- `tls: true` on a service: derived checks reach it over TLS (https for `http` services). The Compose service label is unchanged. (#54)
- `aliases` on a machine: more DNS names (`api.example.com`), as Compose network aliases and in every Linux VM's `/etc/hosts`. Not on Kubernetes. Example: edge-firewall's web answers as www.edge.test. (#55)

### Richer `http` checks
- `http` checks take `method`, `headers`, `body` and `contains` (text the response body must contain): a login, an authenticated API call or a page's content without a script. Rendered as one curl call (the runner says so where curl is missing); a plain GET keeps curl, then wget, then bash. hello-stack checks its page's text and that a POST gets 405. (#51)

### Small containers
- The 256 MB / 5 GB floor on `resources` is a VM's: it now applies only to machines with `vm:`. A container-only machine needs at least 1 cpu and 16 MB (crAPI's services run at 50 to 192 MB). (#49)

### `exec` checks on Docker and Kubernetes
- An `exec` check no longer makes the container targets refuse the spec: each machine's `exec` checks get their own runner (`.isoloom/<docker|kubernetes>/checks/exec-<machine>.sh`), which `isoloom test` pipes into the machine itself (`docker compose exec -T <machine> sh -s`, `kubectl exec -i deploy/<machine> -- sh -s`); the docker-vm `checks` provisioner too. hello-stack reads its seeded greeting with `redis-cli` inside the cache. An `exec` check from a machine without a container of its own is refused, saying so.

### Checks don't re-run init jobs
- `isoloom test` (and the docker-vm `checks` provisioner) ran each runner with `docker compose run`, which starts its dependencies again, and completed `init:` jobs count as not running: every test re-seeded the environment. Runners now run with `--no-deps` (the environment is up); a stand-in for a supplied access machine is started first. (#47)

### Init jobs on a machine nothing depends on
- `docker compose up --wait` fails when a one-shot exits (even with 0) unless a running service depends on it, so an `init:` on a machine nothing depends on (a single-machine lab seeding itself) failed `isoloom run docker` although everything worked. Starting is now two steps when such leaf jobs exist: `up -d --build --wait` on everything else, then each job attached, in order (`up --no-deps --exit-code-from <job> <job>`), failing on its exit code. `isoloom run`, the docker-vm Vagrantfile and the cloud-docker modules do it; files without leaf jobs keep their single `up --wait`. Embedders get the same rule from `generate::start_plan(compose_yaml)` (or `leaf_jobs(spec)` and `start_commands`). (#42)

### A Dockerfile outside the build folder, and build arguments
- `docker.dockerfile`: the Dockerfile, a file anywhere in the project (default: `Dockerfile` in `build`), so an image of your own can wrap a vendored project's source without moving it into your build folder. `docker.args`: build arguments, fixed values. Compose gets `build: { context, dockerfile, args }`; `isoloom import compose` now carries `dockerfile` and literal `args` over (shell-read ones are reported). Example: slow-link. (#40)

### Healthchecks without a shell in the image
- A machine's healthcheck no longer borrows the image's tools (`sh`, then `nc` or `bash`), so distroless, `scratch` and minimal images turn healthy too (OWASP Juice Shop ships on distroless Node). Isoloom brings a static busybox (`busybox:1.37.0-musl`): on Docker a one-shot `isoloom-probe-<arch>` copies it into a volume each machine with services mounts read-only at `/.isoloom-probe`, and the healthcheck runs it in exec form; on Kubernetes an init container copies it into an `emptyDir` for the readiness probe. `init:` jobs still run with the image's own `sh`. (#37)

### More network appliances: Cisco QEMU images and Dynamips
- `appliance: cisco-vios | cisco-vios-l2 | cisco-csr1000v | cisco-c8000v`: vrnetlab's QEMU images, as containerlab runs them: `launch.py` with its arguments (`tc` connection mode), `CLAB_INTFS`, the startup configuration in `/config/startup-config.cfg` (applied once the VM has booted), privileged for /dev/kvm. IOSv's data interfaces are `GigabitEthernet0/1`..., IOS XE's `GigabitEthernet2`.... Example: cisco-qemu (IOSv and CSR1000v, OSPF).
- `appliance: cisco-dynamips` with `docker.firmware: <your IOS .bin>`: a Cisco 7200 emulated by Dynamips, in a container Isoloom builds (Ubuntu's `dynamips`); the data interfaces bind to `FastEthernet0/0`, then `1/0`, `1/1`, `2/0`... on PA-2FE-TX adapters. No KVM needed. Example: cisco-dynamips.
- Appliances' interfaces are named outright (`interface_name`, Compose 2.36+ / Docker 28.1+): Docker's own attach order turned out not to follow `priority`.

### Network appliances: Cisco IOL
- `docker: { image, appliance: cisco-iol | cisco-iol-l2, config }`: a router or switch OS in a container, wired the way its image expects. For Cisco IOL as vrnetlab packages it (`vrnetlab/cisco_iol:<version>`): the management port (`Ethernet0/0`, its own VRF) on Isoloom's management network (10.255.255.0/24), the machine's networks in order as `Ethernet0/1`, `0/2`, `0/3`, `1/0`..., the NETMAP and iouyap files, `IOL_PID`, and a startup configuration with the hostname, `admin`/`admin` over SSH and each interface's address from the spec, followed by the machine's own `config` (OSPF, ACLs). The container gives its data addresses to IOS. Docker targets only; an appliance can't be the access machine or run checks. Isoloom ships no images: build them with vrnetlab from a release you're licensed for. Example: cisco-iol (two sites with OSPF).

### 802.1Q trunks
- A machine on several VLANs of one LAN gets one tagged link instead of one interface per VLAN, as a router-on-a-stick or a server on a trunk port has: on Docker, its interface is named after the LAN with its addresses on `<lan>.<id>` subinterfaces (`office.10`, `office.20`), and frames cross the trunk with their 802.1Q tag (a capture on it shows them). A switch container per LAN (`isoloom-switch-<lan>`, at the controller address of each VLAN, unused on Docker) bridges each VLAN's network to the trunks' subinterfaces. Machines on one VLAN, the router, the internet and the checks are as before. Example: vlan-office's admin box.
- On local VMs (Vagrant) and Proxmox, a Linux VM on several VLANs gets the same view: an interface named after the LAN, its addresses on `<lan>.<id>`, tagged frames on it. The hypervisor still carries each VLAN on its own network; inside the VM a veth pair stands for the trunk and a bridge per VLAN joins its `.<id>` to the VLAN's NIC, rebuilt at boot with the routes. Each `<lan>.<id>` takes its NIC's MAC address (the NIC, now a bridge port, a local one), so the hypervisor sees only the MACs it gave: no promiscuous mode, on every provider.

### Link impairment on the machines too
- `networks.*.tc` now also applies on every machine's own interface on the network, not only on the router's interface into it: traffic between two machines of the network (a direct link with no router, or a LAN) is impaired too, and `delay` is the one-way latency in both directions (80 ms gives a 160 ms round trip through the router). Docker: the machine's network sidecar runs `tc` (netshoot, which has it; the utility image doesn't). Local VMs: a oneshot unit on each Linux VM, re-applied at boot. A network no longer needs the router on it to take `tc`.
- `isoloom tc show|set|disable|reset` acts on the router and on every machine of the network.

## 0.8.1

### Proxmox: checks and a way in
- `isoloom test proxmox` runs the checks: a runner per position in `.isoloom/proxmox/checks/<position>.sh`, piped over SSH to the machine it stands for through the router (`ssh -J isoloom@<address>`), the controller or the first machine for positions that aren't a VM. A machine whose runner runs a `script:` gets the project in cloud-init even without provisioning. The module gains `machines` (each VM's first address), `ssh_user` and `checks` outputs.
- `isoloom connect`, `exec` and `capture` reach Proxmox machines the same way, as the `isoloom` user (`ssh_public_key`).

### Kubernetes
- `isoloom test kubernetes --no-derived` works: the check Jobs are rendered with `kubectl kustomize`, every runner gets `ISOLOOM_DERIVED=0`, and that manifest is applied.

## 0.8.0

The operating model around the spec: derived checks and `isoloom test`, the resolved snapshot, a registry with `status`/`connect`/`exec`/`capture`, instances, defaults, shared fields, clones, host readiness, `graph`/`report`, `message`, link impairment, the `external` target and tools. Every feature below landed between 0.7.2 and this release.

### `tools:` observers beside the environment
- `tools:` attaches observers on every network at reserved addresses (just below the controller's), outside the contract: no `reach` rule, check or resource total names them, and the environment behaves the same without them. `shell` is a built-in toolbox (tcpdump, nmap, curl, dig, netcat: netshoot on Docker, a Debian VM on local VMs); any other name is a container image with an optional `command`, `port` and `publish`. `isoloom connect <tool>` reaches it; the snapshot lists them. Validation reserves the tool addresses and checks names, recipes and ports.

### The `external` target: machines that already exist
- `machines.*.external: { address, user, port, key }` gives a machine's SSH endpoint; when every machine has one, the `external` target is possible. Isoloom creates nothing: `generate` writes `.isoloom/external/` (an Ansible inventory with the `linux`/`windows` and spec groups, `machines.json`, a check runner per machine), `run external` copies the project to each machine and runs its `.sh` steps there and its `.yml` playbooks from here, then the environment's `provision:` playbooks with the inventory; `test external` runs each machine's runner over SSH (probes only); `connect`, `exec` and `capture` reach the address; `status` says which machines answer SSH; `down` only forgets it. Networks and `reach` are expected behavior, verified by the derived checks.
- Example: existing-hosts.

### Link impairment: `networks.*.tc`
- `tc: { delay, jitter, loss, rate }` on a network: Isoloom's router applies Linux netem on its interface into it, so traffic entering from the other networks is delayed, jittered, lossy or capped. Docker (the router container gets iproute2) and local VMs (a oneshot unit re-applies it at boot). Kubernetes and the cloud refuse with the reason (no router in the path). Validation checks the values and that the router is on the network.
- `isoloom tc show|set|disable|reset <network>` changes it on a running environment.
- Example: slow-link, whose check measures the delay; it runs in CI on Docker.

### `message:` once the environment is up
- A top-level `message:` (Markdown) printed by `isoloom run` when the environment is up and by `isoloom message` any time; `{{ machines.web.services.0.publish }}`-style placeholders take any value of the resolved snapshot (addresses, published ports, the instance). The snapshot carries the rendered text for embedders. Validation catches unbalanced or empty placeholders; one pointing at nothing is an error naming it.

### `isoloom graph` and `isoloom report`
- `isoloom graph [--format d2|dot] [-o file]` draws the environment from the resolved snapshot: networks (block, offline), machines (services; the access machine as a person, gateways as diamonds), membership edges with the last octet, `reach` rules as dashed arrows with their ports, Isoloom's router. `-o lab.svg` / `.png` renders with `d2` or `dot` when installed, keeping the source beside it.
- `isoloom report addressing|services|wiring|resources [--md]`: tables from the same snapshot; `services` says from which networks each port may be reached.

### Host readiness: `targets --host` and `isoloom doctor`
- `isoloom targets --host` says, for each possible target, whether this machine can run it and what is missing: Docker and Compose v2; Vagrant and a provider (VirtualBox, VMware, Parallels, libvirt, UTM, QEMU, ESXi; hybrid needs VirtualBox); a reachable Kubernetes context; Terraform and each cloud's credentials (AWS keys/profile/file, `az` login or ARM variables, Google application default credentials, DigitalOcean and Linode tokens, `~/.oci/config`); Proxmox's endpoint and token.
- `isoloom doctor [--json]`: the same for every target, without a spec.
- `isoloom run` makes the check first and stops with the reason instead of failing halfway. `isoloom_core::host` for tools embedding Isoloom.

### VLANs under their LAN
- `networks.<lan>.vlans`: a LAN split into VLANs, by id (1–4094), each with its own `cidr` inside the LAN's block (which may then be as large as a /8) and optionally its own `internet` (the LAN's by default). Machines join one as `<lan>.vlan<id>`; a `reach` rule naming the LAN covers all of its VLANs. Each VLAN becomes a network of its own, `<lan>-vlan<id>`, on every target (right after parsing, so every generator and check works unchanged); the LAN stays a network only when a machine joins it directly. New example: vlan-office.

### `count:` for several of a kind
- A machine with `count: N` (2 to 99) becomes clones `<name>-01` to `<name>-NN`, each one address further along on every network. Where the spec names the base machine, every clone is meant: `depends_on`, a check's `from` (one check per clone), group members, provisioning groups, `isoloom exec <name>`. A gateway and the access machine take no count. The snapshot lists `clones`.

### Shared machine fields: `common:` and `groups:`
- `common:` holds machine fields every machine shares; `groups:` names sets of machines (names, globs like `ws*`, other groups) and the fields they share. They fold into the machines before anything else reads the spec: the machine's own value wins, then the most specific group (a member group over the group that holds it; otherwise the later declared), then `common`. Mappings merge, lists and scalars are replaced.
- What a machine *is* stays its own (`networks`, `services`, `access`); a shared `docker:` or `vm:` only completes an implementation the machine declares (even as `vm: {}`).
- Groups become Ansible inventory groups next to `linux` and `windows`, appear in the snapshot, and `isoloom exec <group> -- <command>` runs on their members. Validation names a member that matches nothing, a reserved or taken group name, and loops.

### Defaults: a hierarchy, and `-s key=value`
- Settings that are a person's or a team's rather than the spec's, in layers (the last wins, leaf by leaf): built in, `~/.isoloom/defaults.yml`, the project's `isoloom.defaults.yml`, `ISOLOOM_<KEY>` environment variables (`__` for a dot), and `-s defaults.<key>=<value>`. Keys: `images` (the image table: `generate --images` is now one more layer of it), `vagrant.provider` (passed to `vagrant up`), `cloud.<cloud>.region` (the generated module's default). Unknown keys are errors naming the layer.
- `-s key=value` on `generate`, `check`, `run`, `down`, `test` and `inspect` overrides the spec for that command (`-s machines.web.vm.os=ubuntu-24.04`), before it is parsed; the file is untouched.
- `isoloom defaults [--system] [--json]` prints every default in effect and where it comes from.

### Instances: the same environment several times on one host
- `--instance <1-99>` on `run`, `down`, `test`, `connect`, `exec`, `capture` and `inspect`. Instance `n` suffixes the name (`segmented-2`: Compose project, container, network, VM and namespace names all differ), moves the Docker networks to other blocks (the second octet shifts by `n`, since Docker refuses two networks on one subnet) and shifts published host ports by 100 per instance. Local VM networks are isolated by name already, so machines keep the addresses the spec writes.
- An instance's files go to `.isoloom-<n>/` next to the committed `.isoloom/` (add `.isoloom-*` to `.gitignore`); the snapshot there says `"instance": n`. The registry and `isoloom status` carry the instance.
- The VM targets' project copy leaves every `.isoloom*` folder behind.

### Lifecycle: `status`, `connect`, `exec`, `capture`
- `isoloom run` records what it brought up in `~/.isoloom/status.yml` (`$ISOLOOM_HOME` to move it): the spec's name, the project folder, the target, the cloud module and when; `isoloom down` forgets it. Tools embedding Isoloom read it through `isoloom_core::registry`.
- `isoloom status [--json]`: every environment up on this host with its live state from the target's own tool (`running (3/3)`, `partly`, `stopped`, `applied (12 resources)`), and `--cleanup <name>` to tear one down and forget it from anywhere.
- `isoloom connect <machine>`: a shell on the machine with the right tool for the target (`docker compose exec`, `vagrant ssh`, `kubectl exec`, `ssh` with Terraform's outputs); a Windows machine gets its RDP/WinRM address instead. The target comes from what `run` recorded for the folder, or `--target`.
- `isoloom exec <machine|all> -- <command>`: the command on one machine, or on every machine that can be reached, each line prefixed with the machine's name.
- `isoloom capture <machine> <network> [-- tcpdump args]`: tcpdump on the machine's interface on that network, found by its address from inside the machine's network namespace (a netshoot container on Docker, `sudo tcpdump` on local and cloud VMs).

### The resolved snapshot and `isoloom inspect`
- `isoloom generate` writes `.isoloom/resolved.json` with every target: the spec after Isoloom has worked everything out. Every address (machines on each network and on Docker's blocks, the router, the controller, the gateways), routes and default gateways, start order, published ports, the targets (possible, generated, refused with the reason, not possible with the reason) and the checks by position with the runner each target names them by. For programs: tools embedding Isoloom read it instead of parsing a Vagrantfile. `resolved_version: 1`.
- `isoloom inspect [PATH] [DIR] [--yaml]` prints the snapshot, or the part a dotted path names (`machines.web.addresses`, `checks.positions.0.runner`).
- JSON outputs keep the spec's order (networks and machines as written).

### Idle containers
- `docker: { idle: true }`: the image runs no service of its own (a stock Linux image whose command is a shell that exits at once), so its container is kept running idle, as a machine to work from. Compose (and every target built on it) sets `entrypoint: [sleep, infinity]`; Kubernetes sets the container's `command`. Without it, such a container exits and restarts in a loop.

### Checks: derived, declared, and `isoloom test`
- Isoloom derives checks from the spec: every service answers from each machine that `reach` (or a shared network) lets through, and from nowhere else; machines whose networks are offline don't reach the internet. They run from each machine's own position. A closed path to a machine with several addresses isn't asserted when another path to it is open.
- Declared checks next to scripts in `checks:`: `http` (a status code, `any` or `blocked`), `tcp` (`open` or `blocked`), `exec` (a command inside the machine, VM targets for now), `script`, each with `from`, `name`, `expect` and `wait` (a retry window). `isoloom validate` names the field when one is off.
- `isoloom test <target>`: runs every check runner for a running target (Compose `check` profile, `vagrant provision --provision-with checks`, Kubernetes Jobs, SSH on `cloud-vm` and `cloud-docker`), prints ✓/✗ per check with the reason, `--json`, `--no-derived`. Proxmox comes next.
- Generators write one runner script per position: `.isoloom/<target>/checks/<machine>.sh` (and `networks.sh` for checks without `from` when the spec has no access machine). Docker runs each in the machine's network namespace (`isoloom-check-<machine>`); Kubernetes gives each Job the machine's network labels; Vagrant uploads it to the machine; `cloud-vm` lists each run in the `checks` output (now a list with `position`, `machine`, `host`, `user`, `command`).
- Scripts run with the project folder as the working directory on every target (they used to run from `/` on Docker).
- Examples: segmented, hello-stack, edge-firewall declare their checks; air-gapped keeps none (all derived); arm-vm has an `exec` check. CI runs `isoloom test` on Docker and Kubernetes.

### Proxmox
- `isoloom targets` now tells the truth: a target can be possible by its machines' editions and still be refused by its generator (a Windows machine on Proxmox, environment-level provisioning). It is listed ✗ with the generator's reason, and `--json` leaves it out, instead of a ✓ that `generate` then declined. New `refusal(spec, target)` in isoloom-core.
- Environment-level `provision:` runs on Proxmox: a controller VM (Debian, on the uplink bridge for its route out and on every network at the controller address) carries the project, its own SSH key and the inventory, waits for each machine's ready marker, then runs the playbooks, as on Vagrant and the clouds. The `isoloom` user on every VM gets the controller's key beside the operator's. The generated Terraform pins `hashicorp/tls` for the key.

## 0.7.2

### cloud-vm on every cloud
- `cloud-vm` (one VM per machine) now generates for Azure, Google Cloud, DigitalOcean, Linode and Oracle Cloud, not only AWS. Each cloud is a driver that builds what it can model and declines the rest with a reason (the lab still runs on the clouds that fit it).
- Azure is faithful to the AWS model: static private addresses, several networks per machine, Windows over WinRM, the Ansible controller.
- Google Cloud takes single-NIC Linux with the controller; multi-NIC and Windows come later.
- DigitalOcean takes a single-network single-VM lab; Linode and Oracle Cloud take single-network Linux with static private addresses.
- Every cloud module matches its `cloud-docker` counterpart's variables and authentication (region, env-based cloud tokens, Azure subscription, Google project creation, Oracle compartment), so one launcher drives both.

### Hardening
- Volume mount paths reject spaces and `:` (they would corrupt the Docker short mount syntax and the provisioners' `mkdir -p`).
- Kubernetes `reach` NetworkPolicies are indexed, so a dash in a network name can no longer collide two of them.
- Spec validation enforces the Kubernetes name-length budget (the `isoloom-<name>` namespace, the `<machine>-published` Service, the `<machine>-<volume>` claim).
- Proxmox honours a machine's declared DNS servers and domain instead of forcing a public resolver.
- arm64 is caught on a built access machine (it would otherwise be given an x86-64 cloud image).

## 0.7.0

### New targets and commands
- `hybrid` target: containers and VMs on the same networks.
- `isoloom run` and `isoloom down`: generate a target and bring it up or tear it down.

### Cloud (AWS / Azure)
- `cloud-vm` on AWS: one VM per machine, machines on several networks, Windows machines, and environment-level `provision:` (Ansible from a controller).
- Terraform expressions are interpolated in set-up commands (a MAC lookup was emitted literally).
- AWS defaults are Free Tier eligible (t3.small, c7i-flex.large, m7i-flex.large).
- Azure defaults to Sweden Central with v6 AMD sizes (D2als_v6, D2as_v6, D4as_v6).
- Cloud outputs copy the project as a tar archive (executable bits preserved) and stop on the first failed step.

### New machine fields
- `arch` (amd64 / arm64): pin a machine's CPU architecture; the Compose platform, the Vagrant box architecture and the Kubernetes node selector all follow it.
- `privileged`: a container that needs kernel access.
- `read_only`: a read-only root filesystem (hardening).
- `tmpfs` and `shm_size`: memory-backed mounts.
- `dns`: resolver, search domains and domain name.

### Fixes
- Vagrant: pin `box_architecture` only for arm64, not the amd64 default, so older boxes without architecture metadata (the GOAD boxes) stop failing `vagrant up`.
- Generated Terraform modules declare a `local` backend, so the state path is honored.
- WinRM over SSL for Windows boxes that need it.
- Keep Windows machines out of the Linux `/etc/hosts` (AD realm joins resolve through the domain controller).
- VMs get 900s to boot (Windows Server 2025 first boot exceeds 600s).
- `isoloom check` validates before checking; serialization errors are surfaced; the CIDR scan is bounded.
- `import vagrant` runs its recorder from a private per-run temp dir.

### Coverage
- Docker Compose and Vagrant coverage raised through the new machine fields; Vagrant Windows boxes and WinRM marked done.
