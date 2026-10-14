# Coverage

Everything each format can do, and whether Isoloom produces it from a spec.

**What belongs in the format:** a machine feature the VM targets can do (local VMs, Proxmox,
cloud VMs). Containers are the lightweight option, used when they can produce the same machine:
a feature they can't makes an environment VM-only (like Windows) instead of staying out.
Container mechanics (capabilities, cgroups) aren't machine features: Isoloom sets them itself.
**100% coverage** means every portable feature is produced.

| Format | Coverage | Portable features | Done | Partly | To do |
| --- | ---: | ---: | ---: | ---: | ---: |
| [Docker Compose](#docker-compose) | 96% | 151 | 143 | 4 | 4 |
| [Vagrant](#vagrant) | 100% | 42 | 42 | 0 | 0 |
| [Vagrant: networks](#vagrant-networks) | 100% | 38 | 38 | 0 | 0 |
| [Vagrant: disks and synced folders](#vagrant-disks-and-synced-folders) | 100% | 12 | 12 | 0 | 0 |
| [Vagrant: provisioners](#vagrant-provisioners) | 100% | 57 | 57 | 0 | 0 |
| [Vagrant: communicators](#vagrant-communicators) | 100% | 11 | 11 | 0 | 0 |
| [Vagrant: VirtualBox](#vagrant-virtualbox) | 100% | 7 | 7 | 0 | 0 |
| [Vagrant: VMware Desktop](#vagrant-vmware-desktop) | 100% | 5 | 5 | 0 | 0 |
| [Vagrant: Parallels](#vagrant-parallels) | 100% | 7 | 7 | 0 | 0 |
| [Vagrant: libvirt](#vagrant-libvirt) | 100% | 19 | 19 | 0 | 0 |
| [Vagrant: Hyper-V](#vagrant-hyper-v) | 100% | 6 | 6 | 0 | 0 |
| [Vagrant: UTM](#vagrant-utm) | 100% | 3 | 3 | 0 | 0 |
| [Vagrant: QEMU](#vagrant-qemu) | 92% | 14 | 13 | 0 | 1 |
| [Vagrant: ESXi](#vagrant-esxi) | 100% | 20 | 20 | 0 | 0 |
| [Vagrant: Docker](#vagrant-docker) | 100% | 13 | 13 | 0 | 0 |
| [Terraform: Proxmox](#terraform-proxmox) | 76% | 119 | 91 | 0 | 28 |
| [Terraform: ESXi](#terraform-esxi) | 0% | 4 | 0 | 0 | 4 |
| [Terraform: AWS](#terraform-aws) | 73% | 132 | 97 | 0 | 35 |
| [Terraform: Azure](#terraform-azure) | 80% | 135 | 108 | 0 | 27 |
| [Terraform: Google Cloud](#terraform-google-cloud) | 72% | 97 | 69 | 2 | 26 |
| [Terraform: DigitalOcean](#terraform-digitalocean) | 77% | 36 | 28 | 0 | 8 |
| [Terraform: Linode](#terraform-linode) | 66% | 59 | 39 | 0 | 20 |
| [Terraform: Oracle Cloud](#terraform-oracle-cloud) | 50% | 152 | 77 | 0 | 75 |
| [Terraform: utility providers](#terraform-utility-providers) | 100% | 16 | 16 | 0 | 0 |
| [cloud-init](#cloud-init) | 96% | 80 | 77 | 0 | 3 |
| [Ansible inventory](#ansible-inventory) | 100% | 31 | 31 | 0 | 0 |
| [Dockerfile](#dockerfile) | 100% | 24 | 24 | 0 | 0 |
| [Kubernetes](#kubernetes) | 97% | 204 | 199 | 1 | 4 |
| [Kustomize](#kustomize) | 100% | 15 | 15 | 0 | 0 |
| [Helm](#helm) | 100% | 19 | 19 | 0 | 0 |

Done includes features produced another way, or set in the image. The lists come from the
formats themselves (every key of the Compose schema, every setting of Vagrant and each provider
plugin, every resource type of each Terraform provider); tests fail on an unclassified entry, when
an example's generated file (Compose, Vagrantfile, Terraform, Kubernetes, Kustomize, Helm) writes a key or resource type its
table doesn't mark as written, or when one marked as written appears in no example's output.
Only what the examples generate is checked: a feature no example uses can still be misclassified.

## Docker Compose

96% of 151 portable features (143 done, 4 partly, 4 to do; 347 features in all). From the compose-spec schema, commit 914ec15d1fa4 (crates/isoloom-coverage/coverage/compose-spec.json).

### Top level

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `version` | n/a | No | Obsolete in Compose |
| `name` | Yes | Yes | From `name` |
| `include` | n/a | No | Compose tooling, not the environment's behavior |
| `include[].path` | n/a | No | Compose tooling, not the environment's behavior |
| `include[].env_file` | n/a | No | Compose tooling, not the environment's behavior |
| `include[].project_directory` | n/a | No | Compose tooling, not the environment's behavior |
| `services` | Yes | Yes | From `machines` |
| `models` | No | No | Served by Docker's model runner: VMs and the cloud have none |
| `networks` | Yes | Yes | From `networks` |
| `volumes` | Yes | Yes | From `machines.*.volumes` |
| `secrets` | Yes | Yes | From `secrets` |
| `configs` | Yes | Yes | From `machines.*.files` |
| `jobs` | Yes | Another way | Via docker.init (one-shot jobs) |

### Services

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `annotations` | n/a | No | Compose tooling, not the environment's behavior |
| `attach` | n/a | No | Compose tooling, not the environment's behavior |
| `blkio_config` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_read_bps` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_read_bps[].path` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_read_bps[].rate` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_read_iops` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_read_iops[].path` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_read_iops[].rate` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_write_bps` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_write_bps[].path` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_write_bps[].rate` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_write_iops` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_write_iops[].path` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.device_write_iops[].rate` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.weight` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.weight_device` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.weight_device[].path` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `blkio_config.weight_device[].weight` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `build` | Yes | Yes | From `machines.*.docker.build` |
| `build.context` | Yes | Yes | From `machines.*.docker.build` |
| `build.dockerfile` | Yes | Yes | From `machines.*.docker.dockerfile` |
| `build.dockerfile_inline` | Yes | Another way | Via machines.*.docker.dockerfile (a file beside the spec) |
| `build.entitlements` | n/a | No | How the image is built, not the machine it produces |
| `build.args` | Yes | Yes | From `machines.*.docker.args` |
| `build.ssh` | n/a | No | How the image is built, not the machine it produces |
| `build.labels` | Yes | Yes | The environment's labels: isoloom.managed and isoloom.environment, for `isoloom gc` |
| `build.cache_from` | n/a | No | How the image is built, not the machine it produces |
| `build.cache_to` | n/a | No | How the image is built, not the machine it produces |
| `build.no_cache` | n/a | No | How the image is built, not the machine it produces |
| `build.no_cache_filter` | n/a | No | How the image is built, not the machine it produces |
| `build.additional_contexts` | n/a | No | How the image is built, not the machine it produces |
| `build.network` | n/a | No | How the image is built, not the machine it produces |
| `build.provenance` | n/a | No | How the image is built, not the machine it produces |
| `build.sbom` | n/a | No | How the image is built, not the machine it produces |
| `build.pull` | n/a | No | How the image is built, not the machine it produces |
| `build.target` | Yes | Yes | From `machines.*.docker.target` (a stage of the Dockerfile) |
| `build.shm_size` | n/a | No | How the image is built, not the machine it produces |
| `build.extra_hosts` | n/a | No | How the image is built, not the machine it produces |
| `build.isolation` | n/a | No | How the image is built, not the machine it produces |
| `build.privileged` | n/a | No | How the image is built, not the machine it produces |
| `build.secrets` | n/a | No | How the image is built, not the machine it produces |
| `build.secrets[].source` | n/a | No | How the image is built, not the machine it produces |
| `build.secrets[].target` | n/a | No | How the image is built, not the machine it produces |
| `build.secrets[].uid` | n/a | No | How the image is built, not the machine it produces |
| `build.secrets[].gid` | n/a | No | How the image is built, not the machine it produces |
| `build.secrets[].mode` | n/a | No | How the image is built, not the machine it produces |
| `build.tags` | n/a | No | How the image is built, not the machine it produces |
| `build.ulimits` | n/a | No | How the image is built, not the machine it produces |
| `build.ulimits.*.hard` | n/a | No | How the image is built, not the machine it produces |
| `build.ulimits.*.soft` | n/a | No | How the image is built, not the machine it produces |
| `build.platforms` | Yes | Another way | Via platform (from machines.*.arch) |
| `cap_add` | Yes | Yes | From `networks.*.gateway` (capabilities only exist for containers: root on a VM has them all) |
| `cap_drop` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `cgroup` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `cgroup_parent` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `command` | Yes | Yes | From `machines.*.command` (and Isoloom's tools: tools.*.command) |
| `configs` | Yes | Yes | From `machines.*.files` |
| `configs[].source` | Yes | Yes | From `machines.*.files` |
| `configs[].target` | Yes | Yes | From `machines.*.files[].path` |
| `configs[].uid` | Yes | Partly | From `machines.*.files[].uid, gid and mode` (a file's `content`); not yet: a project file (`source`) is mounted as it is: an owner or mode for it is refused |
| `configs[].gid` | Yes | Partly | From `machines.*.files[].uid, gid and mode` (a file's `content`); not yet: a project file (`source`) is mounted as it is: an owner or mode for it is refused |
| `configs[].mode` | Yes | Partly | From `machines.*.files[].uid, gid and mode` (a file's `content`); not yet: a project file (`source`) is mounted as it is: an owner or mode for it is refused |
| `container_name` | Yes | Another way | Via the machine's name |
| `cpu_count` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `cpu_percent` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `cpu_period` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `cpu_quota` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `cpu_rt_period` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `cpu_rt_runtime` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `cpu_shares` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `cpus` | Yes | Another way | Via machines.*.resources.cpus, written as deploy.resources.limits |
| `cpuset` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `credential_spec` | No | No | Windows containers only |
| `credential_spec.config` | No | No | Windows containers only |
| `credential_spec.file` | No | No | Windows containers only |
| `credential_spec.registry` | No | No | Windows containers only |
| `depends_on` | Yes | Yes | From `machines.*.depends_on` |
| `depends_on.*.restart` | n/a | No | Compose's restart of dependents on `docker compose restart` |
| `depends_on.*.required` | n/a | No | Lets Compose start without a dependency: Isoloom's are always required |
| `depends_on.*.condition` | Yes | Yes | From `machines.*.depends_on` (healthy, or an init job completed) |
| `deploy` | Yes | Yes | From `machines.*.resources` (replicas: see `scale`) |
| `deploy.mode` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.endpoint_mode` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.replicas` | Yes | Another way | Via machines.*.count (identical machines, each with its own name and address) |
| `deploy.labels` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.rollback_config` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.rollback_config.parallelism` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.rollback_config.delay` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.rollback_config.failure_action` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.rollback_config.monitor` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.rollback_config.max_failure_ratio` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.rollback_config.order` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.update_config` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.update_config.parallelism` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.update_config.delay` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.update_config.failure_action` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.update_config.monitor` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.update_config.max_failure_ratio` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.update_config.order` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.resources` | Yes | Yes | From `machines.*.resources` |
| `deploy.resources.limits` | Yes | Yes | From `machines.*.resources` |
| `deploy.resources.limits.cpus` | Yes | Yes | From `machines.*.resources.cpus` |
| `deploy.resources.limits.memory` | Yes | Yes | From `machines.*.resources.memory_mb` |
| `deploy.resources.limits.pids` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `deploy.resources.reservations` | Yes | Partly | From `machines.*.gpus` (its devices); not yet: cpus and memory: a machine's size is its limit |
| `deploy.resources.reservations.cpus` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `deploy.resources.reservations.memory` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `deploy.resources.reservations.generic_resources` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.resources.reservations.generic_resources[].discrete_resource_spec` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.resources.reservations.generic_resources[].discrete_resource_spec.kind` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.resources.reservations.generic_resources[].discrete_resource_spec.value` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.resources.reservations.devices` | Yes | Yes | From `machines.*.gpus` |
| `deploy.resources.reservations.devices[].capabilities` | Yes | Yes | From `machines.*.gpus` |
| `deploy.resources.reservations.devices[].count` | Yes | Yes | From `machines.*.gpus` |
| `deploy.resources.reservations.devices[].device_ids` | Yes | Yes | From `machines.*.gpus` |
| `deploy.resources.reservations.devices[].driver` | Yes | Yes | From `machines.*.gpus` |
| `deploy.resources.reservations.devices[].options` | Yes | Yes | From `machines.*.gpus` |
| `deploy.restart_policy` | Yes | Another way | Via restart (always unless-stopped: machines stay up like VMs) |
| `deploy.restart_policy.condition` | Yes | Another way | Via restart (always unless-stopped: machines stay up like VMs) |
| `deploy.restart_policy.delay` | Yes | Another way | Via restart (always unless-stopped: machines stay up like VMs) |
| `deploy.restart_policy.max_attempts` | Yes | Another way | Via restart (always unless-stopped: machines stay up like VMs) |
| `deploy.restart_policy.window` | Yes | Another way | Via restart (always unless-stopped: machines stay up like VMs) |
| `deploy.placement` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.placement.constraints` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.placement.preferences` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.placement.preferences[].spread` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.placement.max_replicas_per_node` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `develop` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].ignore` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].include` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].path` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].action` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].target` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].exec` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].exec.command` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].exec.user` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].exec.privileged` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].exec.working_dir` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].exec.environment` | n/a | No | Compose tooling, not the environment's behavior |
| `develop.watch[].initial_sync` | n/a | No | Compose tooling, not the environment's behavior |
| `device_cgroup_rules` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `devices` | Yes | Yes | From `machines.*.devices` |
| `devices[].source` | Yes | Yes | From `machines.*.devices` |
| `devices[].target` | Yes | Yes | From `machines.*.devices` |
| `devices[].permissions` | Yes | Yes | From `machines.*.devices` |
| `dns` | Yes | Yes | From `machines.*.dns.servers` (and the Kubernetes dnsConfig) |
| `dns_opt` | Yes | Yes | From `machines.*.dns.options` (and the Kubernetes dnsConfig) |
| `dns_search` | Yes | Yes | From `machines.*.dns.search` (and the Kubernetes dnsConfig) |
| `domainname` | Yes | Yes | From `machines.*.dns.domain` |
| `entrypoint` | Yes | Yes | From `machines.*.before` (a shell running the steps, then the command; Isoloom's own containers and init jobs) |
| `env_file` | Yes | Another way | Via inputs |
| `env_file[].path` | Yes | Another way | Via inputs |
| `env_file[].format` | Yes | Another way | Via inputs |
| `env_file[].required` | Yes | Another way | Via inputs |
| `environment` | Yes | Yes | From `machines.*.environment and machines.*.inputs` |
| `expose` | Yes | Another way | Via machines.*.services (every port is reachable on the machine's networks) |
| `extends` | n/a | No | Compose tooling, not the environment's behavior |
| `extends.service` | n/a | No | Compose tooling, not the environment's behavior |
| `extends.file` | n/a | No | Compose tooling, not the environment's behavior |
| `external_links` | n/a | No | Legacy; replaced by networks |
| `extra_hosts` | Yes | Yes | From `machines.*.networks` (names of machines on other networks) |
| `gpus` | Yes | Another way | Via machines.*.gpus, written as `deploy.resources.reservations.devices` (the same request, which every Compose version reads) |
| `gpus[].capabilities` | Yes | Another way | Via machines.*.gpus, written as `deploy.resources.reservations.devices` (the same request, which every Compose version reads) |
| `gpus[].count` | Yes | Another way | Via machines.*.gpus, written as `deploy.resources.reservations.devices` (the same request, which every Compose version reads) |
| `gpus[].device_ids` | Yes | Another way | Via machines.*.gpus, written as `deploy.resources.reservations.devices` (the same request, which every Compose version reads) |
| `gpus[].driver` | Yes | Another way | Via machines.*.gpus, written as `deploy.resources.reservations.devices` (the same request, which every Compose version reads) |
| `gpus[].options` | Yes | Another way | Via machines.*.gpus, written as `deploy.resources.reservations.devices` (the same request, which every Compose version reads) |
| `group_add` | Yes | In the image | Set it in the image (`docker.build`) or the VM's provisioning |
| `healthcheck` | Yes | Yes | From `machines.*.services and machines.*.healthcheck` (a probe of every port, and the machine's own check beside it) |
| `healthcheck.disable` | n/a | No | Turns off the image's own check: Isoloom's readiness is its own probe |
| `healthcheck.interval` | Yes | Yes | From `machines.*.healthcheck.interval` (else Isoloom's probe pace) |
| `healthcheck.retries` | Yes | Yes | From `machines.*.healthcheck.retries` (else Isoloom's probe pace) |
| `healthcheck.test` | Yes | Yes | From `machines.*.healthcheck.test and machines.*.healthcheck.http` (after the probe of every port) |
| `healthcheck.timeout` | Yes | Yes | From `machines.*.healthcheck.timeout` (else Isoloom's probe pace) |
| `healthcheck.start_period` | Yes | Yes | From `machines.*.healthcheck.start_period` (else a grace period for slow starters: appliances and built images) |
| `healthcheck.start_interval` | Yes | Another way | Via interval (Isoloom probes at one pace) |
| `hostname` | Yes | Yes | From the machine's name |
| `image` | Yes | Yes | From `machines.*.docker.image` |
| `init` | n/a | No | Container mechanics: a VM always runs its own init |
| `ipc` | No | No | Shares a kernel namespace with the host or another machine: separate VMs can't |
| `isolation` | No | No | Windows containers only |
| `label_file` | n/a | No | Compose tooling, not the environment's behavior |
| `labels` | Yes | Yes | From `machines.*.services` (isoloom.service.<name>, for tools reading the containers); isoloom.managed and isoloom.environment on every service, for `isoloom gc` |
| `links` | n/a | No | Legacy; replaced by networks |
| `logging` | n/a | No | Where the runner collects logs |
| `logging.driver` | n/a | No | Where the runner collects logs |
| `logging.options` | n/a | No | Where the runner collects logs |
| `mac_address` | No | No | Cloud VMs get their MAC address from the provider |
| `mem_limit` | Yes | Another way | Via machines.*.resources.memory_mb, written as deploy.resources.limits |
| `mem_reservation` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `mem_swappiness` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `memswap_limit` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `models` | No | No | Served by Docker's model runner: VMs and the cloud have none |
| `models.*.endpoint_var` | No | No | Served by Docker's model runner: VMs and the cloud have none |
| `models.*.model_var` | No | No | Served by Docker's model runner: VMs and the cloud have none |
| `network_mode` | Yes | Yes | Isoloom's own sidecars and check runner; sharing another machine's network isn't possible between VMs |
| `networks` | Yes | Yes | From `machines.*.networks` (fixed addresses) |
| `networks.*.aliases` | Yes | Yes | From `machines.*.aliases` |
| `networks.*.interface_name` | Yes | Yes | Network appliances: each network on the interface the image expects |
| `networks.*.ipv4_address` | Yes | Yes | From `machines.*.networks` (fixed addresses) |
| `networks.*.ipv6_address` | Yes | Yes | From `networks.*.ipv6` (each machine's address from its octet; none with `addresses: auto`) |
| `networks.*.link_local_ips` | No | No | Link-local addresses on a container's interface: cloud VMs get theirs from the provider |
| `networks.*.mac_address` | Yes | Yes | VLAN trunks: each end found by its fixed MAC address |
| `networks.*.driver_opts` | No | No | Docker network driver options |
| `networks.*.priority` | Yes | Yes | Network appliances: the order their interfaces come up in |
| `networks.*.gw_priority` | Yes | Yes | The hybrid target: the default route through the network shared with the VMs |
| `oom_kill_disable` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `oom_score_adj` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `pid` | No | No | Shares a kernel namespace with the host or another machine: separate VMs can't |
| `pids_limit` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `platform` | Yes | Yes | From `machines.*.arch` (pinned on every container, so the machine runs the same on an x86-64 or an ARM host) |
| `ports` | Yes | Yes | From `machines.*.services[].publish` (on the host's loopback; `/udp` with `protocol: udp`) |
| `ports[].name` | n/a | No | Names the port for tools |
| `ports[].mode` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `ports[].host_ip` | Yes | Another way | Via the short form Isoloom writes (`host_ip:published:target/protocol`) |
| `ports[].target` | Yes | Another way | Via the short form Isoloom writes (`host_ip:published:target/protocol`) |
| `ports[].published` | Yes | Another way | Via the short form Isoloom writes (`host_ip:published:target/protocol`) |
| `ports[].protocol` | Yes | Another way | Via the short form Isoloom writes (`host_ip:published:target/protocol`) |
| `ports[].app_protocol` | n/a | No | Names the port's protocol for tools |
| `post_start` | Yes | Another way | Via machines.*.docker.init (runs once the machine answers) |
| `post_start[].command` | Yes | Another way | Via machines.*.docker.init (runs once the machine answers) |
| `post_start[].user` | Yes | Another way | Via machines.*.docker.init (runs once the machine answers) |
| `post_start[].privileged` | Yes | Another way | Via machines.*.docker.init (runs once the machine answers) |
| `post_start[].working_dir` | Yes | Another way | Via machines.*.docker.init (runs once the machine answers) |
| `post_start[].environment` | Yes | Another way | Via machines.*.docker.init (runs once the machine answers) |
| `pre_start` | Yes | Another way | Via machines.*.depends_on and docker.init |
| `pre_stop` | n/a | No | Compose tooling, not the environment's behavior |
| `pre_stop[].command` | n/a | No | Compose tooling, not the environment's behavior |
| `pre_stop[].user` | n/a | No | Compose tooling, not the environment's behavior |
| `pre_stop[].privileged` | n/a | No | Compose tooling, not the environment's behavior |
| `pre_stop[].working_dir` | n/a | No | Compose tooling, not the environment's behavior |
| `pre_stop[].environment` | n/a | No | Compose tooling, not the environment's behavior |
| `privileged` | Yes | Yes | From `machines.*.privileged` (the container runs privileged; a VM already has it, its workload is root) |
| `profiles` | Yes | Yes | From `checks` (a `check` profile) |
| `provider` | n/a | No | Compose tooling, not the environment's behavior |
| `provider.type` | n/a | No | Compose tooling, not the environment's behavior |
| `provider.options` | n/a | No | Compose tooling, not the environment's behavior |
| `pull_policy` | n/a | No | Compose tooling, not the environment's behavior |
| `pull_refresh_after` | n/a | No | Compose tooling, not the environment's behavior |
| `read_only` | Yes | Yes | From `machines.*.read_only` (a read-only root filesystem; `volumes` stay writable) |
| `restart` | Yes | Yes | Always unless-stopped: machines stay up like VMs |
| `runtime` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `scale` | Yes | Another way | Via machines.*.count (identical machines, each with its own name and address) |
| `secrets` | Yes | Yes | From `machines.*.secrets` |
| `secrets[].source` | Yes | Yes | From `machines.*.secrets[].source` |
| `secrets[].target` | Yes | Yes | From `machines.*.secrets[].target` (default /run/secrets/<name>) |
| `secrets[].uid` | Yes | Yes | From `machines.*.secrets[].uid` |
| `secrets[].gid` | Yes | Yes | From `machines.*.secrets[].gid` |
| `secrets[].mode` | Yes | Yes | From `machines.*.secrets[].mode` |
| `security_opt` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `shm_size` | Yes | Yes | From `machines.*.shm_size` (and a Memory emptyDir on Kubernetes) |
| `stdin_open` | n/a | No | Compose tooling, not the environment's behavior |
| `stop_grace_period` | n/a | No | Compose tooling, not the environment's behavior |
| `stop_signal` | Yes | In the image | Set it in the image (`docker.build`) or the VM's provisioning |
| `storage_opt` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `sysctls` | Yes | Yes | From `machines.*.sysctls and networks.*.gateway` |
| `tmpfs` | Yes | Yes | From `machines.*.tmpfs` (and Memory emptyDirs on Kubernetes) |
| `tty` | n/a | No | Compose tooling, not the environment's behavior |
| `ulimits` | Yes | Yes | From `machines.*.ulimits` |
| `ulimits.*.hard` | Yes | Yes | From `machines.*.ulimits` |
| `ulimits.*.soft` | Yes | Yes | From `machines.*.ulimits` |
| `use_api_socket` | No | No | Hands the host's Docker to a machine: VMs and the cloud have none |
| `user` | Yes | Yes | Isoloom's check runner, as root to drop its default route on offline networks; a machine's own user is its image's |
| `userns_mode` | No | No | Shares a kernel namespace with the host or another machine: separate VMs can't |
| `uts` | No | No | Shares a kernel namespace with the host or another machine: separate VMs can't |
| `volumes` | Yes | Yes | From `machines.*.volumes and volumes` (shared between machines; seeded with the image's files at the path, Kubernetes included; and read-only mounts of init and check scripts) |
| `volumes[].type` | Yes | Another way | Via the short form Isoloom writes (`source:target[:ro]`) |
| `volumes[].source` | Yes | Another way | Via the short form Isoloom writes (`source:target[:ro]`) |
| `volumes[].target` | Yes | Another way | Via the short form Isoloom writes (`source:target[:ro]`) |
| `volumes[].read_only` | Yes | Another way | Via the short form Isoloom writes (`source:target[:ro]`) |
| `volumes[].consistency` | n/a | No | Docker Desktop's file sharing tuning |
| `volumes[].bind` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `volumes[].bind.propagation` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `volumes[].bind.create_host_path` | n/a | No | How Docker treats a missing host folder |
| `volumes[].bind.recursive` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `volumes[].bind.selinux` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `volumes[].volume` | Yes | Another way | Via the short form Isoloom writes (`source:target[:ro]`) |
| `volumes[].volume.labels` | n/a | No | Labels on a volume Compose creates on the fly: Isoloom declares its volumes |
| `volumes[].volume.nocopy` | n/a | No | Skips seeding the volume with the image's files: Isoloom always seeds |
| `volumes[].volume.subpath` | Yes | Not yet | Mounting one folder of a shared volume |
| `volumes[].tmpfs` | Yes | Another way | Via tmpfs (from machines.*.tmpfs) |
| `volumes[].tmpfs.size` | Yes | Not yet | A size for machines.*.tmpfs |
| `volumes[].tmpfs.mode` | Yes | Not yet | A mode for machines.*.tmpfs |
| `volumes[].image` | No | No | Mounts another container image's files: VMs have no images to mount |
| `volumes[].image.subpath` | No | No | Mounts another container image's files: VMs have no images to mount |
| `volumes_from` | Yes | Another way | Via volumes (a volume shared between machines, each mounting it at its own path) |
| `working_dir` | Yes | Yes | From `machines.*.working_dir` |

### Networks

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `name` | Yes | Yes | From `networks.*.external` (the existing network's name; on the hybrid target, the Docker network its host VM created for each network; elsewhere Compose scopes names to the environment) |
| `driver` | No | No | Docker network drivers (overlay, macvlan) have no VM equivalent |
| `driver_opts` | No | No | Docker network driver options |
| `ipam` | Yes | Yes | From `networks.*.cidr` |
| `ipam.driver` | No | No | Docker IPAM drivers have no VM equivalent |
| `ipam.config` | Yes | Yes | From `networks.*.cidr and networks.*.ipv6.cidr` |
| `ipam.config[].subnet` | Yes | Yes | From `networks.*.cidr and networks.*.ipv6.cidr` |
| `ipam.config[].ip_range` | Yes | Yes | From `networks.*.ipv6` (where Docker picks IPv6 addresses, clear of the fixed ones) |
| `ipam.config[].gateway` | Yes | Yes | The address Docker takes, when the first one is a machine's or the router's |
| `ipam.config[].aux_addresses` | n/a | No | Addresses Docker must not hand out: every machine has a fixed one |
| `ipam.options` | No | No | Docker IPAM driver options |
| `external` | Yes | Yes | From `networks.*.external` (an existing Docker network, joined by name at fixed addresses; on the hybrid target, the macvlan networks its Docker host VM creates, shared with the VMs) |
| `external.name` | n/a | No | Legacy spelling of `name` |
| `internal` | Yes | Yes | From `networks.*.internet: false` (when nothing routes) |
| `enable_ipv4` | Yes | Yes | From `networks.*.ipv6.only` (off on an IPv6-only network) |
| `enable_ipv6` | Yes | Yes | From `networks.*.ipv6` |
| `attachable` | n/a | No | Compose tooling, not the environment's behavior |
| `labels` | Yes | Yes | The environment's labels: isoloom.managed and isoloom.environment, for `isoloom gc` |

### Volumes

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `name` | n/a | No | Compose scopes names to the environment |
| `driver` | No | No | Docker volume drivers have no VM equivalent |
| `driver_opts` | No | No | Docker volume driver options |
| `external` | Yes | Not yet | With volumes (data that outlives the environment) |
| `external.name` | n/a | No | Legacy spelling of `name` |
| `labels` | Yes | Yes | The environment's labels: isoloom.managed and isoloom.environment, for `isoloom gc` |

### Configs

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `name` | n/a | No | Compose scopes names to the environment |
| `content` | Yes | Yes | From `machines.*.files[].content` |
| `environment` | Yes | Another way | Via machines.*.files[].content (the text in the spec; a value given at run time is an input) |
| `file` | Yes | Yes | From `machines.*.files[].source` |
| `external` | n/a | No | A config created outside Compose |
| `external.name` | n/a | No | Legacy spelling of `name` |
| `labels` | n/a | No | Compose tooling, not the environment's behavior |
| `template_driver` | n/a | No | Swarm's templating of configs |

### Secrets

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `name` | n/a | No | Compose scopes names to the environment |
| `environment` | Yes | Yes | From `secrets` (every store: `isoloom run` reads the value and sets `ISOLOOM_SECRET_<NAME>` for Compose alone) |
| `file` | Yes | Another way | Via secrets.*.file: `isoloom run` reads the file and hands its value to Compose as `environment` |
| `external` | Yes | Another way | Via secrets.*.vault and secrets.*.sops: stores outside the file, read by `isoloom run` |
| `external.name` | n/a | No | Legacy spelling of `name` |
| `labels` | n/a | No | Compose tooling, not the environment's behavior |
| `driver` | No | No | Docker secret drivers have no VM equivalent |
| `driver_opts` | No | No | Docker secret driver options |
| `template_driver` | n/a | No | Swarm's templating of secrets |

### Models

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `name` | No | No | Served by Docker's model runner: VMs and the cloud have none |
| `model` | No | No | Served by Docker's model runner: VMs and the cloud have none |
| `context_size` | No | No | Served by Docker's model runner: VMs and the cloud have none |
| `runtime_flags` | No | No | Served by Docker's model runner: VMs and the cloud have none |

## Vagrant

100% of 42 portable features (42 done, 0 partly, 0 to do; 100 features in all). From Vagrant 2.4.9 (config.vm, network types, provisioners, communicators, pushes).

### Machine settings (config.vm)

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allow_fstab_modification` | n/a | No | Shared folders are off: the project is copied into each VM |
| `allow_hosts_modification` | n/a | No | Isoloom writes /etc/hosts itself (names of the other machines) |
| `allowed_synced_folder_types` | n/a | No | Shared folders are off: the project is copied into each VM |
| `base_address` | No | No | Cloud VMs get their MAC and addresses from the provider |
| `base_mac` | No | No | Cloud VMs get their MAC and addresses from the provider |
| `boot_timeout` | Yes | Yes | Fixed: 10 minutes |
| `box` | Yes | Yes | From `machines.*.vm.os` (built-in boxes for every OS name), or machines.*.vm.image.vagrant |
| `box_architecture` | Yes | Yes | From `machines.*.arch` |
| `box_check_update` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_ca_cert` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_ca_path` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_checksum` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_checksum_type` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_client_cert` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_disable_ssl_revoke_best_effort` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_insecure` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_location_trusted` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_options` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_extra_download_options` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_server_url` | Yes | Yes | From `machines.*.vm.image.box_server_url` |
| `box_url` | Yes | Yes | From `machines.*.vm.image.box_url` (a URL, or a .box file in the project) |
| `box_version` | Yes | Yes | From `machines.*.vm.image.vagrant_version, or a built-in pin` |
| `clone` | n/a | No | Starts from another Vagrant machine; machines start from their image |
| `cloud_init` | Yes | Another way | Via provisioning steps run any setup |
| `cloud_init_configs` | Yes | Another way | Via provisioning steps run any setup |
| `cloud_init_first_boot_only` | Yes | Another way | Via provisioning steps run any setup |
| `communicator` | Yes | Yes | From `vm.os` (Windows uses WinRM, plain HTTP or SSL per the box's `image.winrm`) |
| `define` | Yes | Yes | From `machines` |
| `disk` | Yes | Yes | From `machines.*.resources.disk_gb` (the root filesystem grown into it) |
| `disks` | Yes | Another way | Via machines.*.resources.disk_gb and machines.*.volumes (one disk, sized, holding the volumes' paths) |
| `graceful_halt_timeout` | n/a | No | Vagrant tooling, not the environment's behavior |
| `guest` | Yes | Yes | From `vm.os` (Windows uses WinRM, plain HTTP or SSL per the box's `image.winrm`) |
| `host_name` | n/a | No | Old name of `hostname` |
| `hostname` | Yes | Yes | From the machine's name |
| `ignore_box_vagrantfile` | n/a | No | Vagrant tooling, not the environment's behavior |
| `network` | Yes | Yes | From `networks` (see the network types below) |
| `post_up_message` | n/a | No | Vagrant tooling, not the environment's behavior |
| `provider` | Yes | Yes | From `machines.*.resources` (see each provider below) |
| `provision` | Yes | Yes | From `machines.*.vm.provision` (see the provisioners below) |
| `provisioners` | n/a | No | Vagrant's own list behind `provision` |
| `synced_folder` | Yes | Yes | The default `/vagrant` share turned off: the project is copied into each VM instead |
| `usable_port_range` | n/a | No | How Vagrant picks host ports for forwarded ports |

### Network types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `forwarded_port` | Yes | Yes | From `machines.*.services[].publish` (on the host's loopback; `protocol: "udp"` with `protocol: udp`) |
| `private_network` | Yes | Yes | From `networks and machines.*.networks` |
| `public_network` | Yes | Yes | From `networks.*.bridged` (the host's LAN, through the interface it names; not on QEMU, Hyper-V or libvirt yet) |

### Provisioners

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `shell` | Yes | Yes | From `machines.*.vm.provision` (.sh, .yml, .yaml, `reboot`), and Isoloom's own steps |
| `file` | Yes | Another way | Via `isoloom_project` (below) copies the project; a file provisioner follows symlinks and lists files when the Vagrantfile loads |
| `ansible` | Yes | Another way | Via the same playbooks, run inside the VM by a shell provisioner |
| `ansible_local` | Yes | Another way | Via a shell provisioner running the playbook with `ansible-playbook -c local` inside the VM, the machine as its inventory (as on every VM target) |
| `chef_solo` | Yes | Another way | Via a `.sh` provisioning step runs the tool (the script installs it) |
| `chef_zero` | Yes | Another way | Via a `.sh` provisioning step runs the tool (the script installs it) |
| `chef_client` | Yes | Another way | Via a `.sh` provisioning step runs the tool (the script installs it) |
| `chef_apply` | Yes | Another way | Via a `.sh` provisioning step runs the tool (the script installs it) |
| `container` | Yes | Another way | Via `docker:` (containers come from the Compose output; VMs run services natively) |
| `docker` | Yes | Another way | Via `docker:` (containers come from the Compose output; VMs run services natively) |
| `podman` | Yes | Another way | Via `docker:` (containers come from the Compose output; VMs run services natively) |
| `puppet` | Yes | Another way | Via a `.sh` provisioning step runs the tool (the script installs it) |
| `puppet_server` | Yes | Another way | Via a `.sh` provisioning step runs the tool (the script installs it) |
| `salt` | Yes | Another way | Via a `.sh` provisioning step runs the tool (the script installs it) |
| `cfengine` | Yes | Another way | Via a `.sh` provisioning step runs the tool (the script installs it) |
| `isoloom_project` | Yes | Yes | Isoloom's own provisioner, defined in the Vagrantfile: the project into each VM as a tar.gz built when the step runs, symlinks kept as links |

### Other settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `config.ssh` | Yes | Yes | A jump host from ISOLOOM_SSH_PROXY_COMMAND, when set; see the communicators' table |
| `config.winrm` | Yes | Yes | From `vm.os` (the Windows box's own account, over WinRM) |
| `config.winssh` | Yes | Yes | From `machines.*.vm.image.winssh` (a Windows box reached over OpenSSH) |
| `config.trigger` | Yes | Yes | Hyper-V: each network's private switch made before a VM starts and removed after the last one, its adapters attached and addressed, its virtual TPM enabled |
| `config.vagrant` | n/a | No | Vagrant tooling, not the environment's behavior |
| `config.push` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |

### Push strategies

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `atlas` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `heroku` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `local-exec` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: address` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: app` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: dir` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: exclude` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: excludes` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: include` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: includes` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: token` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: token_from_vagrant_login` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: uploader_path` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `atlas: vcs` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: destination` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: dir` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: exclude` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: excludes` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: host` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: include` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: includes` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: passive` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: password` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: secure` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `ftp: username` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `heroku: app` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `heroku: dir` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `heroku: git_bin` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `heroku: remote` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `local-exec: args` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `local-exec: inline` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |
| `local-exec: script` | n/a | No | `vagrant push` publishes the project elsewhere: not the environment's behavior |

## Vagrant: networks

100% of 38 portable features (38 done, 0 partly, 0 to do; 58 features in all). From Vagrant 2.4.9 (the networking docs and the providers' network code) and vagrant-libvirt 0.12.2 (its network options).

### Network types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `forwarded_port: auto_correct` | n/a | No | Vagrant moving a forward to a free host port: published ports are fixed |
| `forwarded_port: disabled` | Yes | Yes | A Windows box's own RDP forward, turned off so VMs don't compete for the host port |
| `forwarded_port: guest` | Yes | Yes | From `machines.*.services[].publish` |
| `forwarded_port: guest_ip` | Yes | Another way | Via machines.*.services[].port (forwarded to the machine itself) |
| `forwarded_port: host` | Yes | Yes | From `machines.*.services[].publish` |
| `forwarded_port: host_ip` | Yes | Yes | The host's loopback: a published port isn't open to the LAN |
| `forwarded_port: id` | Yes | Yes | A Windows box's own RDP forward, turned off so VMs don't compete for the host port |
| `forwarded_port: protocol` | Yes | Yes | From `machines.*.services[].protocol` |
| `private_network: adapter` | n/a | No | Which adapter and host network the provider uses |
| `private_network: adapter_ip` | Yes | Another way | Via machines.*.networks (a fixed address on every target, instead of DHCP) |
| `private_network: auto_config` | Yes | Yes | Off for the hybrid target's Docker host: its containers take the addresses on those networks |
| `private_network: bridge` | Yes | Yes | Hyper-V: each VM on the networks' private switches |
| `private_network: dhcp_ip` | Yes | Another way | Via machines.*.networks (a fixed address on every target, instead of DHCP) |
| `private_network: dhcp_lower` | Yes | Another way | Via machines.*.networks (a fixed address on every target, instead of DHCP) |
| `private_network: dhcp_upper` | Yes | Another way | Via machines.*.networks (a fixed address on every target, instead of DHCP) |
| `private_network: intnet` | Yes | Another way | Via `virtualbox__intnet`, the same option scoped to VirtualBox |
| `private_network: ip` | Yes | Yes | From `machines.*.networks` (fixed addresses) |
| `private_network: libvirt__adapter` | n/a | No | How vagrant-libvirt names and cleans up its interfaces |
| `private_network: libvirt__always_destroy` | n/a | No | How vagrant-libvirt names and cleans up its interfaces |
| `private_network: libvirt__bus` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `private_network: libvirt__dhcp_bootp_file` | Yes | Yes | From `machines.*.vm.boot.network.file` (on the machine's first network, DHCP on) |
| `private_network: libvirt__dhcp_bootp_server` | Yes | Yes | From `machines.*.vm.boot.network.server` |
| `private_network: libvirt__dhcp_enabled` | Yes | Yes | Off: machines get fixed addresses; on where a machine boots from the network, `vm.boot.network` |
| `private_network: libvirt__dhcp_start` | Yes | Another way | Via machines.*.networks (a fixed address on every target, instead of DHCP) |
| `private_network: libvirt__dhcp_stop` | Yes | Another way | Via machines.*.networks (a fixed address on every target, instead of DHCP) |
| `private_network: libvirt__domain_name` | Yes | Another way | Via machines.*.dns.domain |
| `private_network: libvirt__driver_name` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `private_network: libvirt__driver_queues` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `private_network: libvirt__forward_device` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `private_network: libvirt__forward_mode` | Yes | Another way | Via networks.*.internet (out through Isoloom's router, the same on every target) |
| `private_network: libvirt__guest_ipv6` | Yes | Yes | From `networks.*.ipv6` (the libvirt network carries the block; the set-up steps give each VM its address; not run on a real host) |
| `private_network: libvirt__host_ip` | Yes | Another way | Via `ip` and `netmask`: libvirt derives the network from them |
| `private_network: libvirt__iface_name` | n/a | No | How vagrant-libvirt names and cleans up its interfaces |
| `private_network: libvirt__ipv6_address` | Yes | Yes | From `networks.*.ipv6` (the libvirt network carries the block; the set-up steps give each VM its address; not run on a real host) |
| `private_network: libvirt__ipv6_prefix` | Yes | Yes | From `networks.*.ipv6` (the libvirt network carries the block; the set-up steps give each VM its address; not run on a real host) |
| `private_network: libvirt__mac` | No | No | Cloud VMs get their MAC and addresses from the provider |
| `private_network: libvirt__model_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `private_network: libvirt__mtu` | No | No | Cloud networks set their own MTU |
| `private_network: libvirt__netmask` | Yes | Another way | Via `ip` and `netmask`: libvirt derives the network from them |
| `private_network: libvirt__network_address` | Yes | Another way | Via `ip` and `netmask`: libvirt derives the network from them |
| `private_network: libvirt__network_name` | Yes | Yes | From `networks` (a libvirt network each, named after the environment) |
| `private_network: libvirt__slot` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `private_network: libvirt__tftp_root` | Yes | Yes | From `machines.*.vm.boot.network.tftp_root` |
| `private_network: libvirt__tunnel_ip` | No | No | Links libvirt hosts together: other targets join machines on one network of their own |
| `private_network: libvirt__tunnel_local_ip` | No | No | Links libvirt hosts together: other targets join machines on one network of their own |
| `private_network: libvirt__tunnel_local_port` | No | No | Links libvirt hosts together: other targets join machines on one network of their own |
| `private_network: libvirt__tunnel_port` | No | No | Links libvirt hosts together: other targets join machines on one network of their own |
| `private_network: libvirt__tunnel_type` | No | No | Links libvirt hosts together: other targets join machines on one network of their own |
| `private_network: mac` | Yes | Yes | The hybrid target's Docker host: a fixed MAC per network, so its macvlan networks find their adapter |
| `private_network: name` | n/a | No | Which adapter and host network the provider uses |
| `private_network: netmask` | Yes | Yes | From `networks.*.cidr` |
| `private_network: nic_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `private_network: type` | Yes | Another way | Via machines.*.networks (a fixed address on every target, instead of DHCP) |
| `private_network: virtualbox__intnet` | Yes | Yes | From `networks` (an internal network each: between the VMs only, no host adapter) |
| `public_network: auto_config` | Yes | Another way | Via networks.*.bridged (Vagrant always configures the bridged interface: a fixed address, or DHCP) |
| `public_network: bridge` | Yes | Yes | From `networks.*.bridged.interface` |
| `public_network: ip` | Yes | Yes | From `machines.*.networks` (a fixed address on a bridged network; DHCP otherwise) |
| `public_network: use_dhcp_assigned_default_route` | Yes | Yes | From `networks.*.bridged.default_route` |

## Vagrant: disks and synced folders

100% of 12 portable features (12 done, 0 partly, 0 to do; 28 features in all). From Vagrant 2.4.9 (the disk config class and the synced folder docs).

### Disk options

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `disk_ext` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `file` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `id` | n/a | No | Vagrant's own identifier for the disk |
| `name` | Yes | Another way | Via machines.*.resources.disk_gb and machines.*.volumes (one disk, sized, holding the volumes' paths) |
| `primary` | Yes | Yes | The VM's own disk, `:disk` and primary, grown to `disk_gb` |
| `provider_config` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `size` | Yes | Yes | From `machines.*.resources.disk_gb` |
| `type` | Yes | Yes | The VM's own disk, `:disk` and primary, grown to `disk_gb` |

### Synced folder options

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `create` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `disabled` | Yes | Yes | The default `/vagrant` share, turned off |
| `group` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `id` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `mount_options` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `nfs_export` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `nfs_udp` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `nfs_version` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `owner` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `rsync__args` | Yes | Another way | Via the project copied into each VM by `isoloom_project` (`.isoloomignore` says what stays out) |
| `rsync__auto` | Yes | Another way | Via the project copied into each VM by `isoloom_project` (`.isoloomignore` says what stays out) |
| `rsync__chown` | Yes | Another way | Via the project copied into each VM by `isoloom_project` (`.isoloomignore` says what stays out) |
| `rsync__exclude` | Yes | Another way | Via the project copied into each VM by `isoloom_project` (`.isoloomignore` says what stays out) |
| `rsync__rsync_ownership` | Yes | Another way | Via the project copied into each VM by `isoloom_project` (`.isoloomignore` says what stays out) |
| `rsync__rsync_path` | Yes | Another way | Via the project copied into each VM by `isoloom_project` (`.isoloomignore` says what stays out) |
| `rsync__verbose` | Yes | Another way | Via the project copied into each VM by `isoloom_project` (`.isoloomignore` says what stays out) |
| `smb_host` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `smb_password` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `smb_username` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |
| `type` | No | No | A live share of the user's folder: cloud VMs and Proxmox can't mount it (the project is copied into each VM instead) |

## Vagrant: provisioners

100% of 57 portable features (57 done, 0 partly, 0 to do; 315 features in all). From Vagrant 2.4.9 (each provisioner's config class, with the classes it inherits from).

### Provisioners

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `*: after` | Yes | Another way | Via the order of machines.*.vm.provision (steps run as listed) |
| `*: before` | Yes | Another way | Via the order of machines.*.vm.provision (steps run as listed) |
| `*: communicator_required` | n/a | No | How Vagrant runs the step: not the machine's state |
| `*: config` | n/a | No | How Vagrant runs the step: not the machine's state |
| `*: id` | n/a | No | How Vagrant runs the step: not the machine's state |
| `*: name` | Yes | Yes | Each step's name, shown as it runs |
| `*: preserve_order` | n/a | No | How Vagrant runs the step: not the machine's state |
| `*: run` | Yes | Yes | `never` for the checks, run only when asked |
| `*: type` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: args` | Yes | Another way | Via machines.*.inputs (values reach a script as environment variables) |
| `shell: binary` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: env` | Yes | Yes | From `machines.*.inputs` (from the host's environment) |
| `shell: inline` | Yes | Yes | From `machines.*.vm.provision, and Isoloom's own steps` |
| `shell: keep_color` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: md5` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: name` | Yes | Another way | Via `*: name`, the same label |
| `shell: path` | Yes | Yes | The checks script, run from the project |
| `shell: powershell_args` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: powershell_elevated_interactive` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: privileged` | n/a | No | Steps run as root, Vagrant's default |
| `shell: reboot` | Yes | Yes | From `machines.*.vm.provision` (`reboot`) |
| `shell: reset` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: sensitive` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: sha1` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: sha256` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: sha384` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: sha512` | n/a | No | How Vagrant runs the step: not the machine's state |
| `shell: upload_path` | n/a | No | How Vagrant runs the step: not the machine's state |
| `file: destination` | Yes | Another way | Via `isoloom_project` copies the project into each VM |
| `file: source` | Yes | Another way | Via `isoloom_project` copies the project into each VM |
| `ansible: ask_become_pass` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible: ask_sudo_pass` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible: ask_vault_pass` | Yes | Another way | Via machines.*.secrets: the vault's password as a secret file, delivered before the machine's playbooks run, named by `vault_password_file` in an `ansible.cfg` beside the playbook |
| `ansible: become` | Yes | Another way | Via the playbook's own `become` |
| `ansible: become_user` | Yes | Another way | Via the playbook's own `become` |
| `ansible: compatibility_mode` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible: config_file` | Yes | Another way | Via an `ansible.cfg` beside the playbook, in the project |
| `ansible: extra_vars` | Yes | Another way | Via provision[].vars |
| `ansible: force_remote_user` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible: galaxy_command` | Yes | Another way | Via provision[].requirements |
| `ansible: galaxy_role_file` | Yes | Another way | Via provision[].requirements |
| `ansible: galaxy_roles_path` | Yes | Another way | Via provision[].requirements |
| `ansible: groups` | Yes | Another way | Via provision[].groups |
| `ansible: host_key_checking` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible: host_vars` | Yes | Another way | Via provision[].host_vars |
| `ansible: inventory_path` | Yes | Another way | Via provision[].inventory |
| `ansible: limit` | Yes | Another way | Via the playbook's own `hosts` |
| `ansible: playbook` | Yes | Another way | Via provision[].ansible |
| `ansible: playbook_command` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible: raw_arguments` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible: raw_ssh_args` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible: show_deprecation_info` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible: skip_tags` | n/a | No | Running part of a playbook: a debugging aid |
| `ansible: start_at_task` | n/a | No | Running part of a playbook: a debugging aid |
| `ansible: sudo` | Yes | Another way | Via the playbook's own `become` |
| `ansible: sudo_user` | Yes | Another way | Via the playbook's own `become` |
| `ansible: tags` | n/a | No | Running part of a playbook: a debugging aid |
| `ansible: vault_password_file` | Yes | Another way | Via machines.*.secrets: the vault's password as a secret file, delivered before the machine's playbooks run, named by `vault_password_file` in an `ansible.cfg` beside the playbook |
| `ansible: verbose` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible: version` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: become` | Yes | Another way | Via the playbook's own `become` |
| `ansible_local: become_user` | Yes | Another way | Via the playbook's own `become` |
| `ansible_local: compatibility_mode` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: config_file` | Yes | Another way | Via an `ansible.cfg` beside the playbook, in the project |
| `ansible_local: extra_vars` | Yes | Another way | Via provision[].vars |
| `ansible_local: galaxy_command` | Yes | Another way | Via provision[].requirements |
| `ansible_local: galaxy_role_file` | Yes | Another way | Via provision[].requirements |
| `ansible_local: galaxy_roles_path` | Yes | Another way | Via provision[].requirements |
| `ansible_local: groups` | Yes | Another way | Via provision[].groups |
| `ansible_local: host_vars` | Yes | Another way | Via provision[].host_vars |
| `ansible_local: install` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: install_mode` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: inventory_path` | Yes | Another way | Via provision[].inventory |
| `ansible_local: limit` | Yes | Another way | Via the playbook's own `hosts` |
| `ansible_local: pip_args` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: pip_install_cmd` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: playbook` | Yes | Another way | Via provision[].ansible |
| `ansible_local: playbook_command` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: provisioning_path` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: raw_arguments` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: show_deprecation_info` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: skip_tags` | n/a | No | Running part of a playbook: a debugging aid |
| `ansible_local: start_at_task` | n/a | No | Running part of a playbook: a debugging aid |
| `ansible_local: sudo` | Yes | Another way | Via the playbook's own `become` |
| `ansible_local: sudo_user` | Yes | Another way | Via the playbook's own `become` |
| `ansible_local: tags` | n/a | No | Running part of a playbook: a debugging aid |
| `ansible_local: tmp_path` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: vault_password_file` | Yes | Another way | Via machines.*.secrets: the vault's password as a secret file, delivered before the machine's playbooks run, named by `vault_password_file` in an `ansible.cfg` beside the playbook |
| `ansible_local: verbose` | n/a | No | How Vagrant installs and runs Ansible |
| `ansible_local: version` | n/a | No | How Vagrant installs and runs Ansible |
| `chef_solo: add_recipe` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: add_role` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: arguments` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: attempts` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: binary_env` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: binary_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: channel` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: cookbooks_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: custom_config_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: data_bags_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: enable_reporting` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: encrypted_data_bag_secret_key_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: environment` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: environments_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: file_backup_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: file_cache_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: formatter` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: http_proxy` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: http_proxy_pass` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: http_proxy_user` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: https_proxy` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: https_proxy_pass` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: https_proxy_user` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: install` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: installer_download_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: json` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: legacy_mode` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: log_level` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: no_proxy` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: node_name` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: nodes_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: omnibus_url` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: prepare_folders_config` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: product` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: provisioning_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: recipe_url` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: roles_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: run_list` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: synced_folder_type` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: verbose_logging` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_solo: version` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: add_recipe` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: add_role` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: arguments` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: attempts` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: binary_env` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: binary_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: channel` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: cookbooks_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: custom_config_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: data_bags_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: enable_reporting` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: encrypted_data_bag_secret_key_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: environment` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: environments_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: file_backup_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: file_cache_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: formatter` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: http_proxy` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: http_proxy_pass` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: http_proxy_user` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: https_proxy` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: https_proxy_pass` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: https_proxy_user` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: install` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: installer_download_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: json` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: log_level` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: no_proxy` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: node_name` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: nodes_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: omnibus_url` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: prepare_folders_config` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: product` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: provisioning_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: roles_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: run_list` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: synced_folder_type` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: verbose_logging` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_zero: version` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: add_recipe` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: add_role` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: arguments` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: attempts` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: binary_env` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: binary_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: channel` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: chef_server_url` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: client_key_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: custom_config_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: delete_client` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: delete_node` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: enable_reporting` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: encrypted_data_bag_secret_key_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: environment` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: file_backup_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: file_cache_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: formatter` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: http_proxy` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: http_proxy_pass` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: http_proxy_user` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: https_proxy` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: https_proxy_pass` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: https_proxy_user` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: install` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: installer_download_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: json` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: log_level` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: no_proxy` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: node_name` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: omnibus_url` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: product` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: provisioning_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: run_list` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: validation_client_name` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: validation_key_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: verbose_logging` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_client: version` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: binary_env` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: binary_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: channel` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: install` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: installer_download_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: log_level` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: omnibus_url` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: product` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: recipe` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: upload_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `chef_apply: version` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `container: build_image` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `container: images` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `container: post_install_provision` | n/a | No | How Vagrant installs the container engine |
| `container: post_install_provisioner` | n/a | No | How Vagrant installs the container engine |
| `container: pull_images` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `container: run` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `docker: build_image` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `docker: images` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `docker: post_install_provision` | n/a | No | How Vagrant installs the container engine |
| `docker: post_install_provisioner` | n/a | No | How Vagrant installs the container engine |
| `docker: pull_images` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `docker: run` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `podman: build_image` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `podman: images` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `podman: kubic` | n/a | No | How Vagrant installs the container engine |
| `podman: post_install_provision` | n/a | No | How Vagrant installs the container engine |
| `podman: post_install_provisioner` | n/a | No | How Vagrant installs the container engine |
| `podman: pull_images` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `podman: run` | Yes | Another way | Via the docker-vm output (containers on one VM, from the Compose file) |
| `puppet: binary_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: environment` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: environment_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: environment_variables` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: expanded_module_paths` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: facter` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: hiera_config_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: manifest_file` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: manifests_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: module_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: nfs` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: options` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: structured_facts` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: synced_folder_args` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: synced_folder_type` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: temp_dir` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet: working_directory` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet_server: binary_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet_server: client_cert_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet_server: client_private_key_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet_server: facter` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet_server: options` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet_server: puppet_node` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `puppet_server: puppet_server` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: always_install` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: bootstrap_options` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: bootstrap_script` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: colorize` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: grains_config` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: install_args` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: install_master` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: install_syndic` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: install_type` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: log_level` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: master_config` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: master_id` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: master_json_config` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: master_key` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: master_pub` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: masterless` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: minion_config` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: minion_id` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: minion_json_config` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: minion_key` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: minion_pub` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: no_minion` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: orchestrations` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: pillar` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: pillar_data` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: python_version` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: run_highstate` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: run_overstate` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: run_service` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: salt_args` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: salt_call_args` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: seed_master` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: temp_config_dir` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: verbose` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `salt: version` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: am_policy_hub` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: classes` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: deb_repo_file` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: deb_repo_line` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: extra_agent_args` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: files_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: force_bootstrap` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: install` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: mode` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: package_name` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: policy_server_address` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: repo_gpg_key_url` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: run_file` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: upload_path` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: yum_repo_file` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `cfengine: yum_repo_url` | n/a | No | An option of that tool: a `.sh` step runs it with its own configuration |
| `isoloom_project: generated` | Yes | Yes | The generated files a step needs, such as the checks, copied with the project |
| `isoloom_project: extra` | Yes | Yes | The generated files a step needs, such as the checks, copied with the project |

## Vagrant: communicators

100% of 11 portable features (11 done, 0 partly, 0 to do; 76 features in all). From Vagrant 2.4.9 (the SSH, WinRM and Windows OpenSSH config classes).

### SSH (config.ssh)

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `compression` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `config` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `connect_retries` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `connect_retry_delay` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `connect_timeout` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `default` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `disable_deprecated_algorithms` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `dsa_authentication` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `export_command_template` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `extra_args` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `forward_agent` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `forward_env` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `forward_x11` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `guest_port` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `host` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `insert_key` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `keep_alive` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `key_type` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `keys_only` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `paranoid` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `password` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `port` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `private_key_path` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `proxy_command` | Yes | Yes | ISOLOOM_SSH_PROXY_COMMAND, when set: the VMs reached through a jump host |
| `pty` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `remote_user` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `shell` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `ssh_command` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `sudo_command` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `username` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |
| `verify_host_key` | n/a | No | How Vagrant reaches the VM over SSH: not the environment's behavior |

### WinRM (config.winrm)

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `basic_auth_only` | Yes | Yes | From `machines.*.vm.image.winrm` (plain HTTP with basic authentication, unless the box says `ssl`) |
| `codepage` | n/a | No | How Vagrant reaches the VM over WinRM: not the environment's behavior |
| `execution_time_limit` | n/a | No | How Vagrant reaches the VM over WinRM: not the environment's behavior |
| `guest_port` | n/a | No | How Vagrant reaches the VM over WinRM: not the environment's behavior |
| `host` | n/a | No | How Vagrant reaches the VM over WinRM: not the environment's behavior |
| `max_tries` | Yes | Yes | Waits for a Windows VM's first boot, longer under emulation |
| `password` | Yes | Yes | From `vm.os` (the Windows box's own account) |
| `port` | n/a | No | How Vagrant reaches the VM over WinRM: not the environment's behavior |
| `retry_delay` | Yes | Yes | Waits for a Windows VM's first boot, longer under emulation |
| `ssl_peer_verification` | n/a | No | How Vagrant reaches the VM over WinRM: not the environment's behavior |
| `timeout` | Yes | Yes | Waits for a Windows VM's first boot, longer under emulation |
| `transport` | Yes | Yes | From `machines.*.vm.image.winrm` (plain HTTP with basic authentication, unless the box says `ssl`) |
| `username` | Yes | Yes | From `vm.os` (the Windows box's own account) |

### Windows OpenSSH (config.winssh)

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `compression` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `config` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `connect_retries` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `connect_retry_delay` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `connect_timeout` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `default` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `disable_deprecated_algorithms` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `dsa_authentication` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `export_command_template` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `extra_args` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `forward_agent` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `forward_env` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `forward_x11` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `guest_port` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `host` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `insert_key` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `keep_alive` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `key_type` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `keys_only` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `paranoid` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `password` | Yes | Yes | From `machines.*.vm.image.winssh` (the Windows box's own account) |
| `port` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `private_key_path` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `proxy_command` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `pty` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `remote_user` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `shell` | Yes | Yes | From `machines.*.vm.image.winssh` (PowerShell runs the provisioners' scripts) |
| `ssh_command` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `sudo_command` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `upload_directory` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |
| `username` | Yes | Yes | From `machines.*.vm.image.winssh` (the Windows box's own account) |
| `verify_host_key` | n/a | No | How Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior |

## Vagrant: VirtualBox

100% of 7 portable features (7 done, 0 partly, 0 to do; 15 features in all). From Vagrant 2.4.9 (the provider's config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `auto_nat_dns_proxy` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `check_guest_additions` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | Yes | From `machines.*.resources` |
| `customizations` | No | No | Raw commands for one hypervisor: no other target understands them |
| `customize` | Yes | Yes | From `machines.*.vm.tpm` (a TPM 2.0, `--tpm-type`), machines.*.nested_virtualization (`--nested-hw-virt on`), and the hybrid target's Docker host (its NICs in promiscuous mode, so its containers get their own addresses on the VMs' networks); no spec field writes raw commands |
| `default_nic_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `destroy_unused_network_interfaces` | n/a | No | Vagrant tooling, not the environment's behavior |
| `functional_vboxsf` | n/a | No | Shared folders are off: the project is copied into each VM |
| `gui` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `linked_clone` | Yes | Yes | A differencing disk, so a clone is instant instead of copying the whole box |
| `linked_clone_snapshot` | n/a | No | Vagrant tooling, not the environment's behavior |
| `memory` | Yes | Yes | From `machines.*.resources` |
| `name` | Yes | Yes | The environment and machine names, as the VM's display name |
| `network_adapter` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |
| `network_adapters` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |

## Vagrant: VMware Desktop

100% of 5 portable features (5 done, 0 partly, 0 to do; 24 features in all). From vagrant-vmware-desktop 3.0.5 (its config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allowlist_verified` | n/a | No | Vagrant tooling, not the environment's behavior |
| `base_address` | No | No | Cloud VMs get their MAC and addresses from the provider |
| `base_mac` | No | No | Cloud VMs get their MAC and addresses from the provider |
| `clone_directory` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `cpus` | Yes | Another way | Via machines.*.resources, written in `vmx` |
| `enable_vmrun_ip_lookup` | n/a | No | Vagrant tooling, not the environment's behavior |
| `force_vmware_license` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `functional_hgfs` | n/a | No | Shared folders are off: the project is copied into each VM |
| `gui` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `linked_clone` | n/a | No | Faster starts from one image: no change in behavior |
| `memory` | Yes | Another way | Via machines.*.resources, written in `vmx` |
| `nat_device` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `network_adapter` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |
| `network_adapters` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |
| `port_forward_network_pause` | n/a | No | Vagrant tooling, not the environment's behavior |
| `shared_folder_special_char` | n/a | No | Shared folders are off: the project is copied into each VM |
| `ssh_info_public` | n/a | No | Vagrant tooling, not the environment's behavior |
| `unmount_default_hgfs` | n/a | No | Shared folders are off: the project is copied into each VM |
| `utility_certificate_path` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `utility_host` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `utility_port` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `verify_vmnet` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `vmx` | Yes | Yes | From `machines.*.resources and machines.*.nested_virtualization: displayName, numvcpus, memsize, and vhv.enable` (nested virtualization) |
| `whitelist_verified` | n/a | No | Vagrant tooling, not the environment's behavior |

## Vagrant: Parallels

100% of 7 portable features (7 done, 0 partly, 0 to do; 15 features in all). From vagrant-parallels 2.4.7 (its config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `check_guest_tools` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | Yes | From `machines.*.resources` |
| `customizations` | No | No | Raw commands for one hypervisor: no other target understands them |
| `customize` | Yes | Yes | From `machines.*.nested_virtualization` (`--nested-virt on`); no spec field writes raw commands |
| `destroy_unused_network_interfaces` | n/a | No | Vagrant tooling, not the environment's behavior |
| `functional_psf` | n/a | No | Shared folders are off: the project is copied into each VM |
| `linked_clone` | Yes | Yes | A differencing disk, so a clone is instant instead of copying the whole box |
| `linked_clone_snapshot` | n/a | No | Vagrant tooling, not the environment's behavior |
| `memory` | Yes | Yes | From `machines.*.resources` |
| `name` | Yes | Yes | The environment and machine names, as the VM's display name |
| `network_adapter` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |
| `network_adapters` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |
| `optimize_power_consumption` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `regen_src_uuid` | n/a | No | Vagrant tooling, not the environment's behavior |
| `update_guest_tools` | n/a | No | Vagrant tooling, not the environment's behavior |

## Vagrant: libvirt

100% of 19 portable features (19 done, 0 partly, 0 to do; 146 features in all). From vagrant-libvirt 0.12.2 (its config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `autostart` | n/a | No | Vagrant tooling, not the environment's behavior |
| `boot` | Yes | Yes | From `machines.*.vm.boot.order` (one `boot` per device, in order) |
| `boot_order` | Yes | Another way | Via `boot`: each call adds its device to the order |
| `cdroms` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `channel` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `channels` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `clock_absolute` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `clock_adjustment` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `clock_basis` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `clock_offset` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `clock_timer` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `clock_timers` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `clock_timezone` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `cmd_line` | Yes | Yes | From `machines.*.vm.boot.cmdline` |
| `connect_via_ssh` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `cpu_affinity` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `cpu_fallback` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `cpu_feature` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `cpu_features` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `cpu_mode` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `cpu_model` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `cpu_topology` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `cpuaffinitiy` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `cpus` | Yes | Yes | From `machines.*.resources` |
| `cpuset` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `cputopology` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `default_prefix` | Yes | Yes | From `name` (the environment's, so labs' domains don't collide) |
| `description` | n/a | No | Vagrant tooling, not the environment's behavior |
| `disk_address_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disk_bus` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disk_controller_model` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disk_device` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disk_driver` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disk_driver_opts` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disks` | Yes | Another way | Via machines.*.resources.disk_gb and machines.*.volumes (one disk, sized, holding the volumes' paths) |
| `driver` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `dtb` | Yes | Yes | From `machines.*.vm.boot.dtb` |
| `emulator_path` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `features` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `features_hyperv` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `floppies` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `forward_ssh_port` | n/a | No | Vagrant tooling, not the environment's behavior |
| `graphics_autoport` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `graphics_gl` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `graphics_ip` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `graphics_passwd` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `graphics_port` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `graphics_type` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `graphics_websocket` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `host` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `host_device_exclude_prefixes` | n/a | No | Vagrant tooling, not the environment's behavior |
| `hyperv_feature` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `id_ssh_key_file` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `initrd` | Yes | Yes | From `machines.*.vm.boot.initrd` |
| `input` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `inputs` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `kernel` | Yes | Yes | From `machines.*.vm.boot.kernel` |
| `keymap` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `kvm_hidden` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `launchsecurity` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `launchsecurity_data` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `loader` | Yes | Yes | From `machines.*.vm.boot.loader` |
| `machine_arch` | Yes | Yes | From `machines.*.arch` |
| `machine_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `machine_virtual_size` | Yes | Yes | From `machines.*.resources.disk_gb` (the root filesystem grown into it) |
| `management_network_address` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_autostart` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_device` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_domain` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_driver_iommu` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_guest_ipv6` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_iface_name` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_keep` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_mac` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_mode` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_model_type` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_mtu` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_name` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_pci_bus` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_pci_slot` | n/a | No | Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `memballoon_enabled` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `memballoon_model` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `memballoon_pci_bus` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `memballoon_pci_slot` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `memory` | Yes | Yes | From `machines.*.resources` |
| `memory_backing` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `memorybacking` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `memtune` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `memtunes` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `mgmt_attach` | n/a | No | Vagrant tooling, not the environment's behavior |
| `nested` | Yes | Yes | From `machines.*.nested_virtualization` |
| `nic_adapter_count` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `nic_model_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `nodeset` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `numa_nodes` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `nvram` | Yes | Yes | From `machines.*.vm.boot.nvram` |
| `password` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `pci` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `pcis` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `port` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `proxy_command` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `qemu_args` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `qemu_env` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `qemu_use_agent` | n/a | No | Vagrant tooling, not the environment's behavior |
| `qemu_use_session` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `qemuargs` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `qemuenv` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `random` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `random_hostname` | n/a | No | Vagrant tooling, not the environment's behavior |
| `redirdev` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `redirdevs` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `redirfilter` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `redirfilters` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `rng` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `serial` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `serials` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `shares` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `smartcard` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `smartcard_dev` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `snapshot_pool_name` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `socket` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `sound_type` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `storage` | Yes | Another way | Via machines.*.resources.disk_gb and machines.*.volumes (one disk, sized, holding the volumes' paths) |
| `storage_pool_name` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `storage_pool_path` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `suspend_mode` | n/a | No | Vagrant tooling, not the environment's behavior |
| `sysinfo` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `system_uri` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `title` | n/a | No | Vagrant tooling, not the environment's behavior |
| `tpm_model` | Yes | Yes | From `machines.*.vm.tpm` (Windows 11's by default): an emulated TPM 2.0 |
| `tpm_path` | n/a | No | The host's own TPM device, passed through: the emulated TPM gives each VM one of its own |
| `tpm_type` | Yes | Yes | From `machines.*.vm.tpm` (Windows 11's by default): an emulated TPM 2.0 |
| `tpm_version` | Yes | Yes | From `machines.*.vm.tpm` (Windows 11's by default): an emulated TPM 2.0 |
| `uri` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `usb` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `usb_controller` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `usbctl_dev` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `usbs` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `username` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `uuid` | n/a | No | Vagrant tooling, not the environment's behavior |
| `video_accel3d` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `video_type` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `video_vram` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `volume_cache` | n/a | No | Vagrant tooling, not the environment's behavior |
| `watchdog` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `watchdog_dev` | No | No | Display, input and host devices: containers and cloud VMs have none |

## Vagrant: Hyper-V

100% of 6 portable features (6 done, 0 partly, 0 to do; 16 features in all). From Vagrant 2.4.9 (the provider's config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `auto_start_action` | n/a | No | Vagrant tooling, not the environment's behavior |
| `auto_stop_action` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | Yes | From `machines.*.resources` |
| `differencing_disk` | n/a | No | Faster starts from one image: no change in behavior |
| `enable_automatic_checkpoints` | n/a | No | Vagrant tooling, not the environment's behavior |
| `enable_checkpoints` | n/a | No | Vagrant tooling, not the environment's behavior |
| `enable_enhanced_session_mode` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `enable_virtualization_extensions` | Yes | Yes | From `machines.*.nested_virtualization` |
| `ip_address_timeout` | n/a | No | Vagrant tooling, not the environment's behavior |
| `linked_clone` | Yes | Yes | A differencing disk, so a clone is instant instead of copying the whole box |
| `mac` | No | No | Cloud VMs get their MAC and addresses from the provider |
| `maxmemory` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `memory` | Yes | Yes | From `machines.*.resources` |
| `vlan_id` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |
| `vm_integration_services` | n/a | No | Vagrant tooling, not the environment's behavior |
| `vmname` | Yes | Yes | The environment and machine names, as the VM's name in Hyper-V |

## Vagrant: UTM

100% of 3 portable features (3 done, 0 partly, 0 to do; 12 features in all). From vagrant_utm 0.1.6 (its config class), for Apple Silicon Macs.

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `check_guest_additions` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | Yes | From `machines.*.resources` |
| `customizations` | No | No | Raw commands for one hypervisor: no other target understands them |
| `customize` | No | No | Raw commands for one hypervisor: no other target understands them |
| `directory_share_mode` | n/a | No | Shared folders are off: the project is copied into each VM |
| `functional_9pfs` | n/a | No | Shared folders are off: the project is copied into each VM |
| `icon` | n/a | No | How UTM shows the VM |
| `memory` | Yes | Yes | From `machines.*.resources` |
| `name` | Yes | Yes | The environment and machine names, as the VM's display name |
| `notes` | n/a | No | How UTM shows the VM |
| `wait_time` | n/a | No | Vagrant tooling, not the environment's behavior |
| `private_network` | No | No | Vagrant_utm 0.1.6 has no private networks (it only forwards ports): `refusal_on` with `utm` refuses a spec whose VMs are on a network |

## Vagrant: QEMU

92% of 14 portable features (13 done, 0 partly, 1 to do; 34 features in all). From vagrant-qemu 0.6.3 (its config class), for Apple Silicon Macs.

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `advanced_network` | Yes | Yes | From `machines.*.networks: two VMs as a listen/connect pair, any other number through a hub` (a QEMU process with no machine), no root; a VM is on one network (`refusal_on` with `qemu` refuses one on more) |
| `arch` | Yes | Yes | From `machines.*.arch` (emulated when the host's CPU is another) |
| `control_port` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpu` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `debug_port` | n/a | No | Vagrant tooling, not the environment's behavior |
| `default_qemu_dir` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `disk_resize` | Yes | Yes | From `machines.*.resources.disk_gb, above the box's own size only` (read from the box with qemu-img): the disk is an overlay of the box's, and a smaller one would cut it |
| `drive_interface` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `extra_drive_args` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `extra_image_opts` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `extra_netdev_args` | n/a | No | Options of the NAT adapter Vagrant reaches the VM through (user-mode networking), not of the lab's networks |
| `extra_qemu_args` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `firmware_format` | Yes | Yes | From `machines.*.vm.boot.firmware_format` (the plugin's own firmware boots the box: the other `vm.boot` fields are refused on QEMU) |
| `graceful_timeout` | n/a | No | Vagrant tooling, not the environment's behavior |
| `homebrew_prefix` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `image_path` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `machine` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `mcast_addr` | Yes | Another way | Via socket_opts to the network's hub: any number of VMs without root, on a Mac too, where multicast doesn't send |
| `memory` | Yes | Yes | From `machines.*.resources` |
| `net_device` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `net_mode` | Yes | Yes | From `machines.*.networks: two VMs as a listen/connect pair, any other number through a hub` (a QEMU process with no machine), no root; a VM is on one network (`refusal_on` with `qemu` refuses one on more) |
| `no_daemonize` | n/a | No | Vagrant tooling, not the environment's behavior |
| `other_default` | n/a | No | Vagrant tooling, not the environment's behavior |
| `qemu_bin` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `qemu_dir` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `smp` | Yes | Yes | From `machines.*.resources.cpus` |
| `socket_opts` | Yes | Yes | From `machines.*.networks: two VMs as a listen/connect pair, any other number through a hub` (a QEMU process with no machine), no root; a VM is on one network (`refusal_on` with `qemu` refuses one on more) |
| `socket_vmnet_client` | Yes | Another way | Via socket_opts to the network's hub: the same network between the VMs, without root (a TAP device and the socket_vmnet daemon need it) |
| `socket_vmnet_socket` | Yes | Another way | Via socket_opts to the network's hub: the same network between the VMs, without root (a TAP device and the socket_vmnet daemon need it) |
| `ssh_auto_correct` | Yes | Yes | Always: each VM forwards SSH from its own host port |
| `ssh_host` | n/a | No | Vagrant tooling, not the environment's behavior |
| `ssh_port` | n/a | No | Vagrant tooling, not the environment's behavior |
| `tap_device` | Yes | Another way | Via socket_opts to the network's hub: the same network between the VMs, without root (a TAP device and the socket_vmnet daemon need it) |
| `vmnet_interface` | Yes | Not yet | With `public_network` (a network bridged to the outside); vmnet-bridged needs root on the Mac |

## Vagrant: ESXi

100% of 20 portable features (20 done, 0 partly, 0 to do; 53 features in all). From vagrant-vmware-esxi 2.5.2 (its config class), for standalone ESXi hosts.

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allow_overwrite` | n/a | No | Vagrant tooling, not the environment's behavior |
| `clone_from_vm` | n/a | No | Faster starts from one image: no change in behavior |
| `custom_vmx_settings` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `debug` | n/a | No | Vagrant tooling, not the environment's behavior |
| `encoded_esxi_password` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `esxi_disk_store` | Yes | Yes | From the ESXI_* environment variables (the host is the user's) |
| `esxi_hostname` | Yes | Yes | From the ESXI_* environment variables (the host is the user's) |
| `esxi_hostport` | Yes | Yes | From the ESXI_* environment variables (the host is the user's) |
| `esxi_password` | Yes | Yes | From the ESXI_* environment variables (the host is the user's) |
| `esxi_private_keys` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `esxi_resource_pool` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `esxi_username` | Yes | Yes | From the ESXI_* environment variables (the host is the user's) |
| `esxi_virtual_network` | Yes | Yes | From networks: a port group per network, from ESXI_VIRTUAL_NETWORK |
| `guest_autostart` | n/a | No | Vagrant tooling, not the environment's behavior |
| `guest_boot_disk_size` | Yes | Yes | From `machines.*.resources.disk_gb` (the root filesystem grown into it) |
| `guest_custom_vmx_settings` | Yes | Yes | From `machines.*.nested_virtualization` (`vhv.enable`); no spec field writes raw settings |
| `guest_disk_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `guest_guestos` | Yes | Yes | From `machines.*.vm.os` (a Windows VM's guest OS; a Linux box keeps its own) |
| `guest_mac_address` | No | No | Cloud VMs get their MAC and addresses from the provider |
| `guest_memsize` | Yes | Yes | From `machines.*.resources` |
| `guest_name` | Yes | Yes | From the environment and machine names |
| `guest_name_prefix` | Yes | Another way | Via guest_name: every VM is named (`<env>-<machine>`), so the plugin never makes up a name with a prefix |
| `guest_nic_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `guest_numvcpus` | Yes | Yes | From `machines.*.resources` |
| `guest_snapshot_includememory` | n/a | No | Vagrant tooling, not the environment's behavior |
| `guest_snapshot_quiesced` | n/a | No | Vagrant tooling, not the environment's behavior |
| `guest_storage` | Yes | Another way | Via machines.*.resources.disk_gb and machines.*.volumes (one disk, sized, holding the volumes' paths) |
| `guest_username` | n/a | No | Vagrant tooling, not the environment's behavior |
| `guest_virtualhw_version` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `guestos` | Yes | Another way | Via guest_guestos, the plugin's newer name for it |
| `lax` | n/a | No | Vagrant tooling, not the environment's behavior |
| `local_allow_overwrite` | n/a | No | Vagrant tooling, not the environment's behavior |
| `local_failonwarning` | n/a | No | Vagrant tooling, not the environment's behavior |
| `local_lax` | n/a | No | Vagrant tooling, not the environment's behavior |
| `local_private_keys` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `local_use_ip_cache` | n/a | No | Vagrant tooling, not the environment's behavior |
| `mac_address` | No | No | Cloud VMs get their MAC and addresses from the provider |
| `memsize` | Yes | Another way | Via guest_memsize (the plugin's newer name for it) |
| `nic_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `numvcpus` | Yes | Another way | Via guest_numvcpus (the plugin's newer name for it) |
| `resource_pool` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `saved_ipaddress` | n/a | No | Vagrant tooling, not the environment's behavior |
| `ssh_username` | n/a | No | Vagrant tooling, not the environment's behavior |
| `supported_guest_disk_types` | n/a | No | The plugin's own list of accepted values |
| `supported_guest_guestos` | n/a | No | The plugin's own list of accepted values |
| `supported_guest_nic_types` | n/a | No | The plugin's own list of accepted values |
| `supported_guest_virtualhw_versions` | n/a | No | The plugin's own list of accepted values |
| `virtual_network` | Yes | Another way | Via esxi_virtual_network (the plugin's newer name for it) |
| `virtualhw_version` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `vm_disk_store` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `vm_disk_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `vmname` | Yes | Another way | Via guest_name (the plugin's newer name for it) |
| `vmname_prefix` | Yes | Another way | Via guest_name: every VM is named (`<env>-<machine>`), so the plugin never makes up a name with a prefix |

## Vagrant: Docker

100% of 13 portable features (13 done, 0 partly, 0 to do; 28 features in all). From Vagrant 2.4.9 (the provider's config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `auth_server` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `build_args` | Yes | Another way | Via machines.*.docker.args, in the Compose output |
| `build_dir` | Yes | Another way | Via machines.*.docker.build, in the Compose output |
| `cmd` | Yes | Another way | Via machines.*.command, in the Compose output |
| `compose` | n/a | No | Vagrant tooling, not the environment's behavior |
| `compose_configuration` | n/a | No | Vagrant tooling, not the environment's behavior |
| `create_args` | No | No | Raw commands for one hypervisor: no other target understands them |
| `dockerfile` | Yes | Another way | Via machines.*.docker.dockerfile, in the Compose output |
| `email` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `env` | Yes | Another way | Via machines.*.environment, in the Compose output |
| `expose` | Yes | Another way | Via machines.*.services[].port, in the Compose output |
| `force_host_vm` | n/a | No | Vagrant tooling, not the environment's behavior |
| `git_repo` | Yes | Another way | Via machines.*.docker.build, in the Compose output |
| `has_ssh` | n/a | No | Vagrant tooling, not the environment's behavior |
| `host_vm_build_dir_options` | n/a | No | Vagrant tooling, not the environment's behavior |
| `image` | Yes | Another way | Via machines.*.docker.image, in the Compose output |
| `link` | Yes | Another way | Via networks (machines reach each other by name), in the Compose output |
| `name` | Yes | Another way | Via the environment and machine names, in the Compose output |
| `password` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `ports` | Yes | Another way | Via machines.*.services[].publish, in the Compose output |
| `privileged` | Yes | Another way | Via machines.*.privileged, in the Compose output |
| `pull` | n/a | No | Vagrant tooling, not the environment's behavior |
| `remains_running` | n/a | No | Vagrant tooling, not the environment's behavior |
| `stop_timeout` | n/a | No | Vagrant tooling, not the environment's behavior |
| `username` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `vagrant_machine` | n/a | No | Vagrant tooling, not the environment's behavior |
| `vagrant_vagrantfile` | n/a | No | Vagrant tooling, not the environment's behavior |
| `volumes` | Yes | Another way | Via machines.*.volumes, in the Compose output |

## Terraform: Proxmox

76% of 119 portable features (91 done, 0 partly, 28 to do; 397 features in all). From bpg/proxmox 0.115.0 (its resource types, and the schema of what Isoloom writes).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `proxmox_acl` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_acme_account` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_acme_certificate` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_acme_dns_plugin` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_apt_repository` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_apt_standard_repository` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_backup_job` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_ceph_pool` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_cloned_vm` | n/a | No | Linked clones: faster starts from one image, no change in behavior (as on Hyper-V and ESXi) |
| `proxmox_cluster_options` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_download_file` | Yes | Yes | Written by the proxmox target, a VM per machine and the router, or docker-vm's Proxmox module |
| `proxmox_hagroup` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_hardware_mapping_dir` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_hardware_mapping_pci` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_hardware_mapping_usb` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_haresource` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_harule` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_metrics_server` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_network_applier` | No | No | Changes the server's own network: no other target has one to change |
| `proxmox_network_linux_bond` | No | No | Changes the server's own network: no other target has one to change |
| `proxmox_network_linux_bridge` | No | No | Changes the server's own network: no other target has one to change |
| `proxmox_network_linux_vlan` | No | No | Changes the server's own network: no other target has one to change |
| `proxmox_node_config` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_node_disk_zfs` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_node_firewall` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_oci_image` | Yes | Not yet | LXC containers: a way to run `docker:` machines on Proxmox |
| `proxmox_pool_membership` | Yes | Not yet | With the Proxmox generator |
| `proxmox_realm_ldap` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_realm_openid` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_realm_sync` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_replication` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_sdn_applier` | Yes | Yes | Written by the proxmox target, a VM per machine and the router, or docker-vm's Proxmox module |
| `proxmox_sdn_controller_evpn` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_fabric_node_openfabric` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_fabric_node_ospf` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_fabric_openfabric` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_fabric_ospf` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_subnet` | Yes | Another way | Via the router VM, which holds each network's router address (a subnet would put it on the host bridge, which routes between networks) |
| `proxmox_sdn_vnet` | Yes | Yes | Written by the proxmox target, a VM per machine and the router, or docker-vm's Proxmox module |
| `proxmox_sdn_zone_evpn` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_zone_qinq` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_zone_simple` | Yes | Yes | Written by the proxmox target, a VM per machine and the router, or docker-vm's Proxmox module |
| `proxmox_sdn_zone_vlan` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_zone_vxlan` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_storage_cifs` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_storage_directory` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_storage_lvm` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_storage_lvmthin` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_storage_nfs` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_storage_pbs` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_storage_zfspool` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_user_token` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_acl` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_acme_account` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_acme_certificate` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_acme_dns_plugin` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_apt_repository` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_apt_standard_repository` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_certificate` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_cloned_vm` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_cluster_firewall` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_cluster_firewall_security_group` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_cluster_options` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_container` | Yes | Not yet | LXC containers: a way to run `docker:` machines on Proxmox |
| `proxmox_virtual_environment_dns` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_download_file` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_file` | Yes | Yes | Written by the proxmox target, a VM per machine and the router, or docker-vm's Proxmox module |
| `proxmox_virtual_environment_firewall_alias` | Yes | Not yet | The Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_firewall_ipset` | Yes | Not yet | The Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_firewall_options` | Yes | Not yet | The Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_firewall_rules` | Yes | Not yet | The Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_group` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_hagroup` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_hardware_mapping_dir` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_hardware_mapping_pci` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_hardware_mapping_usb` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_haresource` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_harule` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_hosts` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_metrics_server` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_network_linux_bridge` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_network_linux_vlan` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_node_firewall` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_oci_image` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_pool` | Yes | Not yet | With the Proxmox generator |
| `proxmox_virtual_environment_pool_membership` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_realm_ldap` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_realm_openid` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_realm_sync` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_replication` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_role` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_sdn_applier` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_fabric_node_openfabric` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_fabric_node_ospf` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_fabric_openfabric` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_fabric_ospf` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_subnet` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_vnet` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_zone_evpn` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_zone_qinq` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_zone_simple` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_zone_vlan` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_sdn_zone_vxlan` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_storage_cifs` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_storage_directory` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_storage_lvm` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_storage_lvmthin` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_storage_nfs` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_storage_pbs` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_storage_zfspool` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_time` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_user` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_user_token` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `proxmox_virtual_environment_vm` | Yes | Yes | Written by the proxmox target, a VM per machine and the router, or docker-vm's Proxmox module |
| `proxmox_virtual_environment_vm2` | n/a | No | An experimental VM resource; Isoloom uses proxmox_virtual_environment_vm |
| `proxmox_vm` | n/a | No | An experimental VM resource; Isoloom uses proxmox_virtual_environment_vm |

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `api_headers` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `api_token` | Yes | Yes | Written by the generator |
| `auth_ticket` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `csrf_prevention_token` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoint` | Yes | Yes | Written by the generator |
| `insecure` | Yes | Yes | Written by the generator |
| `min_tls` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `otp` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `password` | Yes | Yes | Written by the generator |
| `random_vm_id_end` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `random_vm_id_start` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `random_vm_ids` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `tmp_dir` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `username` | Yes | Yes | Written by the generator |
| `ssh` | Yes | Yes | Written by the generator |
| `ssh.agent` | Yes | Yes | Written by the generator |
| `ssh.agent_forwarding` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ssh.agent_socket` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ssh.node_address_source` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ssh.password` | Yes | Yes | Written by the generator |
| `ssh.private_key` | Yes | Yes | Written by the generator |
| `ssh.socks5_password` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ssh.socks5_server` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ssh.socks5_username` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ssh.username` | Yes | Yes | Written by the generator |
| `ssh.node` | Yes | Yes | Written by the generator |
| `ssh.node.address` | Yes | Yes | Written by the generator |
| `ssh.node.name` | Yes | Yes | Written by the generator |
| `ssh.node.port` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |

### proxmox_download_file

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `checksum` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `checksum_algorithm` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `content_type` | Yes | Yes | Written by the generator |
| `datastore_id` | Yes | Yes | Written by the generator |
| `decompression_algorithm` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `file_name` | Yes | Yes | Written by the generator |
| `node_name` | Yes | Yes | Written by the generator |
| `overwrite` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `overwrite_unmanaged` | Yes | Yes | Written by the generator |
| `upload_timeout` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `url` | Yes | Yes | Written by the generator |
| `verify` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |

### proxmox_sdn_applier

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `on_create` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `on_destroy` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |

### proxmox_sdn_vnet

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `alias` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `id` | Yes | Yes | Written by the generator |
| `isolate_ports` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `tag` | Yes | Not yet | With VLANs and trunks (`networks.*.vlans`) |
| `vlan_aware` | Yes | Not yet | With VLANs and trunks (`networks.*.vlans`) |
| `zone` | Yes | Yes | Written by the generator |

### proxmox_sdn_zone_simple

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `dhcp` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `dns` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `dns_zone` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `id` | Yes | Yes | Written by the generator |
| `ipam` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `mtu` | Yes | Not yet | A network's MTU |
| `nodes` | Yes | Yes | Written by the generator |
| `reverse_dns` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |

### proxmox_virtual_environment_file

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `content_type` | Yes | Yes | Written by the generator |
| `datastore_id` | Yes | Yes | Written by the generator |
| `file_mode` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `node_name` | Yes | Yes | Written by the generator |
| `overwrite` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `timeout_upload` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `upload_mode` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_file` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_file.changed` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_file.checksum` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_file.file_name` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_file.insecure` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_file.min_tls` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_file.path` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_raw` | Yes | Yes | Written by the generator |
| `source_raw.data` | Yes | Yes | Written by the generator |
| `source_raw.file_name` | Yes | Yes | Written by the generator |
| `source_raw.resize` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |

### proxmox_virtual_environment_vm

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `acpi` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `bios` | Yes | Yes | Written by the generator |
| `boot_order` | Yes | Yes | Written by the generator |
| `delete_unreferenced_disks_on_destroy` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `hook_script_file_id` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `hotplug` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `keyboard_layout` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `kvm_arguments` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `mac_addresses` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `machine` | Yes | Yes | Written by the generator |
| `migrate` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `name` | Yes | Yes | Written by the generator |
| `network_device` | Yes | Yes | Written by the generator |
| `network_device.bridge` | Yes | Yes | Written by the generator |
| `network_device.disconnected` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `network_device.enabled` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `network_device.firewall` | Yes | Not yet | The Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `network_device.mac_address` | Yes | Yes | Written by the generator |
| `network_device.model` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `network_device.mtu` | Yes | Not yet | A network's MTU |
| `network_device.queues` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `network_device.rate_limit` | Yes | Not yet | A rate limit per machine, beside `networks.*.tc` (a whole network, on its router) |
| `network_device.trunks` | Yes | Not yet | With VLANs and trunks (`networks.*.vlans`) |
| `network_device.vlan_id` | Yes | Not yet | With VLANs and trunks (`networks.*.vlans`) |
| `node_name` | Yes | Yes | Written by the generator |
| `on_boot` | Yes | Yes | Written by the generator |
| `pool_id` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `protection` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `purge_on_destroy` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `reboot` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `reboot_after_update` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `scsi_hardware` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `started` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `stop_on_destroy` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `tablet_device` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `tags` | Yes | Yes | Written by the generator |
| `template` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `timeout_clone` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `timeout_create` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `timeout_migrate` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `timeout_move_disk` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `timeout_reboot` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `timeout_shutdown_vm` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `timeout_start_vm` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `timeout_stop_vm` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `vm_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `agent` | Yes | Yes | Written by the generator |
| `agent.enabled` | Yes | Yes | Written by the generator |
| `agent.timeout` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `agent.trim` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `agent.type` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `agent.wait_for_ip` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `agent.wait_for_ip.disabled` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `agent.wait_for_ip.ipv4` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `agent.wait_for_ip.ipv6` | Yes | Another way | Via networks.*.ipv6: a set-up step gives each VM its IPv6 address, the same on every VM target (not run on a real host) |
| `amd_sev` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `amd_sev.allow_smt` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `amd_sev.kernel_hashes` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `amd_sev.no_debug` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `amd_sev.no_key_sharing` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `amd_sev.type` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `audio_device` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `audio_device.device` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `audio_device.driver` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `audio_device.enabled` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `cdrom` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `cdrom.enabled` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `cdrom.file_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `cdrom.interface` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `clone` | Yes | Yes | Written by the generator |
| `clone.datastore_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `clone.full` | Yes | Yes | Written by the generator |
| `clone.node_name` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `clone.retries` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `clone.vm_id` | Yes | Yes | Written by the generator |
| `cpu` | Yes | Yes | Written by the generator |
| `cpu.affinity` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `cpu.architecture` | Yes | Not yet | With `machines.*.arch`: another CPU architecture |
| `cpu.cores` | Yes | Yes | Written by the generator |
| `cpu.flags` | Yes | Not yet | With `machines.*.arch`: another CPU architecture |
| `cpu.hotplugged` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `cpu.limit` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `cpu.numa` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `cpu.sockets` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `cpu.type` | Yes | Yes | Written by the generator |
| `cpu.units` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk` | Yes | Yes | Written by the generator |
| `disk.aio` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.backup` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.cache` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.datastore_id` | Yes | Yes | Written by the generator |
| `disk.discard` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.file_format` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.file_id` | Yes | Yes | Written by the generator |
| `disk.import_from` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.interface` | Yes | Yes | Written by the generator |
| `disk.iothread` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.path_in_datastore` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.queues` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.replicate` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.serial` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.size` | Yes | Yes | Written by the generator |
| `disk.ssd` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.speed` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.speed.iops_read` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.speed.iops_read_burstable` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.speed.iops_write` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.speed.iops_write_burstable` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.speed.read` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.speed.read_burstable` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.speed.write` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `disk.speed.write_burstable` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `efi_disk` | Yes | Yes | Written by the generator |
| `efi_disk.datastore_id` | Yes | Yes | Written by the generator |
| `efi_disk.file_format` | Yes | Yes | Written by the generator |
| `efi_disk.pre_enrolled_keys` | Yes | Yes | Written by the generator |
| `efi_disk.type` | Yes | Yes | Written by the generator |
| `hostpci` | Yes | Yes | From `machines.*.gpus.ids` (a PCIe device per id: a PCI address, or a resource mapping) |
| `hostpci.device` | Yes | Yes | From `machines.*.gpus.ids` (a PCIe device per id: a PCI address, or a resource mapping) |
| `hostpci.id` | Yes | Yes | From `machines.*.gpus.ids` (a PCIe device per id: a PCI address, or a resource mapping) |
| `hostpci.mapping` | Yes | Yes | From `machines.*.gpus.ids` (a PCIe device per id: a PCI address, or a resource mapping) |
| `hostpci.mdev` | No | No | How the Proxmox server hands its PCI device over (a vGPU slice, a ROM, the primary display): no other target exposes it |
| `hostpci.pcie` | Yes | Yes | From `machines.*.gpus.ids` (a PCIe device per id: a PCI address, or a resource mapping) |
| `hostpci.rom_file` | No | No | How the Proxmox server hands its PCI device over (a vGPU slice, a ROM, the primary display): no other target exposes it |
| `hostpci.rombar` | No | No | How the Proxmox server hands its PCI device over (a vGPU slice, a ROM, the primary display): no other target exposes it |
| `hostpci.xvga` | No | No | How the Proxmox server hands its PCI device over (a vGPU slice, a ROM, the primary display): no other target exposes it |
| `initialization` | Yes | Yes | Written by the generator |
| `initialization.datastore_id` | Yes | Yes | Written by the generator |
| `initialization.file_format` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `initialization.interface` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `initialization.meta_data_file_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `initialization.network_data_file_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `initialization.type` | Yes | Yes | Written by the generator |
| `initialization.upgrade` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `initialization.user_data_file_id` | Yes | Yes | Written by the generator |
| `initialization.vendor_data_file_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `initialization.dns` | Yes | Yes | Written by the generator |
| `initialization.dns.domain` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `initialization.dns.servers` | Yes | Yes | Written by the generator |
| `initialization.ip_config` | Yes | Yes | Written by the generator |
| `initialization.ip_config.ipv4` | Yes | Yes | Written by the generator |
| `initialization.ip_config.ipv4.address` | Yes | Yes | Written by the generator |
| `initialization.ip_config.ipv4.gateway` | Yes | Yes | Written by the generator |
| `initialization.ip_config.ipv6` | Yes | Another way | Via networks.*.ipv6: a set-up step gives each VM its IPv6 address, the same on every VM target (not run on a real host) |
| `initialization.ip_config.ipv6.address` | Yes | Another way | Via networks.*.ipv6: a set-up step gives each VM its IPv6 address, the same on every VM target (not run on a real host) |
| `initialization.ip_config.ipv6.gateway` | Yes | Another way | Via networks.*.ipv6: a set-up step gives each VM its IPv6 address, the same on every VM target (not run on a real host) |
| `initialization.user_account` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `initialization.user_account.keys` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `initialization.user_account.password` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `initialization.user_account.username` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `memory` | Yes | Yes | Written by the generator |
| `memory.dedicated` | Yes | Yes | Written by the generator |
| `memory.floating` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `memory.hugepages` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `memory.keep_hugepages` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `memory.shared` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `numa` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `numa.cpus` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `numa.device` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `numa.hostnodes` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `numa.memory` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `numa.policy` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `operating_system` | Yes | Yes | Written by the generator |
| `operating_system.type` | Yes | Yes | Written by the generator |
| `rng` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `rng.max_bytes` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `rng.period` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `rng.source` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `serial_device` | Yes | Yes | Written by the generator |
| `serial_device.device` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `smbios` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `smbios.family` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `smbios.manufacturer` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `smbios.product` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `smbios.serial` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `smbios.sku` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `smbios.uuid` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `smbios.version` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `startup` | n/a | No | The start order when the server restarts: `depends_on` orders the first start |
| `startup.down_delay` | n/a | No | The start order when the server restarts: `depends_on` orders the first start |
| `startup.order` | n/a | No | The start order when the server restarts: `depends_on` orders the first start |
| `startup.up_delay` | n/a | No | The start order when the server restarts: `depends_on` orders the first start |
| `tpm_state` | Yes | Yes | Written by the generator |
| `tpm_state.datastore_id` | Yes | Yes | Written by the generator |
| `tpm_state.version` | Yes | Yes | Written by the generator |
| `usb` | No | No | A host's USB device for a VM: `devices` reach containers only (a VM has its own virtual devices), and cloud VMs have none |
| `usb.host` | No | No | A host's USB device for a VM: `devices` reach containers only (a VM has its own virtual devices), and cloud VMs have none |
| `usb.mapping` | No | No | A host's USB device for a VM: `devices` reach containers only (a VM has its own virtual devices), and cloud VMs have none |
| `usb.usb3` | No | No | A host's USB device for a VM: `devices` reach containers only (a VM has its own virtual devices), and cloud VMs have none |
| `vga` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `vga.clipboard` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `vga.memory` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `vga.type` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `virtiofs` | Yes | Not yet | Shared volumes (`volumes`) as host folders |
| `virtiofs.cache` | Yes | Not yet | Shared volumes (`volumes`) as host folders |
| `virtiofs.direct_io` | Yes | Not yet | Shared volumes (`volumes`) as host folders |
| `virtiofs.expose_acl` | Yes | Not yet | Shared volumes (`volumes`) as host folders |
| `virtiofs.expose_xattr` | Yes | Not yet | Shared volumes (`volumes`) as host folders |
| `virtiofs.mapping` | Yes | Not yet | Shared volumes (`volumes`) as host folders |
| `watchdog` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `watchdog.action` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `watchdog.enabled` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |
| `watchdog.model` | No | No | Tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it |

## Terraform: ESXi

0% of 4 portable features (0 done, 0 partly, 4 to do; 5 features in all). From josenk/esxi 1.10.3 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `esxi_guest` | Yes | Not yet | With the ESXi generator: a VM per machine, a port group per network |
| `esxi_portgroup` | Yes | Not yet | With the ESXi generator: a VM per machine, a port group per network |
| `esxi_resource_pool` | n/a | No | Where the host places the VMs: no change in behavior |
| `esxi_virtual_disk` | Yes | Not yet | With the ESXi generator: a VM per machine, a port group per network |
| `esxi_vswitch` | Yes | Not yet | With the ESXi generator: a VM per machine, a port group per network |

## Terraform: AWS

73% of 132 portable features (97 done, 0 partly, 35 to do; 2438 features in all). From hashicorp/aws 6.67.0 (its resource types, and the schema of what Isoloom writes).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `aws_accessanalyzer_analyzer` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_accessanalyzer_archive_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_account_alternate_contact` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_account_primary_contact` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_account_region` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_accountaccess_application` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_accountaccess_entitlement` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ami` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `aws_ami_copy` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `aws_ami_from_instance` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `aws_ami_launch_permission` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_arcregionswitch_plan` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_arczonalshift_autoshift_observer_notification_status` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_arczonalshift_zonal_autoshift_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_auditmanager_account_registration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_auditmanager_assessment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_auditmanager_assessment_delegation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_auditmanager_assessment_report` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_auditmanager_control` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_auditmanager_framework` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_auditmanager_framework_share` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_auditmanager_organization_admin_account_registration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_autoscaling_attachment` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_autoscaling_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_autoscaling_group_tag` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_autoscaling_lifecycle_hook` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_autoscaling_notification` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_autoscaling_policy` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_autoscaling_schedule` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_autoscaling_traffic_source_attachment` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_autoscalingplans_scaling_plan` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_backup_framework` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_global_settings` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_logically_air_gapped_vault` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_plan` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_region_settings` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_report_plan` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_restore_testing_plan` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_restore_testing_selection` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_selection` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_vault` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_vault_lock_configuration` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_vault_notifications` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_backup_vault_policy` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_billing_view` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_budgets_budget` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_budgets_budget_action` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ce_anomaly_monitor` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ce_anomaly_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ce_cost_allocation_tag` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ce_cost_category` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_chatbot_slack_channel_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_chatbot_teams_channel_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudcontrolapi_resource` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudformation_stack` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudformation_stack_instances` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudformation_stack_set` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudformation_stack_set_instance` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudformation_type` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudtrail` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudtrail_event_data_store` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudtrail_organization_delegated_admin_account` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_alarm_mute_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_composite_alarm` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_contributor_insight_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_contributor_managed_insight_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_dashboard` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_event_api_destination` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_event_archive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_event_bus` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_event_bus_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_event_connection` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_event_endpoint` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_event_permission` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_event_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_event_target` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_account_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_anomaly_detector` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_data_protection_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_delivery` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_delivery_destination` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_delivery_destination_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_delivery_source` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_destination` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_destination_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_index_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_metric_filter` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_resource_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_s3_table_integration_source` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_storage_tier_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_stream` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_subscription_filter` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_log_transformer` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_metric_alarm` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_metric_stream` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_otel_enrichment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cloudwatch_query_definition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_computeoptimizer_enrollment_status` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_computeoptimizer_recommendation_preferences` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_aggregate_authorization` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_config_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_configuration_aggregator` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_configuration_recorder` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_configuration_recorder_status` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_conformance_pack` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_delivery_channel` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_organization_conformance_pack` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_organization_custom_policy_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_organization_custom_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_organization_managed_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_remediation_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_config_retention_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_controltower_baseline` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_controltower_control` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_controltower_landing_zone` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_costoptimizationhub_enrollment_status` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_costoptimizationhub_preferences` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_cur_report_definition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_default_network_acl` | n/a | No | The account's default network: Isoloom builds its own |
| `aws_default_route_table` | n/a | No | The account's default network: Isoloom builds its own |
| `aws_default_security_group` | n/a | No | The account's default network: Isoloom builds its own |
| `aws_default_subnet` | n/a | No | The account's default network: Isoloom builds its own |
| `aws_default_vpc` | n/a | No | The account's default network: Isoloom builds its own |
| `aws_default_vpc_dhcp_options` | n/a | No | The account's default network: Isoloom builds its own |
| `aws_dlm_lifecycle_policy` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_drs_replication_configuration_template` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_ebs_default_kms_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ebs_encryption_by_default` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ebs_fast_snapshot_restore` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_ebs_snapshot` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_ebs_snapshot_block_public_access` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ebs_snapshot_copy` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_ebs_snapshot_import` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `aws_ebs_volume` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_ebs_volume_copy` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_ec2_allowed_images_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ec2_availability_zone_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_ec2_capacity_block_reservation` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_ec2_capacity_reservation` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_ec2_default_credit_specification` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_ec2_fleet` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_ec2_host` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_ec2_image_block_public_access` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ec2_instance_metadata_defaults` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ec2_instance_state` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_ec2_serial_console_access` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ec2_subnet_cidr_reservation` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_ec2_tag` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_egress_only_internet_gateway` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_eip` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_eip_association` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_flow_log` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_fms_admin_account` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_fms_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_fms_resource_set` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_access_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_account_alias` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_account_password_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_group_membership` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_group_policies_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_group_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_group_policy_attachment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_group_policy_attachments_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_instance_profile` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_openid_connect_provider` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_organizations_features` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_outbound_web_identity_federation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_policy_attachment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_role` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_role_policies_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_role_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_role_policy_attachment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_role_policy_attachments_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_saml_provider` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_security_token_service_preferences` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_server_certificate` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_service_linked_role` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_service_specific_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_signing_certificate` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_user` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_user_group_membership` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_user_login_profile` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_user_policies_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_user_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_user_policy_attachment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_user_policy_attachments_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_user_ssh_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_iam_virtual_mfa_device` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_identitystore_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_identitystore_group_membership` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_identitystore_user` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_imagebuilder_component` | n/a | No | Builds machine images: how an image is made, not what the environment runs |
| `aws_imagebuilder_container_recipe` | n/a | No | Builds machine images: how an image is made, not what the environment runs |
| `aws_imagebuilder_distribution_configuration` | n/a | No | Builds machine images: how an image is made, not what the environment runs |
| `aws_imagebuilder_image` | n/a | No | Builds machine images: how an image is made, not what the environment runs |
| `aws_imagebuilder_image_pipeline` | n/a | No | Builds machine images: how an image is made, not what the environment runs |
| `aws_imagebuilder_image_recipe` | n/a | No | Builds machine images: how an image is made, not what the environment runs |
| `aws_imagebuilder_infrastructure_configuration` | n/a | No | Builds machine images: how an image is made, not what the environment runs |
| `aws_imagebuilder_lifecycle_policy` | n/a | No | Builds machine images: how an image is made, not what the environment runs |
| `aws_imagebuilder_workflow` | n/a | No | Builds machine images: how an image is made, not what the environment runs |
| `aws_instance` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_internet_gateway` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_internet_gateway_attachment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_internetmonitor_monitor` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_invoicing_invoice_unit` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_key_pair` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_launch_configuration` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_launch_template` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_licensemanager_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_licensemanager_grant` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_licensemanager_grant_accepter` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_licensemanager_license_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_main_route_table_association` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_nat_gateway` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_nat_gateway_eip_association` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_network_acl` | Yes | Another way | Via the `ingress` and `egress` blocks of aws_security_group, which carry the reach rules |
| `aws_network_acl_association` | Yes | Another way | Via the `ingress` and `egress` blocks of aws_security_group, which carry the reach rules |
| `aws_network_acl_rule` | Yes | Another way | Via the `ingress` and `egress` blocks of aws_security_group, which carry the reach rules |
| `aws_network_interface` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_network_interface_attachment` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_network_interface_sg_attachment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_networkflowmonitor_monitor` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_networkflowmonitor_scope` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_networkmonitor_monitor` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_networkmonitor_probe` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_notifications_channel_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_notifications_event_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_notifications_managed_notification_account_contact_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_notifications_managed_notification_additional_channel_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_notifications_notification_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_notifications_notification_hub` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_notifications_organizational_unit_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_notifications_organizations_access` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_notificationscontacts_email_contact` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_oam_link` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_oam_sink` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_oam_sink_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_observabilityadmin_centralization_rule_for_organization` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_observabilityadmin_s3_table_integration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_observabilityadmin_telemetry_enrichment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_observabilityadmin_telemetry_evaluation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_observabilityadmin_telemetry_evaluation_for_organization` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_observabilityadmin_telemetry_pipeline` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_observabilityadmin_telemetry_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_observabilityadmin_telemetry_rule_for_organization` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_organizations_account` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_organizations_aws_service_access` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_organizations_delegated_administrator` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_organizations_organization` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_organizations_organizational_unit` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_organizations_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_organizations_policy_attachment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_organizations_resource_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_organizations_tag` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_placement_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_ram_permission` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ram_principal_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ram_resource_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ram_resource_share` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ram_resource_share_accepter` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ram_resource_share_associations_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ram_sharing_with_organization` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_rbin_rule` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_resiliencehub_resiliency_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resiliencehubv2_assertion` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resiliencehubv2_input_source` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resiliencehubv2_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resiliencehubv2_service` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resiliencehubv2_service_function` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resiliencehubv2_system` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resiliencehubv2_user_journey` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resourceexplorer2_index` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resourceexplorer2_view` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resourcegroups_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_resourcegroups_resource` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_rolesanywhere_profile` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_rolesanywhere_trust_anchor` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_route` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_route53_query_log` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_route53_record` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `aws_route53_records_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_route53_vpc_association_authorization` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `aws_route53_zone` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `aws_route53_zone_association` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `aws_route_table` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_route_table_association` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_savingsplans_savings_plan` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_security_group` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_security_group_rule` | Yes | Another way | Via the `ingress` and `egress` blocks of aws_security_group, which carry the reach rules |
| `aws_servicecatalog_budget_resource_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_constraint` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_organizations_access` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_portfolio` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_portfolio_share` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_principal_portfolio_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_product` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_product_portfolio_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_provisioned_product` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_provisioning_artifact` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_service_action` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_tag_option` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalog_tag_option_resource_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalogappregistry_application` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalogappregistry_attribute_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicecatalogappregistry_attribute_group_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicequotas_auto_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicequotas_service_quota` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicequotas_template` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_servicequotas_template_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_snapshot_create_volume_permission` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `aws_spot_datafeed_subscription` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_spot_fleet_request` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_spot_instance_request` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `aws_ssoadmin_account_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_application` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_application_access_scope` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_application_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_application_assignment_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_customer_managed_policy_attachment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_customer_managed_policy_attachments_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_instance_access_control_attributes` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_managed_policy_attachment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_managed_policy_attachments_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_permission_set` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_permission_set_inline_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_permissions_boundary_attachment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_region` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_ssoadmin_trusted_token_issuer` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_subnet` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_uxc_account_customizations` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_volume_attachment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_vpc` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_vpc_block_public_access_exclusion` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_block_public_access_options` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_dhcp_options` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_vpc_dhcp_options_association` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_vpc_encryption_control` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_ipam` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_ipam_organization_admin_account` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_ipam_pool` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_ipam_pool_cidr` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_ipam_pool_cidr_allocation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_ipam_preview_next_cidr` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_ipam_resource_discovery` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_ipam_resource_discovery_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_ipam_scope` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_ipv4_cidr_block_association` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_vpc_ipv6_cidr_block_association` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `aws_vpc_network_performance_metric_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `aws_vpc_security_group_egress_rule` | Yes | Another way | Via the `ingress` and `egress` blocks of aws_security_group, which carry the reach rules |
| `aws_vpc_security_group_ingress_rule` | Yes | Another way | Via the `ingress` and `egress` blocks of aws_security_group, which carry the reach rules |
| `aws_vpc_security_group_rules_exclusive` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |

### Data sources

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `aws_ami` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_availability_zones` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `access_key` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `allowed_account_ids` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `custom_ca_bundle` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ec2_metadata_service_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ec2_metadata_service_endpoint_mode` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `forbidden_account_ids` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `http_proxy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `https_proxy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `insecure` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `max_retries` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `no_proxy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `profile` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `region` | Yes | Yes | Written by the generator |
| `retry_mode` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `s3_us_east_1_regional_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `s3_use_path_style` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `secret_key` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `shared_config_files` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `shared_credentials_files` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `skip_credentials_validation` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `skip_metadata_api_check` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `skip_region_validation` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `skip_requesting_account_id` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `sts_region` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `tag_policy_compliance` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token_bucket_rate_limiter_capacity` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `use_dualstack_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `use_fips_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `user_agent` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role.duration` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role.external_id` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role.policy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role.policy_arns` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role.role_arn` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role.session_name` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role.source_identity` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role.tags` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role.transitive_tag_keys` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role_with_web_identity` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role_with_web_identity.duration` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role_with_web_identity.policy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role_with_web_identity.policy_arns` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role_with_web_identity.role_arn` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role_with_web_identity.session_name` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role_with_web_identity.web_identity_token` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assume_role_with_web_identity.web_identity_token_file` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `default_tags` | Yes | Yes | Written by the generator |
| `default_tags.tags` | Yes | Yes | Written by the generator |
| `endpoints` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.accessanalyzer` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.account` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.accountaccess` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.acm` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.acmpca` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.agentregistry` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.amg` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.amp` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.amplify` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.apigateway` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.apigatewayv2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.appautoscaling` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.appconfig` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.appfabric` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.appflow` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.appintegrations` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.appintegrationsservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.applicationautoscaling` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.applicationinsights` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.applicationsignals` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.appmesh` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.appregistry` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.apprunner` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.appstream` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.appsync` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.arcregionswitch` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.arczonalshift` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.athena` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.auditmanager` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.autoscaling` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.autoscalingplans` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.backup` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.batch` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.bcmdataexports` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.beanstalk` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.bedrock` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.bedrockagent` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.bedrockagentcore` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.bedrockruntime` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.billing` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.budgets` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ce` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.chatbot` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.chime` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.chimesdkmediapipelines` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.chimesdkvoice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cleanrooms` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloud9` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudcontrol` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudcontrolapi` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudformation` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudfront` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudfrontkeyvaluestore` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudhsm` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudhsmv2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudsearch` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudtrail` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudwatch` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudwatchevents` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudwatchevidently` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudwatchlog` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudwatchlogs` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudwatchobservabilityaccessmanager` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cloudwatchrum` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codeartifact` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codebuild` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codecatalyst` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codecommit` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codeconnections` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codedeploy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codeguruprofiler` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codegurureviewer` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codepipeline` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codestarconnections` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.codestarnotifications` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cognitoidentity` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cognitoidentityprovider` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cognitoidp` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.comprehend` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.computeoptimizer` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.config` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.configservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.connect` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.connectcases` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.controltower` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.costandusagereportservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.costexplorer` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.costoptimizationhub` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.cur` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.customerprofiles` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.databasemigration` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.databasemigrationservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.databrew` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.dataexchange` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.datapipeline` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.datasync` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.datazone` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.dax` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.deploy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.detective` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.devicefarm` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.devopsagent` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.devopsguru` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.directconnect` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.directoryservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.directoryservicedata` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.dlm` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.dms` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.docdb` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.docdbelastic` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.drs` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ds` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.dsql` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.dynamodb` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ec2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ecr` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ecrpublic` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ecs` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.efs` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.eks` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.elasticache` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.elasticbeanstalk` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.elasticloadbalancing` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.elasticloadbalancingv2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.elasticsearch` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.elasticsearchservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.elastictranscoder` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.elb` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.elbv2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.emr` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.emrcontainers` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.emrserverless` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.es` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.eventbridge` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.events` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.evidently` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.evs` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.finspace` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.firehose` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.fis` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.fms` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.fsx` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.gamelift` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.glacier` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.globalaccelerator` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.glue` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.gluedatabrew` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.grafana` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.greengrass` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.groundstation` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.guardduty` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.healthlake` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.iam` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.identitystore` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.imagebuilder` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.inspector` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.inspector2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.inspectorv2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.interconnect` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.internetmonitor` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.invoicing` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.iot` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ivs` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ivschat` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.kafka` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.kafkaconnect` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.kendra` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.keyspaces` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.kinesis` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.kinesisanalytics` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.kinesisanalyticsv2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.kinesisvideo` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.kms` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lakeformation` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lambda` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lambdacore` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lambdamicrovms` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.launchwizard` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lex` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lexmodelbuilding` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lexmodelbuildingservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lexmodels` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lexmodelsv2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lexv2models` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.licensemanager` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.lightsail` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.location` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.locationservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.logs` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.m2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.macie2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mailmanager` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.managedgrafana` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mediaconnect` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mediaconvert` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.medialive` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mediapackage` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mediapackagev2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mediapackagevod` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mediastore` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.memorydb` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mgn` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mpa` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mq` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.msk` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mwaa` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.mwaaserverless` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.neptune` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.neptunegraph` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.networkfirewall` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.networkflowmonitor` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.networkmanager` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.networkmonitor` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.notifications` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.notificationscontacts` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.oam` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.observabilityadmin` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.odb` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.opensearch` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.opensearchingestion` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.opensearchserverless` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.opensearchservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.organizations` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.osis` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.outposts` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.paymentcryptography` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.pcaconnectorad` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.pcs` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.pinpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.pinpointsmsvoicev2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.pipes` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.polly` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.pricing` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.prometheus` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.prometheusservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.qbusiness` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.qldb` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.quicksight` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ram` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.rbin` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.rds` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.rdsdata` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.rdsdataservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.recyclebin` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.redshift` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.redshiftdata` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.redshiftdataapiservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.redshiftserverless` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.rekognition` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.resiliencehub` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.resiliencehubv2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.resourceexplorer2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.resourcegroups` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.resourcegroupstagging` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.resourcegroupstaggingapi` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.rolesanywhere` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.route53` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.route53domains` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.route53profiles` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.route53recoverycontrolconfig` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.route53recoveryreadiness` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.route53resolver` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.rum` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.s3` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.s3api` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.s3control` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.s3files` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.s3outposts` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.s3tables` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.s3vectors` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.sagemaker` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.savingsplans` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.scheduler` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.schemas` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.secretsmanager` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.securityhub` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.securitylake` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.serverlessapplicationrepository` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.serverlessapprepo` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.serverlessrepo` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.servicecatalog` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.servicecatalogappregistry` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.servicediscovery` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.servicequotas` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ses` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.sesv2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.sfn` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.shield` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.signer` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.sns` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.sqs` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ssm` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ssmcontacts` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ssmincidents` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ssmquicksetup` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ssmsap` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.sso` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.ssoadmin` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.stepfunctions` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.storagegateway` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.sts` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.swf` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.synthetics` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.taxsettings` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.timestreaminfluxdb` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.timestreamquery` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.timestreamwrite` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.transcribe` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.transcribeservice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.transfer` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.uxc` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.verifiedpermissions` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.vpclattice` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.waf` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.wafregional` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.wafv2` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.wellarchitected` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.workmail` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.workspaces` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.workspacesweb` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `endpoints.xray` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ignore_tags` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ignore_tags.key_prefixes` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ignore_tags.keys` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |

### aws_eip

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `address` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `associate_with_private_ip` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `customer_owned_ipv4_pool` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `domain` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `instance` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `ipam_pool_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_border_group` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `network_interface` | Yes | Yes | Written by the generator |
| `public_ipv4_pool` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### aws_instance

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `ami` | Yes | Yes | Written by the generator |
| `associate_public_ip_address` | Yes | Yes | Written by the generator |
| `availability_zone` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `disable_api_stop` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `disable_api_termination` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `ebs_optimized` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `enable_primary_ipv6` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `force_destroy` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `get_password_data` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `hibernation` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `host_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `host_resource_group_arn` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `iam_instance_profile` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `instance_initiated_shutdown_behavior` | Yes | Yes | Written by the generator |
| `instance_type` | Yes | Yes | Written by the generator |
| `ipv6_address_count` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_addresses` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `key_name` | Yes | Yes | Written by the generator |
| `monitoring` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `placement_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `placement_group_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `placement_partition_number` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `private_ip` | Yes | Yes | Written by the generator |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `secondary_private_ips` | Yes | Not yet | With several addresses on one network |
| `security_groups` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_dest_check` | Yes | Not yet | With `networks.*.gateway`: a network routed through one of its machines |
| `subnet_id` | Yes | Yes | Written by the generator |
| `tags` | Yes | Yes | Written by the generator |
| `tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `tenancy` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `user_data` | Yes | Yes | Written by the generator |
| `user_data_base64` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `user_data_replace_on_change` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `volume_tags` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `vpc_security_group_ids` | Yes | Yes | Written by the generator |
| `capacity_reservation_specification` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `capacity_reservation_specification.capacity_reservation_preference` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `capacity_reservation_specification.capacity_reservation_target` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `capacity_reservation_specification.capacity_reservation_target.capacity_reservation_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `capacity_reservation_specification.capacity_reservation_target.capacity_reservation_resource_group_arn` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `cpu_options` | Yes | Yes | From `machines.*.nested_virtualization` |
| `cpu_options.amd_sev_snp` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `cpu_options.core_count` | Yes | Another way | Via `resources.cpus`, through the machine type Isoloom picks |
| `cpu_options.nested_virtualization` | Yes | Yes | From `machines.*.nested_virtualization` |
| `cpu_options.threads_per_core` | Yes | Another way | Via `resources.cpus`, through the machine type Isoloom picks |
| `credit_specification` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `credit_specification.cpu_credits` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ebs_block_device` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `ebs_block_device.delete_on_termination` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `ebs_block_device.device_name` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `ebs_block_device.encrypted` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ebs_block_device.iops` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ebs_block_device.kms_key_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ebs_block_device.snapshot_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ebs_block_device.tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `ebs_block_device.tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `ebs_block_device.throughput` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ebs_block_device.volume_size` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `ebs_block_device.volume_type` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `enclave_options` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `enclave_options.enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ephemeral_block_device` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ephemeral_block_device.device_name` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ephemeral_block_device.no_device` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ephemeral_block_device.virtual_name` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `instance_market_options` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `instance_market_options.market_type` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `instance_market_options.spot_options` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `instance_market_options.spot_options.instance_interruption_behavior` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `instance_market_options.spot_options.max_price` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `instance_market_options.spot_options.spot_instance_type` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `instance_market_options.spot_options.valid_until` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `launch_template` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `launch_template.id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `launch_template.name` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `launch_template.version` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `maintenance_options` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `maintenance_options.auto_recovery` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `metadata_options` | Yes | Yes | Written by the generator |
| `metadata_options.http_endpoint` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `metadata_options.http_protocol_ipv6` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `metadata_options.http_put_response_hop_limit` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `metadata_options.http_tokens` | Yes | Yes | Written by the generator |
| `metadata_options.instance_metadata_tags` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `network_interface.delete_on_termination` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `network_interface.device_index` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `network_interface.network_card_index` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `network_interface.network_interface_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `primary_network_interface` | Yes | Yes | Written by the generator |
| `primary_network_interface.network_interface_id` | Yes | Yes | Written by the generator |
| `private_dns_name_options` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `private_dns_name_options.enable_resource_name_dns_a_record` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `private_dns_name_options.enable_resource_name_dns_aaaa_record` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `private_dns_name_options.hostname_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `root_block_device` | Yes | Yes | Written by the generator |
| `root_block_device.delete_on_termination` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `root_block_device.encrypted` | Yes | Yes | Written by the generator |
| `root_block_device.iops` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `root_block_device.kms_key_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `root_block_device.tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `root_block_device.tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `root_block_device.throughput` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `root_block_device.volume_size` | Yes | Yes | Written by the generator |
| `root_block_device.volume_type` | Yes | Yes | Written by the generator |
| `secondary_network_interface` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `secondary_network_interface.delete_on_termination` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `secondary_network_interface.device_index` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `secondary_network_interface.interface_type` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `secondary_network_interface.network_card_index` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `secondary_network_interface.private_ip_address_count` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `secondary_network_interface.secondary_subnet_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### aws_internet_gateway

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `tags` | Yes | Yes | Written by the generator |
| `tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `vpc_id` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### aws_key_pair

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `key_name` | Yes | Yes | Written by the generator |
| `key_name_prefix` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `public_key` | Yes | Yes | Written by the generator |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |

### aws_network_interface

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `enable_primary_ipv6` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `interface_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ipv4_prefix_count` | Yes | Not yet | With several addresses on one network |
| `ipv4_prefixes` | Yes | Not yet | With several addresses on one network |
| `ipv6_address_count` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_address_list` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_address_list_enabled` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_addresses` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_prefix_count` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_prefixes` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `private_ip` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `private_ip_list` | Yes | Not yet | With several addresses on one network |
| `private_ip_list_enabled` | Yes | Not yet | With several addresses on one network |
| `private_ips` | Yes | Yes | Written by the generator |
| `private_ips_count` | Yes | Not yet | With several addresses on one network |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `security_groups` | Yes | Yes | Written by the generator |
| `source_dest_check` | Yes | Yes | Written by the generator |
| `subnet_id` | Yes | Yes | Written by the generator |
| `tags` | Yes | Yes | Written by the generator |
| `tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `attachment` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `attachment.device_index` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `attachment.instance` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `attachment.network_card_index` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `ena_srd_specification` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ena_srd_specification.ena_srd_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ena_srd_specification.ena_srd_udp_specification` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ena_srd_specification.ena_srd_udp_specification.ena_srd_udp_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |

### aws_network_interface_attachment

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `device_index` | Yes | Yes | Written by the generator |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `instance_id` | Yes | Yes | Written by the generator |
| `network_card_index` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface_id` | Yes | Yes | Written by the generator |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |

### aws_route_table

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `propagating_vgws` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `route` | Yes | Yes | Written by the generator |
| `route.carrier_gateway_id` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `route.cidr_block` | Yes | Yes | Written by the generator |
| `route.core_network_arn` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `route.destination_prefix_list_id` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `route.egress_only_gateway_id` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `route.gateway_id` | Yes | Yes | Written by the generator |
| `route.ipv6_cidr_block` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `route.local_gateway_id` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `route.nat_gateway_id` | Yes | Not yet | Private networks reaching the internet through NAT |
| `route.network_interface_id` | Yes | Not yet | With `networks.*.gateway`: a network routed through one of its machines |
| `route.odb_network_arn` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `route.transit_gateway_id` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `route.vpc_endpoint_id` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `route.vpc_peering_connection_id` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `tags` | Yes | Yes | Written by the generator |
| `tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `vpc_id` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### aws_route_table_association

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `gateway_id` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `route_table_id` | Yes | Yes | Written by the generator |
| `subnet_id` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### aws_security_group

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `egress` | Yes | Yes | Written by the generator |
| `egress.cidr_blocks` | Yes | Yes | Written by the generator |
| `egress.description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `egress.from_port` | Yes | Yes | Written by the generator |
| `egress.ipv6_cidr_blocks` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `egress.prefix_list_ids` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `egress.protocol` | Yes | Yes | Written by the generator |
| `egress.security_groups` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `egress.self` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `egress.to_port` | Yes | Yes | Written by the generator |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `ingress` | Yes | Yes | Written by the generator |
| `ingress.cidr_blocks` | Yes | Yes | Written by the generator |
| `ingress.description` | Yes | Yes | Written by the generator |
| `ingress.from_port` | Yes | Yes | Written by the generator |
| `ingress.ipv6_cidr_blocks` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ingress.prefix_list_ids` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `ingress.protocol` | Yes | Yes | Written by the generator |
| `ingress.security_groups` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `ingress.self` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `ingress.to_port` | Yes | Yes | Written by the generator |
| `name` | Yes | Yes | Written by the generator |
| `name_prefix` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `revoke_rules_on_delete` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `vpc_id` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |

### aws_subnet

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `assign_ipv6_address_on_creation` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `availability_zone` | Yes | Yes | Written by the generator |
| `availability_zone_id` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `cidr_block` | Yes | Yes | Written by the generator |
| `customer_owned_ipv4_pool` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `enable_dns64` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `enable_lni_at_device_index` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `enable_resource_name_dns_a_record_on_launch` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `enable_resource_name_dns_aaaa_record_on_launch` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `ipv4_ipam_pool_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ipv4_netmask_length` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ipv6_cidr_block` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_ipam_pool_id` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_native` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_netmask_length` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `map_customer_owned_ip_on_launch` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `map_public_ip_on_launch` | Yes | Yes | Written by the generator |
| `outpost_arn` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `private_dns_hostname_type_on_launch` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `tags` | Yes | Yes | Written by the generator |
| `tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `vpc_id` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |

### aws_vpc

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `assign_generated_ipv6_cidr_block` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `cidr_block` | Yes | Yes | Written by the generator |
| `enable_dns_hostnames` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `enable_dns_support` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `enable_network_address_usage_metrics` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `instance_tenancy` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `ipv4_ipam_pool_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ipv4_netmask_length` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ipv6_cidr_block` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_cidr_block_network_border_group` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_ipam_pool_id` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `ipv6_netmask_length` | No | No | An AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `tags` | Yes | Yes | Written by the generator |
| `tags_all` | n/a | No | Names, labels and descriptions in the account: no change in behavior |

### aws_vpc_ipv4_cidr_block_association

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `cidr_block` | Yes | Yes | Written by the generator |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `ipv4_ipam_pool_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ipv4_netmask_length` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `vpc_id` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |

### data.aws_ami

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allow_unsafe_filter` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `executable_users` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `include_deprecated` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `most_recent` | Yes | Yes | Written by the generator |
| `name_regex` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `owners` | Yes | Yes | Written by the generator |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `tags` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `uefi_data` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `filter` | Yes | Yes | Written by the generator |
| `filter.name` | Yes | Yes | Written by the generator |
| `filter.values` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |

### data.aws_availability_zones

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `all_availability_zones` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `exclude_names` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `exclude_zone_ids` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `region` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `state` | Yes | Yes | Written by the generator |
| `filter` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `filter.name` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `filter.values` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |

And 1373 other resource types that only this cloud has: its managed services, network services and public DNS. Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Azure

80% of 135 portable features (108 done, 0 partly, 27 to do; 1559 features in all). From hashicorp/azurerm 5.8.0 (its resource types, and the schema of what Isoloom writes).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `azurerm_advisor_suppression` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_application_insights` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_application_insights_analytics_item` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_application_insights_api_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_application_insights_smart_detection_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_application_insights_standard_web_test` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_application_insights_web_test` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_application_insights_workbook` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_application_insights_workbook_template` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_application_security_group` | Yes | Another way | Via the `security_rule` blocks of azurerm_network_security_group, which carry the reach rules |
| `azurerm_automanage_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_availability_set` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_backup_container_storage_account` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_backup_policy_file_share` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_backup_policy_vm` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_backup_policy_vm_workload` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_backup_protected_file_share` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_backup_protected_vm` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_billing_account_cost_management_export` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_capacity_reservation` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_capacity_reservation_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_consumption_budget_management_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_consumption_budget_resource_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_consumption_budget_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_cost_anomaly_alert` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_cost_management_scheduled_action` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_dashboard_grafana` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_dashboard_grafana_managed_private_endpoint` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_data_protection_backup_instance_blob_storage` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_instance_data_lake_storage` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_instance_disk` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_instance_kubernetes_cluster` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_instance_mysql_flexible_server` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_instance_postgresql_flexible_server` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_policy_blob_storage` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_policy_data_lake_storage` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_policy_disk` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_policy_kubernetes_cluster` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_policy_mysql_flexible_server` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_policy_postgresql_flexible_server` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_vault` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_backup_vault_customer_managed_key` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_data_protection_resource_guard` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_dedicated_host` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_dedicated_host_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_disk_access` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_disk_encryption_set` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_federated_identity_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_image` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `azurerm_lighthouse_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_lighthouse_definition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_linux_virtual_machine` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_linux_virtual_machine_scale_set` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_log_analytics_cluster` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_cluster_customer_managed_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_data_export_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_datasource_windows_event` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_datasource_windows_performance_counter` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_linked_service` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_linked_storage_account` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_query_pack` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_query_pack_query` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_saved_search` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_solution` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_storage_insights` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_workspace` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_workspace_table` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_workspace_table_custom_log` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_log_analytics_workspace_table_microsoft` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_maintenance_assignment_dedicated_host` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_maintenance_assignment_dynamic_scope` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_maintenance_assignment_virtual_machine` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_maintenance_assignment_virtual_machine_scale_set` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_maintenance_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_managed_disk` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_managed_disk_sas_token` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_management_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_management_group_policy_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_management_group_policy_exemption` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_management_group_policy_remediation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_management_group_policy_set_definition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_management_group_subscription_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_management_group_template_deployment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_management_lock` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_marketplace_agreement` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_marketplace_role_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_aad_diagnostic_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_action_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_activity_log_alert` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_alert_processing_rule_action_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_alert_processing_rule_suppression` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_alert_prometheus_rule_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_autoscale_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_data_collection_endpoint` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_data_collection_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_data_collection_rule_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_diagnostic_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_metric_alert` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_private_link_scope` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_private_link_scoped_service` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_scheduled_query_rules_alert` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_scheduled_query_rules_alert_v2` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_scheduled_query_rules_log` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_smart_detector_alert_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_monitor_workspace` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_nat_gateway` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_nat_gateway_public_ip_association` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_network_connection_monitor` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_network_interface` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_network_interface_application_security_group_association` | Yes | Another way | Via the `security_rule` blocks of azurerm_network_security_group, which carry the reach rules |
| `azurerm_network_interface_security_group_association` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_network_security_group` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_network_security_rule` | Yes | Another way | Via the `security_rule` blocks of azurerm_network_security_group, which carry the reach rules |
| `azurerm_network_watcher` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_network_watcher_flow_log` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_orchestrated_virtual_machine_scale_set` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_pim_active_role_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_pim_eligible_role_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_policy_definition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_policy_set_definition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_policy_virtual_machine_configuration_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_portal_dashboard` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_portal_tenant_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_private_dns_a_record` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `azurerm_private_dns_aaaa_record` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `azurerm_private_dns_cname_record` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `azurerm_private_dns_mx_record` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `azurerm_private_dns_ptr_record` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `azurerm_private_dns_srv_record` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `azurerm_private_dns_txt_record` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `azurerm_private_dns_zone` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `azurerm_private_dns_zone_virtual_network_link` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `azurerm_proximity_placement_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_public_ip` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_recovery_services_vault` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_recovery_services_vault_resource_guard_association` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_resource_deployment_script_azure_cli` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_deployment_script_azure_power_shell` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_group` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_resource_group_cost_management_export` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_group_cost_management_view` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_group_policy_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_group_policy_exemption` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_group_policy_remediation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_group_template_deployment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_policy_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_policy_exemption` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_policy_remediation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_provider_feature_registration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_resource_provider_registration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_role_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_role_definition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_role_management_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_route` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_route_table` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_security_center_assessment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_security_center_assessment_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_security_center_automation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_security_center_contact` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_security_center_server_vulnerability_assessment_virtual_machine` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_security_center_server_vulnerability_assessments_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_security_center_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_security_center_storage_defender` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_security_center_subscription_pricing` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_security_center_workspace` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_shared_image` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `azurerm_shared_image_gallery` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `azurerm_shared_image_version` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `azurerm_site_recovery_fabric` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_hyperv_network_mapping` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_hyperv_replication_policy` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_hyperv_replication_policy_association` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_network_mapping` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_protection_container` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_protection_container_mapping` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_replicated_vm` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_replication_policy` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_replication_recovery_plan` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_services_vault_hyperv_site` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_vmware_replicated_vm` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_vmware_replication_policy` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_site_recovery_vmware_replication_policy_association` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_snapshot` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_ssh_public_key` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_subnet` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_subnet_nat_gateway_association` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_subnet_network_security_group_association` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_subnet_route_table_association` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_subscription_cost_management_export` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_subscription_cost_management_view` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_subscription_policy_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_subscription_policy_exemption` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_subscription_policy_remediation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_subscription_template_deployment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_tenant_template_deployment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_user_assigned_identity` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_virtual_machine` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_virtual_machine_automanage_configuration_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_virtual_machine_data_disk_attachment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_virtual_machine_implicit_data_disk_from_source` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_virtual_machine_packet_capture` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `azurerm_virtual_machine_restore_point` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_virtual_machine_restore_point_collection` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `azurerm_virtual_machine_scale_set` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_virtual_machine_scale_set_extension` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_virtual_machine_scale_set_packet_capture` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_virtual_machine_scale_set_standby_pool` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `azurerm_virtual_network` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_virtual_network_dns_servers` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `azurerm_windows_virtual_machine` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_windows_virtual_machine_scale_set` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `ado_pipeline_service_connection_id` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `auxiliary_tenant_ids` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `client_certificate` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `client_certificate_password` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `client_certificate_path` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `client_id` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `client_id_file_path` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `client_secret` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `client_secret_file_path` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `disable_correlation_request_id` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `disable_terraform_partner_id` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `environment` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `metadata_host` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `msi_api_version` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `msi_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `oidc_request_token` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `oidc_request_url` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `oidc_token` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `oidc_token_file_path` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `partner_id` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `resource_provider_registrations` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `resource_providers_to_register` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `storage_use_azuread` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `subscription_id` | Yes | Yes | Written by the generator |
| `tenant_id` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `use_aks_workload_identity` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `use_cli` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `use_msi` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `use_oidc` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features` | Yes | Yes | Written by the generator |
| `features.persist_id_on_create_before_polling_for_completion` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.skip_import_check_on_create_and_allow_overwriting_existing_resources` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.api_management` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.api_management.purge_soft_delete_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.api_management.recover_soft_deleted` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.app_configuration` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.app_configuration.purge_soft_delete_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.app_configuration.recover_soft_deleted` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.application_insights` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.application_insights.disable_generated_rule` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.cognitive_account` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.cognitive_account.purge_soft_delete_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.databricks_workspace` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.databricks_workspace.force_delete` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.enhanced_validation` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.enhanced_validation.locations` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.enhanced_validation.preflight_enabled` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.enhanced_validation.preflight_location_fallback` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.enhanced_validation.resource_providers` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.purge_soft_delete_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.purge_soft_deleted_certificates_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.purge_soft_deleted_hardware_security_module_keys_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.purge_soft_deleted_hardware_security_modules_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.purge_soft_deleted_keys_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.purge_soft_deleted_secrets_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.recover_soft_deleted_certificates` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.recover_soft_deleted_hardware_security_module_keys` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.recover_soft_deleted_key_vaults` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.recover_soft_deleted_keys` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.key_vault.recover_soft_deleted_secrets` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.log_analytics_workspace` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.log_analytics_workspace.permanently_delete_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.machine_learning` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.machine_learning.purge_soft_deleted_workspace_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.managed_disk` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.managed_disk.expand_without_downtime` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.netapp` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.netapp.delete_backups_on_backup_vault_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.netapp.prevent_volume_destruction` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.postgresql_flexible_server` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.postgresql_flexible_server.restart_server_on_configuration_value_change` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.recovery_service` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.recovery_service.purge_protected_items_from_vault_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.recovery_service.vm_backup_stop_protection_and_retain_data_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.recovery_service.vm_backup_suspend_protection_and_retain_data_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.recovery_services_vaults` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.recovery_services_vaults.recover_soft_deleted_backup_protected_vm` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.resource_group` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.resource_group.prevent_deletion_if_contains_resources` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.servicebus` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.servicebus.auto_delete_subscription_default_rule` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.storage` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.storage.data_plane_available` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.subscription` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.subscription.prevent_cancellation_on_destroy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.template_deployment` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.template_deployment.delete_nested_items_during_deletion` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.virtual_machine` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.virtual_machine.delete_os_disk_on_deletion` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.virtual_machine.detach_implicit_data_disk_on_deletion` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.virtual_machine.skip_shutdown_and_force_delete` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.virtual_machine_scale_set` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.virtual_machine_scale_set.force_delete` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.virtual_machine_scale_set.reimage_on_manual_upgrade` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.virtual_machine_scale_set.roll_instances_when_required` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `features.virtual_machine_scale_set.scale_to_zero_before_deletion` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |

### azurerm_linux_virtual_machine

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `admin_password` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `admin_username` | Yes | Yes | Written by the generator |
| `allow_extension_operations` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `availability_set_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `bypass_platform_safety_checks_on_user_schedule_enabled` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `capacity_reservation_group_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `computer_name` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `custom_data` | Yes | Yes | Written by the generator |
| `dedicated_host_group_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `dedicated_host_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `disable_password_authentication` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk_controller_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `edge_zone` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `encryption_at_host_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `eviction_policy` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `extensions_time_budget` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `license_type` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `location` | Yes | Yes | Written by the generator |
| `max_bid_price` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `name` | Yes | Yes | Written by the generator |
| `network_interface_ids` | Yes | Yes | Written by the generator |
| `os_managed_disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `patch_assessment_mode` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `patch_mode` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `platform_fault_domain` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `priority` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `provision_vm_agent` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `proximity_placement_group_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `reboot_setting` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `resource_group_name` | Yes | Yes | Written by the generator |
| `secure_boot_enabled` | Yes | Yes | Written by the generator |
| `size` | Yes | Yes | Written by the generator |
| `source_image_id` | Yes | Yes | Written by the generator |
| `tags` | Yes | Yes | Written by the generator |
| `user_data` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `virtual_machine_scale_set_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `vtpm_enabled` | Yes | Yes | Written by the generator |
| `zone` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `additional_capabilities` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `additional_capabilities.hibernation_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `additional_capabilities.ultra_ssd_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `admin_ssh_key` | Yes | Yes | Written by the generator |
| `admin_ssh_key.public_key` | Yes | Yes | Written by the generator |
| `admin_ssh_key.username` | Yes | Yes | Written by the generator |
| `boot_diagnostics` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_diagnostics.storage_account_uri` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.automatic_upgrade_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.configuration_blob_uri` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.order` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.tag` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.treat_failure_as_deployment_failure_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.version_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `identity` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `identity.identity_ids` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `identity.type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk` | Yes | Yes | Written by the generator |
| `os_disk.caching` | Yes | Yes | Written by the generator |
| `os_disk.disk_encryption_set_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.disk_size_gb` | Yes | Yes | Written by the generator |
| `os_disk.name` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `os_disk.secure_vm_disk_encryption_set_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.security_encryption_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.storage_account_type` | Yes | Yes | Written by the generator |
| `os_disk.write_accelerator_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.diff_disk_settings` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.diff_disk_settings.option` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.diff_disk_settings.placement` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_image_notification` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_image_notification.timeout` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `plan` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `plan.name` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `plan.product` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `plan.publisher` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `secret` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `secret.key_vault_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `secret.certificate` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `secret.certificate.url` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `source_image_reference` | Yes | Yes | Written by the generator |
| `source_image_reference.offer` | Yes | Yes | Written by the generator |
| `source_image_reference.publisher` | Yes | Yes | Written by the generator |
| `source_image_reference.sku` | Yes | Yes | Written by the generator |
| `source_image_reference.version` | Yes | Yes | Written by the generator |
| `termination_notification` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `termination_notification.enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `termination_notification.timeout` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### azurerm_network_interface

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `accelerated_networking_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `auxiliary_mode` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `auxiliary_sku` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `dns_servers` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `edge_zone` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `internal_dns_name_label` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `ip_forwarding_enabled` | Yes | Yes | Written by the generator |
| `location` | Yes | Yes | Written by the generator |
| `name` | Yes | Yes | Written by the generator |
| `resource_group_name` | Yes | Yes | Written by the generator |
| `tags` | Yes | Yes | Written by the generator |
| `ip_configuration` | Yes | Yes | Written by the generator |
| `ip_configuration.gateway_load_balancer_frontend_ip_configuration_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ip_configuration.name` | Yes | Yes | Written by the generator |
| `ip_configuration.primary` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `ip_configuration.private_ip_address` | Yes | Yes | Written by the generator |
| `ip_configuration.private_ip_address_allocation` | Yes | Yes | Written by the generator |
| `ip_configuration.private_ip_address_version` | Yes | Not yet | The cloud takes the spec's unique local block (the Azure module writes it), but cloud-vm generates only when AWS takes the environment, which it refuses with IPv6 |
| `ip_configuration.public_ip_address_id` | Yes | Yes | Written by the generator |
| `ip_configuration.subnet_id` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### azurerm_network_interface_security_group_association

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `network_interface_id` | Yes | Yes | Written by the generator |
| `network_security_group_id` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |

### azurerm_network_security_group

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `location` | Yes | Yes | Written by the generator |
| `name` | Yes | Yes | Written by the generator |
| `resource_group_name` | Yes | Yes | Written by the generator |
| `security_rule` | Yes | Yes | Written by the generator |
| `security_rule.access` | Yes | Yes | Written by the generator |
| `security_rule.description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `security_rule.destination_address_prefix` | Yes | Yes | Written by the generator |
| `security_rule.destination_address_prefixes` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `security_rule.destination_application_security_group_ids` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `security_rule.destination_port_range` | Yes | Yes | Written by the generator |
| `security_rule.destination_port_ranges` | Yes | Yes | Written by the generator |
| `security_rule.direction` | Yes | Yes | Written by the generator |
| `security_rule.name` | Yes | Yes | Written by the generator |
| `security_rule.priority` | Yes | Yes | Written by the generator |
| `security_rule.protocol` | Yes | Yes | Written by the generator |
| `security_rule.source_address_prefix` | Yes | Yes | Written by the generator |
| `security_rule.source_address_prefixes` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `security_rule.source_application_security_group_ids` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `security_rule.source_port_range` | Yes | Yes | Written by the generator |
| `security_rule.source_port_ranges` | n/a | No | A source port filter: reach rules name destination ports, from any source port |
| `tags` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### azurerm_public_ip

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allocation_method` | Yes | Yes | Written by the generator |
| `ddos_protection_mode` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ddos_protection_plan_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `domain_name_label` | No | No | Public DNS and domains: local targets have no public names |
| `domain_name_label_scope` | No | No | Public DNS and domains: local targets have no public names |
| `edge_zone` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `idle_timeout_in_minutes` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ip_tags` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ip_version` | No | No | A public IPv6 address: local targets have none, and Isoloom reaches machines over IPv4 |
| `location` | Yes | Yes | Written by the generator |
| `name` | Yes | Yes | Written by the generator |
| `public_ip_prefix_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `resource_group_name` | Yes | Yes | Written by the generator |
| `reverse_fqdn` | No | No | Public DNS and domains: local targets have no public names |
| `sku` | Yes | Yes | Written by the generator |
| `sku_tier` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `tags` | Yes | Yes | Written by the generator |
| `zones` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### azurerm_resource_group

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `location` | Yes | Yes | Written by the generator |
| `managed_by` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `name` | Yes | Yes | Written by the generator |
| `tags` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### azurerm_subnet

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `address_prefixes` | Yes | Yes | Written by the generator |
| `default_outbound_access_enabled` | Yes | Not yet | `networks.*.internet: false` enforced by the cloud |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `name` | Yes | Yes | Written by the generator |
| `network_security_group_id_wo` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `network_security_group_id_wo_version` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `private_endpoint_network_policies` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `private_link_service_network_policies_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `resource_group_name` | Yes | Yes | Written by the generator |
| `route_table_id_wo` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `route_table_id_wo_version` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `service_endpoint_policy_ids` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `sharing_scope` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `virtual_network_name` | Yes | Yes | Written by the generator |
| `delegation` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `delegation.name` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `delegation.service_delegation` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `delegation.service_delegation.actions` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `delegation.service_delegation.name` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ip_address_pool` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ip_address_pool.id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ip_address_pool.number_of_ip_addresses` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `service_endpoint` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `service_endpoint.network_identifier` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `service_endpoint.service` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### azurerm_virtual_network

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `address_space` | Yes | Yes | Written by the generator |
| `bgp_community` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `dns_servers` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `edge_zone` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `flow_timeout_in_minutes` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `location` | Yes | Yes | Written by the generator |
| `name` | Yes | Yes | Written by the generator |
| `private_endpoint_vnet_policies` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `resource_group_name` | Yes | Yes | Written by the generator |
| `subnet` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.address_prefixes` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.default_outbound_access_enabled` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.delegation` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.delegation.name` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.delegation.service_delegation` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.delegation.service_delegation.actions` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.delegation.service_delegation.name` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.name` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.private_endpoint_network_policies` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.private_link_service_network_policies_enabled` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.route_table_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.security_group` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.service_endpoint` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.service_endpoint.network_identifier` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.service_endpoint.service` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `subnet.service_endpoint_policy_ids` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `tags` | Yes | Yes | Written by the generator |
| `ddos_protection_plan` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ddos_protection_plan.enable` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ddos_protection_plan.id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `encryption` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `encryption.enforcement` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ip_address_pool` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ip_address_pool.id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ip_address_pool.number_of_ip_addresses` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### azurerm_windows_virtual_machine

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `admin_password` | Yes | Yes | Written by the generator |
| `admin_username` | Yes | Yes | Written by the generator |
| `allow_extension_operations` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `automatic_updates_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `availability_set_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `bypass_platform_safety_checks_on_user_schedule_enabled` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `capacity_reservation_group_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `computer_name` | Yes | Yes | Written by the generator |
| `custom_data` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `dedicated_host_group_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `dedicated_host_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `disk_controller_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `edge_zone` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `encryption_at_host_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `eviction_policy` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `extensions_time_budget` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `hotpatching_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `license_type` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `location` | Yes | Yes | Written by the generator |
| `max_bid_price` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `name` | Yes | Yes | Written by the generator |
| `network_interface_ids` | Yes | Yes | Written by the generator |
| `os_managed_disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `patch_assessment_mode` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `patch_mode` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `platform_fault_domain` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `priority` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `provision_vm_agent` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `proximity_placement_group_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `reboot_setting` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `resource_group_name` | Yes | Yes | Written by the generator |
| `secure_boot_enabled` | Yes | Not yet | With a TPM on Windows in the cloud: AWS's NitroTPM Windows images (TPM-Windows_Server-*) first |
| `size` | Yes | Yes | Written by the generator |
| `source_image_id` | Yes | Yes | Written by the generator |
| `tags` | Yes | Yes | Written by the generator |
| `timezone` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `user_data` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `virtual_machine_scale_set_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `vtpm_enabled` | Yes | Not yet | With a TPM on Windows in the cloud: AWS's NitroTPM Windows images (TPM-Windows_Server-*) first |
| `zone` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `additional_capabilities` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `additional_capabilities.hibernation_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `additional_capabilities.ultra_ssd_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `additional_unattend_content` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `additional_unattend_content.content` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `additional_unattend_content.setting` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_diagnostics` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_diagnostics.storage_account_uri` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.automatic_upgrade_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.configuration_blob_uri` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.order` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.tag` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.treat_failure_as_deployment_failure_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gallery_application.version_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `identity` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `identity.identity_ids` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `identity.type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk` | Yes | Yes | Written by the generator |
| `os_disk.caching` | Yes | Yes | Written by the generator |
| `os_disk.disk_encryption_set_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.disk_size_gb` | Yes | Yes | Written by the generator |
| `os_disk.name` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `os_disk.secure_vm_disk_encryption_set_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.security_encryption_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.storage_account_type` | Yes | Yes | Written by the generator |
| `os_disk.write_accelerator_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.diff_disk_settings` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.diff_disk_settings.option` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_disk.diff_disk_settings.placement` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_image_notification` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `os_image_notification.timeout` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `plan` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `plan.name` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `plan.product` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `plan.publisher` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `secret` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `secret.key_vault_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `secret.certificate` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `secret.certificate.store` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `secret.certificate.url` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `source_image_reference` | Yes | Yes | Written by the generator |
| `source_image_reference.offer` | Yes | Yes | Written by the generator |
| `source_image_reference.publisher` | Yes | Yes | Written by the generator |
| `source_image_reference.sku` | Yes | Yes | Written by the generator |
| `source_image_reference.version` | Yes | Yes | Written by the generator |
| `termination_notification` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `termination_notification.enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `termination_notification.timeout` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |
| `winrm_listener` | Yes | Yes | Written by the generator |
| `winrm_listener.certificate_url` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `winrm_listener.protocol` | Yes | Yes | Written by the generator |

And 894 other resource types that only this cloud has: its managed services, network services and public DNS. Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Google Cloud

72% of 97 portable features (69 done, 2 partly, 26 to do; 1850 features in all). From hashicorp/google 8.5.0 (its resource types, and the schema of what Isoloom writes).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `google_access_context_manager_access_level` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_access_level_condition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_access_levels` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_access_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_access_policy_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_access_policy_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_access_policy_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_authorized_orgs_desc` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_egress_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_gcp_user_access_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_ingress_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_service_perimeter` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_service_perimeter_dry_run_egress_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_service_perimeter_dry_run_ingress_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_service_perimeter_dry_run_resource` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_service_perimeter_egress_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_service_perimeter_ingress_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_service_perimeter_resource` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_access_context_manager_service_perimeters` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_apikeys_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_assured_workloads_workload` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_backup_dr_backup_plan` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_backup_dr_backup_plan_association` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_backup_dr_backup_vault` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_backup_dr_management_server` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_backup_dr_restore_workload` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_backup_dr_service_config` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_billing_account_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_billing_account_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_billing_account_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_billing_budget` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_billing_project_info` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_billing_subaccount` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_asset_folder_feed` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_asset_organization_feed` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_asset_project_feed` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_identity_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_identity_group_membership` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_quotas_quota_adjuster_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_quotas_quota_preference` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_security_compliance_cloud_control` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_security_compliance_framework` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_security_compliance_framework_deployment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_cloud_support_support_event_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_address` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `google_compute_attached_disk` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `google_compute_autoscaler` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_bulk_per_instance_config` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_disk` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `google_compute_disk_async_replication` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_compute_disk_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_disk_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_disk_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_disk_resource_policy_attachment` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_firewall` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_compute_firewall_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_firewall_policy_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_firewall_policy_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_firewall_policy_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_firewall_policy_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_firewall_policy_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_firewall_policy_with_rules` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_image` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `google_compute_image_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_image_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_image_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_instance` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_compute_instance_from_template` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_instance_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_instance_group_manager` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_instance_group_membership` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_instance_group_named_port` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_instance_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_instance_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_instance_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_instance_settings` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_instance_template` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_instance_template_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_instance_template_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_instance_template_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_instant_snapshot` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_compute_instant_snapshot_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_instant_snapshot_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_instant_snapshot_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_network` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_compute_network_firewall_policy` | Yes | Another way | Via google_compute_firewall rules, which carry the reach rules, and the instances' metadata for SSH keys |
| `google_compute_network_firewall_policy_association` | Yes | Another way | Via google_compute_firewall rules, which carry the reach rules, and the instances' metadata for SSH keys |
| `google_compute_network_firewall_policy_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_network_firewall_policy_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_network_firewall_policy_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_network_firewall_policy_rule` | Yes | Another way | Via google_compute_firewall rules, which carry the reach rules, and the instances' metadata for SSH keys |
| `google_compute_network_firewall_policy_with_rules` | Yes | Another way | Via google_compute_firewall rules, which carry the reach rules, and the instances' metadata for SSH keys |
| `google_compute_node_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_node_template` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_organization_security_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_organization_security_policy_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_organization_security_policy_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_per_instance_config` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_preview_feature` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_project_cloud_armor_tier` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_project_default_network_tier` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_project_metadata` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_project_metadata_item` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_region_autoscaler` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_region_commitment` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_region_disk` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_region_disk_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_region_disk_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_region_disk_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_region_disk_resource_policy_attachment` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_region_instance_group_manager` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_region_instance_template` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_region_instant_snapshot` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_compute_region_instant_snapshot_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_region_instant_snapshot_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_region_instant_snapshot_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_region_network_firewall_policy` | Yes | Another way | Via google_compute_firewall rules, which carry the reach rules, and the instances' metadata for SSH keys |
| `google_compute_region_network_firewall_policy_association` | Yes | Another way | Via google_compute_firewall rules, which carry the reach rules, and the instances' metadata for SSH keys |
| `google_compute_region_network_firewall_policy_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_region_network_firewall_policy_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_region_network_firewall_policy_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_region_network_firewall_policy_rule` | Yes | Another way | Via google_compute_firewall rules, which carry the reach rules, and the instances' metadata for SSH keys |
| `google_compute_region_network_firewall_policy_with_rules` | Yes | Another way | Via google_compute_firewall rules, which carry the reach rules, and the instances' metadata for SSH keys |
| `google_compute_region_per_instance_config` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_region_resize_request` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_reservation` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_resize_request` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_resource_policy` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_resource_policy_attachment` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_rollout_plan` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_route` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `google_compute_router` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `google_compute_router_nat` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `google_compute_router_nat_address` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `google_compute_shared_vpc_host_project` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_shared_vpc_service_project` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_snapshot` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_compute_snapshot_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_snapshot_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_snapshot_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_snapshot_settings` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `google_compute_storage_pool` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `google_compute_storage_pool_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_storage_pool_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_storage_pool_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_subnetwork` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_compute_subnetwork_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_subnetwork_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_compute_subnetwork_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_contact_center_insights_analysis_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_contact_center_insights_assessment_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_contact_center_insights_auto_labeling_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_contact_center_insights_encryption_spec` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_contact_center_insights_qa_question` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_contact_center_insights_qa_scorecard` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_contact_center_insights_qa_scorecard_revision` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_contact_center_insights_view` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_dns_managed_zone` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `google_dns_managed_zone_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_dns_managed_zone_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_dns_managed_zone_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_dns_policy` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `google_dns_record_set` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `google_essential_contacts_contact` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_folder` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_folder_access_approval_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_folder_iam_audit_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_folder_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_folder_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_folder_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_folder_organization_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_access_boundary_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_deny_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_folder_access_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_folders_policy_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_oauth_client` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_oauth_client_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_organization_access_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_organizations_policy_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_principal_access_boundary_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_project_access_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_projects_policy_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workforce_pool` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workforce_pool_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workforce_pool_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workforce_pool_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workforce_pool_provider` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workforce_pool_provider_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workforce_pool_provider_scim_tenant` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workforce_pool_provider_scim_token` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workload_identity_pool` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workload_identity_pool_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workload_identity_pool_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workload_identity_pool_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workload_identity_pool_managed_identity` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workload_identity_pool_namespace` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_iam_workload_identity_pool_provider` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_billing_account_bucket_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_billing_account_exclusion` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_billing_account_sink` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_folder_bucket_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_folder_exclusion` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_folder_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_folder_sink` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_linked_dataset` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_log_scope` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_log_view` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_log_view_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_log_view_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_log_view_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_metric` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_organization_bucket_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_organization_exclusion` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_organization_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_organization_sink` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_project_bucket_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_project_exclusion` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_project_sink` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_logging_saved_query` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_alert_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_custom_service` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_dashboard` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_metric_descriptor` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_monitored_project` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_notification_channel` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_service` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_slo` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_snooze` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_monitoring_uptime_check_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_network_management_connectivity_test` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_network_management_network_monitoring_provider` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_network_management_organization_vpc_flow_logs_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_network_management_vpc_flow_logs_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_observability_bucket` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_observability_folder_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_observability_link` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_observability_organization_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_observability_project_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_observability_trace_scope` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_org_policy_custom_constraint` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_org_policy_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_organization_access_approval_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_organization_iam_audit_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_organization_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_organization_iam_custom_role` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_organization_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_organization_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_organization_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_os_login_ssh_public_key` | Yes | Another way | Via google_compute_firewall rules, which carry the reach rules, and the instances' metadata for SSH keys |
| `google_privileged_access_manager_entitlement` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_project` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_project_access_approval_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_project_default_service_accounts` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_project_iam_audit_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_project_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_project_iam_custom_role` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_project_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_project_iam_member_remove` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_project_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_project_organization_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_project_service` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_project_usage_export_bucket` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_resource_manager_capability` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_resource_manager_capability_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_resource_manager_lien` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_service_account` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_service_account_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_service_account_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_service_account_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_service_account_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_tags_location_tag_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_tags_tag_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_tags_tag_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_tags_tag_key_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_tags_tag_key_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_tags_tag_key_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_tags_tag_value` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_tags_tag_value_iam_binding` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_tags_tag_value_iam_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_tags_tag_value_iam_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `google_workload_identity_service_agent` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `access_approval_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `access_context_manager_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `access_token` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `active_directory_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `add_terraform_attribution_label` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `agent_identity_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `agent_registry_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `agentic_applications_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `alloydb_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `apigee_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `apihub_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `apikeys_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `app_engine_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `apphub_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `artifact_registry_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `assured_workloads_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `backup_dr_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `beyondcorp_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `big_query_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `biglake_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `biglake_hive_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `biglake_iceberg_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `bigquery_analytics_hub_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `bigquery_connection_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `bigquery_data_transfer_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `bigquery_datapolicy_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `bigquery_datapolicyv2_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `bigquery_reservation_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `bigtable_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `billing_budgets_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `billing_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `billing_project` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `binary_authorization_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `blockchain_node_engine_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `certificate_manager_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ces_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `chronicle_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_asset_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_billing_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_build_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_functions_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_identity_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_ids_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_quotas_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_resource_manager_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_run_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_run_v2_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_scheduler_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_security_compliance_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_support_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloud_tasks_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloudbuildv2_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `clouddeploy_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `clouddomains_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `cloudfunctions2_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `colab_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `composer_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `compute_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `config_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `contact_center_insights_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `container_analysis_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `container_attached_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `container_aws_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `container_azure_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `container_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `core_billing_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `credentials` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `data_catalog_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `data_fusion_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `data_lineage_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `data_loss_prevention_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `data_pipeline_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `database_migration_service_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `dataflow_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `dataform_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `dataplex_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `dataproc_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `dataproc_gdc_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `dataproc_metastore_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `datastream_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `default_labels` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `deletion_policy` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `deployment_manager_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `developer_connect_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `dialogflow_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `dialogflow_cx_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `discovery_engine_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `dns_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `document_ai_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `document_ai_warehouse_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `edgecontainer_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `edgenetwork_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `essential_contacts_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `eventarc_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `filestore_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `firebase_app_check_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `firebase_app_hosting_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `firebase_data_connect_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `firebase_remote_config_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `firebaserules_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `firestore_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `gemini_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `gke_backup_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `gke_hub2_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `gke_hub_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `gkeonprem_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `healthcare_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `hypercomputecluster_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `iam2_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `iam3_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `iam_beta_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `iam_credentials_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `iam_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `iam_workforce_pool_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `iap_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `identity_platform_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `impersonate_service_account` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `impersonate_service_account_delegates` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `integration_connectors_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `integrations_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `kms_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `license_manager_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `logging_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `looker_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `lustre_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `managed_kafka_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `memcache_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `memorystore_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `migration_center_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `model_armor_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `model_armor_global_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `monitoring_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `netapp_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `network_connectivity_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `network_connectivityv1_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `network_management_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `network_managementv1_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `network_security_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `network_services_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `observability_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `oracle_database_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `org_policy_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `os_config_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `os_config_v2_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `os_login_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `parallelstore_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `parameter_manager_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `parameter_manager_regional_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `poll_interval` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `prefer_global_endpoints` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `prefer_regional_endpoints` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `privateca_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `privileged_access_manager_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `project` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `public_ca_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `pubsub_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `pubsub_lite_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `recaptcha_enterprise_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `redis_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `region` | Yes | Yes | Written by the generator |
| `request_reason` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `request_timeout` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `resource_manager3_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `resource_manager_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `resource_manager_v3_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `scopes` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `secret_manager_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `secret_manager_regional_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `secure_source_manager_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `security_center_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `security_center_management_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `security_center_v2_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `securityposture_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `service_directory_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `service_management_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `service_networking_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `service_usage_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `site_verification_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `source_repo_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `spanner_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `sql_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `storage_batch_operations_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `storage_control_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `storage_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `storage_ftp_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `storage_insights_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `storage_transfer_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `tags_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `tags_location_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `terraform_attribution_label_addition_strategy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `transcoder_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `universe_domain` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `user_project_override` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `vector_search_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `vertex_ai_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `vmwareengine_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `vpc_access_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `workbench_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `workflows_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `workload_identity_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `workstations_custom_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `zone` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `batching` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `batching.enable_batching` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `batching.send_after` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `external_credentials` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `external_credentials.audience` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `external_credentials.identity_token` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `external_credentials.service_account_email` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |

### google_compute_firewall

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `deletion_policy` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `destination_ranges` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `direction` | Yes | Yes | Written by the generator |
| `disabled` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `enable_logging` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `name` | Yes | Yes | Written by the generator |
| `network` | Yes | Yes | Written by the generator |
| `priority` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `project` | Yes | Yes | Written by the generator |
| `source_ranges` | Yes | Yes | Written by the generator |
| `source_service_accounts` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `source_tags` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `target_service_accounts` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `target_tags` | Yes | Yes | Written by the generator |
| `allow` | Yes | Yes | Written by the generator |
| `allow.ports` | Yes | Yes | Written by the generator |
| `allow.protocol` | Yes | Yes | Written by the generator |
| `deny` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `deny.ports` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `deny.protocol` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `log_config` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `log_config.metadata` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `params` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `params.resource_manager_tags` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### google_compute_instance

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allow_stopping_for_update` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `can_ip_forward` | Yes | Not yet | With `networks.*.gateway`: a network routed through one of its machines |
| `deletion_policy` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `deletion_protection` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `desired_status` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `enable_display` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `hostname` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `key_revocation_action_type` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `labels` | Yes | Yes | Written by the generator |
| `machine_type` | Yes | Yes | Written by the generator |
| `metadata` | Yes | Yes | Written by the generator |
| `metadata_startup_script` | Yes | Yes | Written by the generator |
| `min_cpu_platform` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `name` | Yes | Yes | Written by the generator |
| `project` | Yes | Yes | Written by the generator |
| `resource_policies` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `tags` | Yes | Yes | Written by the generator |
| `zone` | Yes | Yes | Written by the generator |
| `advanced_machine_features` | Yes | Yes | From `machines.*.nested_virtualization` |
| `advanced_machine_features.enable_nested_virtualization` | Yes | Yes | From `machines.*.nested_virtualization` |
| `advanced_machine_features.enable_uefi_networking` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `advanced_machine_features.performance_monitoring_unit` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `advanced_machine_features.threads_per_core` | Yes | Another way | Via `resources.cpus`, through the machine type Isoloom picks |
| `advanced_machine_features.turbo_mode` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `advanced_machine_features.visible_core_count` | Yes | Another way | Via `resources.cpus`, through the machine type Isoloom picks |
| `attached_disk` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `attached_disk.device_name` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `attached_disk.disk_encryption_key_raw` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `attached_disk.disk_encryption_key_rsa` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `attached_disk.disk_encryption_service_account` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `attached_disk.force_attach` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `attached_disk.kms_key_self_link` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `attached_disk.mode` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `attached_disk.source` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `boot_disk` | Yes | Yes | Written by the generator |
| `boot_disk.auto_delete` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `boot_disk.device_name` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `boot_disk.disk_encryption_key_raw` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.disk_encryption_key_rsa` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.disk_encryption_service_account` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.force_attach` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `boot_disk.guest_os_features` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.interface` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.kms_key_self_link` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.mode` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `boot_disk.source` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `boot_disk.initialize_params` | Yes | Yes | Written by the generator |
| `boot_disk.initialize_params.architecture` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `boot_disk.initialize_params.enable_confidential_compute` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.image` | Yes | Yes | Written by the generator |
| `boot_disk.initialize_params.labels` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `boot_disk.initialize_params.provisioned_iops` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.provisioned_throughput` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.replica_zones` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `boot_disk.initialize_params.resource_manager_tags` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.resource_policies` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.size` | Yes | Yes | Written by the generator |
| `boot_disk.initialize_params.snapshot` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `boot_disk.initialize_params.storage_pool` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.type` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `boot_disk.initialize_params.source_image_encryption_key` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.source_image_encryption_key.kms_key_self_link` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.source_image_encryption_key.kms_key_service_account` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.source_image_encryption_key.raw_key` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.source_image_encryption_key.rsa_encrypted_key` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.source_snapshot_encryption_key` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.source_snapshot_encryption_key.kms_key_self_link` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.source_snapshot_encryption_key.kms_key_service_account` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.source_snapshot_encryption_key.raw_key` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_disk.initialize_params.source_snapshot_encryption_key.rsa_encrypted_key` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `confidential_instance_config` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `confidential_instance_config.confidential_instance_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `confidential_instance_config.enable_confidential_compute` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `guest_accelerator` | Yes | Another way | Via machines.*.gpus.count, as a G2 machine type (its L4 GPUs attached) |
| `guest_accelerator.count` | Yes | Another way | Via machines.*.gpus.count, as a G2 machine type (its L4 GPUs attached) |
| `guest_accelerator.type` | Yes | Another way | Via machines.*.gpus.count, as a G2 machine type (its L4 GPUs attached) |
| `instance_encryption_key` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `instance_encryption_key.kms_key_self_link` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `instance_encryption_key.kms_key_service_account` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface` | Yes | Yes | Written by the generator |
| `network_interface.igmp_query` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface.internal_ipv6_prefix_length` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `network_interface.ipv6_address` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `network_interface.network` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `network_interface.network_attachment` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface.network_ip` | Yes | Yes | Written by the generator |
| `network_interface.nic_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface.queue_count` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface.stack_type` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `network_interface.subnetwork` | Yes | Yes | Written by the generator |
| `network_interface.subnetwork_project` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `network_interface.vlan` | Yes | Not yet | With VLANs and trunks (`networks.*.vlans`) |
| `network_interface.access_config` | Yes | Yes | Written by the generator |
| `network_interface.access_config.nat_ip` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `network_interface.access_config.network_tier` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface.access_config.public_ptr_domain_name` | No | No | Public DNS and domains: local targets have no public names |
| `network_interface.alias_ip_range` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface.alias_ip_range.ip_cidr_range` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface.alias_ip_range.subnetwork_range_name` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_interface.ipv6_access_config` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `network_interface.ipv6_access_config.external_ipv6` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `network_interface.ipv6_access_config.external_ipv6_prefix_length` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `network_interface.ipv6_access_config.name` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `network_interface.ipv6_access_config.network_tier` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `network_interface.ipv6_access_config.public_ptr_domain_name` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `network_performance_config` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_performance_config.total_egress_bandwidth_tier` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `params` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `params.resource_manager_tags` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `reservation_affinity` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `reservation_affinity.type` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `reservation_affinity.specific_reservation` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `reservation_affinity.specific_reservation.key` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `reservation_affinity.specific_reservation.values` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `scheduling` | Yes | Partly | From `machines.*.gpus.count` (`on_host_maintenance = TERMINATE`); not yet: the other scheduling options: the cloud's own |
| `scheduling.automatic_restart` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.availability_domain` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.host_error_timeout_seconds` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.instance_termination_action` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.min_node_cpus` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.on_host_maintenance` | Yes | Partly | From `machines.*.gpus.count` (`on_host_maintenance = TERMINATE`); not yet: the other scheduling options: the cloud's own |
| `scheduling.preemptible` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.provisioning_model` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.termination_time` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.local_ssd_recovery_timeout` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.local_ssd_recovery_timeout.nanos` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.local_ssd_recovery_timeout.seconds` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.max_run_duration` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.max_run_duration.nanos` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.max_run_duration.seconds` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.node_affinities` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.node_affinities.key` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.node_affinities.operator` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.node_affinities.values` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.on_instance_stop_action` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scheduling.on_instance_stop_action.discard_local_ssd` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `scratch_disk` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `scratch_disk.device_name` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `scratch_disk.interface` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `scratch_disk.size` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `service_account` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `service_account.email` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `service_account.scopes` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `shielded_instance_config` | Yes | Yes | Written by the generator |
| `shielded_instance_config.enable_integrity_monitoring` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `shielded_instance_config.enable_secure_boot` | Yes | Yes | Written by the generator |
| `shielded_instance_config.enable_vtpm` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |
| `workload_identity_config` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `workload_identity_config.identity` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `workload_identity_config.identity_certificate_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |

### google_compute_network

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `auto_create_subnetworks` | Yes | Yes | Written by the generator |
| `bgp_always_compare_med` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `bgp_best_path_selection_mode` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `bgp_inter_region_cost` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `delete_bgp_always_compare_med` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `delete_default_routes_on_create` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `deletion_policy` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `enable_ula_internal_ipv6` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `internal_ipv6_range` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `mtu` | Yes | Not yet | A network's MTU |
| `name` | Yes | Yes | Written by the generator |
| `network_firewall_policy_enforcement_order` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `network_profile` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `project` | Yes | Yes | Written by the generator |
| `routing_mode` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `params` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `params.resource_manager_tags` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### google_compute_subnetwork

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allow_subnet_cidr_routes_overlap` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `deletion_policy` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `external_ipv6_prefix` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `internal_ipv6_prefix` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `ip_cidr_range` | Yes | Yes | Written by the generator |
| `ip_collection` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `ipv6_access_type` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `name` | Yes | Yes | Written by the generator |
| `network` | Yes | Yes | Written by the generator |
| `private_ip_google_access` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `private_ipv6_google_access` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `project` | Yes | Yes | Written by the generator |
| `purpose` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `region` | Yes | Yes | Written by the generator |
| `reserved_internal_range` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `resolve_subnet_mask` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `role` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `send_secondary_ip_range_if_empty` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `stack_type` | No | No | Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud |
| `log_config` | n/a | No | The cloud's own logging and monitoring: no change in behavior |
| `log_config.aggregation_interval` | n/a | No | The cloud's own logging and monitoring: no change in behavior |
| `log_config.filter_expr` | n/a | No | The cloud's own logging and monitoring: no change in behavior |
| `log_config.flow_sampling` | n/a | No | The cloud's own logging and monitoring: no change in behavior |
| `log_config.metadata` | n/a | No | The cloud's own logging and monitoring: no change in behavior |
| `log_config.metadata_fields` | n/a | No | The cloud's own logging and monitoring: no change in behavior |
| `params` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `params.resource_manager_tags` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `secondary_ip_range` | Yes | Not yet | With several addresses on one network |
| `secondary_ip_range.ip_cidr_range` | Yes | Not yet | With several addresses on one network |
| `secondary_ip_range.range_name` | Yes | Not yet | With several addresses on one network |
| `secondary_ip_range.reserved_internal_range` | Yes | Not yet | With several addresses on one network |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### google_project

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `auto_create_network` | Yes | Yes | Written by the generator |
| `billing_account` | Yes | Yes | Written by the generator |
| `deletion_policy` | Yes | Yes | Written by the generator |
| `folder_id` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `labels` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `name` | Yes | Yes | Written by the generator |
| `org_id` | Yes | Yes | Written by the generator |
| `project_id` | Yes | Yes | Written by the generator |
| `tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### google_project_service

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `deletion_policy` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `disable_dependent_services` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `disable_on_destroy` | Yes | Yes | Written by the generator |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `project` | Yes | Yes | Written by the generator |
| `service` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.read` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

And 1087 other resource types that only this cloud has: its managed services, network services and public DNS. Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: DigitalOcean

77% of 36 portable features (28 done, 0 partly, 8 to do; 150 features in all). From digitalocean/digitalocean 2.103.0 (its resource types, and the schema of what Isoloom writes).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `digitalocean_custom_image` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `digitalocean_droplet` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `digitalocean_droplet_autoscale` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `digitalocean_droplet_snapshot` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `digitalocean_firewall` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `digitalocean_floating_ip` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `digitalocean_floating_ip_assignment` | n/a | No | An older name of a resource type the provider still has: no change in behavior |
| `digitalocean_monitor_alert` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `digitalocean_nfs_snapshot` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `digitalocean_project` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `digitalocean_project_resources` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `digitalocean_reserved_ip` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `digitalocean_reserved_ip_assignment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `digitalocean_reserved_ipv6` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `digitalocean_reserved_ipv6_assignment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `digitalocean_ssh_key` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `digitalocean_tag` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `digitalocean_uptime_alert` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `digitalocean_uptime_check` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `digitalocean_volume` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `digitalocean_volume_attachment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `digitalocean_volume_snapshot` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `digitalocean_vpc` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `digitalocean_vpc_nat_gateway` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `api_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `http_retry_max` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `http_retry_wait_max` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `http_retry_wait_min` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `requests_per_second` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `spaces_access_id` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `spaces_endpoint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `spaces_secret_key` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |

### digitalocean_droplet

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `backups` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `droplet_agent` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `gpu_partition_mode` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `graceful_shutdown` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `image` | Yes | Yes | Written by the generator |
| `ipv6` | No | No | DigitalOcean VPCs are IPv4 only (a droplet's IPv6 address is a public one): Isoloom refuses IPv6 networks on DigitalOcean |
| `ipv6_address` | No | No | DigitalOcean VPCs are IPv4 only (a droplet's IPv6 address is a public one): Isoloom refuses IPv6 networks on DigitalOcean |
| `monitoring` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `name` | Yes | Yes | Written by the generator |
| `private_networking` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `public_networking` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `region` | Yes | Yes | Written by the generator |
| `resize_disk` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `size` | Yes | Yes | Written by the generator |
| `ssh_keys` | Yes | Yes | Written by the generator |
| `tags` | Yes | Yes | Written by the generator |
| `user_data` | Yes | Yes | Written by the generator |
| `volume_ids` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `vpc_uuid` | Yes | Yes | Written by the generator |
| `backup_policy` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `backup_policy.hour` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `backup_policy.plan` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `backup_policy.weekday` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### digitalocean_firewall

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `droplet_ids` | Yes | Yes | Written by the generator |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `name` | Yes | Yes | Written by the generator |
| `tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `inbound_rule` | Yes | Yes | Written by the generator |
| `inbound_rule.action` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `inbound_rule.port_range` | Yes | Yes | Written by the generator |
| `inbound_rule.protocol` | Yes | Yes | Written by the generator |
| `inbound_rule.source_addresses` | Yes | Yes | Written by the generator |
| `inbound_rule.source_droplet_ids` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `inbound_rule.source_kubernetes_ids` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `inbound_rule.source_load_balancer_uids` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `inbound_rule.source_tags` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `outbound_rule` | Yes | Yes | Written by the generator |
| `outbound_rule.action` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `outbound_rule.destination_addresses` | Yes | Yes | Written by the generator |
| `outbound_rule.destination_droplet_ids` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `outbound_rule.destination_kubernetes_ids` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `outbound_rule.destination_load_balancer_uids` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `outbound_rule.destination_tags` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `outbound_rule.port_range` | Yes | Yes | Written by the generator |
| `outbound_rule.protocol` | Yes | Yes | Written by the generator |

### digitalocean_ssh_key

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `name` | Yes | Yes | Written by the generator |
| `public_key` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |

### digitalocean_vpc

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `ip_range` | Yes | Yes | Written by the generator |
| `name` | Yes | Yes | Written by the generator |
| `region` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |

And 55 other resource types that only this cloud has: its managed services, network services and public DNS. Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Linode

66% of 59 portable features (39 done, 0 partly, 20 to do; 497 features in all). From linode/linode 4.7.0 (its resource types, and the schema of what Isoloom writes).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `linode_account_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `linode_consumer_image_share_group_token` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `linode_firewall` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `linode_firewall_device` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_firewall_settings` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `linode_iam_user` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `linode_image` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `linode_instance` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `linode_instance_config` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_instance_disk` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_instance_ip` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_instance_shared_ips` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_interface` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_ipv6_range` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_lock` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `linode_monitor_alert_definition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `linode_monitor_logs_destination` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `linode_monitor_logs_stream` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `linode_networking_ip` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_networking_ip_assignment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_placement_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `linode_placement_group_assignment` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `linode_producer_image_share_group` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `linode_producer_image_share_group_member` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `linode_reserved_ip` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_reserved_ip_assignment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_sshkey` | Yes | Another way | Via each instance's own `authorized_keys`, which installs the environment's key without storing it in the account |
| `linode_token` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `linode_user` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `linode_volume` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `linode_vpc` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `linode_vpc_subnet` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `api_ca_path` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `api_version` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `config_path` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `config_profile` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `disable_internal_cache` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `event_poll_ms` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `lke_event_poll_ms` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `lke_node_ready_poll_ms` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `max_retry_delay_ms` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `min_retry_delay_ms` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `obj_access_key` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `obj_bucket_force_delete` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `obj_secret_key` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `obj_use_temp_keys` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `skip_implicit_reboots` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `skip_instance_delete_poll` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `skip_instance_ready_poll` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `skip_lke_cluster_delete_poll` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ua_prefix` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `url` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |

### linode_firewall

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `disabled` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `inbound_policy` | Yes | Yes | Written by the generator |
| `interfaces` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `label` | Yes | Yes | Written by the generator |
| `linodes` | Yes | Yes | Written by the generator |
| `nodebalancers` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `outbound_policy` | Yes | Yes | Written by the generator |
| `tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `inbound` | Yes | Yes | Written by the generator |
| `inbound.action` | Yes | Yes | Written by the generator |
| `inbound.description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `inbound.ipv4` | Yes | Yes | Written by the generator |
| `inbound.ipv6` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `inbound.label` | Yes | Yes | Written by the generator |
| `inbound.ports` | Yes | Yes | Written by the generator |
| `inbound.protocol` | Yes | Yes | Written by the generator |
| `outbound` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `outbound.action` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `outbound.description` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `outbound.ipv4` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `outbound.ipv6` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `outbound.label` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `outbound.ports` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `outbound.protocol` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |

### linode_instance

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `authorized_keys` | Yes | Yes | Written by the generator |
| `authorized_users` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `backup_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `backups_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `boot_config_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `boot_size` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `booted` | Yes | Yes | Written by the generator |
| `disk_encryption` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `firewall_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `image` | Yes | Yes | Written by the generator |
| `interface_generation` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `ipv4` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `kernel` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `label` | Yes | Yes | Written by the generator |
| `maintenance_policy` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `migration_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `network_helper` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `placement_group_externally_managed` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `private_ip` | Yes | Not yet | With several addresses on one network |
| `region` | Yes | Yes | Written by the generator |
| `resize_disk` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `root_pass` | Yes | Yes | Written by the generator |
| `shared_ipv4` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `stackscript_data` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `stackscript_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `swap_size` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `tags` | Yes | Yes | Written by the generator |
| `type` | Yes | Yes | Written by the generator |
| `watchdog_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `alerts` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `alerts.cpu` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `alerts.io` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `alerts.network_in` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `alerts.network_out` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `alerts.transfer_quota` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `config` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.comments` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.kernel` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.memory_limit` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.root_device` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.run_level` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.virt_mode` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sda` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sda.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sda.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sda.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaa` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaa.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaa.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaa.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdab` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdab.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdab.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdab.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdac` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdac.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdac.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdac.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdad` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdad.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdad.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdad.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdae` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdae.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdae.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdae.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaf` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaf.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaf.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaf.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdag` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdag.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdag.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdag.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdah` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdah.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdah.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdah.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdai` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdai.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdai.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdai.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaj` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaj.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaj.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaj.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdak` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdak.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdak.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdak.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdal` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdal.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdal.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdal.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdam` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdam.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdam.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdam.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdan` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdan.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdan.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdan.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdao` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdao.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdao.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdao.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdap` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdap.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdap.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdap.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaq` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaq.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaq.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaq.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdar` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdar.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdar.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdar.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdas` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdas.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdas.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdas.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdat` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdat.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdat.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdat.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdau` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdau.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdau.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdau.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdav` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdav.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdav.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdav.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaw` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaw.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaw.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaw.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdax` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdax.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdax.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdax.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sday` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sday.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sday.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sday.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaz` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaz.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaz.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdaz.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdb` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdb.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdb.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdb.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdba` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdba.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdba.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdba.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbb` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbb.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbb.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbb.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbc` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbc.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbc.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbc.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbd` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbd.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbd.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbd.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbe` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbe.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbe.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbe.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbf` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbf.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbf.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbf.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbg` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbg.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbg.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbg.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbh` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbh.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbh.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbh.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbi` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbi.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbi.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbi.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbj` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbj.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbj.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbj.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbk` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbk.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbk.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbk.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbl` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbl.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbl.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdbl.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdc` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdc.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdc.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdc.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdd` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdd.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdd.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdd.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sde` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sde.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sde.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sde.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdf` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdf.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdf.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdf.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdg` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdg.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdg.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdg.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdh` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdh.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdh.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdh.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdi` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdi.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdi.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdi.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdj` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdj.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdj.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdj.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdk` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdk.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdk.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdk.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdl` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdl.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdl.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdl.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdm` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdm.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdm.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdm.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdn` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdn.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdn.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdn.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdo` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdo.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdo.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdo.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdp` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdp.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdp.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdp.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdq` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdq.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdq.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdq.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdr` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdr.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdr.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdr.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sds` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sds.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sds.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sds.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdt` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdt.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdt.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdt.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdu` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdu.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdu.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdu.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdv` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdv.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdv.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdv.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdw` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdw.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdw.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdw.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdx` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdx.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdx.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdx.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdy` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdy.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdy.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdy.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdz` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdz.disk_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdz.disk_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.devices.sdz.volume_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.helpers` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.helpers.devtmpfs_automount` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.helpers.distro` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.helpers.modules_dep` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.helpers.network` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.helpers.updatedb_disabled` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ip_ranges` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ipam_address` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.primary` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.purpose` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.subnet_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ipv4` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ipv4.nat_1_1` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ipv4.vpc` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ipv6` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ipv6.is_public` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ipv6.range` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ipv6.range.range` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ipv6.slaac` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `config.interface.ipv6.slaac.range` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.authorized_keys` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.authorized_users` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.filesystem` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.image` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.read_only` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.root_pass` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.size` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.stackscript_data` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `disk.stackscript_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `interface` | Yes | Yes | Written by the generator |
| `interface.ip_ranges` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `interface.ipam_address` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `interface.label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `interface.primary` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `interface.purpose` | Yes | Yes | Written by the generator |
| `interface.subnet_id` | Yes | Yes | Written by the generator |
| `interface.ipv4` | Yes | Yes | Written by the generator |
| `interface.ipv4.nat_1_1` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `interface.ipv4.vpc` | Yes | Yes | Written by the generator |
| `interface.ipv6` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `interface.ipv6.is_public` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `interface.ipv6.range` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `interface.ipv6.range.range` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `interface.ipv6.slaac` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `interface.ipv6.slaac.range` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `linode_interfaces` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.firewall_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.default_route` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.default_route.ipv4` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.default_route.ipv6` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.public` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.public.ipv4` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.public.ipv4.addresses` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.public.ipv4.addresses.address` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.public.ipv4.addresses.primary` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.public.ipv6` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.public.ipv6.ranges` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.public.ipv6.ranges.range` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.rdma_vpc` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.rdma_vpc.subnet_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.rdma_vpc.ipv4` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.rdma_vpc.ipv4.addresses` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.rdma_vpc.ipv4.addresses.address` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.rdma_vpc.ipv4.addresses.primary` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vlan` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vlan.ipam_address` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vlan.vlan_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vpc` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vpc.subnet_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vpc.ipv4` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vpc.ipv4.addresses` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vpc.ipv4.addresses.address` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vpc.ipv4.addresses.nat_1_1_address` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vpc.ipv4.addresses.primary` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vpc.ipv4.ranges` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `linode_interfaces.vpc.ipv4.ranges.range` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `metadata` | Yes | Yes | Written by the generator |
| `metadata.user_data` | Yes | Yes | Written by the generator |
| `placement_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `placement_group.compliant_only` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `placement_group.id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `placement_group.placement_group_policy` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### linode_vpc

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `ipv4` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `ipv4.range` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `ipv6` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `ipv6.allocation_class` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `ipv6.range` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `label` | Yes | Yes | Written by the generator |
| `region` | Yes | Yes | Written by the generator |
| `vpc_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |

### linode_vpc_subnet

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `ipv4` | Yes | Yes | Written by the generator |
| `ipv6` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `ipv6.range` | No | No | Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode |
| `label` | Yes | Yes | Written by the generator |
| `vpc_id` | Yes | Yes | Written by the generator |

And 15 other resource types that only this cloud has: its managed services, network services and public DNS. Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Oracle Cloud

50% of 152 portable features (77 done, 0 partly, 75 to do; 1320 features in all). From oracle/oci 9.8.0 (its resource types, and the schema of what Isoloom writes).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `oci_announcements_service_announcement_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_announcements_service_announcement_subscriptions_actions_change_compartment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_announcements_service_announcement_subscriptions_filter_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apiaccesscontrol_privileged_api_control` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apiaccesscontrol_privileged_api_request` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apm_apm_domain` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apm_config_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apm_config_data_file` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apm_synthetics_dedicated_vantage_point` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apm_synthetics_monitor` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apm_synthetics_on_premise_vantage_point` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apm_synthetics_on_premise_vantage_point_worker` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apm_synthetics_script` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_apm_traces_scheduled_query` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_audit_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_autoscaling_auto_scaling_configuration` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_budget_alert_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_budget_budget` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_budget_cost_alert_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_budget_cost_anomaly_event` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_budget_cost_anomaly_monitor` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_budget_cost_anomaly_monitor_costanomalymonitorenabletoggles_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_capacity_management_internal_occm_demand_signal` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_capacity_management_internal_occm_demand_signal_delivery` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_capacity_management_occ_availability_catalog` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_capacity_management_occ_capacity_request` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_capacity_management_occ_customer_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_capacity_management_occ_customer_group_occ_customer` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_capacity_management_occm_demand_signal` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_capacity_management_occm_demand_signal_item` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_cloud_bridge_agent` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_bridge_agent_dependency` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_bridge_agent_plugin` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_bridge_asset` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_bridge_asset_source` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_bridge_discovery_schedule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_bridge_environment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_bridge_inventory` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_adhoc_query` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_cloud_guard_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_data_mask_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_data_source` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_detector_recipe` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_managed_list` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_responder_recipe` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_saved_query` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_security_recipe` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_security_zone` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_target` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_guard_wlp_agent` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_migrations_migration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_migrations_migration_asset` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_migrations_migration_plan` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_migrations_replication_schedule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cloud_migrations_target_asset` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_cluster_placement_groups_cluster_placement_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_compute_cloud_at_customer_ccc_infrastructure` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_compute_cloud_at_customer_ccc_upgrade_schedule` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_app_catalog_listing_resource_version_agreement` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_core_app_catalog_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_core_boot_volume` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_boot_volume_backup` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_core_cluster_network` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_compute_capacity_report` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_compute_capacity_reservation` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_compute_capacity_topology` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_compute_cluster` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_compute_gpu_memory_cluster` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_compute_gpu_memory_fabric` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_compute_host` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_compute_host_group` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_compute_image_capability_schema` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `oci_core_dedicated_vm_host` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_default_dhcp_options` | n/a | No | The account's default network: Isoloom builds its own |
| `oci_core_default_drg_route_table` | n/a | No | The account's default network: Isoloom builds its own |
| `oci_core_default_route_table` | n/a | No | The account's default network: Isoloom builds its own |
| `oci_core_default_security_list` | n/a | No | The account's default network: Isoloom builds its own |
| `oci_core_dhcp_options` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_image` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `oci_core_instance` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_instance_configuration` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_instance_maintenance_event` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_instance_pool` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_instance_pool_instance` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_core_internet_gateway` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_ipv6` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_listing_resource_version_agreement` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_core_nat_gateway` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_network_security_group` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_network_security_group_security_rule` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_private_ip` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_public_ip` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_route_table` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_route_table_attachment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_security_list` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_shape_management` | Yes | Another way | Via machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id |
| `oci_core_subnet` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_vcn` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_vlan` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_vnic_attachment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_volume` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_volume_attachment` | Yes | Not yet | With the cloud generator (one VM per machine on its own networks) |
| `oci_core_volume_backup` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_core_volume_backup_policy` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_core_volume_backup_policy_assignment` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_core_volume_group` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_core_volume_group_backup` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_costad_cost_alert_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_costad_cost_anomaly_event` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_costad_cost_anomaly_monitor` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_costad_cost_anomaly_monitor_costanomalymonitorenabletoggles_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_delegate_access_control_delegation_control` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_delegate_access_control_delegation_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_demand_signal_occ_demand_signal` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_demand_signal_occ_metric_alarm` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_disaster_recovery_automatic_dr_configuration` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_disaster_recovery_dr_plan` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_disaster_recovery_dr_plan_execution` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_disaster_recovery_dr_protection_group` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_dns_record` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `oci_dns_resolver` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `oci_dns_resolver_endpoint` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `oci_dns_rrset` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `oci_dns_view` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `oci_dns_zone` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `oci_events_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_api_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_auth_token` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_authentication_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_compartment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_customer_secret_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_data_plane_generate_scoped_access_token` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_db_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domain` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domain_replication_to_region` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_account_recovery_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_api_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_app` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_app_role` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_approval_workflow` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_approval_workflow_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_approval_workflow_step` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_auth_token` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_authentication_factor_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_cloud_gate` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_cloud_gate_mapping` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_cloud_gate_server` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_condition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_customer_secret_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_dynamic_resource_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_grant` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_identity_proofing_provider` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_identity_proofing_provider_template` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_identity_propagation_trust` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_identity_provider` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_identity_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_kmsi_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_mapped_attribute` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_my_api_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_my_auth_token` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_my_customer_secret_key` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_my_oauth2client_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_my_request` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_my_smtp_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_my_support_account` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_my_user_db_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_network_perimeter` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_notification_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_oauth2client_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_oauth_client_certificate` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_oauth_partner_certificate` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_password_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_security_question` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_security_question_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_self_registration_profile` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_setting` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_smtp_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_social_identity_provider` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_user` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_domains_user_db_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_dynamic_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_identity_provider` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_idp_group_mapping` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_import_standard_tags_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_network_source` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_smtp_credential` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_tag` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_tag_default` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_tag_namespace` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_ui_password` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_user` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_user_capabilities_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_identity_user_group_membership` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_license_manager_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_license_manager_license_record` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_license_manager_product_license` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_limits_quota` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_log_analytics_entity` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_log_analytics_entity_associations_add` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_log_analytics_entity_associations_remove` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_log_analytics_entity_type` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_log_analytics_import_custom_content` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_log_analytics_log_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_log_analytics_object_collection_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_log_analytics_preferences_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_log_analytics_resource_categories_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_log_analytics_unprocessed_data_bucket_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_namespace` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_namespace_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_namespace_ingest_time_rule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_namespace_ingest_time_rules_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_namespace_lookup` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_namespace_lookups_append_data_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_namespace_lookups_update_data_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_namespace_scheduled_task` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_namespace_storage_archival_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_log_analytics_namespace_storage_enable_disable_archiving` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_logging_log` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_logging_log_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_logging_log_saved_search` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_logging_unified_agent_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_management_dashboard_management_dashboards_import` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_management_dashboard_management_saved_search` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_marketplace_accepted_agreement` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_marketplace_listing_package_agreement` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_marketplace_marketplace_external_attested_metadata` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_marketplace_publication` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_metering_computation_custom_table` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_metering_computation_query` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_metering_computation_schedule` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_metering_computation_usage` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_metering_computation_usage_carbon_emission` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_metering_computation_usage_carbon_emissions_query` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_metering_computation_usage_statement_email_recipients_group` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_monitoring_alarm` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_monitoring_alarm_suppression` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_ons_notification_topic` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_ons_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_operator_access_control_operator_control` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_operator_access_control_operator_control_assignment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_optimizer_enrollment_status` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_optimizer_profile` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_optimizer_recommendation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_optimizer_resource_action` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_recovery_protected_database` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_recovery_protection_policy` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_recovery_recovery_service_subnet` | n/a | No | Backs up, copies or restores disks: no change in how the environment behaves |
| `oci_resource_analytics_monitored_region` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_resource_analytics_resource_analytics_instance` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_resource_analytics_resource_analytics_instance_oac_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_resource_analytics_tenancy_attachment` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_resource_scheduler_schedule` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `oci_sch_service_connector` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_security_attribute_security_attribute` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_security_attribute_security_attribute_namespace` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_self_subscription` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_service_catalog_private_application` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_service_catalog_service_catalog` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_service_catalog_service_catalog_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_baselineable_metric` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_config` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_discovery_job` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_maintenance_window` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_maintenance_windows_retry_failed_operation` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_maintenance_windows_stop` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_metric_extension` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_metric_extension_metric_extension_on_given_resources_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_metric_extensions_test_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_monitored_resource` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_monitored_resource_task` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_monitored_resource_type` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_monitored_resources_associate_monitored_resource` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_monitored_resources_list_member` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_monitored_resources_search` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_monitored_resources_search_association` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_monitoring_template` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_monitoring_template_alarm_condition` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_monitoring_template_monitoring_template_on_given_resources_management` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_stack_monitoring_process_set` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_tenantmanagercontrolplane_subscription_mapping` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_usage_proxy_subscription_redeemable_user` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_zpr_configuration` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |
| `oci_zpr_zpr_policy` | n/a | No | Administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves |

### Data sources

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `oci_core_images` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_identity_availability_domains` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `auth` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `config_file_profile` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `disable_auto_retries` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `dual_stack_endpoint_enabled` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `fingerprint` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `ignore_defined_tags` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `private_key` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `private_key_password` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `private_key_path` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `realm_specific_service_endpoint_template_enabled` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `region` | Yes | Yes | Written by the generator |
| `retries_config_file` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `retry_duration_seconds` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `tenancy_ocid` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `test_time_maintenance_reboot_due` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token_exchange_auth` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token_exchange_client_id` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token_exchange_client_secret` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token_exchange_domain_url` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token_exchange_public_key` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token_exchange_requested_token_type` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token_exchange_resource_type` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token_exchange_rpst_exp` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `token_exchange_subject_token_type` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `user_ocid` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `workload_identity_token_path` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |

### oci_core_instance

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `async` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `availability_domain` | Yes | Yes | Written by the generator |
| `capacity_reservation_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `cluster_placement_group_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `compartment_id` | Yes | Yes | Written by the generator |
| `compute_cluster_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `dedicated_vm_host_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `defined_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `display_name` | Yes | Yes | Written by the generator |
| `extended_metadata` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `fault_domain` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `freeform_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `hostname_label` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `image` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `instance_configuration_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `ipxe_script` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `is_ai_enterprise_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `is_pv_encryption_in_transit_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `metadata` | Yes | Yes | Written by the generator |
| `preserve_boot_volume` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `preserve_data_volumes_created_at_launch` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `security_attributes` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `shape` | Yes | Yes | Written by the generator |
| `state` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `subnet_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `update_operation_constraint` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `agent_config` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `agent_config.are_all_plugins_disabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `agent_config.is_management_disabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `agent_config.is_monitoring_disabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `agent_config.plugins_config` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `agent_config.plugins_config.desired_state` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `agent_config.plugins_config.name` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `availability_config` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `availability_config.is_live_migration_preferred` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `availability_config.recovery_action` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `create_vnic_details` | Yes | Yes | Written by the generator |
| `create_vnic_details.assign_ipv6ip` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `create_vnic_details.assign_private_dns_record` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `create_vnic_details.assign_public_ip` | Yes | Yes | Written by the generator |
| `create_vnic_details.defined_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `create_vnic_details.display_name` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `create_vnic_details.freeform_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `create_vnic_details.hostname_label` | Yes | Yes | Written by the generator |
| `create_vnic_details.nsg_ids` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `create_vnic_details.private_ip` | Yes | Yes | Written by the generator |
| `create_vnic_details.private_ip_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `create_vnic_details.security_attributes` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `create_vnic_details.skip_source_dest_check` | Yes | Not yet | With `networks.*.gateway`: a network routed through one of its machines |
| `create_vnic_details.subnet_cidr` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `create_vnic_details.subnet_id` | Yes | Yes | Written by the generator |
| `create_vnic_details.vlan_id` | Yes | Not yet | With VLANs and trunks (`networks.*.vlans`) |
| `create_vnic_details.ipv6address_ipv6subnet_cidr_pair_details` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `create_vnic_details.ipv6address_ipv6subnet_cidr_pair_details.ipv6address` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `create_vnic_details.ipv6address_ipv6subnet_cidr_pair_details.ipv6id` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `create_vnic_details.ipv6address_ipv6subnet_cidr_pair_details.ipv6subnet_cidr` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `instance_options` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `instance_options.are_legacy_imds_endpoints_disabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `launch_options` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `launch_options.boot_volume_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `launch_options.firmware` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `launch_options.is_consistent_volume_naming_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `launch_options.is_pv_encryption_in_transit_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `launch_options.network_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `launch_options.remote_data_volume_type` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `launch_volume_attachments` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.device` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.display_name` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.encryption_in_transit_type` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.is_agent_auto_iscsi_login_enabled` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.is_pv_encryption_in_transit_enabled` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.is_read_only` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.is_shareable` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.type` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.use_chap` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.volume_id` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.launch_create_volume_details` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.launch_create_volume_details.compartment_id` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.launch_create_volume_details.display_name` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.launch_create_volume_details.kms_key_id` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.launch_create_volume_details.size_in_gbs` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.launch_create_volume_details.volume_creation_type` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `launch_volume_attachments.launch_create_volume_details.vpus_per_gb` | Yes | Not yet | With data disks: persistent volumes on VMs |
| `licensing_configs` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `licensing_configs.license_type` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `licensing_configs.type` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `placement_constraint_details` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `placement_constraint_details.compute_bare_metal_host_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `placement_constraint_details.compute_host_group_id` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `placement_constraint_details.type` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `platform_config` | Yes | Yes | Written by the generator |
| `platform_config.are_virtual_instructions_enabled` | Yes | Yes | From `machines.*.nested_virtualization` |
| `platform_config.config_map` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `platform_config.is_access_control_service_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `platform_config.is_input_output_memory_management_unit_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `platform_config.is_measured_boot_enabled` | Yes | Yes | Written by the generator |
| `platform_config.is_memory_encryption_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `platform_config.is_secure_boot_enabled` | Yes | Yes | Written by the generator |
| `platform_config.is_symmetric_multi_threading_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `platform_config.is_trusted_platform_module_enabled` | Yes | Yes | Written by the generator |
| `platform_config.numa_nodes_per_socket` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `platform_config.percentage_of_cores_enabled` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `platform_config.type` | Yes | Yes | Written by the generator |
| `preemptible_instance_config` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `preemptible_instance_config.preemption_action` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `preemptible_instance_config.preemption_action.preserve_boot_volume` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `preemptible_instance_config.preemption_action.type` | n/a | No | How the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves |
| `shape_config` | Yes | Yes | Written by the generator |
| `shape_config.baseline_ocpu_utilization` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `shape_config.local_volume_size_in_gbs` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `shape_config.memory_in_gbs` | Yes | Yes | Written by the generator |
| `shape_config.nvmes` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `shape_config.ocpus` | Yes | Yes | Written by the generator |
| `shape_config.resource_management` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `shape_config.vcpus` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_details` | Yes | Yes | Written by the generator |
| `source_details.boot_volume_size_in_gbs` | Yes | Yes | Written by the generator |
| `source_details.boot_volume_vpus_per_gb` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `source_details.is_preserve_boot_volume_enabled` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `source_details.kms_key_id` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `source_details.source_id` | Yes | Yes | Written by the generator |
| `source_details.source_type` | Yes | Yes | Written by the generator |
| `source_details.instance_source_image_filter_details` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_details.instance_source_image_filter_details.compartment_id` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_details.instance_source_image_filter_details.defined_tags_filter` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_details.instance_source_image_filter_details.operating_system` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `source_details.instance_source_image_filter_details.operating_system_version` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### oci_core_internet_gateway

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `compartment_id` | Yes | Yes | Written by the generator |
| `defined_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `display_name` | Yes | Yes | Written by the generator |
| `enabled` | n/a | No | How Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior |
| `freeform_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `route_table_id` | No | No | One of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it |
| `vcn_id` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### oci_core_route_table

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `compartment_id` | Yes | Yes | Written by the generator |
| `defined_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `display_name` | Yes | Yes | Written by the generator |
| `freeform_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `vcn_id` | Yes | Yes | Written by the generator |
| `route_rules` | Yes | Yes | Written by the generator |
| `route_rules.cidr_block` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `route_rules.description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `route_rules.destination` | Yes | Yes | Written by the generator |
| `route_rules.destination_type` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `route_rules.network_entity_id` | Yes | Yes | Written by the generator |
| `route_rules.route_type` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### oci_core_security_list

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `compartment_id` | Yes | Yes | Written by the generator |
| `defined_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `display_name` | Yes | Yes | Written by the generator |
| `freeform_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `vcn_id` | Yes | Yes | Written by the generator |
| `egress_security_rules` | Yes | Yes | Written by the generator |
| `egress_security_rules.description` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `egress_security_rules.destination` | Yes | Yes | Written by the generator |
| `egress_security_rules.destination_type` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `egress_security_rules.protocol` | Yes | Yes | Written by the generator |
| `egress_security_rules.stateless` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `egress_security_rules.icmp_options` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.icmp_options.code` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.icmp_options.type` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.tcp_options` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.tcp_options.max` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.tcp_options.min` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.tcp_options.source_port_range` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.tcp_options.source_port_range.max` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.tcp_options.source_port_range.min` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.udp_options` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.udp_options.max` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.udp_options.min` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.udp_options.source_port_range` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.udp_options.source_port_range.max` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `egress_security_rules.udp_options.source_port_range.min` | Yes | Not yet | Outbound rules: reach enforced on traffic leaving a machine too |
| `ingress_security_rules` | Yes | Yes | Written by the generator |
| `ingress_security_rules.description` | Yes | Yes | Written by the generator |
| `ingress_security_rules.protocol` | Yes | Yes | Written by the generator |
| `ingress_security_rules.source` | Yes | Yes | Written by the generator |
| `ingress_security_rules.source_type` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `ingress_security_rules.stateless` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `ingress_security_rules.icmp_options` | Yes | Not yet | ICMP in reach rules |
| `ingress_security_rules.icmp_options.code` | Yes | Not yet | ICMP in reach rules |
| `ingress_security_rules.icmp_options.type` | Yes | Not yet | ICMP in reach rules |
| `ingress_security_rules.tcp_options` | Yes | Yes | Written by the generator |
| `ingress_security_rules.tcp_options.max` | Yes | Yes | Written by the generator |
| `ingress_security_rules.tcp_options.min` | Yes | Yes | Written by the generator |
| `ingress_security_rules.tcp_options.source_port_range` | n/a | No | A source port filter: reach rules name destination ports, from any source port |
| `ingress_security_rules.tcp_options.source_port_range.max` | n/a | No | A source port filter: reach rules name destination ports, from any source port |
| `ingress_security_rules.tcp_options.source_port_range.min` | n/a | No | A source port filter: reach rules name destination ports, from any source port |
| `ingress_security_rules.udp_options` | Yes | Yes | Written by the generator |
| `ingress_security_rules.udp_options.max` | Yes | Yes | Written by the generator |
| `ingress_security_rules.udp_options.min` | Yes | Yes | Written by the generator |
| `ingress_security_rules.udp_options.source_port_range` | n/a | No | A source port filter: reach rules name destination ports, from any source port |
| `ingress_security_rules.udp_options.source_port_range.max` | n/a | No | A source port filter: reach rules name destination ports, from any source port |
| `ingress_security_rules.udp_options.source_port_range.min` | n/a | No | A source port filter: reach rules name destination ports, from any source port |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### oci_core_subnet

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `availability_domain` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `cidr_block` | Yes | Yes | Written by the generator |
| `compartment_id` | Yes | Yes | Written by the generator |
| `defined_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `dhcp_options_id` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `display_name` | Yes | Yes | Written by the generator |
| `dns_label` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `freeform_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `ipv4cidr_blocks` | Yes | Not yet | With several addresses on one network |
| `ipv6cidr_block` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `ipv6cidr_blocks` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `prohibit_internet_ingress` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `prohibit_public_ip_on_vnic` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `route_table_id` | Yes | Yes | Written by the generator |
| `security_list_ids` | Yes | Yes | Written by the generator |
| `vcn_id` | Yes | Yes | Written by the generator |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### oci_core_vcn

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `cidr_block` | Yes | Yes | Written by the generator |
| `cidr_blocks` | Yes | Yes | Written by the generator |
| `compartment_id` | Yes | Yes | Written by the generator |
| `defined_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `display_name` | Yes | Yes | Written by the generator |
| `dns_label` | Yes | Not yet | Private DNS: the environment's names (machines and `aliases`) served by the platform's own DNS |
| `freeform_tags` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `ipv6private_cidr_blocks` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `is_ipv6enabled` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `is_oracle_gua_allocation_enabled` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `security_attributes` | No | No | An option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it |
| `byoipv6cidr_details` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `byoipv6cidr_details.byoipv6range_id` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `byoipv6cidr_details.ipv6cidr_block` | Yes | Not yet | Dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then |
| `timeouts` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.create` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.delete` | n/a | No | How long Terraform waits: no change in behavior |
| `timeouts.update` | n/a | No | How long Terraform waits: no change in behavior |

### data.oci_core_images

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `compartment_id` | Yes | Yes | Written by the generator |
| `display_name` | n/a | No | Names, labels and descriptions in the account: no change in behavior |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `operating_system` | Yes | Yes | Written by the generator |
| `operating_system_version` | Yes | Yes | Written by the generator |
| `shape` | Yes | Yes | Written by the generator |
| `sort_by` | Yes | Yes | Written by the generator |
| `sort_order` | Yes | Yes | Written by the generator |
| `state` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `filter` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `filter.name` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `filter.regex` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |
| `filter.values` | n/a | No | Another way to write what Isoloom already sets, or a default it keeps: no change in behavior |

### data.oci_identity_availability_domains

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `compartment_id` | Yes | Yes | Written by the generator |
| `id` | n/a | No | The identifier the provider sets: no change in behavior |
| `filter` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `filter.name` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `filter.regex` | n/a | No | Where in the account or region the resource lives: no change in behavior |
| `filter.values` | n/a | No | Where in the account or region the resource lives: no change in behavior |

And 732 other resource types that only this cloud has: its managed services, network services and public DNS. Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: utility providers

100% of 16 portable features (16 done, 0 partly, 0 to do; 225 features in all). From Terraform's built-in provider, hashicorp/random 3.9.1, hashicorp/tls 4.4.1, hashicorp/local 2.9.1, hashicorp/time 0.14.2, hashicorp/null 3.3.2, hashicorp/cloudinit 2.4.1 and hashicorp/http 3.6.2 (their whole schemas).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `terraform_data` | Yes | Yes | Written by the generators: the environment's key and passwords, the project's file list, Windows restarts, and the steps run on each machine |
| `random_bytes` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `random_id` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `random_integer` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `random_password` | Yes | Yes | Written by the generators: the environment's key and passwords, the project's file list, Windows restarts, and the steps run on each machine |
| `random_pet` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `random_shuffle` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `random_string` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `random_uuid` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `random_uuid4` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `random_uuid7` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `tls_cert_request` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `tls_locally_signed_cert` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `tls_private_key` | Yes | Yes | Written by the generators: the environment's key and passwords, the project's file list, Windows restarts, and the steps run on each machine |
| `tls_self_signed_cert` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `local_file` | Yes | Yes | Written by the generators: the environment's key and passwords, the project's file list, Windows restarts, and the steps run on each machine |
| `local_sensitive_file` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `time_offset` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `time_rotating` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `time_sleep` | Yes | Yes | Written by the generators: the environment's key and passwords, the project's file list, Windows restarts, and the steps run on each machine |
| `time_static` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `null_resource` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `cloudinit_config` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### Data sources

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `terraform_remote_state` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `tls_certificate` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `tls_public_key` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `local_command` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `local_file` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `local_sensitive_file` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `null_data_source` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `cloudinit_config` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `http` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `proxy` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `proxy.from_env` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `proxy.password` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `proxy.url` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |
| `proxy.username` | n/a | No | How Terraform reaches and authenticates to the API: no change in behavior |

### terraform_data

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `input` | Yes | Yes | Written by the generator |
| `triggers_replace` | Yes | Yes | Written by the generator |
| `store` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `store.input` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `store.replace` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `store.sensitive` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `store.version` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### data.terraform_remote_state

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `backend` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `config` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `defaults` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `workspace` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### random_bytes

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `keepers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `length` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### random_id

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `byte_length` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `keepers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `prefix` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### random_integer

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `keepers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `max` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `min` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `seed` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### random_password

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `keepers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `length` | Yes | Yes | Written by the generator |
| `lower` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `min_lower` | Yes | Yes | Written by the generator |
| `min_numeric` | Yes | Yes | Written by the generator |
| `min_special` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `min_upper` | Yes | Yes | Written by the generator |
| `number` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `numeric` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `override_special` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `special` | Yes | Yes | Written by the generator |
| `upper` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### random_pet

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `keepers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `length` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `prefix` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `separator` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### random_shuffle

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `input` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `keepers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `result_count` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `seed` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### random_string

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `keepers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `length` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `lower` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `min_lower` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `min_numeric` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `min_special` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `min_upper` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `number` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `numeric` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `override_special` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `special` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `upper` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### random_uuid

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `keepers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### random_uuid4

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `keepers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### random_uuid7

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `keepers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### tls_cert_request

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `dns_names` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `ip_addresses` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `private_key_pem` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `private_key_pem_wo` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `private_key_pem_wo_version` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `uris` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.common_name` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.country` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.email_address` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.locality` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.organization` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.organizational_unit` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.postal_code` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.province` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.serial_number` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.street_address` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### tls_locally_signed_cert

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allowed_uses` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `ca_cert_pem` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `ca_private_key_pem` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `ca_private_key_pem_wo` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `ca_private_key_pem_wo_version` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `cert_request_pem` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `early_renewal_hours` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `is_ca_certificate` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `max_path_length` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `set_subject_key_id` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `validity_period_hours` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### tls_private_key

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `algorithm` | Yes | Yes | Written by the generator |
| `ecdsa_curve` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `rsa_bits` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### tls_self_signed_cert

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allowed_uses` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `dns_names` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `early_renewal_hours` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `ip_addresses` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `is_ca_certificate` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `max_path_length` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `private_key_pem` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `private_key_pem_wo` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `private_key_pem_wo_version` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `set_authority_key_id` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `set_subject_key_id` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `uris` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `validity_period_hours` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.common_name` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.country` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.email_address` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.locality` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.organization` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.organizational_unit` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.postal_code` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.province` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.serial_number` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `subject.street_address` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### data.tls_certificate

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `content` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `url` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `verify_chain` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### data.tls_public_key

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `private_key_openssh` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `private_key_pem` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### local_file

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `content` | Yes | Yes | Written by the generator |
| `content_base64` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `directory_permission` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `file_permission` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `filename` | Yes | Yes | Written by the generator |
| `sensitive_content` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `source` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### local_sensitive_file

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `content` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `content_base64` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `directory_permission` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `file_permission` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `filename` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `source` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### data.local_command

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allow_non_zero_exit_code` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `arguments` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `command` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `stdin` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `working_directory` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### data.local_file

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `filename` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### data.local_sensitive_file

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `filename` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### time_offset

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `base_rfc3339` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `offset_days` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `offset_hours` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `offset_minutes` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `offset_months` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `offset_seconds` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `offset_years` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `triggers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### time_rotating

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `rfc3339` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `rotation_days` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `rotation_hours` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `rotation_minutes` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `rotation_months` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `rotation_rfc3339` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `rotation_years` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `triggers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### time_sleep

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `create_duration` | Yes | Yes | Written by the generator |
| `destroy_duration` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `triggers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### time_static

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `rfc3339` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `triggers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### null_resource

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `triggers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### data.null_data_source

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `has_computed_default` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `inputs` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### cloudinit_config

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `base64_encode` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `boundary` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `gzip` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `part` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `part.content` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `part.content_type` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `part.filename` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `part.merge_type` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### data.cloudinit_config

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `base64_encode` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `boundary` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `gzip` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `part` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `part.content` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `part.content_type` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `part.filename` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `part.merge_type` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

### data.http

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `ca_cert_pem` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `client_cert_pem` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `client_key_pem` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `insecure` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `method` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `request_body` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `request_headers` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `request_timeout_ms` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `url` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `retry` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `retry.attempts` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `retry.max_delay_ms` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |
| `retry.min_delay_ms` | n/a | No | A value, file or wait on Terraform's side: no change in how the environment behaves |

## cloud-init

96% of 80 portable features (77 done, 0 partly, 3 to do; 149 features in all). From cloud-init 26.2's schema-cloud-config-v1.json (crates/isoloom-coverage/coverage/cloud-init.txt).

### Top level

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `allow_public_ssh_keys` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ansible` | Yes | Another way | Via provision[].ansible (the controller runs the playbooks) |
| `apk_repos` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `apt` | Yes | Another way | Via constraints (the prelude points apt and dnf at the proxy and the mirrors) |
| `apt_pipelining` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `apt_reboot_if_required` | n/a | No | A deprecated spelling of a key in this table |
| `apt_update` | n/a | No | A deprecated spelling of a key in this table |
| `apt_upgrade` | n/a | No | A deprecated spelling of a key in this table |
| `authkey_hash` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `autoinstall` | No | No | Ubuntu's installer: the targets boot ready-made images |
| `bootcmd` | Yes | Yes | From `constraints` (the prelude, at every boot) |
| `byobu_by_default` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `ca-certs` | n/a | No | A deprecated spelling of a key in this table |
| `ca_certs` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `chef` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `chpasswd` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `cloud_config_modules` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `cloud_final_modules` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `cloud_init_modules` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `create_hostname_file` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `device_aliases` | Yes | Not yet | Data disks, partitioned and formatted (Wave 3: persistent volumes and data disks) |
| `disable_ec2_metadata` | No | No | EC2's metadata service: AWS only |
| `disable_root` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `disable_root_opts` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `disk_setup` | Yes | Not yet | Data disks, partitioned and formatted (Wave 3: persistent volumes and data disks) |
| `drivers` | No | No | NVIDIA drivers: most targets have no GPU to pass through |
| `fan` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `final_message` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `fqdn` | Yes | Yes | From `machines.*.dns.domain` (the machine's name with its domain; the host name stays the short one) |
| `fs_setup` | Yes | Not yet | Data disks, partitioned and formatted (Wave 3: persistent volumes and data disks) |
| `groups` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `growpart` | Yes | Another way | Via machines.*.resources.disk_gb (the disk-grow step) |
| `grub-dpkg` | n/a | No | A deprecated spelling of a key in this table |
| `grub_dpkg` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `hostname` | Yes | Yes | From `machines` |
| `keyboard` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `landscape` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `launch-index` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `locale` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `locale_configfile` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `lxd` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `manage_etc_hosts` | Yes | Another way | Via machines and machines.*.aliases (the hosts step names every machine) |
| `manage_resolv_conf` | Yes | Another way | Via machines.*.dns (Proxmox's own cloud-init DNS settings) |
| `mcollective` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `merge_how` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `merge_type` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `migrate` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `mount_default_fields` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `mounts` | Yes | Another way | Via volumes (a shared volume is mounted from /etc/fstab) |
| `no_ssh_fingerprints` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ntp` | Yes | Another way | Via constraints.ntp (the prelude sets the time servers) |
| `output` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `package_reboot_if_required` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `package_update` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `package_upgrade` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `packages` | Yes | Yes | The tools the steps need: nftables, curl, the guest agent... |
| `password` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `phone_home` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `power_state` | Yes | Yes | From `machines.*.vm.provision` (a reboot step) |
| `prefer_fqdn_over_hostname` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `preserve_hostname` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `puppet` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `random_seed` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `reporting` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `resize_rootfs` | Yes | Another way | Via machines.*.resources.disk_gb (the disk-grow step) |
| `resolv_conf` | Yes | Another way | Via machines.*.dns (Proxmox's own cloud-init DNS settings) |
| `rh_subscription` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `rpi` | No | No | Raspberry Pi hardware: no VM target has it |
| `rsyslog` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `runcmd` | Yes | Yes | From `machines.*.vm.provision` (every step, in order) |
| `salt_minion` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `snap` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `spacewalk` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `ssh` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ssh_authorized_keys` | Yes | Another way | Via users.*.ssh_authorized_keys (Isoloom's user's keys) |
| `ssh_deletekeys` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ssh_fp_console_blacklist` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ssh_genkeytypes` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ssh_import_id` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ssh_key_console_blacklist` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ssh_keys` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ssh_publish_hostkeys` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ssh_pwauth` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ssh_quiet_keygen` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `swap` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `system_info` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `timezone` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `ubuntu_advantage` | n/a | No | A deprecated spelling of a key in this table |
| `ubuntu_pro` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `updates` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `user` | n/a | No | Isoloom's own user: it needs a name, sudo, a shell and the key, nothing more |
| `users` | Yes | Yes | Isoloom's user, with the operator's and the controller's keys |
| `vendor_data` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `version` | n/a | No | Cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's |
| `wireguard` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `write_files` | Yes | Yes | From `machines.*.vm.provision` (the project's files, the router's and the steps' files) |
| `yum_repo_dir` | Yes | Another way | Via constraints (the prelude points apt and dnf at the proxy and the mirrors) |
| `yum_repos` | Yes | Another way | Via constraints (the prelude points apt and dnf at the proxy and the mirrors) |
| `zypper` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |

### Users (users.*)

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `create-groups` | n/a | No | A deprecated spelling of a key in this table |
| `create_groups` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `doas` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `expiredate` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `gecos` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `groups` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `hashed-passwd` | n/a | No | A deprecated spelling of a key in this table |
| `hashed_passwd` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `homedir` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `inactive` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `lock-passwd` | n/a | No | A deprecated spelling of a key in this table |
| `lock_passwd` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `name` | Yes | Yes | `isoloom` |
| `no-create-home` | n/a | No | A deprecated spelling of a key in this table |
| `no-log-init` | n/a | No | A deprecated spelling of a key in this table |
| `no-user-group` | n/a | No | A deprecated spelling of a key in this table |
| `no_create_home` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `no_log_init` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `no_user_group` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `passwd` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `plain-text-passwd` | n/a | No | A deprecated spelling of a key in this table |
| `plain_text_passwd` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `primary-group` | n/a | No | A deprecated spelling of a key in this table |
| `primary_group` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `selinux-user` | n/a | No | A deprecated spelling of a key in this table |
| `selinux_user` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `shell` | Yes | Yes | Bash |
| `snapuser` | Yes | In the image | One OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`) |
| `ssh-authorized-keys` | n/a | No | A deprecated spelling of a key in this table |
| `ssh-import-id` | n/a | No | A deprecated spelling of a key in this table |
| `ssh-redirect-user` | n/a | No | A deprecated spelling of a key in this table |
| `ssh_authorized_keys` | Yes | Yes | The operator's key, and the controller's |
| `ssh_import_id` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `ssh_redirect_user` | n/a | No | SSH access mechanics: Isoloom reaches its machines as its own user, with its own key |
| `sudo` | Yes | Yes | Passwordless sudo: the steps run as root |
| `system` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |
| `uid` | Yes | In the image | Set it in the machine's provisioning (`vm.provision`) or its image |

### Files (write_files.*)

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `append` | n/a | No | How a file is written: Isoloom sets the path, mode, encoding and content it needs |
| `content` | Yes | Yes | From `machines.*.vm.provision` (the project's files) |
| `defer` | n/a | No | How a file is written: Isoloom sets the path, mode, encoding and content it needs |
| `encoding` | Yes | Yes | Base64, for the project's files |
| `owner` | n/a | No | How a file is written: Isoloom sets the path, mode, encoding and content it needs |
| `path` | Yes | Yes | From `machines.*.vm.provision` (the project's files) |
| `permissions` | Yes | Yes | The inputs file and the reboot scripts |
| `source` | n/a | No | How a file is written: Isoloom sets the path, mode, encoding and content it needs |

### Restart (power_state)

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `condition` | Yes | Yes | Only while a reboot step has a part left |
| `delay` | n/a | No | When cloud-init restarts the machine: right away, once it is done, is what the steps need |
| `message` | Yes | Yes | The reboot step |
| `mode` | Yes | Yes | Reboot |
| `timeout` | n/a | No | When cloud-init restarts the machine: right away, once it is done, is what the steps need |

## Ansible inventory

100% of 31 portable features (31 done, 0 partly, 0 to do; 143 features in all). From ansible-core v2.21.5's behavioral parameters, and pywinrm v0.5.0's for `ansible_winrm_*` (crates/isoloom-coverage/coverage/ansible-inventory.txt).

### Parameters

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `ansible_admin_users` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_async_dir` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_become` | Yes | Yes | Every Linux host: the steps and playbooks run as root |
| `ansible_become_exe` | Yes | Another way | Via (passwordless sudo as root: Isoloom's user needs no other method, user or password) |
| `ansible_become_flags` | Yes | Another way | Via (passwordless sudo as root: Isoloom's user needs no other method, user or password) |
| `ansible_become_method` | Yes | Another way | Via (passwordless sudo as root: Isoloom's user needs no other method, user or password) |
| `ansible_become_pass` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_become_password` | Yes | Another way | Via (passwordless sudo as root: Isoloom's user needs no other method, user or password) |
| `ansible_become_user` | Yes | Another way | Via (passwordless sudo as root: Isoloom's user needs no other method, user or password) |
| `ansible_common_remote_group` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_connection` | Yes | Yes | From `machines.*.vm.os` (WinRM for Windows; SSH with PowerShell on external and for a box with `vm.image.winssh`) |
| `ansible_connection_user` | n/a | No | Ansible's own behavior on the controller, not the machine's |
| `ansible_control_path` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_control_path_dir` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_deprecation_warnings` | n/a | No | Ansible's own behavior on the controller, not the machine's |
| `ansible_docker_extra_args` | n/a | No | Ansible's own behavior on the controller, not the machine's |
| `ansible_facts_modules` | n/a | No | Ansible's own behavior on the controller, not the machine's |
| `ansible_host` | Yes | Yes | From `machines` (the first address; `external.address` on external) |
| `ansible_host_key_checking` | Yes | Another way | Via (the controller's ANSIBLE_HOST_KEY_CHECKING=False: the machines are new) |
| `ansible_inject_invocation` | n/a | No | Ansible's own behavior on the controller, not the machine's |
| `ansible_interpreter_python_fallback` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_local_become_strip_preamble` | n/a | No | Ansible's own behavior on the controller, not the machine's |
| `ansible_local_become_success_timeout` | n/a | No | Ansible's own behavior on the controller, not the machine's |
| `ansible_module_compression` | n/a | No | Ansible's own behavior on the controller, not the machine's |
| `ansible_network_os` | n/a | No | Ansible's own behavior on the controller, not the machine's |
| `ansible_no_target_syslog` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_password` | Yes | Yes | The Vagrant boxes' account; the generated Windows password on Proxmox and the clouds |
| `ansible_pipelining` | Yes | Another way | Via (the controller's ANSIBLE_PIPELINING=True) |
| `ansible_port` | Yes | Yes | From `machines.*.external.port and machines.*.vm.image.winrm` |
| `ansible_private_key` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_private_key_file` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_private_key_passphrase` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_psrp_auth` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_ca_cert` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_cert_trust_path` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_cert_validation` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_certificate_key_password` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_certificate_key_pem` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_certificate_pem` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_configuration_name` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_connection_backoff` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_connection_timeout` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_credssp_auth_mechanism` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_credssp_disable_tlsv1_2` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_credssp_minimum_version` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_host` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_ignore_proxy` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_max_envelope_size` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_message_encryption` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_negotiate_delegate` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_negotiate_hostname_override` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_negotiate_send_cbt` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_negotiate_service` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_no_profile` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_operation_timeout` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_path` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_port` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_protocol` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_proxy` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_read_timeout` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_reconnection_backoff` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_reconnection_retries` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_psrp_user` | n/a | No | PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH |
| `ansible_python_interpreter` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_python_module_rlimit_nofile` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_remote_tmp` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_runas_flags` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_runas_pass` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_runas_user` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_scp_executable` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_scp_extra_args` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_sftp_batch_mode` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_sftp_executable` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_sftp_extra_args` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_shell_allow_world_readable_temp` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_shell_executable` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_shell_type` | Yes | Yes | PowerShell, for Windows hosts over SSH on external |
| `ansible_ssh_args` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_ssh_common_args` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_ssh_executable` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_ssh_extra_args` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_ssh_host` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_ssh_host_key_checking` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_ssh_pass` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_ssh_password` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_ssh_password_mechanism` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_ssh_pipelining` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_ssh_pkcs11_provider` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_ssh_port` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_ssh_private_key` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_ssh_private_key_file` | Yes | Yes | From `machines.*.external.key` (the controller's own key elsewhere) |
| `ansible_ssh_private_key_passphrase` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_ssh_retries` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_ssh_timeout` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_ssh_transfer_method` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_ssh_use_tty` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_ssh_user` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_ssh_verbosity` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_sshpass_prompt` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_su_exe` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_su_flags` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_su_pass` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_su_prompt_l10n` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_su_user` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_sudo_chdir` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_sudo_exe` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_sudo_flags` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_sudo_pass` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_sudo_user` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_system_tmpdirs` | Yes | Another way | Via provision[].host_vars (any variable, on the host's line) |
| `ansible_target_log_info` | n/a | No | Ansible's own behavior on the controller, not the machine's |
| `ansible_timeout` | n/a | No | How the controller's SSH client connects: Isoloom's defaults reach every machine |
| `ansible_user` | Yes | Yes | From `machines.*.external.user and machines.*.vm.os` (each image's own user; `isoloom` on Proxmox and on the clouds' Windows) |
| `ansible_win_async_startup_timeout` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_ca_trust_path` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_cert_key_pem` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_cert_pem` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_connection_timeout` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_credssp_disable_tlsv1_2` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_host` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_winrm_kerberos_delegation` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_kerberos_hostname_override` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_keytab` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_kinit_args` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_kinit_cmd` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_kinit_env_vars` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_kinit_mode` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_message_encryption` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_operation_timeout_sec` | Yes | Yes | 400 s: a domain controller's promotion keeps a call waiting |
| `ansible_winrm_pass` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_winrm_password` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_winrm_path` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_port` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_winrm_proxy` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_read_timeout_sec` | Yes | Yes | 500 s, longer than the operation timeout |
| `ansible_winrm_realm` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_scheme` | Yes | Yes | From `machines.*.vm.image.winrm` (plain HTTP on the clouds) |
| `ansible_winrm_send_cbt` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_server_cert_validation` | Yes | Yes | Off: the machines' certificates are self-signed |
| `ansible_winrm_service` | n/a | No | How the controller's WinRM client connects: Isoloom's defaults reach every Windows machine |
| `ansible_winrm_transport` | Yes | Yes | From `machines.*.vm.image.winrm` (NTLM over HTTPS, basic over HTTP) |
| `ansible_winrm_user` | n/a | No | Another spelling of a parameter in this table (an older or a method-specific one) |
| `ansible_worker_session_isolation` | n/a | No | Ansible's own behavior on the controller, not the machine's |

## Dockerfile

100% of 24 portable features (24 done, 0 partly, 0 to do; 44 features in all). From BuildKit's Dockerfile frontend dockerfile/1.28.0 (crates/isoloom-coverage/coverage/dockerfile.txt).

### Instructions

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `ADD` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |
| `ADD --checksum` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `ADD --chmod` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |
| `ADD --chown` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |
| `ADD --exclude` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `ADD --keep-git-dir` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `ADD --link` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `ADD --unpack` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `ARG` | Yes | Another way | Via machines.*.docker.args (Compose's build arguments) |
| `CMD` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |
| `COPY` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |
| `COPY --chmod` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |
| `COPY --chown` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |
| `COPY --exclude` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `COPY --from` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `COPY --link` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `COPY --parents` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `ENTRYPOINT` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |
| `ENV` | Yes | Another way | Via machines.*.environment (set on the machine itself) |
| `EXPOSE` | Yes | Another way | Via machines.*.services (every port is reachable on the machine's networks) |
| `FROM` | Yes | Yes | From `machines.*.docker.appliance` (the emulator's base image, through the registry mirrors) |
| `FROM --platform` | Yes | Another way | Via machines.*.arch (Compose's platform for the build) |
| `HEALTHCHECK` | Yes | Another way | Via machines.*.services (Isoloom probes every port, in Compose's healthcheck) |
| `HEALTHCHECK --interval` | Yes | Another way | Via machines.*.services (Isoloom probes every port, in Compose's healthcheck) |
| `HEALTHCHECK --retries` | Yes | Another way | Via machines.*.services (Isoloom probes every port, in Compose's healthcheck) |
| `HEALTHCHECK --start-interval` | Yes | Another way | Via machines.*.services (Isoloom probes every port, in Compose's healthcheck) |
| `HEALTHCHECK --start-period` | Yes | Another way | Via machines.*.services (Isoloom probes every port, in Compose's healthcheck) |
| `HEALTHCHECK --timeout` | Yes | Another way | Via machines.*.services (Isoloom probes every port, in Compose's healthcheck) |
| `LABEL` | n/a | No | Image metadata: no effect on the machine |
| `MAINTAINER` | n/a | No | Image metadata: no effect on the machine |
| `ONBUILD` | n/a | No | Runs in images built from this one, not in this machine |
| `RUN` | Yes | Yes | From `machines.*.docker.appliance` (installs the emulator) |
| `RUN --device` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `RUN --mount` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `RUN --network` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `RUN --security` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `SHELL` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `STOPSIGNAL` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |
| `USER` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |
| `VOLUME` | Yes | Another way | Via machines.*.volumes |
| `WORKDIR` | Yes | In the image | The machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`) |

### Parser directives

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `check` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `escape` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |
| `syntax` | n/a | No | How the image is built (caching, layers, sandboxing), not what the machine is |

## Kubernetes

97% of 204 portable features (199 done, 1 partly, 4 to do; 375 features in all). From the Kubernetes v1.37.0 OpenAPI document (the version CI's kind cluster runs).

### Instructions

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `Kinds: Binding` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: ComponentStatus` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: ConfigMap` | Yes | Yes | The project's scripts the pods run: `init:` jobs, checks |
| `Kinds: ControllerRevision` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: CronJob` | Yes | In the image | A scheduled task is the machine's own (cron in its image or its set-up) |
| `Kinds: DaemonSet` | n/a | No | One pod per node: the cluster's agents, not an environment's machines |
| `Kinds: Deployment` | Yes | Yes | From `machines` (one pod each) |
| `Kinds: Endpoints` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: Event` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: IPAddress` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: Ingress` | Yes | Another way | Via `publish`: a LoadBalancer Service on the published port |
| `Kinds: IngressClass` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: Job` | Yes | Yes | From `checks` (a Job per position; `helm test` Pods in the chart) |
| `Kinds: LimitRange` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: Namespace` | Yes | Yes | The environment's own namespace, from its name and instance |
| `Kinds: NetworkPolicy` | Yes | Yes | From networks and reach |
| `Kinds: Node` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: PersistentVolume` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: PersistentVolumeClaim` | Yes | Yes | From `volumes` |
| `Kinds: Pod` | Yes | Yes | From `checks` (a Job per position; `helm test` Pods in the chart) |
| `Kinds: PodTemplate` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: ReplicaSet` | Yes | Another way | Via a Deployment of one replica, with its claims |
| `Kinds: ReplicationController` | Yes | Another way | Via a Deployment of one replica, with its claims |
| `Kinds: ResourceQuota` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: Secret` | Yes | Yes | From `inputs` (in the chart) |
| `Kinds: Service` | Yes | Yes | From `machines.*.services` (its name for the others, and what it publishes) |
| `Kinds: ServiceAccount` | n/a | No | An identity for the Kubernetes API: machines don't call it |
| `Kinds: ServiceCIDR` | n/a | No | Cluster administration: what the cluster runs on, not what an environment is |
| `Kinds: StatefulSet` | Yes | Another way | Via a Deployment of one replica, with its claims |
| `Namespace: apiVersion` | Yes | Yes | Every object Isoloom writes |
| `Namespace: kind` | Yes | Yes | Every object Isoloom writes |
| `Namespace: metadata` | Yes | Yes | Every object Isoloom writes |
| `Namespace: spec` | n/a | No | A namespace's finalizers: how the API deletes it |
| `NamespaceSpec: finalizers` | n/a | No | How the API names and deletes objects: Isoloom names each one |
| `Deployment: apiVersion` | Yes | Yes | Every object Isoloom writes |
| `Deployment: kind` | Yes | Yes | Every object Isoloom writes |
| `Deployment: metadata` | Yes | Yes | Every object Isoloom writes |
| `Deployment: spec` | Yes | Yes | Every object Isoloom writes |
| `DeploymentSpec: minReadySeconds` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `DeploymentSpec: paused` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `DeploymentSpec: progressDeadlineSeconds` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `DeploymentSpec: replicas` | Yes | Yes | One: a machine is one pod |
| `DeploymentSpec: revisionHistoryLimit` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `DeploymentSpec: selector` | Yes | Yes | From `machines` |
| `DeploymentSpec: strategy` | Yes | Yes | Recreate: its claims are mounted by one pod at a time |
| `DeploymentSpec: template` | Yes | Yes | From `machines` |
| `DeploymentStrategy: rollingUpdate` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `DeploymentStrategy: type` | Yes | Yes | Recreate: its claims are mounted by one pod at a time |
| `PodTemplateSpec: metadata` | Yes | Yes | From `machines` |
| `PodTemplateSpec: spec` | Yes | Yes | From `machines` |
| `Pod: apiVersion` | Yes | Yes | Every object Isoloom writes |
| `Pod: kind` | Yes | Yes | Every object Isoloom writes |
| `Pod: metadata` | Yes | Yes | Every object Isoloom writes |
| `Pod: spec` | Yes | Yes | Every object Isoloom writes |
| `PodSpec: activeDeadlineSeconds` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: affinity` | Yes | Yes | From `volumes` (a machine sharing a volume runs on its owner's node) |
| `PodSpec: automountServiceAccountToken` | n/a | No | Access to the Kubernetes API: machines don't call it |
| `PodSpec: containers` | Yes | Yes | From `machines` (the machine's container, and one for its `init:` jobs) |
| `PodSpec: dnsConfig` | Yes | Yes | From `machines.*.dns` |
| `PodSpec: dnsPolicy` | Yes | Yes | From `machines.*.dns` |
| `PodSpec: enableServiceLinks` | n/a | No | Access to the Kubernetes API: machines don't call it |
| `PodSpec: ephemeralContainers` | n/a | No | Debugging containers added to a running pod |
| `PodSpec: evictionResponders` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: hostAliases` | No | No | It points names at fixed addresses, and a pod has none: the spec's `aliases` name a machine, which Kubernetes reaches only by its Service's name (no dots), so Kubernetes refuses `aliases` |
| `PodSpec: hostIPC` | No | No | Shares the node's own namespaces: cloud VMs have no host to share |
| `PodSpec: hostNetwork` | No | No | Shares the node's own namespaces: cloud VMs have no host to share |
| `PodSpec: hostPID` | No | No | Shares the node's own namespaces: cloud VMs have no host to share |
| `PodSpec: hostUsers` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSpec: hostname` | Yes | Yes | The machine's name |
| `PodSpec: hostnameOverride` | Yes | Another way | Via `hostname`, the machine's name |
| `PodSpec: imagePullSecrets` | n/a | No | Credentials of a private registry: the cluster's setup |
| `PodSpec: initContainers` | Yes | Yes | From `machines.*.depends_on` (waits for the dependencies' services), and Isoloom's probe and volume seeds |
| `PodSpec: nodeName` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: nodeSelector` | Yes | Yes | From `machines.*.arch` (a node of the machine's architecture) |
| `PodSpec: os` | No | No | Windows containers only: Isoloom's Windows machines are VMs, which Kubernetes refuses (a Linux pod needs neither field) |
| `PodSpec: overhead` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSpec: preemptionPolicy` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: priority` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: priorityClassName` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: readinessGates` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: resourceClaims` | Yes | Another way | Via machines.*.gpus, as the device plugin's resource in `limits` (`nvidia.com/gpu`, `amd.com/gpu`, `gpu.intel.com/i915`) |
| `PodSpec: resources` | Yes | Another way | Via each container's `resources` |
| `PodSpec: restartPolicy` | Yes | Yes | From `checks` (run once) |
| `PodSpec: runtimeClassName` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSpec: schedulerName` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: schedulingGates` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: schedulingGroup` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: securityContext` | Yes | Yes | From `machines.*.sysctls` |
| `PodSpec: serviceAccount` | n/a | No | Access to the Kubernetes API: machines don't call it |
| `PodSpec: serviceAccountName` | n/a | No | Access to the Kubernetes API: machines don't call it |
| `PodSpec: setHostnameAsFQDN` | Yes | Another way | Via `subdomain`: the pod's full name is in its hosts file (`hostname -f`), and its hostname stays the short name, as on every other target |
| `PodSpec: shareProcessNamespace` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSpec: subdomain` | Yes | Partly | From `machines.*.dns.domain, as one label` (`corp.local` is `corp-local`); not yet: the pod's full name is `<machine>.<subdomain>.<namespace>.svc.<cluster domain>`: Kubernetes names pods under the cluster's domain, not `dns.domain` |
| `PodSpec: terminationGracePeriodSeconds` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: tolerations` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: topologySpreadConstraints` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: volumes` | Yes | Yes | From `volumes, machines.*.tmpfs and machines.*.shm_size` |
| `Container: args` | Yes | Yes | From `machines.*.command` |
| `Container: command` | Yes | Yes | From `machines.*.before` (a shell running the steps, then the command), machines.*.docker.idle and machines.*.docker.init (and Isoloom's own containers) |
| `Container: env` | Yes | Yes | From `machines.*.environment and machines.*.inputs` |
| `Container: envFrom` | Yes | Another way | Via `env`, one variable at a time |
| `Container: image` | Yes | Yes | From `machines.*.docker.image` (or the image the project builds; a value of its own in the chart) |
| `Container: imagePullPolicy` | Yes | Yes | IfNotPresent: an image built on the node is used as it is |
| `Container: lifecycle` | Yes | Another way | Via machines.*.docker.init: a container beside the machine's, run once it answers |
| `Container: livenessProbe` | n/a | No | Restarts a container Kubernetes finds unhealthy: the cluster's self-healing, not the machine's behavior |
| `Container: name` | Yes | Yes | The machine's name |
| `Container: ports` | Yes | Yes | From `machines.*.services` |
| `Container: readinessProbe` | Yes | Yes | From `machines.*.services and machines.*.healthcheck` (ready when its services' ports answer, dormant ones aside, and its own check passes) |
| `Container: resizePolicy` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Container: resources` | Yes | Yes | From `machines.*.resources` |
| `Container: restartPolicy` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Container: restartPolicyRules` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Container: securityContext` | Yes | Yes | From `machines.*.privileged and machines.*.read_only` |
| `Container: startupProbe` | Yes | Yes | From `machines.*.services, when machines.*.healthcheck.http is the readiness probe` (the ports answer first) |
| `Container: stdin` | n/a | No | A terminal for whoever attaches: `isoloom connect` opens its own |
| `Container: stdinOnce` | n/a | No | A terminal for whoever attaches: `isoloom connect` opens its own |
| `Container: terminationMessagePath` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Container: terminationMessagePolicy` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Container: tty` | n/a | No | A terminal for whoever attaches: `isoloom connect` opens its own |
| `Container: volumeDevices` | Yes | Not yet | Raw data disks, with persistent volumes and data disks |
| `Container: volumeMounts` | Yes | Yes | From `machines.*.volumes, machines.*.tmpfs and machines.*.shm_size` |
| `Container: workingDir` | Yes | Yes | From `machines.*.working_dir` |
| `ContainerPort: containerPort` | Yes | Yes | From `machines.*.services[].port` |
| `ContainerPort: hostIP` | Yes | Another way | Via `publish`: a LoadBalancer Service on the published port |
| `ContainerPort: hostPort` | Yes | Another way | Via `publish`: a LoadBalancer Service on the published port |
| `ContainerPort: name` | n/a | No | A name for the port within Kubernetes: Isoloom refers to ports by number |
| `ContainerPort: protocol` | Yes | Yes | From `machines.*.services[].protocol` |
| `EnvVar: name` | Yes | Yes | From `machines.*.environment` (and Isoloom's own variables) |
| `EnvVar: value` | Yes | Yes | From `machines.*.environment` (and Isoloom's own variables) |
| `EnvVar: valueFrom` | Yes | Yes | From `machines.*.inputs` |
| `EnvVarSource: configMapKeyRef` | Yes | Another way | Via `value`: environment values are written as they are |
| `EnvVarSource: fieldRef` | n/a | No | Access to the Kubernetes API: machines don't call it |
| `EnvVarSource: fileKeyRef` | Yes | Another way | Via `value`: environment values are written as they are |
| `EnvVarSource: resourceFieldRef` | n/a | No | Access to the Kubernetes API: machines don't call it |
| `EnvVarSource: secretKeyRef` | Yes | Yes | From `machines.*.inputs` |
| `SecretKeySelector: key` | Yes | Yes | From `machines.*.inputs` (the inputs' Secret) |
| `SecretKeySelector: name` | Yes | Yes | From `machines.*.inputs` (the inputs' Secret) |
| `SecretKeySelector: optional` | Yes | Yes | From `inputs` (an unset input is empty) |
| `ResourceRequirements: claims` | Yes | Another way | Via machines.*.gpus, as the device plugin's resource in `limits` (`nvidia.com/gpu`, `amd.com/gpu`, `gpu.intel.com/i915`) |
| `ResourceRequirements: limits` | Yes | Yes | From `machines.*.resources` (cpus, memory_mb) and machines.*.gpus (the device plugin's resource) |
| `ResourceRequirements: requests` | n/a | No | What the scheduler reserves on a node: the limits are the machine's size |
| `Probe: exec` | Yes | Yes | From `machines.*.services and machines.*.healthcheck.test` (Isoloom's port probe, the same on every target, and the machine's own command) |
| `Probe: failureThreshold` | Yes | Yes | From `machines.*.healthcheck.retries` (else for up to 5 minutes) |
| `Probe: grpc` | Yes | Another way | Via machines.*.healthcheck.test: a gRPC health client the image carries (`grpc_health_probe`) |
| `Probe: httpGet` | Yes | Yes | From `machines.*.healthcheck.http` |
| `Probe: initialDelaySeconds` | Yes | Yes | From `machines.*.healthcheck.start_period` |
| `Probe: periodSeconds` | Yes | Yes | From `machines.*.healthcheck.interval` (else every 5 seconds) |
| `Probe: successThreshold` | Yes | Yes | 1 with a healthcheck: one passing try makes the machine ready, as on Compose and the VMs |
| `Probe: tcpSocket` | Yes | Another way | Via `exec`: Isoloom's port probe checks every service port, TCP and UDP |
| `Probe: terminationGracePeriodSeconds` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Probe: timeoutSeconds` | Yes | Yes | From `machines.*.healthcheck.timeout` |
| `ExecAction: command` | Yes | Yes | From `machines.*.services and machines.*.healthcheck.test` (Isoloom's port probe, the same on every target, and the machine's own command) |
| `HTTPGetAction: host` | Yes | Another way | Via the pod's own address, the default: the healthcheck requests the machine itself |
| `HTTPGetAction: httpHeaders` | Yes | Another way | Via machines.*.healthcheck.test: a command that requests the page as it needs (headers, HTTPS) |
| `HTTPGetAction: path` | Yes | Yes | From `machines.*.healthcheck.http` |
| `HTTPGetAction: port` | Yes | Yes | From `machines.*.healthcheck.http` |
| `HTTPGetAction: protocol` | Yes | Another way | Via machines.*.healthcheck.test: a command that requests the page as it needs (headers, HTTPS) |
| `HTTPGetAction: scheme` | Yes | Another way | Via machines.*.healthcheck.test: a command that requests the page as it needs (headers, HTTPS) |
| `SecurityContext: allowPrivilegeEscalation` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `SecurityContext: appArmorProfile` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `SecurityContext: capabilities` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `SecurityContext: privileged` | Yes | Yes | From `machines.*.privileged` |
| `SecurityContext: procMount` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `SecurityContext: readOnlyRootFilesystem` | Yes | Yes | From `machines.*.read_only` |
| `SecurityContext: runAsGroup` | Yes | Yes | The volumes' seeds run as root, to copy any file |
| `SecurityContext: runAsNonRoot` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `SecurityContext: runAsUser` | Yes | Yes | The volumes' seeds run as root, to copy any file |
| `SecurityContext: seLinuxOptions` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `SecurityContext: seccompProfile` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `SecurityContext: windowsOptions` | No | No | Windows containers only: Isoloom's Windows machines are VMs, which Kubernetes refuses (a Linux pod needs neither field) |
| `PodSecurityContext: appArmorProfile` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSecurityContext: fsGroup` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSecurityContext: fsGroupChangePolicy` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSecurityContext: runAsGroup` | Yes | Another way | Via each container's `securityContext` |
| `PodSecurityContext: runAsNonRoot` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSecurityContext: runAsUser` | Yes | Another way | Via each container's `securityContext` |
| `PodSecurityContext: seLinuxChangePolicy` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSecurityContext: seLinuxOptions` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSecurityContext: seccompProfile` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSecurityContext: supplementalGroups` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSecurityContext: supplementalGroupsPolicy` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSecurityContext: sysctls` | Yes | Yes | From `machines.*.sysctls` |
| `PodSecurityContext: windowsOptions` | No | No | Windows containers only: Isoloom's Windows machines are VMs, which Kubernetes refuses (a Linux pod needs neither field) |
| `Sysctl: name` | Yes | Yes | From `machines.*.sysctls` |
| `Sysctl: value` | Yes | Yes | From `machines.*.sysctls` |
| `PodDNSConfig: nameservers` | Yes | Yes | From `machines.*.dns.servers` |
| `PodDNSConfig: options` | Yes | Yes | From `machines.*.dns.options` (with the cluster's resolver unless the machine has servers of its own) |
| `PodDNSConfig: searches` | Yes | Yes | From `machines.*.dns.search` |
| `PodDNSConfigOption: name` | Yes | Yes | From `machines.*.dns.options` (`name` or `name:value`) |
| `PodDNSConfigOption: value` | Yes | Yes | From `machines.*.dns.options` (`name` or `name:value`) |
| `Affinity: nodeAffinity` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Affinity: podAffinity` | Yes | Yes | From `volumes` (a machine sharing a volume runs on its owner's node) |
| `Affinity: podAntiAffinity` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodAffinity: preferredDuringSchedulingIgnoredDuringExecution` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodAffinity: requiredDuringSchedulingIgnoredDuringExecution` | Yes | Yes | From `volumes` (a machine sharing a volume runs on its owner's node) |
| `PodAffinityTerm: labelSelector` | Yes | Yes | From `volumes` (a machine sharing a volume runs on its owner's node) |
| `PodAffinityTerm: matchLabelKeys` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodAffinityTerm: mismatchLabelKeys` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodAffinityTerm: namespaceSelector` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodAffinityTerm: namespaces` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodAffinityTerm: topologyKey` | Yes | Yes | From `volumes` (a machine sharing a volume runs on its owner's node) |
| `LabelSelector: matchExpressions` | Yes | Another way | Via `matchLabels` |
| `LabelSelector: matchLabels` | Yes | Yes | From machines and networks (the labels the selectors and policies match) |
| `Volume: awsElasticBlockStore` | No | No | One storage system's plugin: no other target has it |
| `Volume: azureDisk` | No | No | One storage system's plugin: no other target has it |
| `Volume: azureFile` | No | No | One storage system's plugin: no other target has it |
| `Volume: cephfs` | No | No | One storage system's plugin: no other target has it |
| `Volume: cinder` | No | No | One storage system's plugin: no other target has it |
| `Volume: configMap` | Yes | Yes | The project's scripts the pods run: `init:` jobs, checks |
| `Volume: csi` | No | No | One storage system's plugin: no other target has it |
| `Volume: downwardAPI` | n/a | No | The pod's own details and API tokens as files: machines don't read them |
| `Volume: emptyDir` | Yes | Yes | From `machines.*.tmpfs and machines.*.shm_size` (and Isoloom's probe) |
| `Volume: ephemeral` | Yes | Another way | Via `emptyDir`, scratch space that goes with the pod |
| `Volume: fc` | No | No | One storage system's plugin: no other target has it |
| `Volume: flexVolume` | No | No | One storage system's plugin: no other target has it |
| `Volume: flocker` | No | No | One storage system's plugin: no other target has it |
| `Volume: gcePersistentDisk` | No | No | One storage system's plugin: no other target has it |
| `Volume: gitRepo` | n/a | No | Deprecated by Kubernetes |
| `Volume: glusterfs` | No | No | One storage system's plugin: no other target has it |
| `Volume: hostPath` | No | No | A folder of the node: cloud VMs share no files with a host |
| `Volume: image` | n/a | No | Another image's files as a volume: the machine's image holds its files |
| `Volume: iscsi` | No | No | One storage system's plugin: no other target has it |
| `Volume: name` | Yes | Yes | From `volumes` (and Isoloom's own: the scripts, the probe) |
| `Volume: nfs` | No | No | One storage system's plugin: no other target has it |
| `Volume: persistentVolumeClaim` | Yes | Yes | From `machines.*.volumes` |
| `Volume: photonPersistentDisk` | No | No | One storage system's plugin: no other target has it |
| `Volume: portworxVolume` | No | No | One storage system's plugin: no other target has it |
| `Volume: projected` | n/a | No | The pod's own details and API tokens as files: machines don't read them |
| `Volume: quobyte` | No | No | One storage system's plugin: no other target has it |
| `Volume: rbd` | No | No | One storage system's plugin: no other target has it |
| `Volume: scaleIO` | No | No | One storage system's plugin: no other target has it |
| `Volume: secret` | Yes | Yes | From `machines.*.secrets` (keys of the `isoloom-secrets` Secret, which `isoloom run` creates) |
| `Volume: storageos` | No | No | One storage system's plugin: no other target has it |
| `Volume: vsphereVolume` | No | No | One storage system's plugin: no other target has it |
| `VolumeMount: bindMountOptions` | n/a | No | A mount detail of the node: Isoloom mounts whole volumes, read-write |
| `VolumeMount: mountPath` | Yes | Yes | From `machines.*.volumes, machines.*.tmpfs and machines.*.shm_size` |
| `VolumeMount: mountPropagation` | n/a | No | A mount detail of the node: Isoloom mounts whole volumes, read-write |
| `VolumeMount: name` | Yes | Yes | From `machines.*.volumes, machines.*.tmpfs and machines.*.shm_size` |
| `VolumeMount: readOnly` | Yes | Yes | Isoloom's probe, mounted read-only |
| `VolumeMount: recursiveReadOnly` | Yes | Another way | Via `readOnly` |
| `VolumeMount: subPath` | Yes | Yes | From `machines.*.files and machines.*.secrets` (each file mounted at its path) |
| `VolumeMount: subPathExpr` | n/a | No | A mount detail of the node: Isoloom mounts whole volumes, read-write |
| `PersistentVolumeClaimVolumeSource: claimName` | Yes | Yes | From `volumes` |
| `PersistentVolumeClaimVolumeSource: readOnly` | Yes | Another way | Via the mount's `readOnly` |
| `ConfigMapVolumeSource: defaultMode` | Yes | Yes | The project's scripts, executable |
| `ConfigMapVolumeSource: defaultUser` | n/a | No | A mount detail of the node: Isoloom mounts whole volumes, read-write |
| `ConfigMapVolumeSource: items` | n/a | No | A mount detail of the node: Isoloom mounts whole volumes, read-write |
| `ConfigMapVolumeSource: name` | Yes | Yes | The project's scripts, executable |
| `ConfigMapVolumeSource: optional` | n/a | No | A mount detail of the node: Isoloom mounts whole volumes, read-write |
| `EmptyDirVolumeSource: medium` | Yes | Yes | From `machines.*.tmpfs and machines.*.shm_size` (in memory) |
| `EmptyDirVolumeSource: mode` | n/a | No | A mount detail of the node: Isoloom mounts whole volumes, read-write |
| `EmptyDirVolumeSource: sizeLimit` | Yes | Yes | From `machines.*.tmpfs and machines.*.shm_size` (in memory) |
| `SecretVolumeSource: defaultMode` | Yes | Another way | Via each key's `mode` |
| `SecretVolumeSource: defaultUser` | Yes | Not yet | A secret's `uid` on Kubernetes, once the clusters Isoloom targets all have it |
| `SecretVolumeSource: items` | Yes | Yes | From `machines.*.secrets` (the machine's own secrets only) |
| `SecretVolumeSource: optional` | n/a | No | `isoloom run` creates the Secret before the pods start: a missing secret stops the run instead |
| `SecretVolumeSource: secretName` | Yes | Yes | From `secrets` (the `isoloom-secrets` Secret) |
| `KeyToPath: key` | Yes | Yes | From `machines.*.secrets` (the machine's own secrets only) |
| `KeyToPath: mode` | Yes | Yes | From `machines.*.secrets[].mode` (default 0444) |
| `KeyToPath: path` | Yes | Yes | From `machines.*.secrets` (the machine's own secrets only) |
| `KeyToPath: user` | Yes | Not yet | A secret's `uid` on Kubernetes, once the clusters Isoloom targets all have it |
| `PersistentVolumeClaim: apiVersion` | Yes | Yes | Every object Isoloom writes |
| `PersistentVolumeClaim: kind` | Yes | Yes | Every object Isoloom writes |
| `PersistentVolumeClaim: metadata` | Yes | Yes | Every object Isoloom writes |
| `PersistentVolumeClaim: spec` | Yes | Yes | Every object Isoloom writes |
| `PersistentVolumeClaimSpec: accessModes` | Yes | Yes | From `volumes` (ReadWriteOnce: a shared volume's machines run on one node) |
| `PersistentVolumeClaimSpec: dataSource` | n/a | No | A claim cloned from a snapshot: snapshots are `isoloom snapshot`'s |
| `PersistentVolumeClaimSpec: dataSourceRef` | n/a | No | A claim cloned from a snapshot: snapshots are `isoloom snapshot`'s |
| `PersistentVolumeClaimSpec: resources` | Yes | Yes | From `volumes` (1Gi each) |
| `PersistentVolumeClaimSpec: selector` | n/a | No | The cluster's storage setup: Isoloom's claims take its default |
| `PersistentVolumeClaimSpec: storageClassName` | n/a | No | The cluster's storage setup: Isoloom's claims take its default |
| `PersistentVolumeClaimSpec: volumeAttributesClassName` | n/a | No | The cluster's storage setup: Isoloom's claims take its default |
| `PersistentVolumeClaimSpec: volumeMode` | Yes | Not yet | Raw data disks, with persistent volumes and data disks |
| `PersistentVolumeClaimSpec: volumeName` | n/a | No | The cluster's storage setup: Isoloom's claims take its default |
| `VolumeResourceRequirements: limits` | n/a | No | The cluster's storage setup: Isoloom's claims take its default |
| `VolumeResourceRequirements: requests` | Yes | Yes | From `volumes` (1Gi each) |
| `Service: apiVersion` | Yes | Yes | Every object Isoloom writes |
| `Service: kind` | Yes | Yes | Every object Isoloom writes |
| `Service: metadata` | Yes | Yes | Every object Isoloom writes |
| `Service: spec` | Yes | Yes | Every object Isoloom writes |
| `ServiceSpec: allocateLoadBalancerNodePorts` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: clusterIP` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: clusterIPs` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: externalIPs` | Yes | Another way | Via `publish`: a LoadBalancer Service on the published port |
| `ServiceSpec: externalName` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: externalTrafficPolicy` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: healthCheckNodePort` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: internalTrafficPolicy` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: ipFamilies` | Yes | Yes | From `networks.*.ipv6` (PreferDualStack; SingleStack IPv6 on IPv6-only networks) |
| `ServiceSpec: ipFamilyPolicy` | Yes | Yes | From `networks.*.ipv6` (PreferDualStack; SingleStack IPv6 on IPv6-only networks) |
| `ServiceSpec: loadBalancerClass` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: loadBalancerIP` | Yes | Another way | Via `publish`: a LoadBalancer Service on the published port |
| `ServiceSpec: loadBalancerSourceRanges` | Yes | Another way | Via the published port's NetworkPolicy |
| `ServiceSpec: ports` | Yes | Yes | From `machines.*.services` (and their `publish`) |
| `ServiceSpec: publishNotReadyAddresses` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: selector` | Yes | Yes | From `machines.*.services` (and their `publish`) |
| `ServiceSpec: sessionAffinity` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: sessionAffinityConfig` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: trafficDistribution` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServiceSpec: type` | Yes | Yes | From `machines.*.services[].publish` (a LoadBalancer Service) |
| `ServicePort: appProtocol` | n/a | No | How the cluster routes a Service: machines reach each other by name and port |
| `ServicePort: name` | Yes | Yes | From `machines.*.services` (and their `publish`) |
| `ServicePort: nodePort` | Yes | Another way | Via `publish`: a LoadBalancer Service on the published port |
| `ServicePort: port` | Yes | Yes | From `machines.*.services` (and their `publish`) |
| `ServicePort: protocol` | Yes | Yes | From `machines.*.services` (and their `publish`) |
| `ServicePort: targetPort` | Yes | Yes | From `machines.*.services` (and their `publish`) |
| `NetworkPolicy: apiVersion` | Yes | Yes | Every object Isoloom writes |
| `NetworkPolicy: kind` | Yes | Yes | Every object Isoloom writes |
| `NetworkPolicy: metadata` | Yes | Yes | Every object Isoloom writes |
| `NetworkPolicy: spec` | Yes | Yes | Every object Isoloom writes |
| `NetworkPolicySpec: egress` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `NetworkPolicySpec: ingress` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `NetworkPolicySpec: podSelector` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `NetworkPolicySpec: policyTypes` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `NetworkPolicyIngressRule: from` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `NetworkPolicyIngressRule: ports` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `NetworkPolicyEgressRule: ports` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `NetworkPolicyEgressRule: to` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `NetworkPolicyPeer: ipBlock` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `NetworkPolicyPeer: namespaceSelector` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `NetworkPolicyPeer: podSelector` | Yes | Yes | From networks and reach (deny by default; a network's machines reach each other) |
| `IPBlock: cidr` | Yes | Yes | From `machines.*.services[].publish and offline networks` (anyone in, or nothing out) |
| `IPBlock: except` | n/a | No | Carves addresses out of a range: Isoloom's rules name whole networks |
| `NetworkPolicyPort: endPort` | Yes | Another way | Via `port`, one rule per port |
| `NetworkPolicyPort: port` | Yes | Yes | From `reach[].ports and machines.*.services[].publish` |
| `NetworkPolicyPort: protocol` | Yes | Yes | From `reach[].ports and machines.*.services[].publish` |
| `Job: apiVersion` | Yes | Yes | Every object Isoloom writes |
| `Job: kind` | Yes | Yes | Every object Isoloom writes |
| `Job: metadata` | Yes | Yes | Every object Isoloom writes |
| `Job: spec` | Yes | Yes | Every object Isoloom writes |
| `JobSpec: activeDeadlineSeconds` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: backoffLimit` | Yes | Yes | From `checks` (run once: a failure is a result) |
| `JobSpec: backoffLimitPerIndex` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: completionMode` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: completions` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: managedBy` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: manualSelector` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: maxFailedIndexes` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: parallelism` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: podFailurePolicy` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: podReplacementPolicy` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: scheduling` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: selector` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: successPolicy` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: suspend` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `JobSpec: template` | Yes | Yes | From `checks` (a Job per position) |
| `JobSpec: ttlSecondsAfterFinished` | n/a | No | How Kubernetes rolls out and retries: a machine is one pod, replaced when it changes |
| `ConfigMap: apiVersion` | Yes | Yes | Every object Isoloom writes |
| `ConfigMap: binaryData` | Yes | Yes | The project's scripts, read from the chart's files |
| `ConfigMap: data` | Yes | Yes | From `machines.*.files[].content` (the project's scripts are `binaryData`, byte for byte) |
| `ConfigMap: immutable` | n/a | No | An API safeguard: Isoloom replaces the object when the spec changes |
| `ConfigMap: kind` | Yes | Yes | Every object Isoloom writes |
| `ConfigMap: metadata` | Yes | Yes | Every object Isoloom writes |
| `Secret: apiVersion` | Yes | Yes | Every object Isoloom writes |
| `Secret: data` | Yes | Yes | From `inputs` (in the chart) |
| `Secret: immutable` | n/a | No | An API safeguard: Isoloom replaces the object when the spec changes |
| `Secret: kind` | Yes | Yes | Every object Isoloom writes |
| `Secret: metadata` | Yes | Yes | Every object Isoloom writes |
| `Secret: stringData` | Yes | Another way | Via `data` |
| `Secret: type` | Yes | Yes | Opaque |
| `ObjectMeta: annotations` | Yes | Yes | From `checks` (`helm test` hooks, in the chart) |
| `ObjectMeta: creationTimestamp` | n/a | No | Written by the cluster, not by a manifest |
| `ObjectMeta: deletionGracePeriodSeconds` | n/a | No | Written by the cluster, not by a manifest |
| `ObjectMeta: deletionTimestamp` | n/a | No | Written by the cluster, not by a manifest |
| `ObjectMeta: finalizers` | n/a | No | How the API names and deletes objects: Isoloom names each one |
| `ObjectMeta: generateName` | n/a | No | How the API names and deletes objects: Isoloom names each one |
| `ObjectMeta: generation` | n/a | No | Written by the cluster, not by a manifest |
| `ObjectMeta: labels` | Yes | Yes | From machines and networks (the labels the selectors and policies match) |
| `ObjectMeta: managedFields` | n/a | No | Written by the cluster, not by a manifest |
| `ObjectMeta: name` | Yes | Yes | Every object Isoloom writes |
| `ObjectMeta: namespace` | Yes | Another way | Via the kustomization's `namespace` and the Helm release's |
| `ObjectMeta: ownerReferences` | n/a | No | How the API names and deletes objects: Isoloom names each one |
| `ObjectMeta: resourceVersion` | n/a | No | Written by the cluster, not by a manifest |
| `ObjectMeta: selfLink` | n/a | No | Written by the cluster, not by a manifest |
| `ObjectMeta: uid` | n/a | No | Written by the cluster, not by a manifest |

## Kustomize

100% of 15 portable features (15 done, 0 partly, 0 to do; 46 features in all). From SchemaStore's kustomization.json (pinned).

### Instructions

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `Kustomization: apiVersion` | Yes | Yes | The manifests, and the checks' Jobs beside them |
| `Kustomization: bases` | n/a | No | Kustomizations layered on others: Isoloom's is one layer |
| `Kustomization: buildMetadata` | n/a | No | Kustomize's plugins and build settings: no effect on the environment |
| `Kustomization: commonAnnotations` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: commonLabels` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: components` | n/a | No | Kustomizations layered on others: Isoloom's is one layer |
| `Kustomization: configMapGenerator` | Yes | Yes | The project's scripts the pods run: `init:` jobs, checks |
| `Kustomization: configurations` | n/a | No | Kustomize's plugins and build settings: no effect on the environment |
| `Kustomization: crds` | n/a | No | Custom resource definitions: Isoloom writes built-in kinds only |
| `Kustomization: generatorOptions` | Yes | Yes | The project's scripts the pods run: `init:` jobs, checks |
| `Kustomization: generators` | n/a | No | Kustomize's plugins and build settings: no effect on the environment |
| `Kustomization: helmCharts` | n/a | No | Helm inside Kustomize: Isoloom writes the chart beside the manifests |
| `Kustomization: helmGlobals` | n/a | No | Helm inside Kustomize: Isoloom writes the chart beside the manifests |
| `Kustomization: images` | Yes | Another way | Via the chart's `images` values |
| `Kustomization: inventory` | n/a | No | Kustomize's plugins and build settings: no effect on the environment |
| `Kustomization: kind` | Yes | Yes | The manifests, and the checks' Jobs beside them |
| `Kustomization: labels` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: metadata` | n/a | No | Kustomize's plugins and build settings: no effect on the environment |
| `Kustomization: namePrefix` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: nameSuffix` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: namespace` | Yes | Yes | The environment's namespace |
| `Kustomization: openapi` | n/a | No | Kustomize's plugins and build settings: no effect on the environment |
| `Kustomization: patches` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: patchesJson6902` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: patchesStrategicMerge` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: replacements` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: replicas` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: resources` | Yes | Yes | The manifests, and the checks' Jobs beside them |
| `Kustomization: secretGenerator` | Yes | Another way | Via the chart's inputs Secret, from its values |
| `Kustomization: sortOptions` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `Kustomization: transformers` | n/a | No | Kustomize's plugins and build settings: no effect on the environment |
| `Kustomization: validators` | n/a | No | Kustomize's plugins and build settings: no effect on the environment |
| `Kustomization: vars` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `ConfigMapArgs: KVSources` | Yes | Another way | Via `files`: each script as it is |
| `ConfigMapArgs: behavior` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `ConfigMapArgs: env` | Yes | Another way | Via `files`: each script as it is |
| `ConfigMapArgs: envs` | Yes | Another way | Via `files`: each script as it is |
| `ConfigMapArgs: files` | Yes | Yes | The project's scripts the pods run: `init:` jobs, checks |
| `ConfigMapArgs: literals` | Yes | Another way | Via `files`: each script as it is |
| `ConfigMapArgs: name` | Yes | Yes | The project's scripts the pods run: `init:` jobs, checks |
| `ConfigMapArgs: namespace` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `ConfigMapArgs: options` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `GeneratorOptions: annotations` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `GeneratorOptions: disableNameSuffixHash` | Yes | Yes | The pods name the scripts' ConfigMap as it is |
| `GeneratorOptions: immutable` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |
| `GeneratorOptions: labels` | n/a | No | Edits the objects it builds: Isoloom writes them as they should be |

## Helm

100% of 19 portable features (19 done, 0 partly, 0 to do; 52 features in all). From SchemaStore's chart.json (pinned) and Helm 3.16's chart documentation.

### Instructions

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `Chart: annotations` | n/a | No | How the chart is listed in a repository: no effect on the environment |
| `Chart: apiVersion` | Yes | Yes | V2 |
| `Chart: appVersion` | Yes | Yes | 0.1.0; `isoloom chart --version` packages another |
| `Chart: dependencies` | n/a | No | Charts within the chart: an environment is one chart |
| `Chart: deprecated` | n/a | No | How the chart is listed in a repository: no effect on the environment |
| `Chart: description` | Yes | Yes | From `name` (the environment it installs) |
| `Chart: home` | n/a | No | How the chart is listed in a repository: no effect on the environment |
| `Chart: icon` | n/a | No | How the chart is listed in a repository: no effect on the environment |
| `Chart: keywords` | n/a | No | How the chart is listed in a repository: no effect on the environment |
| `Chart: kubeVersion` | n/a | No | The cluster versions the chart accepts: Isoloom writes objects every supported version has |
| `Chart: maintainers` | n/a | No | How the chart is listed in a repository: no effect on the environment |
| `Chart: name` | Yes | Yes | From `name` |
| `Chart: sources` | n/a | No | How the chart is listed in a repository: no effect on the environment |
| `Chart: type` | Yes | Yes | Application |
| `Chart: version` | Yes | Yes | 0.1.0; `isoloom chart --version` packages another |
| `ChartDependencies: alias` | n/a | No | Charts within the chart: an environment is one chart |
| `ChartDependencies: condition` | n/a | No | Charts within the chart: an environment is one chart |
| `ChartDependencies: depends-on` | n/a | No | Charts within the chart: an environment is one chart |
| `ChartDependencies: import-values` | n/a | No | Charts within the chart: an environment is one chart |
| `ChartDependencies: name` | n/a | No | Charts within the chart: an environment is one chart |
| `ChartDependencies: repository` | n/a | No | Charts within the chart: an environment is one chart |
| `ChartDependencies: tags` | n/a | No | Charts within the chart: an environment is one chart |
| `ChartDependencies: version` | n/a | No | Charts within the chart: an environment is one chart |
| `ChartMaintainers: email` | n/a | No | How the chart is listed in a repository: no effect on the environment |
| `ChartMaintainers: name` | n/a | No | How the chart is listed in a repository: no effect on the environment |
| `ChartMaintainers: url` | n/a | No | How the chart is listed in a repository: no effect on the environment |
| `Chart files: Chart.yaml` | Yes | Yes | The chart itself |
| `Chart files: values.yaml` | Yes | Yes | From `machines` (each one's image) and inputs |
| `Chart files: values.schema.json` | Yes | Yes | From `machines` (each one's image, required, non-empty) and inputs (scalars): `helm install` rejects any other value |
| `Chart files: templates/` | Yes | Yes | The environment's objects, its scripts and its inputs |
| `Chart files: templates/NOTES.txt` | Yes | Yes | From `checks` (how to run them after an install) |
| `Chart files: templates/_helpers.tpl` | n/a | No | Named templates shared by a chart's templates: Isoloom writes each object whole |
| `Chart files: templates/tests/` | Yes | Yes | From `checks` (a `helm test` Pod per position) |
| `Chart files: .Files` | Yes | Yes | From checks and the project's scripts (read into ConfigMaps) |
| `Chart files: charts/` | n/a | No | Charts within the chart: an environment is one chart |
| `Chart files: crds/` | No | No | Custom resource definitions are cluster-wide: no other target has them |
| `Chart files: Chart.lock` | n/a | No | Charts within the chart: an environment is one chart |
| `Chart files: LICENSE` | n/a | No | The chart's packaging and documentation: no effect on the environment |
| `Chart files: README.md` | n/a | No | The chart's packaging and documentation: no effect on the environment |
| `Chart files: .helmignore` | n/a | No | The chart's packaging and documentation: no effect on the environment |
| `Hooks: pre-install` | Yes | Another way | Via init containers, which wait for each machine's dependencies |
| `Hooks: post-install` | Yes | Another way | Via machines.*.docker.init: a container beside the machine's, run once it answers |
| `Hooks: pre-delete` | n/a | No | Steps around Helm's own operations: an environment has no tear-down steps |
| `Hooks: post-delete` | n/a | No | Steps around Helm's own operations: an environment has no tear-down steps |
| `Hooks: pre-upgrade` | Yes | Another way | Via init containers, which wait for each machine's dependencies |
| `Hooks: post-upgrade` | Yes | Another way | Via machines.*.docker.init: a container beside the machine's, run once it answers |
| `Hooks: pre-rollback` | n/a | No | Steps around Helm's own operations: an environment has no tear-down steps |
| `Hooks: post-rollback` | n/a | No | Steps around Helm's own operations: an environment has no tear-down steps |
| `Hooks: test` | Yes | Yes | From `checks` (`helm test` runs them) |
| `Hooks: helm.sh/hook-weight` | n/a | No | Orders hooks of one kind: Isoloom's checks are independent |
| `Hooks: helm.sh/hook-delete-policy` | Yes | Yes | From `checks` (a run replaces the last one's Pods) |
| `Hooks: helm.sh/resource-policy` | n/a | No | Keeps an object when the release is deleted: `isoloom down` removes everything |

## From the spec's side

The other direction: every field of an Isoloom spec, and what each output does with it. A column per
output, each cloud of `cloud-vm` its own (aws to oci). An output inherits what its rendering does
(the Compose file runs on docker, hosted, docker-vm and cloud-docker; one Kubernetes backend writes
the manifests, the Helm chart and the Kustomize overlay; hybrid is Compose for its containers and
Vagrant for its VMs), and every backend is asked to render each field's probe spec: what it refuses
is marked ✗, with its reason below.

✓ done · ◐ partial · planned · ✗ refused by the backend · n/a: means nothing for this output · info: descriptive only · core: read by Isoloom itself

| Field | docker | hosted | kubernetes | helm | kustomize | vagrant | docker-vm | hybrid | proxmox | aws | azure | gcp | digitalocean | linode | oci | cloud-docker | cloud-services | external |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| `version` | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core |
| `name` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | core |
| `networks.*.cidr` | ✓ | ✓ | ◐ | ◐ | ◐ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ✓ | n/a | ✓ |
| `networks.*.tc.delay` | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | n/a | planned |
| `networks.*.tc.jitter` | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | n/a | planned |
| `networks.*.tc.loss` | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | n/a | planned |
| `networks.*.tc.rate` | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | n/a | planned |
| `networks.*.docker.cidr` | ✓ | ✓ | n/a | n/a | n/a | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `networks.*.vlans.*.cidr` | ✓ | ✓ | ◐ | ◐ | ◐ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ◐ | ✗ | ✗ | ✗ | ✓ | n/a | ✓ |
| `networks.*.switch` | ✓ | ✓ | n/a | n/a | n/a | n/a | ✓ | ✗ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `networks.*.vlans.*.internet` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ◐ | ✗ | ✗ | ✗ | ✓ | n/a | ◐ |
| `networks.*.internet` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ✓ | n/a | ◐ |
| `networks.*.gateway` | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | n/a | n/a |
| `networks.*.ipv6.cidr` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | planned | ✗ | ✗ | ✗ | ✗ | ✓ | n/a | ✓ |
| `networks.*.ipv6.addresses` | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | n/a | ✓ |
| `networks.*.ipv6.only` | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | n/a | ✗ |
| `networks.*.external` | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `networks.*.bridged.interface` | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `networks.*.bridged.dhcp` | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `networks.*.bridged.default_route` | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `networks.*.bridged.vlan` | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `reach[].from` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ | n/a | ◐ |
| `reach[].to` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ | n/a | ◐ |
| `reach[].ports` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ | n/a | ◐ |
| `inputs` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | planned |
| `secrets` | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | ✓ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ✓ | n/a | ◐ |
| `machines.*.networks` | ✓ | ✓ | ◐ | ◐ | ◐ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ✓ | n/a | ✓ |
| `machines.*.services[].port` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ |
| `cloud.provider` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | ✓ | info |
| `cloud.terraform` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | ✓ | info |
| `cloud.vars` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | ✓ | info |
| `cloud.outputs` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | ✓ | info |
| `cloud.hourly_usd` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | ✓ | info |
| `cloud.before` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | ✓ | info |
| `cloud.env` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | ✓ | info |
| `cloud.apply_retries` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | ✓ | info |
| `cloud.requires` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | ✓ | info |
| `machines.*.services[].name` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info |
| `machines.*.services[].publish` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a |
| `machines.*.services[].fixed` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a |
| `machines.*.services[].dormant` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ |
| `machines.*.services[].protocol` | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | ✓ | ◐ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ |
| `machines.*.services[].tls` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info |
| `machines.*.services[].http` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info |
| `machines.*.inputs` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ✓ | n/a | planned |
| `machines.*.secrets` | ✓ | ✓ | ◐ | ◐ | ◐ | ◐ | ✓ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ✓ | n/a | ◐ |
| `machines.*.environment` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | planned |
| `machines.*.sysctls` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | planned |
| `machines.*.ulimits` | ✓ | ✓ | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | planned |
| `machines.*.aliases` | ✓ | ✓ | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | planned |
| `machines.*.arch` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `machines.*.privileged` | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.nested_virtualization` | ✗ | ✗ | ✗ | ✗ | ✗ | ◐ | ✗ | ✗ | ✓ | ◐ | ✗ | ◐ | ✗ | ✗ | ◐ | ✗ | n/a | n/a |
| `machines.*.gpus.count` | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ◐ | ◐ | ◐ | ◐ | ◐ | ✗ | ✗ | n/a | n/a |
| `machines.*.gpus.ids` | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `machines.*.gpus.driver` | ✓ | ✓ | ◐ | ◐ | ◐ | ✗ | ✗ | ✗ | n/a | ◐ | ◐ | ◐ | ◐ | ◐ | ✗ | ✗ | n/a | n/a |
| `machines.*.gpus.capabilities` | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✗ | n/a | n/a |
| `machines.*.gpus.options` | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✗ | n/a | n/a |
| `machines.*.devices[].host` | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `machines.*.devices[].guest` | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `machines.*.devices[].permissions` | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `machines.*.read_only` | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.tmpfs` | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.shm_size` | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.dns.servers` | ✓ | ✓ | ◐ | ◐ | ◐ | n/a | ✓ | ✓ | ✓ | planned | planned | planned | planned | planned | planned | ✓ | n/a | n/a |
| `machines.*.dns.search` | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | planned | planned | planned | planned | planned | planned | planned | ✓ | n/a | n/a |
| `machines.*.dns.domain` | ✓ | ✓ | ◐ | ◐ | ◐ | n/a | ✓ | ✓ | ✓ | planned | planned | planned | planned | planned | planned | ✓ | n/a | n/a |
| `machines.*.dns.options` | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | planned | planned | planned | planned | planned | planned | planned | ✓ | n/a | n/a |
| `machines.*.resources.cpus` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | planned | planned | planned | planned | planned | planned | ✓ | n/a | n/a |
| `machines.*.resources.memory_mb` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a |
| `machines.*.resources.disk_gb` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a |
| `machines.*.command` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.working_dir` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.before` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.healthcheck.test` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.healthcheck.http.port` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.healthcheck.http.path` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.healthcheck.interval` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.healthcheck.timeout` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.healthcheck.retries` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.healthcheck.start_period` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.depends_on` | ✓ | ✓ | ◐ | ◐ | ◐ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✓ | ✓ | n/a | ◐ |
| `volumes.*.owner` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ◐ | ◐ | ◐ | ✗ | ◐ | ◐ | ✓ | n/a | planned |
| `machines.*.volumes` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ✓ | n/a | planned |
| `machines.*.files[].path` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.files[].source` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.files[].content` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✗ |
| `machines.*.files[].uid` | ◐ | ◐ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | n/a | ✗ |
| `machines.*.files[].gid` | ◐ | ◐ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | n/a | ✗ |
| `machines.*.files[].mode` | ◐ | ◐ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | n/a | ✗ |
| `machines.*.access` | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | ✓ | ◐ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✓ | ✓ | n/a | ✓ |
| `machines.*.docker.image` | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.docker.build` | ✓ | ✓ | ◐ | ◐ | ◐ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.docker.dockerfile` | ✓ | ✓ | ◐ | ◐ | ◐ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.docker.target` | ✓ | ✓ | ◐ | ◐ | ◐ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.docker.args` | ✓ | ✓ | ◐ | ◐ | ◐ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.docker.init` | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.docker.appliance` | ✓ | ✓ | n/a | n/a | n/a | n/a | ✓ | ✗ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.docker.firmware` | ✓ | ✓ | n/a | n/a | n/a | n/a | ✓ | ✗ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.docker.config` | ✓ | ✓ | n/a | n/a | n/a | n/a | ✓ | ✗ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.docker.idle` | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a |
| `machines.*.vm.os` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | n/a | n/a | ◐ |
| `machines.*.vm.image.vagrant` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| `machines.*.vm.image.vagrant_version` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| `machines.*.vm.image.qemu` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| `machines.*.vm.image.hyperv` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| `machines.*.vm.image.winrm` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| `machines.*.vm.image.winssh` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| `machines.*.vm.image.box_url` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| `machines.*.vm.image.box_server_url` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| `machines.*.vm.image.proxmox` | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| `machines.*.vm.image.aws` | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a |
| `machines.*.vm.image.azure` | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a |
| `machines.*.vm.image.gcp` | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a |
| `machines.*.vm.image.digitalocean` | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a |
| `machines.*.vm.image.linode` | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a |
| `machines.*.vm.image.oci` | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a |
| `machines.*.vm.boot.kernel` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.initrd` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.cmdline` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.dtb` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.loader` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.nvram` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.network.tftp_root` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.network.file` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.network.server` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.firmware` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.firmware_format` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.boot.order` | n/a | n/a | n/a | n/a | n/a | ◐ | n/a | ✗ | ◐ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | n/a | n/a |
| `machines.*.vm.provision` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ◐ |
| `machines.*.vm.tpm` | n/a | n/a | n/a | n/a | n/a | ✓ | n/a | ✓ | ✓ | ◐ | ◐ | ◐ | ✗ | ✗ | ◐ | n/a | n/a | n/a |
| `provision[].ansible` | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | planned | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | n/a | ✓ |
| `provision[].inventory` | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | planned | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | n/a | ✓ |
| `provision[].groups` | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | planned | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | n/a | ✓ |
| `provision[].vars` | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | planned | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | n/a | ✓ |
| `provision[].host_vars` | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | planned | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | n/a | ✓ |
| `controller` | ✗ | ✗ | n/a | n/a | n/a | ✓ | ✗ | planned | ✓ | ◐ | ◐ | ◐ | ✗ | ✗ | ✗ | ✗ | n/a | n/a |
| `provision[].requirements` | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | planned | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | n/a | planned |
| `checks[]` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ✓ | ✓ | ◐ |
| `checks[].name` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `checks[].from` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ |
| `checks[].http` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `checks[].tcp` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `checks[].method` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `checks[].headers` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `checks[].body` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `checks[].contains` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `checks[].follow` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `checks[].send` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `checks[].exec` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ |
| `checks[].script` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | planned |
| `checks[].expect` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `checks[].wait` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `targets` | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core |
| `tools` | ✓ | ✓ | planned | planned | planned | ✓ | ✓ | ✓ | planned | planned | planned | planned | planned | planned | planned | ✓ | n/a | n/a |
| `machines.*.external.address` | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | ✓ |
| `machines.*.external.user` | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | ✓ |
| `machines.*.external.port` | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | ✓ |
| `machines.*.external.key` | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | ✓ |
| `message` | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | info | ✓ | info |
| `machines.*.count` | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core |
| `common` | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core | core |
| `groups` | core | core | core | core | core | ✓ | core | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | core | n/a | ✓ |

| Output | Handled | Done | Partial | Planned | Refused |
| --- | ---: | ---: | ---: | ---: | ---: |
| docker | 87% of 110 | 95 | 3 | 0 | 12 |
| hosted | 87% of 110 | 95 | 3 | 0 | 12 |
| kubernetes | 71% of 102 | 67 | 12 | 1 | 22 |
| helm | 71% of 102 | 67 | 12 | 1 | 22 |
| kustomize | 71% of 102 | 67 | 12 | 1 | 22 |
| vagrant | 82% of 115 | 86 | 17 | 0 | 12 |
| docker-vm | 79% of 110 | 86 | 3 | 0 | 21 |
| hybrid | 66% of 135 | 86 | 7 | 7 | 35 |
| proxmox | 79% of 109 | 85 | 4 | 3 | 17 |
| aws | 61% of 115 | 62 | 17 | 6 | 30 |
| azure | 60% of 115 | 62 | 16 | 7 | 30 |
| gcp | 61% of 115 | 62 | 17 | 6 | 30 |
| digitalocean | 49% of 115 | 51 | 11 | 6 | 47 |
| linode | 51% of 115 | 53 | 12 | 6 | 44 |
| oci | 51% of 115 | 53 | 12 | 6 | 44 |
| cloud-docker | 78% of 110 | 85 | 3 | 0 | 22 |
| cloud-services | 100% of 25 | 25 | 0 | 0 | 0 |
| external | 50% of 74 | 32 | 11 | 14 | 17 |

### Refusals

In the backends' words, from their probes (a machine a reason names is the probe's own).

- **kubernetes, helm, kustomize**: Link impairment (`tc`) needs a router in the path; Kubernetes has none (`networks.*.tc.delay`, `networks.*.tc.jitter`, `networks.*.tc.loss`, `networks.*.tc.rate`)
- **aws, azure**: Link impairment (`tc`) needs a router in the path; the cloud's security groups have none (`networks.*.tc.delay`, `networks.*.tc.jitter`, `networks.*.tc.loss`, `networks.*.tc.rate`)
- **gcp**: Link impairment (`tc`) needs a router in the path; the cloud's firewall rules have none (`networks.*.tc.delay`, `networks.*.tc.jitter`, `networks.*.tc.loss`, `networks.*.tc.rate`)
- **digitalocean, linode, oci**: Link impairment (`tc`) needs a router in the path; the cloud's firewalls have none (`networks.*.tc.delay`, `networks.*.tc.jitter`, `networks.*.tc.loss`, `networks.*.tc.rate`)
- **digitalocean**: DigitalOcean cloud-vm takes single-network labs so far (`networks.*.vlans.*.cidr`, `networks.*.vlans.*.internet`, `networks.*.gateway`, `reach[].from`, `reach[].to`, `reach[].ports`)
- **linode**: Linode cloud-vm takes single-network labs so far (`networks.*.vlans.*.cidr`, `networks.*.vlans.*.internet`, `networks.*.gateway`, `reach[].from`, `reach[].to`, `reach[].ports`)
- **oci**: Oracle Cloud cloud-vm takes single-network labs so far (`networks.*.vlans.*.cidr`, `networks.*.vlans.*.internet`, `networks.*.gateway`, `reach[].from`, `reach[].to`, `reach[].ports`)
- **hybrid**: Machine `sw` is a network appliance (`docker.appliance`), which runs on the Docker targets (`networks.*.switch`)
- **kubernetes, helm, kustomize**: Networks with a `gateway` machine on Kubernetes come later (`networks.*.gateway`)
- **hybrid**: Networks with a `gateway` machine in hybrid environments come later (`networks.*.gateway`)
- **proxmox**: Networks with a `gateway` machine on Proxmox come later (`networks.*.gateway`)
- **aws, azure, gcp**: Networks with a `gateway` machine in the cloud come later (`networks.*.gateway`)
- **hybrid**: Network `lan` is IPv6 (`ipv6`), which this target's networks don't carry yet (`networks.*.ipv6.cidr`, `networks.*.ipv6.addresses`, `networks.*.ipv6.only`)
- **aws**: Network `lan` is IPv6: an AWS VPC takes an Amazon-provided (or your own registered) IPv6 block, never one the spec picks (`networks.*.ipv6.cidr`)
- **gcp**: Network `lan` is IPv6: Google Cloud assigns a VPC's internal IPv6 range itself (inside fd20::/20), so the spec's block can't be kept (`networks.*.ipv6.cidr`)
- **digitalocean**: Network `lan` is IPv6: DigitalOcean VPCs are IPv4 only (a droplet's IPv6 address is a public one) (`networks.*.ipv6.cidr`)
- **linode**: Network `lan` is IPv6: a Linode VPC's IPv6 ranges are Linode's to assign, so the spec's block can't be kept (`networks.*.ipv6.cidr`)
- **oci**: Network `lan` is IPv6: dual-stack virtual cloud networks on Oracle Cloud come later (`networks.*.ipv6.cidr`)
- **vagrant, proxmox, aws, azure, gcp, digitalocean, linode, oci**: Network `lan` leaves IPv6 addresses to the target (`ipv6.addresses: auto`), and nothing on this target hands them out: use `static` (`networks.*.ipv6.addresses`)
- **vagrant, proxmox, aws, azure, gcp, digitalocean, linode, oci, external**: Network `lan` is IPv6-only (`ipv6.only`): this target needs IPv4 on every network (its router, its set-up, its transport) (`networks.*.ipv6.only`)
- **kubernetes, helm, kustomize, vagrant, docker-vm, hybrid, aws, azure, gcp, digitalocean, linode, oci, cloud-docker**: Network `corp` is an existing network (`external: corp-net`), and this target makes every network it uses (`docker`, `hosted` and `proxmox` join existing ones) (`networks.*.external`)
- **docker, hosted, kubernetes, helm, kustomize, docker-vm, hybrid, aws, azure, gcp, digitalocean, linode, oci, cloud-docker**: Network `home` is bridged to a host interface (`bridged`), which this target has no way to do (`vagrant` and `proxmox` bridge one) (`networks.*.bridged.interface`, `networks.*.bridged.dhcp`, `networks.*.bridged.default_route`, `networks.*.bridged.vlan`)
- **vagrant**: Network `home`: Vagrant's bridged networks take no VLAN tag (Proxmox does) (`networks.*.bridged.vlan`)
- **proxmox**: Machine `a`: arm64 on Proxmox comes later (arm64 is uncommon on Proxmox hosts) (`machines.*.arch`)
- **aws**: Machine `a`: arm64 on AWS comes later (Graviton instances and arm64 AMIs) (`machines.*.arch`)
- **azure**: Machine `a`: arm64 on Azure comes later (Arm-based sizes and arm64 images) (`machines.*.arch`)
- **gcp**: Machine `a`: arm64 on Google Cloud comes later (Arm-based machine types and arm64 images) (`machines.*.arch`)
- **digitalocean**: Machine `a`: arm64 on DigitalOcean comes later (it has no arm64 droplets) (`machines.*.arch`)
- **linode**: Machine `a`: arm64 on Linode comes later (it has no arm64 plans) (`machines.*.arch`)
- **oci**: Machine `a`: arm64 on Oracle Cloud comes later (Ampere shapes and arm64 images) (`machines.*.arch`)
- **cloud-docker**: Machine `a`: arm64 on a cloud VM comes later (Graviton instances and arm64 images) (`machines.*.arch`)
- **docker, hosted, kubernetes, helm, kustomize, docker-vm, hybrid, cloud-docker**: Machine `a` runs VMs inside (`nested_virtualization`), which a container can't: it runs as a VM on the VM targets (`machines.*.nested_virtualization`)
- **azure**: Machine `a`: nested virtualization (`nested_virtualization`) on Azure comes later (AWS, Google Cloud and Oracle Cloud have it) (`machines.*.nested_virtualization`)
- **digitalocean**: Machine `a`: nested virtualization (`nested_virtualization`) on DigitalOcean comes later (AWS, Google Cloud and Oracle Cloud have it) (`machines.*.nested_virtualization`)
- **linode**: Machine `a`: nested virtualization (`nested_virtualization`) on Linode comes later (AWS, Google Cloud and Oracle Cloud have it) (`machines.*.nested_virtualization`)
- **vagrant**: Machine `a` asks for GPUs (`gpus`): Vagrant passes no GPU to a local VM (Proxmox and the cloud targets do) (`machines.*.gpus.count`, `machines.*.gpus.ids`, `machines.*.gpus.driver`, `machines.*.gpus.capabilities`, `machines.*.gpus.options`)
- **docker-vm, hybrid, cloud-docker**: Machine `a` asks for GPUs (`gpus`): its container runs on a VM Isoloom creates, which has no GPU (the `docker` and `kubernetes` targets give the host's) (`machines.*.gpus.count`, `machines.*.gpus.ids`, `machines.*.gpus.driver`, `machines.*.gpus.capabilities`, `machines.*.gpus.options`)
- **proxmox**: Machine `a`: Proxmox passes the server's GPUs by name: list them in `gpus.ids` (PCI addresses such as 0000:01:00.0, or resource mappings) (`machines.*.gpus.count`)
- **oci**: Machine `a`: GPUs on Oracle Cloud come later (its GPU shapes are fixed, not the flexible shape every machine has, and take images built for them) (`machines.*.gpus.count`, `machines.*.gpus.ids`, `machines.*.gpus.driver`)
- **kubernetes, helm, kustomize**: Machine `a`: `gpus.ids`: Kubernetes' scheduler picks the GPUs; give a `count` instead (`machines.*.gpus.ids`)
- **aws**: Machine `a`: a AWS VM's GPUs come with its instance type: give `gpus` as a count, not `all` or `ids` (`machines.*.gpus.ids`)
- **azure**: Machine `a`: a Azure VM's GPUs come with its instance type: give `gpus` as a count, not `all` or `ids` (`machines.*.gpus.ids`)
- **gcp**: Machine `a`: a Google Cloud VM's GPUs come with its instance type: give `gpus` as a count, not `all` or `ids` (`machines.*.gpus.ids`)
- **digitalocean**: Machine `a`: a DigitalOcean VM's GPUs come with its instance type: give `gpus` as a count, not `all` or `ids` (`machines.*.gpus.ids`)
- **linode**: Machine `a`: a Linode VM's GPUs come with its instance type: give `gpus` as a count, not `all` or `ids` (`machines.*.gpus.ids`)
- **kubernetes, helm, kustomize**: Machine `a`: `gpus.options` are Docker's GPU driver options, which Kubernetes has no place for (`machines.*.gpus.options`)
- **kubernetes, helm, kustomize, docker-vm, hybrid, cloud-docker**: Machine `a` is given host devices (`devices`): only a container on your own Docker host reaches them (the `docker` target), not one in a VM Isoloom creates or a Kubernetes pod (`machines.*.devices[].host`, `machines.*.devices[].guest`, `machines.*.devices[].permissions`)
- **vagrant, proxmox, aws, azure, gcp, digitalocean, linode, oci**: Machine `a` is given host devices (`devices`): a VM has its own virtual devices, so only a container gets the host's (a GPU reaches a VM through `gpus`) (`machines.*.devices[].host`, `machines.*.devices[].guest`, `machines.*.devices[].permissions`)
- **external**: Machine `a` sets its own process (`command`, `working_dir`, `before` or `healthcheck`): an existing host runs what it already runs, Isoloom starts nothing on it (`machines.*.command`, `machines.*.working_dir`, `machines.*.before`, `machines.*.healthcheck.test`, `machines.*.healthcheck.http.port`, `machines.*.healthcheck.http.path`, `machines.*.healthcheck.interval`, `machines.*.healthcheck.timeout`, `machines.*.healthcheck.retries`, `machines.*.healthcheck.start_period`)
- **digitalocean**: DigitalOcean cloud-vm takes single-machine labs so far (it can't pin private addresses) (`machines.*.depends_on`, `volumes.*.owner`, `machines.*.access`)
- **hybrid**: Volume `data` is shared by containers and VMs: one storage can't be both a Docker volume and an NFS export, for now (`volumes.*.owner`)
- **external**: Machine `a` has `files`: Isoloom doesn't write on an existing host, so place them with a `provision:` playbook (`machines.*.files[].path`, `machines.*.files[].source`, `machines.*.files[].content`, `machines.*.files[].uid`, `machines.*.files[].gid`, `machines.*.files[].mode`)
- **hybrid**: Machine `r` is a network appliance (`docker.appliance`), which runs on the Docker targets (`machines.*.docker.appliance`, `machines.*.docker.firmware`, `machines.*.docker.config`)
- **hybrid, aws, azure, gcp, digitalocean, linode, oci**: Machine `a` boots its own way (`vm.boot`): this target boots a VM from its image's own kernel and firmware; Vagrant's libvirt provider boots it (`machines.*.vm.boot.kernel`, `machines.*.vm.boot.initrd`, `machines.*.vm.boot.cmdline`, `machines.*.vm.boot.dtb`, `machines.*.vm.boot.loader`, `machines.*.vm.boot.nvram`, `machines.*.vm.boot.network.tftp_root`, `machines.*.vm.boot.network.file`, `machines.*.vm.boot.network.server`, `machines.*.vm.boot.firmware`, `machines.*.vm.boot.firmware_format`, `machines.*.vm.boot.order`)
- **proxmox**: Machine `a` sets `vm.boot.kernel`: Proxmox boots the VM's disk through its own firmware (`firmware`, `order` and `firmware_format` apply); Vagrant's libvirt provider boots it (`machines.*.vm.boot.kernel`, `machines.*.vm.boot.initrd`, `machines.*.vm.boot.cmdline`, `machines.*.vm.boot.dtb`)
- **proxmox**: Machine `a` sets `vm.boot.loader`: Proxmox boots the VM's disk through its own firmware (`firmware`, `order` and `firmware_format` apply); Vagrant's libvirt provider boots it (`machines.*.vm.boot.loader`, `machines.*.vm.boot.nvram`)
- **proxmox**: Machine `a` sets `vm.boot.network`: Proxmox boots the VM's disk through its own firmware (`firmware`, `order` and `firmware_format` apply); Vagrant's libvirt provider boots it (`machines.*.vm.boot.network.tftp_root`, `machines.*.vm.boot.network.file`, `machines.*.vm.boot.network.server`)
- **digitalocean**: Machine `a` asks for a virtual TPM (`vm.tpm: true`): DigitalOcean gives none (`machines.*.vm.tpm`)
- **linode**: Machine `a` asks for a virtual TPM (`vm.tpm: true`): Linode gives none (`machines.*.vm.tpm`)
- **docker, hosted, kubernetes, helm, kustomize, docker-vm, cloud-docker**: Environment-level provisioning (`provision:`) runs on VM targets for now (`provision[].ansible`, `provision[].inventory`, `provision[].groups`, `provision[].vars`, `provision[].host_vars`, `controller`, `provision[].requirements`)
- **digitalocean**: DigitalOcean cloud-vm has no Ansible controller yet (`provision[].ansible`, `provision[].inventory`, `provision[].groups`, `provision[].vars`, `provision[].host_vars`, `controller`, `provision[].requirements`)
- **linode**: Linode cloud-vm has no Ansible controller yet (`provision[].ansible`, `provision[].inventory`, `provision[].groups`, `provision[].vars`, `provision[].host_vars`, `controller`, `provision[].requirements`)
- **oci**: Oracle Cloud cloud-vm has no Ansible controller yet (`provision[].ansible`, `provision[].inventory`, `provision[].groups`, `provision[].vars`, `provision[].host_vars`, `controller`, `provision[].requirements`)
