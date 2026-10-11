# Changelog

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

### A reboot step for Linux VMs
- `reboot` in a Linux machine's `vm.provision` (`[kernel.sh, reboot, app.sh]`) restarts it and goes on with the next step once it's back: a new kernel or a boot option takes effect before the steps that need it. Vagrant restarts the VM itself (the shell provisioner's `reboot: true`). On Proxmox, cloud-init restarts the machine once it has run the steps before it (`power_state`), and a unit runs the rest at the next boot, then writes the ready marker. On the clouds, the set-up splits into one `remote-exec` per part: between two, the instance restarts (new SSH logins closed first, so Terraform reconnects only once it's back), the next part checks the boot changed, brings extra interfaces up again and schedules the auto-stop again; inputs move from `/tmp` to `/var/lib/isoloom` first. `isoloom run external` restarts the machine over SSH and waits for a new boot id. `isoloom import vagrant` reads `reboot: true`. Windows machines are refused with the reason: they restart from the environment's playbooks (`ansible.windows.win_reboot`). Example: existing-hosts' cache turns transparent hugepages off, restarts, then installs Redis, and a check proves the option took. (#62)

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
- `appliance: cisco-vios | cisco-viosl2 | cisco-csr1000v | cisco-c8000v`: vrnetlab's QEMU images, as containerlab runs them: `launch.py` with its arguments (`tc` connection mode), `CLAB_INTFS`, the startup configuration in `/config/startup-config.cfg` (applied once the VM has booted), privileged for /dev/kvm. IOSv's data interfaces are `GigabitEthernet0/1`..., IOS XE's `GigabitEthernet2`.... Example: cisco-qemu (IOSv and CSR1000v, OSPF).
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
