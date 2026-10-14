# Coverage

Everything each format can do, and whether Isoloom produces it from a spec.

**What belongs in the format:** a machine feature the VM targets can do (local VMs, Proxmox,
cloud VMs). Containers are the lightweight option, used when they can produce the same machine:
a feature they can't makes an environment VM-only (like Windows) instead of staying out.
Container mechanics (capabilities, cgroups) aren't machine features: Isoloom sets them itself.
**100% coverage** means every portable feature is produced.

| Format | Coverage | Portable features | Done | Partly | To do |
| --- | ---: | ---: | ---: | ---: | ---: |
| [Docker Compose](#docker-compose) | 68% | 148 | 99 | 4 | 45 |
| [Vagrant](#vagrant) | 88% | 36 | 32 | 0 | 4 |
| [Vagrant: VirtualBox](#vagrant-virtualbox) | 100% | 7 | 7 | 0 | 0 |
| [Vagrant: VMware Desktop](#vagrant-vmware-desktop) | 90% | 5 | 4 | 1 | 0 |
| [Vagrant: Parallels](#vagrant-parallels) | 100% | 6 | 6 | 0 | 0 |
| [Vagrant: libvirt](#vagrant-libvirt) | 35% | 20 | 7 | 0 | 13 |
| [Vagrant: Hyper-V](#vagrant-hyper-v) | 100% | 6 | 6 | 0 | 0 |
| [Vagrant: UTM](#vagrant-utm) | 75% | 4 | 3 | 0 | 1 |
| [Vagrant: QEMU](#vagrant-qemu) | 36% | 15 | 4 | 3 | 8 |
| [Vagrant: ESXi](#vagrant-esxi) | 57% | 19 | 11 | 0 | 8 |
| [Terraform: Proxmox](#terraform-proxmox) | 46% | 15 | 7 | 0 | 8 |
| [Terraform: ESXi](#terraform-esxi) | 0% | 4 | 0 | 0 | 4 |
| [Terraform: AWS](#terraform-aws) | 63% | 19 | 12 | 0 | 7 |
| [Terraform: Azure](#terraform-azure) | 50% | 18 | 9 | 0 | 9 |
| [Terraform: Google Cloud](#terraform-google-cloud) | 50% | 12 | 6 | 0 | 6 |
| [Terraform: DigitalOcean](#terraform-digitalocean) | 50% | 8 | 4 | 0 | 4 |
| [Terraform: Linode](#terraform-linode) | 55% | 9 | 5 | 0 | 4 |
| [Terraform: Oracle Cloud](#terraform-oracle-cloud) | 46% | 13 | 6 | 0 | 7 |
| [cloud-init](#cloud-init) | 95% | 80 | 76 | 0 | 4 |
| [Ansible inventory](#ansible-inventory) | 95% | 31 | 28 | 3 | 0 |
| [Dockerfile](#dockerfile) | 100% | 24 | 24 | 0 | 0 |
| [Kubernetes](#kubernetes) | 88% | 192 | 170 | 1 | 21 |
| [Kustomize](#kustomize) | 100% | 15 | 15 | 0 | 0 |
| [Helm](#helm) | 94% | 19 | 18 | 0 | 1 |

Done includes features produced another way, or set in the image. The lists come from the
formats themselves (every key of the Compose schema, every setting of Vagrant and each provider
plugin, every resource type of each Terraform provider); tests fail on an unclassified entry, when
an example's generated file (Compose, Vagrantfile, Terraform, Kubernetes, Kustomize, Helm) writes a key or resource type its
table doesn't mark as written, or when one marked as written appears in no example's output.
Only what the examples generate is checked: a feature no example uses can still be misclassified.

## Docker Compose

68% of 148 portable features (99 done, 4 partly, 45 to do; 347 features in all). From the compose-spec schema, commit 914ec15d1fa4 (crates/isoloom-coverage/coverage/compose-spec.json).

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
| `secrets` | Yes | Not yet | Real secrets (#147): files from a secret store, never environment values |
| `configs` | Yes | Not yet | Machine files: content placed at a path on the machine, on every target |
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
| `build.target` | Yes | Not yet | A build stage, beside machines.*.docker.dockerfile |
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
| `command` | Yes | Partly | Isoloom's tools: tools.*.command; not yet: a machine's own command |
| `configs` | Yes | Not yet | Machine files: content placed at a path on the machine, on every target |
| `configs[].source` | Yes | Not yet | Machine files: content placed at a path on the machine, on every target |
| `configs[].target` | Yes | Not yet | Machine files: content placed at a path on the machine, on every target |
| `configs[].uid` | Yes | Not yet | Machine files: content placed at a path on the machine, on every target |
| `configs[].gid` | Yes | Not yet | Machine files: content placed at a path on the machine, on every target |
| `configs[].mode` | Yes | Not yet | Machine files: content placed at a path on the machine, on every target |
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
| `deploy.resources.reservations` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `deploy.resources.reservations.cpus` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `deploy.resources.reservations.memory` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `deploy.resources.reservations.generic_resources` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.resources.reservations.generic_resources[].discrete_resource_spec` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.resources.reservations.generic_resources[].discrete_resource_spec.kind` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.resources.reservations.generic_resources[].discrete_resource_spec.value` | n/a | No | Swarm's scheduling across hosts: an environment's containers share one |
| `deploy.resources.reservations.devices` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `deploy.resources.reservations.devices[].capabilities` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `deploy.resources.reservations.devices[].count` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `deploy.resources.reservations.devices[].device_ids` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `deploy.resources.reservations.devices[].driver` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `deploy.resources.reservations.devices[].options` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
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
| `devices` | Yes | Not yet | Device passthrough where the target has the device, refused where it has none |
| `devices[].source` | Yes | Not yet | Device passthrough where the target has the device, refused where it has none |
| `devices[].target` | Yes | Not yet | Device passthrough where the target has the device, refused where it has none |
| `devices[].permissions` | Yes | Not yet | Device passthrough where the target has the device, refused where it has none |
| `dns` | Yes | Yes | From `machines.*.dns.servers` (and the Kubernetes dnsConfig) |
| `dns_opt` | Yes | Not yet | Resolver options, beside machines.*.dns |
| `dns_search` | Yes | Yes | From `machines.*.dns.search` (and the Kubernetes dnsConfig) |
| `domainname` | Yes | Yes | From `machines.*.dns.domain` |
| `entrypoint` | Yes | Yes | Isoloom's own containers and init jobs; a machine's own is its image's |
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
| `gpus` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `gpus[].capabilities` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `gpus[].count` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `gpus[].device_ids` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `gpus[].driver` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `gpus[].options` | Yes | Not yet | GPU passthrough where the target has a GPU, refused where it has none |
| `group_add` | Yes | In the image | Set it in the image (`docker.build`) or the VM's provisioning |
| `healthcheck` | Yes | Partly | From `machines.*.services` (a probe of every port); not yet: a custom healthcheck command |
| `healthcheck.disable` | n/a | No | Turns off the image's own check: Isoloom's readiness is its own probe |
| `healthcheck.interval` | Yes | Yes | Isoloom's probe pace |
| `healthcheck.retries` | Yes | Yes | Isoloom's probe pace |
| `healthcheck.test` | Yes | Partly | From `machines.*.services` (a probe of every port); not yet: a custom healthcheck command |
| `healthcheck.timeout` | Yes | Yes | Isoloom's probe pace |
| `healthcheck.start_period` | Yes | Yes | A grace period for slow starters: appliances and built images |
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
| `networks.*.ipv6_address` | Yes | Not yet | IPv6 networks |
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
| `secrets` | Yes | Not yet | Real secrets (#147): files from a secret store, never environment values |
| `secrets[].source` | Yes | Not yet | Real secrets (#147): files from a secret store, never environment values |
| `secrets[].target` | Yes | Not yet | Real secrets (#147): files from a secret store, never environment values |
| `secrets[].uid` | Yes | Not yet | Real secrets (#147): files from a secret store, never environment values |
| `secrets[].gid` | Yes | Not yet | Real secrets (#147): files from a secret store, never environment values |
| `secrets[].mode` | Yes | Not yet | Real secrets (#147): files from a secret store, never environment values |
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
| `working_dir` | Yes | Not yet | A machine's working directory, with its command |

### Networks

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `name` | Yes | Yes | The hybrid target: the Docker network its host VM created for each network; elsewhere Compose scopes names to the environment |
| `driver` | No | No | Docker network drivers (overlay, macvlan) have no VM equivalent |
| `driver_opts` | No | No | Docker network driver options |
| `ipam` | Yes | Yes | From `networks.*.cidr` |
| `ipam.driver` | No | No | Docker IPAM drivers have no VM equivalent |
| `ipam.config` | Yes | Yes | From `networks.*.cidr` |
| `ipam.config[].subnet` | Yes | Yes | From `networks.*.cidr` |
| `ipam.config[].ip_range` | n/a | No | Docker's pool for addresses it picks: every machine has a fixed one |
| `ipam.config[].gateway` | Yes | Yes | The address Docker takes, when the first one is a machine's or the router's |
| `ipam.config[].aux_addresses` | n/a | No | Addresses Docker must not hand out: every machine has a fixed one |
| `ipam.options` | No | No | Docker IPAM driver options |
| `external` | Yes | Partly | The hybrid target: its containers join the macvlan networks the Docker host VM creates, shared with the VMs; not yet: joining a network outside the environment (a Docker network, a host bridge, a VPC) |
| `external.name` | n/a | No | Legacy spelling of `name` |
| `internal` | Yes | Yes | From `networks.*.internet: false` (when nothing routes) |
| `enable_ipv4` | n/a | No | Networks are IPv4 (the default) |
| `enable_ipv6` | Yes | Not yet | IPv6 networks |
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
| `content` | Yes | Not yet | Machine files: content placed at a path on the machine, on every target |
| `environment` | Yes | Not yet | Machine files: content placed at a path on the machine, on every target |
| `file` | Yes | Not yet | Machine files: content placed at a path on the machine, on every target |
| `external` | n/a | No | A config created outside Compose |
| `external.name` | n/a | No | Legacy spelling of `name` |
| `labels` | n/a | No | Compose tooling, not the environment's behavior |
| `template_driver` | n/a | No | Swarm's templating of configs |

### Secrets

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `name` | n/a | No | Compose scopes names to the environment |
| `environment` | Yes | Not yet | Real secrets (#147): files from a secret store, never environment values |
| `file` | Yes | Not yet | Real secrets (#147): files from a secret store, never environment values |
| `external` | Yes | Not yet | Real secrets (#147): files from a secret store, never environment values |
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

88% of 36 portable features (32 done, 0 partly, 4 to do; 62 features in all). From Vagrant 2.4.9 (config.vm, network types, provisioners).

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
| `box_server_url` | Yes | Not yet | Custom images (a box URL, a Proxmox template, a cloud image) |
| `box_url` | Yes | Not yet | Custom images (a box URL, a Proxmox template, a cloud image) |
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
| `synced_folder` | Yes | Yes | Disabled: the project is copied into each VM instead |
| `usable_port_range` | n/a | No | How Vagrant picks host ports for forwarded ports |

### Network types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `private_network` | Yes | Yes | From `networks and machines.*.networks` |
| `forwarded_port` | Yes | Yes | From `machines.*.services[].publish` (on the host's loopback; `protocol: "udp"` with `protocol: udp`) |
| `public_network` | Yes | Not yet | A network bridged to the outside (Compose `external`, a host bridge, a VPC) |

### Provisioners

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `shell` | Yes | Yes | From `machines.*.vm.provision` (.sh, .yml, .yaml, `reboot`), and Isoloom's own steps |
| `file` | Yes | Another way | Via `isoloom_project` (below) copies the project; a file provisioner follows symlinks and lists files when the Vagrantfile loads |
| `isoloom_project` | Yes | Yes | Isoloom's own provisioner, defined in the Vagrantfile: the project into each VM as a tar.gz built when the step runs, symlinks kept as links |
| `ansible_local` | Yes | Another way | Via a shell provisioner running the playbook with `ansible-playbook -c local` inside the VM, the machine as its inventory (as on every VM target) |
| `ansible` | Yes | Another way | Via the same playbooks, run inside the VM by a shell provisioner |
| `chef` | Yes | Another way | Via a `.sh` provisioning step can run any tool |
| `puppet` | Yes | Another way | Via a `.sh` provisioning step can run any tool |
| `salt` | Yes | Another way | Via a `.sh` provisioning step can run any tool |
| `cfengine` | Yes | Another way | Via a `.sh` provisioning step can run any tool |
| `docker` | Yes | Another way | Via `docker:` (containers come from the Compose output; VMs run services natively) |
| `podman` | Yes | Another way | Via `docker:` (containers come from the Compose output; VMs run services natively) |
| `container` | Yes | Another way | Via `docker:` (containers come from the Compose output; VMs run services natively) |

### Other settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `config.ssh` | n/a | No | Vagrant tooling, not the environment's behavior |
| `config.winrm` | Yes | Yes | From `vm.os` (the Windows box's own account, over WinRM) |
| `config.winssh` | Yes | Not yet | With Windows guests (which make an environment VM-only) |
| `config.trigger` | n/a | No | Vagrant tooling, not the environment's behavior |
| `config.vagrant` | n/a | No | Vagrant tooling, not the environment's behavior |

## Vagrant: VirtualBox

100% of 7 portable features (7 done, 0 partly, 0 to do; 15 features in all). From Vagrant 2.4.9 (the provider's config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `auto_nat_dns_proxy` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `check_guest_additions` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | Yes | From `machines.*.resources` |
| `customizations` | No | No | Raw commands for one hypervisor: no other target understands them |
| `customize` | Yes | Yes | The hybrid target's Docker host only: its NICs in promiscuous mode, so its containers get their own addresses on the VMs' networks; no spec field writes raw commands |
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

90% of 5 portable features (4 done, 1 partly, 0 to do; 24 features in all). From vagrant-vmware-desktop 3.0.5 (its config class).

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
| `vmx` | Yes | Partly | From `machines.*.resources`; not yet: only displayName, numvcpus and memsize |
| `whitelist_verified` | n/a | No | Vagrant tooling, not the environment's behavior |

## Vagrant: Parallels

100% of 6 portable features (6 done, 0 partly, 0 to do; 15 features in all). From vagrant-parallels 2.4.7 (its config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `check_guest_tools` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | Yes | From `machines.*.resources` |
| `customizations` | No | No | Raw commands for one hypervisor: no other target understands them |
| `customize` | No | No | Raw commands for one hypervisor: no other target understands them |
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

35% of 20 portable features (7 done, 0 partly, 13 to do; 146 features in all). From vagrant-libvirt 0.12.2 (its config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `autostart` | n/a | No | Vagrant tooling, not the environment's behavior |
| `boot` | Yes | Not yet | Custom images with their own kernel and boot (makes an environment VM-only) |
| `boot_order` | Yes | Not yet | Custom images with their own kernel and boot (makes an environment VM-only) |
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
| `cmd_line` | Yes | Not yet | Custom images with their own kernel and boot (makes an environment VM-only) |
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
| `dtb` | Yes | Not yet | Custom images with their own kernel and boot (makes an environment VM-only) |
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
| `initrd` | Yes | Not yet | Custom images with their own kernel and boot (makes an environment VM-only) |
| `input` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `inputs` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `kernel` | Yes | Not yet | Custom images with their own kernel and boot (makes an environment VM-only) |
| `keymap` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `kvm_hidden` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `launchsecurity` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `launchsecurity_data` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `loader` | Yes | Not yet | Custom images with their own kernel and boot (makes an environment VM-only) |
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
| `nested` | Yes | Not yet | Nested virtualization: running VMs inside (makes an environment VM-only) |
| `nic_adapter_count` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `nic_model_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `nodeset` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `numa_nodes` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `nvram` | Yes | Not yet | Custom images with their own kernel and boot (makes an environment VM-only) |
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
| `tpm_model` | Yes | Not yet | With Windows guests (which make an environment VM-only) |
| `tpm_path` | Yes | Not yet | With Windows guests (which make an environment VM-only) |
| `tpm_type` | Yes | Not yet | With Windows guests (which make an environment VM-only) |
| `tpm_version` | Yes | Not yet | With Windows guests (which make an environment VM-only) |
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
| `enable_virtualization_extensions` | Yes | Yes | From `machines.*.privileged` (nested virtualization) |
| `ip_address_timeout` | n/a | No | Vagrant tooling, not the environment's behavior |
| `linked_clone` | Yes | Yes | A differencing disk, so a clone is instant instead of copying the whole box |
| `mac` | No | No | Cloud VMs get their MAC address from the provider |
| `maxmemory` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `memory` | Yes | Yes | From `machines.*.resources` |
| `vlan_id` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |
| `vm_integration_services` | n/a | No | Vagrant tooling, not the environment's behavior |
| `vmname` | Yes | Yes | The environment and machine names, as the VM's name in Hyper-V |

## Vagrant: UTM

75% of 4 portable features (3 done, 0 partly, 1 to do; 12 features in all). From vagrant_utm 0.1.6 (its config class), for Apple Silicon Macs.

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
| `private_network` | Yes | Not yet | Not verified: vagrant_utm doesn't document `config.vm.network private_network` |

## Vagrant: QEMU

36% of 15 portable features (4 done, 3 partly, 8 to do; 34 features in all). From vagrant-qemu 0.6.3 (its config class), for Apple Silicon Macs.

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `advanced_network` | Yes | Partly | From `machines.*.networks`; not yet: without root, QEMU links exactly two VMs per network, each VM on one network (`refusal_on` with `qemu` says when a spec doesn't fit) |
| `arch` | Yes | Yes | From `machines.*.arch` (emulated when the host's CPU is another) |
| `control_port` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpu` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `debug_port` | n/a | No | Vagrant tooling, not the environment's behavior |
| `default_qemu_dir` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `disk_resize` | Yes | Not yet | Machines.*.resources.disk_gb, once the box's own size is known: the plugin makes the disk an overlay of that size, and one below the box's truncates it |
| `drive_interface` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `extra_drive_args` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `extra_image_opts` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `extra_netdev_args` | Yes | Not yet | More than two VMs on a QEMU network (vmnet or socket_vmnet, which need root on the Mac) |
| `extra_qemu_args` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `firmware_format` | Yes | Not yet | Custom images with their own kernel and boot (makes an environment VM-only) |
| `graceful_timeout` | n/a | No | Vagrant tooling, not the environment's behavior |
| `homebrew_prefix` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `image_path` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `machine` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `mcast_addr` | Yes | Not yet | More than two VMs on a QEMU network (vmnet or socket_vmnet, which need root on the Mac) |
| `memory` | Yes | Yes | From `machines.*.resources` |
| `net_device` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `net_mode` | Yes | Partly | From `machines.*.networks`; not yet: without root, QEMU links exactly two VMs per network, each VM on one network (`refusal_on` with `qemu` says when a spec doesn't fit) |
| `no_daemonize` | n/a | No | Vagrant tooling, not the environment's behavior |
| `other_default` | n/a | No | Vagrant tooling, not the environment's behavior |
| `qemu_bin` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `qemu_dir` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `smp` | Yes | Yes | From `machines.*.resources.cpus` |
| `socket_opts` | Yes | Partly | From `machines.*.networks`; not yet: without root, QEMU links exactly two VMs per network, each VM on one network (`refusal_on` with `qemu` says when a spec doesn't fit) |
| `socket_vmnet_client` | Yes | Not yet | More than two VMs on a QEMU network (vmnet or socket_vmnet, which need root on the Mac) |
| `socket_vmnet_socket` | Yes | Not yet | More than two VMs on a QEMU network (vmnet or socket_vmnet, which need root on the Mac) |
| `ssh_auto_correct` | Yes | Yes | Always: each VM forwards SSH from its own host port |
| `ssh_host` | n/a | No | Vagrant tooling, not the environment's behavior |
| `ssh_port` | n/a | No | Vagrant tooling, not the environment's behavior |
| `tap_device` | Yes | Not yet | More than two VMs on a QEMU network (vmnet or socket_vmnet, which need root on the Mac) |
| `vmnet_interface` | Yes | Not yet | More than two VMs on a QEMU network (vmnet or socket_vmnet, which need root on the Mac) |

## Vagrant: ESXi

57% of 19 portable features (11 done, 0 partly, 8 to do; 53 features in all). From vagrant-vmware-esxi 2.5.2 (its config class), for standalone ESXi hosts.

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
| `guest_custom_vmx_settings` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `guest_disk_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `guest_guestos` | Yes | Not yet | With Windows guests (which make an environment VM-only) |
| `guest_mac_address` | No | No | Cloud VMs get their MAC address from the provider |
| `guest_memsize` | Yes | Yes | From `machines.*.resources` |
| `guest_name` | Yes | Yes | From the environment and machine names |
| `guest_name_prefix` | Yes | Not yet | With the ESXi provider block (not verified on an ESXi host yet) |
| `guest_nic_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `guest_numvcpus` | Yes | Yes | From `machines.*.resources` |
| `guest_snapshot_includememory` | n/a | No | Vagrant tooling, not the environment's behavior |
| `guest_snapshot_quiesced` | n/a | No | Vagrant tooling, not the environment's behavior |
| `guest_storage` | Yes | Another way | Via machines.*.resources.disk_gb and machines.*.volumes (one disk, sized, holding the volumes' paths) |
| `guest_username` | n/a | No | Vagrant tooling, not the environment's behavior |
| `guest_virtualhw_version` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `guestos` | Yes | Not yet | With Windows guests (which make an environment VM-only) |
| `lax` | n/a | No | Vagrant tooling, not the environment's behavior |
| `local_allow_overwrite` | n/a | No | Vagrant tooling, not the environment's behavior |
| `local_failonwarning` | n/a | No | Vagrant tooling, not the environment's behavior |
| `local_lax` | n/a | No | Vagrant tooling, not the environment's behavior |
| `local_private_keys` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `local_use_ip_cache` | n/a | No | Vagrant tooling, not the environment's behavior |
| `mac_address` | No | No | Cloud VMs get their MAC address from the provider |
| `memsize` | Yes | Not yet | With the ESXi provider block (not verified on an ESXi host yet) |
| `nic_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `numvcpus` | Yes | Not yet | With the ESXi provider block (not verified on an ESXi host yet) |
| `resource_pool` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `saved_ipaddress` | n/a | No | Vagrant tooling, not the environment's behavior |
| `ssh_username` | n/a | No | Vagrant tooling, not the environment's behavior |
| `supported_guest_disk_types` | n/a | No | The plugin's own list of accepted values |
| `supported_guest_guestos` | n/a | No | The plugin's own list of accepted values |
| `supported_guest_nic_types` | n/a | No | The plugin's own list of accepted values |
| `supported_guest_virtualhw_versions` | n/a | No | The plugin's own list of accepted values |
| `virtual_network` | Yes | Not yet | The plugin's older name for esxi_virtual_network |
| `virtualhw_version` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `vm_disk_store` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `vm_disk_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `vmname` | Yes | Not yet | With the ESXi provider block (not verified on an ESXi host yet) |
| `vmname_prefix` | Yes | Not yet | With the ESXi provider block (not verified on an ESXi host yet) |

## Terraform: Proxmox

46% of 15 portable features (7 done, 0 partly, 8 to do; 116 features in all). From bpg/proxmox 0.115.0 (its resource types).

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
| `proxmox_download_file` | Yes | Yes | From `machines.*.vm.os` (cloud images) |
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
| `proxmox_sdn_applier` | Yes | Yes | From `networks` |
| `proxmox_sdn_controller_evpn` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_fabric_node_openfabric` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_fabric_node_ospf` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_fabric_openfabric` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_fabric_ospf` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_subnet` | Yes | Another way | Via the router VM, which holds each network's router address (a subnet would put it on the host bridge, which routes between networks) |
| `proxmox_sdn_vnet` | Yes | Yes | From `networks` |
| `proxmox_sdn_zone_evpn` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_zone_qinq` | n/a | No | How a cluster spreads networks over its nodes: no change in behavior |
| `proxmox_sdn_zone_simple` | Yes | Yes | From `networks` |
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
| `proxmox_virtual_environment_acl` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_acme_account` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_acme_certificate` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_acme_dns_plugin` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_apt_repository` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_apt_standard_repository` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_certificate` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_cloned_vm` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_cluster_firewall` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_cluster_firewall_security_group` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_cluster_options` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_container` | Yes | Not yet | LXC containers: a way to run `docker:` machines on Proxmox |
| `proxmox_virtual_environment_dns` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_download_file` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_file` | Yes | Yes | Cloud-init for each VM |
| `proxmox_virtual_environment_firewall_alias` | Yes | Not yet | The Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_firewall_ipset` | Yes | Not yet | The Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_firewall_options` | Yes | Not yet | The Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_firewall_rules` | Yes | Not yet | The Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_group` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_hagroup` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_hardware_mapping_dir` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_hardware_mapping_pci` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_hardware_mapping_usb` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_haresource` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_harule` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_hosts` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_metrics_server` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_network_linux_bridge` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_network_linux_vlan` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_node_firewall` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_oci_image` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_pool` | Yes | Not yet | With the Proxmox generator |
| `proxmox_virtual_environment_pool_membership` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_realm_ldap` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_realm_openid` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_realm_sync` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_replication` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_role` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_sdn_applier` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_fabric_node_openfabric` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_fabric_node_ospf` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_fabric_openfabric` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_fabric_ospf` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_subnet` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_vnet` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_zone_evpn` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_zone_qinq` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_zone_simple` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_zone_vlan` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_zone_vxlan` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_cifs` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_directory` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_lvm` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_lvmthin` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_nfs` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_pbs` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_zfspool` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_time` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_user` | No | No | Administers the Proxmox server itself: no other target has a server to administer |
| `proxmox_virtual_environment_user_token` | n/a | No | Older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_vm` | Yes | Yes | From `machines` (and the router) |
| `proxmox_virtual_environment_vm2` | n/a | No | An older, experimental VM resource |
| `proxmox_vm` | n/a | No | An experimental VM resource; Isoloom uses proxmox_virtual_environment_vm |

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

63% of 19 portable features (12 done, 0 partly, 7 to do; 1728 features in all). From hashicorp/aws 6.67.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `aws_ebs_volume` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_eip` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_eip_association` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_instance` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_internet_gateway` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_key_pair` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_nat_gateway` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_network_interface` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_network_interface_attachment` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_route` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_route_table` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_route_table_association` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_security_group` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_subnet` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_volume_attachment` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_vpc` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_vpc_ipv4_cidr_block_association` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `aws_vpc_security_group_egress_rule` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_vpc_security_group_ingress_rule` | Yes | Not yet | With the cloud generator (one VM per machine) |

And 1709 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Azure

50% of 18 portable features (9 done, 0 partly, 9 to do; 1106 features in all). From hashicorp/azurerm 5.8.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `azurerm_linux_virtual_machine` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_managed_disk` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_nat_gateway` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_network_interface` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_network_interface_security_group_association` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_network_security_group` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_network_security_rule` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_public_ip` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_resource_group` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_route` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_route_table` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_subnet` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_subnet_nat_gateway_association` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_subnet_network_security_group_association` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_subnet_route_table_association` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_virtual_machine_data_disk_attachment` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_virtual_network` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `azurerm_windows_virtual_machine` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |

And 1088 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Google Cloud

50% of 12 portable features (6 done, 0 partly, 6 to do; 1369 features in all). From hashicorp/google 8.5.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `google_compute_address` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_attached_disk` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_disk` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_firewall` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_compute_instance` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_compute_network` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_compute_route` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_router` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_router_nat` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_subnetwork` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_project` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `google_project_service` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |

And 1357 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: DigitalOcean

50% of 8 portable features (4 done, 0 partly, 4 to do; 79 features in all). From digitalocean/digitalocean 2.103.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `digitalocean_droplet` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `digitalocean_firewall` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `digitalocean_reserved_ip` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `digitalocean_reserved_ip_assignment` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `digitalocean_ssh_key` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `digitalocean_volume` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `digitalocean_volume_attachment` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `digitalocean_vpc` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |

And 71 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Linode

55% of 9 portable features (5 done, 0 partly, 4 to do; 47 features in all). From linode/linode 4.7.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `linode_firewall` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `linode_instance` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `linode_instance_config` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `linode_instance_disk` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `linode_instance_ip` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `linode_sshkey` | Yes | Another way | Via each instance's own `authorized_keys`, which installs the environment's key without storing it in the account |
| `linode_volume` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `linode_vpc` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `linode_vpc_subnet` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |

And 38 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Oracle Cloud

46% of 13 portable features (6 done, 0 partly, 7 to do; 1020 features in all). From oracle/oci 9.8.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `oci_core_instance` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_internet_gateway` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_nat_gateway` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_network_security_group` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_network_security_group_security_rule` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_public_ip` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_route_table` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_security_list` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_subnet` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_vcn` | Yes | Yes | Written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file |
| `oci_core_vnic_attachment` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_volume` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_volume_attachment` | Yes | Not yet | With the cloud generator (one VM per machine) |

And 1007 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## cloud-init

95% of 80 portable features (76 done, 0 partly, 4 to do; 149 features in all). From cloud-init 26.2's schema-cloud-config-v1.json (crates/isoloom-coverage/coverage/cloud-init.txt).

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
| `fqdn` | Yes | Not yet | A machine's full name, from its name and `dns.domain` |
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

95% of 31 portable features (28 done, 3 partly, 0 to do; 143 features in all). From ansible-core v2.21.5's behavioral parameters, and pywinrm v0.5.0's for `ansible_winrm_*` (crates/isoloom-coverage/coverage/ansible-inventory.txt).

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
| `ansible_connection` | Yes | Partly | From `machines.*.vm.os` (WinRM for Windows; SSH with PowerShell on external); not yet: Proxmox's Windows VMs (no WinRM account yet) |
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
| `ansible_password` | Yes | Partly | The Vagrant boxes' account; the clouds' Windows password; not yet: Proxmox's Windows VMs (no WinRM account yet) |
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
| `ansible_user` | Yes | Partly | From `machines.*.external.user and machines.*.vm.os` (each image's own user; `isoloom` on Proxmox); not yet: Proxmox's Windows VMs (no WinRM account yet) |
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

88% of 192 portable features (170 done, 1 partly, 21 to do; 358 features in all). From the Kubernetes v1.37.0 OpenAPI document (the version CI's kind cluster runs).

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
| `PodSpec: hostAliases` | Yes | Not yet | The spec's `aliases`: Kubernetes gives no fixed pod address to point them at |
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
| `PodSpec: os` | Yes | Not yet | Windows machines in Windows containers (Windows makes an environment VM-only today) |
| `PodSpec: overhead` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSpec: preemptionPolicy` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: priority` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: priorityClassName` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: readinessGates` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: resourceClaims` | Yes | Not yet | Device passthrough (GPUs), where the cluster offers it |
| `PodSpec: resources` | Yes | Another way | Via each container's `resources` |
| `PodSpec: restartPolicy` | Yes | Yes | From `checks` (run once) |
| `PodSpec: runtimeClassName` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSpec: schedulerName` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: schedulingGates` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: schedulingGroup` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: securityContext` | Yes | Yes | From `machines.*.sysctls` |
| `PodSpec: serviceAccount` | n/a | No | Access to the Kubernetes API: machines don't call it |
| `PodSpec: serviceAccountName` | n/a | No | Access to the Kubernetes API: machines don't call it |
| `PodSpec: setHostnameAsFQDN` | Yes | Not yet | The machine's `dns.domain` as its fully qualified name |
| `PodSpec: shareProcessNamespace` | n/a | No | Container mechanics (users, profiles, namespaces): not a machine feature |
| `PodSpec: subdomain` | Yes | Not yet | The machine's `dns.domain` as its fully qualified name |
| `PodSpec: terminationGracePeriodSeconds` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: tolerations` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: topologySpreadConstraints` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `PodSpec: volumes` | Yes | Yes | From `volumes, machines.*.tmpfs and machines.*.shm_size` |
| `Container: args` | Yes | Not yet | A machine's own `command` (the image's default runs today) |
| `Container: command` | Yes | Yes | From `machines.*.docker.idle and machines.*.docker.init` (and Isoloom's own containers) |
| `Container: env` | Yes | Yes | From `machines.*.environment and machines.*.inputs` |
| `Container: envFrom` | Yes | Another way | Via `env`, one variable at a time |
| `Container: image` | Yes | Yes | From `machines.*.docker.image` (or the image the project builds; a value of its own in the chart) |
| `Container: imagePullPolicy` | Yes | Yes | IfNotPresent: an image built on the node is used as it is |
| `Container: lifecycle` | Yes | Another way | Via machines.*.docker.init: a container beside the machine's, run once it answers |
| `Container: livenessProbe` | n/a | No | Restarts a container Kubernetes finds unhealthy: the cluster's self-healing, not the machine's behavior |
| `Container: name` | Yes | Yes | The machine's name |
| `Container: ports` | Yes | Yes | From `machines.*.services` |
| `Container: readinessProbe` | Yes | Partly | From `machines.*.services` (ready when its services' ports answer; dormant ones aren't waited for); not yet: a custom healthcheck command |
| `Container: resizePolicy` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Container: resources` | Yes | Yes | From `machines.*.resources` |
| `Container: restartPolicy` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Container: restartPolicyRules` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Container: securityContext` | Yes | Yes | From `machines.*.privileged and machines.*.read_only` |
| `Container: startupProbe` | Yes | Another way | Via `readinessProbe`, which waits as long as the machine takes to start |
| `Container: stdin` | n/a | No | A terminal for whoever attaches: `isoloom connect` opens its own |
| `Container: stdinOnce` | n/a | No | A terminal for whoever attaches: `isoloom connect` opens its own |
| `Container: terminationMessagePath` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Container: terminationMessagePolicy` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Container: tty` | n/a | No | A terminal for whoever attaches: `isoloom connect` opens its own |
| `Container: volumeDevices` | Yes | Not yet | Raw data disks, with persistent volumes and data disks |
| `Container: volumeMounts` | Yes | Yes | From `machines.*.volumes, machines.*.tmpfs and machines.*.shm_size` |
| `Container: workingDir` | Yes | In the image | The image's WORKDIR |
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
| `ResourceRequirements: claims` | Yes | Not yet | Device passthrough (GPUs), where the cluster offers it |
| `ResourceRequirements: limits` | Yes | Yes | From `machines.*.resources` (cpus, memory_mb) |
| `ResourceRequirements: requests` | n/a | No | What the scheduler reserves on a node: the limits are the machine's size |
| `Probe: exec` | Yes | Yes | From `machines.*.services` (Isoloom's port probe, the same on every target) |
| `Probe: failureThreshold` | Yes | Yes | Every 5 seconds, for up to 5 minutes |
| `Probe: grpc` | Yes | Not yet | With a custom healthcheck command (the probe checks the services' ports today) |
| `Probe: httpGet` | Yes | Not yet | With a custom healthcheck command (the probe checks the services' ports today) |
| `Probe: initialDelaySeconds` | Yes | Not yet | With a custom healthcheck command (the probe checks the services' ports today) |
| `Probe: periodSeconds` | Yes | Yes | Every 5 seconds, for up to 5 minutes |
| `Probe: successThreshold` | Yes | Not yet | With a custom healthcheck command (the probe checks the services' ports today) |
| `Probe: tcpSocket` | Yes | Another way | Via `exec`: Isoloom's port probe checks every service port, TCP and UDP |
| `Probe: terminationGracePeriodSeconds` | n/a | No | Kubernetes scheduling and lifecycle: where and how long a pod runs, not how the machine behaves |
| `Probe: timeoutSeconds` | Yes | Not yet | With a custom healthcheck command (the probe checks the services' ports today) |
| `ExecAction: command` | Yes | Yes | From `machines.*.services` (Isoloom's port probe, the same on every target) |
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
| `SecurityContext: windowsOptions` | Yes | Not yet | Windows machines in Windows containers (Windows makes an environment VM-only today) |
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
| `PodSecurityContext: windowsOptions` | Yes | Not yet | Windows machines in Windows containers (Windows makes an environment VM-only today) |
| `Sysctl: name` | Yes | Yes | From `machines.*.sysctls` |
| `Sysctl: value` | Yes | Yes | From `machines.*.sysctls` |
| `PodDNSConfig: nameservers` | Yes | Yes | From `machines.*.dns.servers` |
| `PodDNSConfig: options` | Yes | Not yet | Resolver options (`ndots`, `timeout`), which every machine's resolver has |
| `PodDNSConfig: searches` | Yes | Yes | From `machines.*.dns.search` |
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
| `Volume: secret` | Yes | Not yet | Real secrets: files a machine reads, not environment variables |
| `Volume: storageos` | No | No | One storage system's plugin: no other target has it |
| `Volume: vsphereVolume` | No | No | One storage system's plugin: no other target has it |
| `VolumeMount: bindMountOptions` | n/a | No | A mount detail of the node: Isoloom mounts whole volumes, read-write |
| `VolumeMount: mountPath` | Yes | Yes | From `machines.*.volumes, machines.*.tmpfs and machines.*.shm_size` |
| `VolumeMount: mountPropagation` | n/a | No | A mount detail of the node: Isoloom mounts whole volumes, read-write |
| `VolumeMount: name` | Yes | Yes | From `machines.*.volumes, machines.*.tmpfs and machines.*.shm_size` |
| `VolumeMount: readOnly` | Yes | Yes | Isoloom's probe, mounted read-only |
| `VolumeMount: recursiveReadOnly` | Yes | Another way | Via `readOnly` |
| `VolumeMount: subPath` | Yes | Not yet | A machine's `files`, each mounted at its own path |
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
| `ServiceSpec: ipFamilies` | Yes | Not yet | With IPv6 networks |
| `ServiceSpec: ipFamilyPolicy` | Yes | Not yet | With IPv6 networks |
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
| `ConfigMap: data` | Yes | Another way | Via `binaryData`, which keeps a script byte for byte |
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

94% of 19 portable features (18 done, 0 partly, 1 to do; 52 features in all). From SchemaStore's chart.json (pinned) and Helm 3.16's chart documentation.

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
| `Chart files: values.schema.json` | Yes | Not yet | The values' schema (`images`, `inputs`), so `helm install` rejects a mistyped value |
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
