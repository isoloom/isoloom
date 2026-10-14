# Changelog

## Unreleased

### One Ansible inventory, and the external target on the plan
- One inventory renderer (`isoloom_render::inventory`) from the plan, with each target's way of reaching its hosts as an `Access` (Vagrant, Proxmox, a cloud, external). The external target and Proxmox use it; Vagrant and the clouds switch as they move onto the plan. Proxmox's controller inventory now has the spec's `groups:` (it only had the `provision:` steps' groups), so a playbook can target `pair` in ansible-pair there too.
- The external target renders from the plan (`ExternalBackend`): the same inventory, `machines.json` and check runners as before, byte for byte, and its refusals come from its capabilities.
- A machine with `external:` is planned as what it is (a VM, a container) on every target but `external`; it was planned as an existing host everywhere.
- Breaking (library): `isoloom_external::generate` is replaced by `isoloom_external::render(&Environment)` and `ExternalBackend`. `MachinePlan` gains `os`, `winrm`, `external` and `host_vars`, `Environment` gains `inventory_groups`.

### JSON events on every lifecycle command (#146)
- `--json` on `run`, `down`, `reset`, `provision`, `test`, `capture`, `status`, `connect`, `exec`, `tc` and `gc`. The long commands print newline-delimited events: `step` (`target`, `machine`, `step`, `state`: running, done or failed, `detail`, `time`), the tools' output as `log` events, `check` results, and a final `result`. The one-shot commands print one document (`connect --json`: the command that opens the shell, not run). A failed command's last line is `{ "error": { "code", "message", "machine", "step", "hint", "problems" } }` with a stable code. Described in docs/json.md; the JSON Schema is docs/events.schema.json and `isoloom schema --events`. Without `--json` every command prints what it printed before.
- Breaking: `test --json` prints events (a `check` per result, the summary as the `result` event's data) instead of one pretty document.
- Library: `isoloom_core::events` (the events, `ErrorCode`, `Emitter` with a callback or a channel) and `isoloom_core::tool::run_logged`, for tools embedding Isoloom; `host::check_in` and `host::run_limited_in` take extra environment variables.

### No `unsafe`
- A profile's inputs reach every tool through `Command::envs` (and the readiness checks see them) instead of the command line setting its own environment. `unsafe_code` is `forbid` workspace-wide.

### Validation in sections
- The spec's validation is one validator per section (`isoloom-spec/src/validate/`), each function at most 60 lines, with the same problems in the same order. `Problem::new` and `Problem::section`.

### Quality gates
- Workspace lints (clippy pedantic with a short allow-list, no `unsafe`, no `unwrap`, `panic`, `dbg!` or `todo!`, at most 5 parameters), `clippy.toml` shape limits, and new CI jobs: per-crate line coverage floors (cargo-llvm-cov), API docs without warnings, unused dependencies (cargo-machete), `cargo deny check` (advisories, licenses, bans, sources), spelling (typos) and determinism (every example rendered twice). Lints not passing yet are listed in ARCHITECTURE.md, to deny by the end of Wave 2.
- Property tests for IPv4 blocks and the shell, HCL, Ruby and PowerShell quoting; snapshot tests of every example's normalized spec. Ruby strings now escape `#$` and `#@` (interpolations) as well as `#{`; PowerShell strings double the typographic single quotes, which PowerShell also reads as quotes. One `rb` helper in isoloom-render replaces the Vagrant crate's three copies.
- Breaking (library): typed errors with `thiserror`. `LoadError` gains `Yaml`, `Expand { stage, reason }` and `Profile { problems }` in place of `Parse(String)`, and `Read` becomes `Read { path, source }`; `Problem` is an error type; isoloom-import returns `ImportError`; the registry returns `RegistryError`, `report` a `ReportError` and `run_limited` a `HostError`. Messages are unchanged.
- ARCHITECTURE.md, CONTRIBUTING.md and the first ADRs (docs/adr/).

### One crate per layer and backend
- `isoloom-core` is split into `isoloom-spec`, `isoloom-plan`, `isoloom-render`, the backends (`isoloom-compose`, `isoloom-kubernetes`, `isoloom-terraform`, `isoloom-vagrant`, `isoloom-external`), `isoloom-targets`, `isoloom-import`, `isoloom-coverage` and `isoloom-runtime`. No behavior change: every generated file is byte-identical. `isoloom-core` re-exports them at the same paths as before (`isoloom_core::load`, `::generate`, `::resolved`, `::checks`, `::host`, `::registry`...), so code using it compiles unchanged.

### Volumes shared between machines
- A top-level `volumes:` declares volumes several machines mount: a machine volume of that name is the shared one, at the machine's own path. `owner` says which machine holds the data (default: the first that mounts it); the others start after it. On Docker one named volume for all; on Kubernetes one claim, seeded by the owner, its other users scheduled on the owner's node (so a ReadWriteOnce claim works on any storage class); on the VM targets (Vagrant, Proxmox, every cloud) the owner serves its directory over NFS and the others mount it before their steps, at boot too (fstab), waiting for the owner up to 15 minutes. Hybrid refuses a volume shared by a container and a VM, for now.
- Validated: a shared volume is mounted, its owner is one of its machines, each user shares a network with the owner, Linux machines only.
- Example: shared-files (a writer stores a file, the reader finds it at its own path), run on Docker and on two VirtualBox VMs over NFS; CI runs it on Docker and on kind. Compose 92% → 94%: `volumes` done, `volumes_from` covered by shared volumes.

### libvirt runs a machine as its own CPU
- The libvirt provider block sets `machine_arch` from the machine's `arch` (`x86_64`, `aarch64`), as the QEMU provider already did: an arm64 spec no longer runs as the host's CPU on libvirt. Isoloom's own VMs keep the host's. vagrant-libvirt leaves the machine type to libvirt's default for that architecture. libvirt 30% → 35%.

### A machine's environment, kernel parameters and process limits, on every target
- `environment` (fixed values), `sysctls` and `ulimits` (one value, or `{ soft, hard }`) on a machine, or shared through `common:` and groups. A container gets them from its runtime (Compose `environment`, `sysctls`, `ulimits`; on Kubernetes the env and the pod's sysctls, and `ulimits` refused: it has none per container). A VM gets them from one set-up script run as root before its steps, the same on Vagrant, Proxmox and every cloud: `/etc/environment` and systemd's `DefaultEnvironment`, `/etc/sysctl.d`, `limits.d` and systemd's `DefaultLimit*`, then systemd re-executed so the services the steps install start with them. On Windows the environment is set machine-wide.
- Validated: variable names; kernel parameter names; a machine with `docker:` may only set the sysctls a container can (`net.*`, `kernel.shm*`, `kernel.msg*`, `kernel.sem`, `fs.mqueue.*`): leaving `docker:` out makes it VM-only; known limit names, soft not above hard; no sysctls or ulimits on Windows.
- Example: tuned-web (nginx with `APP_MODE`, a 4096-deep accept queue and 8192/16384 open files), whose checks read the three from inside the machine. Run on Docker (CI) and on VirtualBox: a service started by systemd gets the environment and the limits. Compose 88% → 92%.

### The disk size on every Vagrant provider
- `resources.disk_gb` now sizes a VM's disk on Vagrant too, as it already did on Proxmox and the clouds: Vagrant's own `vm.disk` (VirtualBox, VMware, Hyper-V), libvirt's `machine_virtual_size` and ESXi's `guest_boot_disk_size`. It is a minimum: a box whose disk is already bigger keeps it (the providers warn and go on). A `disk` step then grows the root filesystem into the space, before the machine's steps: the root partition when it is the disk's last, LVM's volumes, ext4 or XFS; `Resize-Partition` on Windows. Run on VirtualBox: an Ubuntu 24.04 box's root went from 31 to 77 GB.
- QEMU is left out on purpose: its plugin makes the disk an overlay of that size, and one below the box's truncates it.
- The settings that add disks (`disks`, libvirt `storage`, ESXi `guest_storage`) are covered another way: a machine's one disk, sized, holds its volumes' paths. Vagrant 83% → 88%, libvirt 15% → 30%, ESXi 47% → 57%.

### A Helm chart beside the Kubernetes manifests
- The Kubernetes output gains `.isoloom/kubernetes/chart/`: the same manifests as a Helm chart, for customers who install with Helm and for chart registries. The namespace is the release's; each machine's image is a value (`images.<machine>`, pulled through the constraints' mirrors when there are some; a built image is named there once pushed); the inputs are a Secret from `inputs.<NAME>`; the checks are `helm test` Pods (the `exec` ones, run inside a machine, stay with `isoloom test kubernetes`); `NOTES.txt` carries the message (as written, or a pointer to `isoloom message` when it has placeholders). Anything in the manifests that Helm would read as a template is printed as it is.
- The chart works away from the project: `isoloom generate`, `run` and `check` copy the scripts its pods run (init jobs, check scripts) into `chart/files/project/`, so `check` fails when a copy is stale. A script the project doesn't have yet is left to `validate`, and the chart's `required` names it at install. Library: `project_copies(files, dir)`.
- `isoloom chart [DIR] --version 4.2.1 [-o DIR]` writes the chart and packages it with `helm package` as `<name>-4.2.1.tgz` (version and app version), the artifact to hand over or push.
- CI lints the chart, installs it on kind beside the kustomize copy, runs `helm test` and packages it, for hello-stack and segmented. (#134)

### Constraints: a customer's proxy, mirrors and time servers, on every target
- `constraints:` at the top of the spec (usually set by a profile's `spec:`) says what the network an environment runs in imposes. `registries` maps a registry to its mirror (`docker.io: registry.corp/dockerhub` pulls `redis:7` as `registry.corp/dockerhub/library/redis:7`): every image of the Compose file and the Kubernetes manifests goes through it, Isoloom's own router and check images included, and so do the `FROM` lines of the Dockerfiles Isoloom writes; images built from the project are local tags and stay as they are. `proxy` (`http`, `https`, `no_proxy`) reaches the machines' containers as environment variables and their builds as Docker's predefined build arguments, never Isoloom's checks, with the environment's own machine names and network blocks (and localhost) always reached directly. `mirrors` (`debian`, `ubuntu`, `pypi`, `npm`) and `ntp`.
- Linux VMs run a preamble as root before anything else: the proxy for every shell, systemd's services, sudo, apt, dnf/yum and Docker's daemon; Debian's and Ubuntu's apt sources rewritten to the mirrors (`debian-security` left alone); `/etc/pip.conf` and `/etc/npmrc`; chrony or systemd-timesyncd on the time servers. It is idempotent, and runs as each VM's first Vagrant provisioner (machines, router, controller, the `shell` tool, the Docker hosts of docker-vm and hybrid), as `bootcmd` on Proxmox (before cloud-init installs packages), and first after cloud-init on every cloud (cloud-vm and cloud-docker). Windows machines aren't covered yet.
- Validation names the field: a proxy or mirror that isn't a URL, a registry key that isn't a host, a mirror given with a scheme. Without `constraints:`, every generated file is unchanged. (#133)

### Profiles: one spec, one file per customer or site
- `--profile <name>` on every command (or `ISOLOOM_PROFILE`) reads `profiles/<name>.yml` in the project: what differs for one customer or site, instead of a fork of the spec or a list of `-s` kept elsewhere. `target:` pins the target (the default when none is given, and the only one allowed); `spec:` is merged into the spec before `-s`; `defaults:` is a layer of the defaults between the project's file and `--images`, so `isoloom defaults` names it as the source; `inputs:` gives the spec's launch-time inputs their values, an input already in the environment winning. A profile names only what the spec has: an unknown machine, network or input is an error, so a renamed machine can't leave a profile silently doing nothing. `isoloom profiles` lists them and checks each (the names it uses, the spec it makes, that its target can run it), failing when one doesn't: one CI step for every site. `run` records the profile in `~/.isoloom/status.yml` (`profile`), and `status` shows it. Library: `profile` (load, list, check, pin), `load_profiled` and `defaults::load_profiled`. (#132)

### Library: a stability policy
- The README ("Using the library") and the crate docs say what `isoloom-core` supports: the crate root and the modules `model`, `validate`, `targets`, `generate` (with `resolved`), `checks`, `instance`, `defaults`, `registry` and `host`. The other public modules are internal (public for the command line). While at 0.x, a breaking change to the supported surface bumps the minor version (a new public field or enum variant isn't one) and gets a "Breaking (library)" line here; release tags are the pinning points. CI runs `cargo semver-checks` on every pull request, against its base and the last tag, and reports what changed in the job summary (a warning, not a failure). (#108)
- `~/.isoloom/status.yml` starts with `version: 1` (`registry::Registry::version`, `registry::VERSION`), written on every save; a file without it reads as version 1, and unknown fields are still ignored. Build a `Registry` with `Registry::default()`. (#108)
- Breaking (library): `checks::sq`, `vlans::vlan_network`, `groups::RESERVED`, `registry::rfc3339` and `defaults::at_path` are no longer public. (#108)

### One "can this run here" question
- `refusal_on(spec, target, provider)`: `refusal` for a target run with a given provider (the name its tool knows it by, as `defaults.yml` sets it). Today only `vagrant` on `qemu` refuses more (without root, QEMU links two VMs per network, each VM on one); `None` is `refusal`. `isoloom run vagrant` asks it for `vagrant.provider` from the defaults and refuses up front instead of failing in `vagrant up`. `qemu_refusal` is deprecated (a thin wrapper, same answers): use `refusal_on(spec, Target::Vagrant, Some("qemu"))`, which also says why Vagrant can't run the spec at all. (#107)
- `cloud.hourly_usd` is documented for what it is: the author's estimate of the hourly cost, for whoever runs the environment. `isoloom run cloud-services` says it before applying, and the resolved snapshot carries it. The registry's `project` (docs/registry.md) is documented for anyone who brings a Compose file up under another project name; `isoloom run` never sets it. (#107)

### Kubernetes volumes start with the image's files
- A volume mounted over a path the image has (`volumes: { ctf: /chroot }`) started empty on Kubernetes, hiding the image's files there, while Docker seeds a named volume with them: the same spec ran on Docker, hosted and docker-vm and broke on Kubernetes. Each claim is now seeded from the image the same way: an init container of the machine's image (before the machine and its `init:` jobs) copies the image's files at the path into the claim (`cp -a`, as root, owners and modes kept) while the claim is empty (a `lost+found` aside); a claim already holding data is left alone, and a path the image doesn't have leaves it empty. A copy that fails (a claim too small for the image's files) fails the pod, and the next try clears what it left (a `.isoloom-seeding` marker stands in the claim while copying) and copies again. Volume names are now at most 58 characters (the init container is `seed-<volume>`). It runs Isoloom's static busybox, so images without a shell or `cp` (distroless, `scratch`) are seeded too. hello-stack and segmented's `/data` claims get theirs. (#102)

### UDP services
- `protocol: udp` on a service (DNS, SNMP, TFTP, IKE, syslog labs): a port served over both protocols is two services, one per protocol. Compose publishes it as `<port>/udp` (and labels it `udp:<port>`); Vagrant (vagrant, docker-vm, hybrid) forwards it with `protocol: "udp"`; Kubernetes declares `protocol: UDP` on the container port, the Service port (`p<port>-udp`) and the published LoadBalancer and its NetworkPolicy; Proxmox's router forwards a published port as UDP; the clouds open a published UDP port as UDP (AWS, Azure, Google Cloud, DigitalOcean, Linode, Oracle; cloud-vm and cloud-docker), redirect it as UDP when the published port differs, and open the Windows firewall for it as UDP. UDP has no handshake, so nothing waits for it: it stays out of the healthcheck, the readiness probe, the init-job and dependency waits, and of the derived checks (answering or blocked alike); a check of your own says how (`script: checks/resolve.sh` with `nslookup`). `depends_on` a machine whose services are all UDP is refused. `reach` ports already opened TCP and UDP on the routers, Kubernetes and Linux cloud VMs; Windows cloud VMs (AWS, Azure) now open UDP too. Validated: no `http` or `tls` on UDP, a port listed twice per protocol, a host port published once per protocol. A Windows cloud VM can't publish a UDP service on another port (its port proxy is TCP only): refused with the reason. `isoloom import compose` reads `53:53/udp` and `protocol: udp`, `import vagrant` a forwarded port's `protocol: "udp"`. The snapshot says `protocol: udp`; `isoloom report services` and `graph` show `53/udp`; the message's published ports are read per protocol. Outputs name a UDP service `<name>-udp` when a TCP service of the machine has the same name (DNS's `dns` over both), so published outputs and labels keep both. vagrant-libvirt skips UDP forwarded ports: the Vagrantfile says so next to each, and `isoloom run` warns when the machines run on libvirt. `import compose` notes a port of another protocol (`/sctp`), drafted as TCP. Example: udp-dns (dnsmasq answering across a router that lets only port 53 through), run in CI on Docker. (#85)

### `http` checks follow redirects
- `follow: true` on an `http` check follows redirects, up to 5 hops, and compares the final answer's status (default 200) and body; a POST answered with a 301, 302 or 303 goes on as a GET, as in a browser (curl gets the form data, not `-X POST`, which would POST to every hop), other methods are sent again to each hop: a page that redirects (a login page, a PHP app's index) no longer needs `expect: any` plus a script. curl gets `-L --max-redirs 5`; wget, which follows redirects whatever it's told (busybox's can't be told otherwise), has each answer's status read, the last of at most 6 compared. Without `follow`, the first answer counts on every runner: a wget-only runner (the busybox check image) compared the last one, so a check that passed there only past a redirect now says `got 301` as curl did. bash's fallback (neither curl nor wget) doesn't follow. Validated: `follow` goes with `http`, not with `expect: blocked`. The snapshot says `follow: true`. Example: hello-stack's `/hello` answers 301, and 200 past the redirect. (#86)

### `isoloom provision`
- `isoloom provision [machine...] [--target t]`: the environment's provisioning again, on a running environment, in place, for a step that failed or was interrupted (a host that stopped answering) without an hour of rebuild. With `provision:` steps, the controller (booted if halted, halted again after) gets the project's current files and runs the playbooks again, limited to the named machines (`--limit`); without, the named machines' own `vm.provision` steps run again (`vagrant provision`). Steps are meant to be idempotent. Local VMs (vagrant, docker-vm, hybrid) for now.

### Windows steps can use the project's files
- A Windows machine's `.ps1` steps were uploaded one by one, so a step couldn't reach the project's other files (vendored tools, scripts). The project now goes to `C:\isoloom` once, before the steps, and each step runs from there (`-File C:\isoloom\<step>`, in that folder), as Linux steps run from /opt/isoloom. Vagrant: the `project` step uploads its archive over WinRM; cloud-vm (AWS, Azure): the host's archive, as a Linux machine's, with the set-up script now in `C:\ProgramData\isoloom`. The archive is unpacked by PowerShell over .NET's GZipStream (PowerShell 2.0 and later; Windows Server 2016 and older have no tar.exe); symbolic links are left out, and a name Windows can't take is skipped with a warning. `.isoloomignore` keeps the copy small. (#59)

### The controller halts after provisioning
- On local VMs (Vagrant, and the Docker-on-a-VM and hybrid outputs), the controller halts once its provisioning is done, keeping its disk: it no longer holds memory for the life of the environment. `isoloom test` boots it for the checks (waiting out a halt that's just starting) and halts it again. `controller: { keep_running: true }` keeps it up, as a jump host. Proxmox and the clouds keep it running for now.
- Roles: in the resolved snapshot, each machine has `role: target` and the controller `role: infra` (with `keep_running`). `isoloom status` counts the spec's machines only on Vagrant outputs: a halted controller (or Isoloom's router and tools) never reads as a machine down.

### `.isoloomignore`
- An `.isoloomignore` at the project's root lists what the machines don't get when the project is copied into them, gitignore-style: `#` comments, `!` brings a path back, a trailing `/` matches folders only, a `/` at the start or in the middle anchors a pattern at the root (otherwise it matches at any depth), `*`, `?`, `[abc]`, `**`; the last pattern matching a path, or a folder it is in, decides. Every copy honors it, read when the copy is made: Vagrant's `project` step (vagrant, hybrid, docker-vm), Proxmox's cloud-init, the cloud modules' archive (cloud-vm, cloud-docker, docker-vm on Proxmox: `tar -X` with the list Terraform computes, the archive's top folder stripped when unpacked) and `isoloom run external`. Each reads a pattern as the same regular expression. Proxmox also leaves out `.vagrant` and `.terraform` folders at any depth now. (#61)

### A lighter controller, sized by the spec
- The controller that runs `provision:` playbooks defaults to 1 CPU and 512 MB on every target (it was 1 GB). On local VMs its box is `generic/alpine319` (about 100 MB to download instead of 675 MB, for VirtualBox, libvirt, VMware and Parallels alike): Python, ansible-core and pywinrm install in seconds. Proxmox and the clouds keep their Debian images, at the new size (AWS `t3.micro`, Azure `Standard_B1s`, GCP `e2-small`).
- `controller:` (top level) sets it like a machine: `image: { vagrant, vagrant_version }` for a box of your own (its setup handles Alpine and Debian), `resources: { cpus, memory_mb, disk_gb }` (disk on Proxmox) for many forks against a large environment. Example: ansible-pair gives it 768 MB.

### The project goes into VMs as an archive
- The Vagrant outputs (vagrant, hybrid, docker-vm) copied the project with one `file` provisioner per top-level entry, listed when the Vagrantfile loaded: the copy followed symbolic links, so a vendored tree with a link loop failed it (`Too many levels of symbolic links`) and Vagrant destroyed the VM, and an entry deleted during `vagrant up` failed it too. A VM's `project` step is now Isoloom's own provisioner (`isoloom_project`, defined in the Vagrantfile): a tar.gz of the project as it is when the step runs, written by Vagrant's own Ruby (no `tar` needed on the host, Windows included), symbolic links kept as links, unpacked in /opt/isoloom. `.git`, `.vagrant` and `.terraform` stay out at any depth (a `.vagrant` holds private keys). (#67, #63)

### A reboot step for Linux VMs
- `reboot` in a Linux machine's `vm.provision` (`[kernel.sh, reboot, app.sh]`) restarts it and goes on with the next step once it's back: a new kernel or a boot option takes effect before the steps that need it. Vagrant restarts the VM itself (the shell provisioner's `reboot: true`). On Proxmox, cloud-init restarts the machine once it has run the steps before it (`power_state`), and a unit runs the rest at the next boot, then writes the ready marker. On the clouds, the set-up splits into one `remote-exec` per part: between two, the instance restarts (new SSH logins closed first, so Terraform reconnects only once it's back), the next part checks the boot changed, brings extra interfaces up again and schedules the auto-stop again; inputs move from `/tmp` to `/var/lib/isoloom` first. `isoloom run external` restarts the machine over SSH and waits for a new boot id. `isoloom import vagrant` reads `reboot: true`. Windows machines are refused with the reason: they restart from the environment's playbooks (`ansible.windows.win_reboot`). Example: existing-hosts' cache turns transparent hugepages off, restarts, then installs Redis, and a check proves the option took. (#62)

### Dormant services: a port that answers only later
- `dormant: true` on a service: a port that only answers once something wakes it (the bind shell an exploit opens). It is declared, published if asked and opened by `reach` like any service, but nothing waits for it: left out of the Docker healthcheck (so `depends_on` and `up --wait` don't wait for it), the Kubernetes readiness probe and init-job wait, and the dependency waits on the VM targets (Vagrant, Proxmox, the clouds); no derived check expects it to answer (the `blocked` ones still apply). A machine whose services are all dormant has no healthcheck; `depends_on` such a machine is refused (nothing to wait for). The snapshot says `dormant: true`. segmented's web server has a dormant 4444. (#69)

### Derived checks in Windows-only environments
- Derived checks run from Linux machines, so an environment of Windows machines (or network appliances) had none: its `services` went unchecked. When no machine can run them, every service is now checked at each of its addresses from the environment's networks: the controller on Vagrant, AWS and Azure (created for it when nothing else needed one), the check runner on every network on Docker and Kubernetes. Nothing stands where `reach` blocks a service or a network is offline, so those aren't asserted. On hybrid, where the containers' derived checks don't run (the checks run from the VM side), the controller does the same when no Linux VM can: mixed-office's hybrid output now checks files01 and the intranet. With a Linux machine to run them, nothing changes. Example: windows-hello. (#60)

### Per-machine inventory variables
- `provision[].host_vars`: a machine's name -> its variables, written on its line of the inventory Isoloom generates (local VMs, Proxmox, cloud VMs, external machines), for playbooks that key on per-host values (its domain, a role flag). Values keep their types: each is written as a Python literal, which Ansible's INI inventory reads back (a quoted `"10"` stays a string; lists, maps, booleans and `null` as they are). Validation: every key names a machine, every variable is a valid name. With `groups:` and `vars:`, a spec can now hold a whole Ansible inventory without an inventory file. Example: ansible-pair's cache gets its `maxmemory`.

### `tcp` checks read a greeting
- `tcp` checks take `contains` (text the bytes read in the first 5 seconds of the connection must contain: a banner, a prompt) and `send` (a line written first, a newline added; end it with `\r` for CRLF), retried for `wait` like any check: proving the right service answers no longer needs a `(sleep 4) | curl telnet://...` script. Rendered with `nc` (busybox in the runner image, the VM's own nc whatever its flavor), else bash's `/dev/tcp`; stdin stays open while reading and nc is stopped after 5 seconds, so nc flavors that quit at the end of stdin or never quit behave the same. hello-stack sends the cache a `PING` and expects `+PONG`. (#73)

### Typed playbook variables
- `provision[].vars` takes any YAML value: strings as before, and numbers, booleans, lists and maps, passed to `ansible-playbook` as JSON (`-e`) with their types (a list stays a list, not a string to iterate over). `isoloom run`'s own playbook runs pass them as JSON too (they split values on spaces before). Example: ansible-pair's `tools: [curl, jq]` and `cache_port: 6379`.

### `import compose` reads keys set twice
- A key set twice in one mapping (secDevLabs camplake-api's `environment` sets `MONGO_PORT` twice) no longer stops `isoloom import compose` with `duplicate entry`: the last value is kept, as Compose does, and the draft's "Changed on the way in" notes say which key, where, and the value kept. (#68)

### A Cisco switch for a LAN
- `switch: <machine>` on a LAN split into `vlans`: a switch appliance of the environment (`docker.appliance: cisco-iol-l2` or `cisco-vios-l2`) switches the LAN instead of Isoloom's Linux switch container, so frames between a VLAN and a trunk cross real IOS. The machine lists no `networks`: its data ports are an access port on each VLAN's network (in VLAN order, at the controller address, unused on Docker), then a trunk port on each trunk machine's link. Isoloom's startup configuration declares the VLANs (`vtp mode transparent`, `vlan <id>` named after the network) and sets each port (`switchport mode access` + `access vlan`, or `trunk encapsulation dot1q` + `mode trunk` + `allowed vlan`); the machine's own `config` follows. Trunk machines are unchanged; their sidecar waits for the switch to start (IOS reports no readiness: checks through it `wait` for it to boot). Validated: a switch kind, no `networks`, `services` or `aliases` of its own, one LAN per switch, no `count`, no machine on the LAN directly. Docker targets only, like every appliance. Example: cisco-switch.

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
