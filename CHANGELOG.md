# Changelog

## Unreleased

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
