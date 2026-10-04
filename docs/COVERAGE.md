# Coverage

Everything each format can do, and whether Isoloom produces it from a spec. The lists come from
the formats themselves: every key of the Compose schema, every setting of Vagrant's and each
provider plugin's configuration. Tests fail when a list has an unclassified entry, or when it
disagrees with what Isoloom really generates. For Terraform, each provider's resource types
(the arguments of each are detailed as its generator is built).

| Format | Features | Yes | Partly | No |
| --- | ---: | ---: | ---: | ---: |
| [Docker Compose](#docker-compose) | 118 | 14 | 7 | 97 |
| [Vagrant](#vagrant) | 61 | 11 | 1 | 49 |
| [Vagrant: VirtualBox](#vagrant-virtualbox) | 15 | 3 | 0 | 12 |
| [Vagrant: VMware Desktop](#vagrant-vmware-desktop) | 24 | 0 | 1 | 23 |
| [Vagrant: Parallels](#vagrant-parallels) | 15 | 3 | 0 | 12 |
| [Vagrant: libvirt](#vagrant-libvirt) | 146 | 2 | 0 | 144 |
| [Vagrant: Hyper-V](#vagrant-hyper-v) | 16 | 0 | 0 | 16 |
| [Terraform: Proxmox](#terraform-proxmox) | 116 | 0 | 0 | 116 |

Every **No** says why: another way to get the same effect, in the image, not yet, by design, or
not needed (the format's own tooling).

## Docker Compose

From compose-spec schema, commit 914ec15d1fa4 (crates/isoloom-core/coverage/compose-spec.json).

### Top level

| Key | Implemented | Notes |
| --- | --- | --- |
| `version` | No | By design: obsolete in Compose |
| `name` | Yes | From `name` |
| `include` | No | Not needed: Compose tooling, not the environment's behavior |
| `services` | Yes | From `machines` |
| `networks` | Yes | From `networks` |
| `volumes` | No | Not yet: persistent or shared data: no concept in the format yet |
| `secrets` | No | Another way: via inputs (values given at launch, never baked into images) |
| `configs` | No | In the image: set it in the image (`docker.build`) |
| `models` | No | Not needed: Compose tooling, not the environment's behavior |
| `jobs` | No | Another way: via docker.init (one-shot jobs) |

### Services

| Key | Implemented | Notes |
| --- | --- | --- |
| `annotations` | No | Not needed: Compose tooling, not the environment's behavior |
| `attach` | No | Not needed: Compose tooling, not the environment's behavior |
| `blkio_config` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `build` | Yes | From `machines.*.docker.build` |
| `cap_add` | Partly | From `networks.*.gateway`; not: only NET_ADMIN, for gateways and Isoloom's own router |
| `cap_drop` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `cgroup` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `cgroup_parent` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `command` | No | In the image: set it in the image (`docker.build`) |
| `configs` | No | In the image: set it in the image (`docker.build`) |
| `container_name` | No | By design: names come from the environment; the hostname is the machine's name |
| `cpu_count` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `cpu_percent` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `cpu_period` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `cpu_quota` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `cpu_rt_period` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `cpu_rt_runtime` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `cpu_shares` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `cpus` | No | Another way: via machines.*.resources.cpus, written as deploy.resources.limits |
| `cpuset` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `credential_spec` | No | By design: Windows containers only |
| `depends_on` | Yes | From `machines.*.depends_on` |
| `deploy` | Partly | From `machines.*.resources`; not: only CPU and memory limits |
| `develop` | No | Not needed: Compose tooling, not the environment's behavior |
| `device_cgroup_rules` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `devices` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `dns` | No | Not yet: a DNS concept for networks (servers, search domains) |
| `dns_opt` | No | Not yet: with DNS |
| `dns_search` | No | Not yet: with DNS |
| `domainname` | No | Not yet: with DNS |
| `entrypoint` | Partly | Written for Isoloom's own containers and init jobs; not: a machine's own: set it in its image |
| `env_file` | No | Another way: via inputs |
| `environment` | Partly | From `machines.*.inputs`; not: only values given at launch; fixed values belong in the image |
| `expose` | No | Another way: via machines.*.services (every port is reachable on the machine's networks) |
| `extends` | No | Not needed: Compose tooling, not the environment's behavior |
| `external_links` | No | Not needed: legacy; replaced by networks |
| `extra_hosts` | Yes | From `machines.*.networks` (names of machines on other networks) |
| `gpus` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `group_add` | No | In the image: set it in the image (`docker.build`) |
| `healthcheck` | Yes | From `machines.*.services` (a probe of every port) |
| `hostname` | Yes | From the machine's name |
| `image` | Yes | From `machines.*.docker.image` |
| `init` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `ipc` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `isolation` | No | By design: Windows containers only |
| `label_file` | No | Not needed: Compose tooling, not the environment's behavior |
| `labels` | No | Not needed: Compose tooling, not the environment's behavior |
| `links` | No | Not needed: legacy; replaced by networks |
| `logging` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `mac_address` | No | By design: addresses are fixed at the IP level, the same on every target |
| `mem_limit` | No | Another way: via machines.*.resources.memory_mb, written as deploy.resources.limits |
| `mem_reservation` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `mem_swappiness` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `memswap_limit` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `models` | No | Not needed: Compose tooling, not the environment's behavior |
| `network_mode` | Partly | Internal; not: used by Isoloom's sidecars and check runner; a machine can't share another's network |
| `networks` | Yes | From `machines.*.networks` (fixed addresses) |
| `oom_kill_disable` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `oom_score_adj` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `pid` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `pids_limit` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `platform` | No | Not yet: a CPU architecture field (amd64, arm64), for VMs too |
| `ports` | No | By design: nothing is published on the host: the environment is reached from its own networks |
| `post_start` | No | Another way: via machines.*.docker.init (runs once the machine answers) |
| `pre_start` | No | Another way: via machines.*.depends_on and docker.init |
| `pre_stop` | No | Not needed: Compose tooling, not the environment's behavior |
| `privileged` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `profiles` | Yes | From `checks` (a `check` profile) |
| `provider` | No | Not needed: Compose tooling, not the environment's behavior |
| `pull_policy` | No | Not needed: Compose tooling, not the environment's behavior |
| `pull_refresh_after` | No | Not needed: Compose tooling, not the environment's behavior |
| `read_only` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `restart` | Yes | Always unless-stopped: machines stay up like VMs |
| `runtime` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `scale` | No | By design: machines are individuals with their own addresses |
| `secrets` | No | Another way: via machines.*.inputs |
| `security_opt` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `shm_size` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `stdin_open` | No | Not needed: Compose tooling, not the environment's behavior |
| `stop_grace_period` | No | Not needed: Compose tooling, not the environment's behavior |
| `stop_signal` | No | In the image: set it in the image (`docker.build`) |
| `storage_opt` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `sysctls` | Partly | From `networks.*.gateway`; not: only forwarding, for gateways and Isoloom's own router |
| `tmpfs` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `tty` | No | Not needed: Compose tooling, not the environment's behavior |
| `ulimits` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `use_api_socket` | No | By design: would hand the host's Docker to a machine |
| `user` | No | In the image: set it in the image (`docker.build`) |
| `userns_mode` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `uts` | No | Not yet, undecided: a container-runtime knob with no VM equivalent; would need a field under `docker:` |
| `volumes` | Partly | Read-only mounts of init and check scripts; not: persistent or shared data: no concept in the format yet |
| `volumes_from` | No | Not yet: with volumes |
| `working_dir` | No | In the image: set it in the image (`docker.build`) |

### Networks

| Key | Implemented | Notes |
| --- | --- | --- |
| `attachable` | No | Not needed: Compose tooling, not the environment's behavior |
| `driver` | No | By design: always a bridge: the same layer-2 network as on VM targets |
| `driver_opts` | No | By design: host-level tuning: no meaning for the same machine as a VM |
| `enable_ipv4` | No | By design: networks are IPv4 (the default) |
| `enable_ipv6` | No | Not yet: IPv6 networks |
| `external` | No | By design: an environment is self-contained |
| `internal` | Yes | From `networks.*.internet: false` (when nothing routes) |
| `ipam` | Yes | From `networks.*.cidr` |
| `labels` | No | Not needed: Compose tooling, not the environment's behavior |
| `name` | No | By design: names are scoped to the environment by Compose |

### Volumes

| Key | Implemented | Notes |
| --- | --- | --- |
| `driver` | No | Not yet: with volumes |
| `driver_opts` | No | Not yet: with volumes |
| `external` | No | By design: an environment is self-contained |
| `labels` | No | Not needed: Compose tooling, not the environment's behavior |
| `name` | No | By design: names are scoped to the environment by Compose |

## Vagrant

From Vagrant 2.4.9 (config.vm, network types, provisioners).

### Machine settings (config.vm)

| Key | Implemented | Notes |
| --- | --- | --- |
| `allow_fstab_modification` | No | Not needed: shared folders are off: the project is copied into each VM |
| `allow_hosts_modification` | No | Not needed: Isoloom writes /etc/hosts itself (names of the other machines) |
| `allowed_synced_folder_types` | No | Not needed: shared folders are off: the project is copied into each VM |
| `base_address` | No | By design: addresses are fixed at the IP level, the same on every target |
| `base_mac` | No | By design: addresses are fixed at the IP level, the same on every target |
| `boot_timeout` | Yes | Fixed: 10 minutes |
| `box` | Partly | From `machines.*.vm.os`; not: Linux images only; Windows comes later |
| `box_architecture` | No | Not yet: a CPU architecture field (amd64, arm64) |
| `box_check_update` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_ca_cert` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_ca_path` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_checksum` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_checksum_type` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_client_cert` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_disable_ssl_revoke_best_effort` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_insecure` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_location_trusted` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_download_options` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_extra_download_options` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `box_server_url` | No | Not yet, undecided: custom images: would need a field under `vm:` |
| `box_url` | No | Not yet, undecided: custom images: would need a field under `vm:` |
| `box_version` | No | Not yet: pinning images to a version |
| `clone` | No | Not needed: starts from another Vagrant machine; machines start from their image |
| `cloud_init` | No | Another way: via provisioning steps run any setup |
| `cloud_init_configs` | No | Another way: via provisioning steps run any setup |
| `cloud_init_first_boot_only` | No | Another way: via provisioning steps run any setup |
| `communicator` | No | Not yet: with Windows guests |
| `define` | Yes | From `machines` |
| `disk` | No | Not yet: machines.*.resources.disk_gb |
| `disks` | No | Not yet: machines.*.resources.disk_gb |
| `graceful_halt_timeout` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `guest` | No | Not yet: with Windows guests |
| `host_name` | No | Not needed: old name of `hostname` |
| `hostname` | Yes | From the machine's name |
| `ignore_box_vagrantfile` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `network` | Yes | From `networks` (see the network types below) |
| `post_up_message` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `provider` | Yes | From `machines.*.resources` (see each provider below) |
| `provision` | Yes | From `machines.*.vm.provision` (see the provisioners below) |
| `provisioners` | No | Not needed: Vagrant's own list behind `provision` |
| `synced_folder` | Yes | Disabled: the project is copied into each VM instead |
| `usable_port_range` | No | By design: would expose the environment on the user's machine |

### Network types

| Key | Implemented | Notes |
| --- | --- | --- |
| `private_network` | Yes | From `networks and machines.*.networks` |
| `forwarded_port` | No | By design: would expose the environment on the user's machine |
| `public_network` | No | By design: would put the environment on the user's LAN |

### Provisioners

| Key | Implemented | Notes |
| --- | --- | --- |
| `shell` | Yes | From `machines.*.vm.provision` (.sh), and Isoloom's own steps |
| `file` | Yes | The project, copied into each VM |
| `ansible_local` | Yes | From `machines.*.vm.provision` (.yml, .yaml) |
| `ansible` | No | By design: Ansible runs inside the VM, never on the user's machine |
| `chef` | No | Another way: via a `.sh` provisioning step can run any tool |
| `puppet` | No | Another way: via a `.sh` provisioning step can run any tool |
| `salt` | No | Another way: via a `.sh` provisioning step can run any tool |
| `cfengine` | No | Another way: via a `.sh` provisioning step can run any tool |
| `docker` | No | By design: VMs run their services natively; containers come from `docker:` |
| `podman` | No | By design: VMs run their services natively; containers come from `docker:` |
| `container` | No | By design: VMs run their services natively; containers come from `docker:` |

### Other settings

| Key | Implemented | Notes |
| --- | --- | --- |
| `config.ssh` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `config.winrm` | No | Not yet: with Windows guests |
| `config.winssh` | No | Not yet: with Windows guests |
| `config.trigger` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `config.vagrant` | No | Not needed: Vagrant tooling, not the environment's behavior |

## Vagrant: VirtualBox

From Vagrant 2.4.9 (the provider's config class).

### Settings

| Key | Implemented | Notes |
| --- | --- | --- |
| `auto_nat_dns_proxy` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `check_guest_additions` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | From `machines.*.resources` |
| `customizations` | No | By design: provider-specific commands: the same machine must behave the same on every provider |
| `customize` | No | By design: provider-specific commands: the same machine must behave the same on every provider |
| `default_nic_type` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `destroy_unused_network_interfaces` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `functional_vboxsf` | No | Not needed: shared folders are off: the project is copied into each VM |
| `gui` | No | By design: display, input and host devices: an environment is reached over its networks |
| `linked_clone` | No | Not yet: faster starts from one image (no change in behavior) |
| `linked_clone_snapshot` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `memory` | Yes | From `machines.*.resources` |
| `name` | Yes | The environment and machine names, as the VM's display name |
| `network_adapter` | No | By design: networks come from `config.vm.network private_network`, the same on every provider |
| `network_adapters` | No | By design: networks come from `config.vm.network private_network`, the same on every provider |

## Vagrant: VMware Desktop

From vagrant-vmware-desktop 3.0.5 (its config class).

### Settings

| Key | Implemented | Notes |
| --- | --- | --- |
| `allowlist_verified` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `base_address` | No | By design: addresses are fixed at the IP level, the same on every target |
| `base_mac` | No | By design: addresses are fixed at the IP level, the same on every target |
| `clone_directory` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `cpus` | No | Another way: via machines.*.resources, written in `vmx` |
| `enable_vmrun_ip_lookup` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `force_vmware_license` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `functional_hgfs` | No | Not needed: shared folders are off: the project is copied into each VM |
| `gui` | No | By design: display, input and host devices: an environment is reached over its networks |
| `linked_clone` | No | Not yet: faster starts from one image (no change in behavior) |
| `memory` | No | Another way: via machines.*.resources, written in `vmx` |
| `nat_device` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `network_adapter` | No | By design: networks come from `config.vm.network private_network`, the same on every provider |
| `network_adapters` | No | By design: networks come from `config.vm.network private_network`, the same on every provider |
| `port_forward_network_pause` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `shared_folder_special_char` | No | Not needed: shared folders are off: the project is copied into each VM |
| `ssh_info_public` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `unmount_default_hgfs` | No | Not needed: shared folders are off: the project is copied into each VM |
| `utility_certificate_path` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `utility_host` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `utility_port` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `verify_vmnet` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `vmx` | Partly | From `machines.*.resources`; not: only displayName, numvcpus and memsize |
| `whitelist_verified` | No | Not needed: Vagrant tooling, not the environment's behavior |

## Vagrant: Parallels

From vagrant-parallels 2.4.7 (its config class).

### Settings

| Key | Implemented | Notes |
| --- | --- | --- |
| `check_guest_tools` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | From `machines.*.resources` |
| `customizations` | No | By design: provider-specific commands: the same machine must behave the same on every provider |
| `customize` | No | By design: provider-specific commands: the same machine must behave the same on every provider |
| `destroy_unused_network_interfaces` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `functional_psf` | No | Not needed: shared folders are off: the project is copied into each VM |
| `linked_clone` | No | Not yet: faster starts from one image (no change in behavior) |
| `linked_clone_snapshot` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `memory` | Yes | From `machines.*.resources` |
| `name` | Yes | The environment and machine names, as the VM's display name |
| `network_adapter` | No | By design: networks come from `config.vm.network private_network`, the same on every provider |
| `network_adapters` | No | By design: networks come from `config.vm.network private_network`, the same on every provider |
| `optimize_power_consumption` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `regen_src_uuid` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `update_guest_tools` | No | Not needed: Vagrant tooling, not the environment's behavior |

## Vagrant: libvirt

From vagrant-libvirt 0.12.2 (its config class).

### Settings

| Key | Implemented | Notes |
| --- | --- | --- |
| `autostart` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `boot` | No | By design: machines boot from their image |
| `boot_order` | No | By design: machines boot from their image |
| `cdroms` | No | By design: display, input and host devices: an environment is reached over its networks |
| `channel` | No | By design: display, input and host devices: an environment is reached over its networks |
| `channels` | No | By design: display, input and host devices: an environment is reached over its networks |
| `clock_absolute` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `clock_adjustment` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `clock_basis` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `clock_offset` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `clock_timer` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `clock_timers` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `clock_timezone` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `cmd_line` | No | By design: machines boot from their image |
| `connect_via_ssh` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `cpu_affinity` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `cpu_fallback` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `cpu_feature` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `cpu_features` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `cpu_mode` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `cpu_model` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `cpu_topology` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `cpuaffinitiy` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `cpus` | Yes | From `machines.*.resources` |
| `cpuset` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `cputopology` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `default_prefix` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `description` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `disk_address_type` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `disk_bus` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `disk_controller_model` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `disk_device` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `disk_driver` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `disk_driver_opts` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `disks` | No | Not yet: machines.*.resources.disk_gb |
| `driver` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `dtb` | No | By design: machines boot from their image |
| `emulator_path` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `features` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `features_hyperv` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `floppies` | No | By design: display, input and host devices: an environment is reached over its networks |
| `forward_ssh_port` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `graphics_autoport` | No | By design: display, input and host devices: an environment is reached over its networks |
| `graphics_gl` | No | By design: display, input and host devices: an environment is reached over its networks |
| `graphics_ip` | No | By design: display, input and host devices: an environment is reached over its networks |
| `graphics_passwd` | No | By design: display, input and host devices: an environment is reached over its networks |
| `graphics_port` | No | By design: display, input and host devices: an environment is reached over its networks |
| `graphics_type` | No | By design: display, input and host devices: an environment is reached over its networks |
| `graphics_websocket` | No | By design: display, input and host devices: an environment is reached over its networks |
| `host` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `host_device_exclude_prefixes` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `hyperv_feature` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `id_ssh_key_file` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `initrd` | No | By design: machines boot from their image |
| `input` | No | By design: display, input and host devices: an environment is reached over its networks |
| `inputs` | No | By design: display, input and host devices: an environment is reached over its networks |
| `kernel` | No | By design: machines boot from their image |
| `keymap` | No | By design: display, input and host devices: an environment is reached over its networks |
| `kvm_hidden` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `launchsecurity` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `launchsecurity_data` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `loader` | No | By design: machines boot from their image |
| `machine_arch` | No | Not yet: a CPU architecture field (amd64, arm64) |
| `machine_type` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `machine_virtual_size` | No | Not yet: machines.*.resources.disk_gb |
| `management_network_address` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_autostart` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_device` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_domain` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_driver_iommu` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_guest_ipv6` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_iface_name` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_keep` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_mac` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_mode` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_model_type` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_mtu` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_name` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_pci_bus` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `management_network_pci_slot` | No | Not needed: Vagrant's management network, used to provision (like the NAT interface elsewhere) |
| `memballoon_enabled` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `memballoon_model` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `memballoon_pci_bus` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `memballoon_pci_slot` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `memory` | Yes | From `machines.*.resources` |
| `memory_backing` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `memorybacking` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `memtune` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `memtunes` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `mgmt_attach` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `nested` | No | Not yet, undecided: nested virtualization: would need a field under `vm:` |
| `nic_adapter_count` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `nic_model_type` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `nodeset` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `numa_nodes` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `nvram` | No | By design: machines boot from their image |
| `password` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `pci` | No | By design: display, input and host devices: an environment is reached over its networks |
| `pcis` | No | By design: display, input and host devices: an environment is reached over its networks |
| `port` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `proxy_command` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `qemu_args` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `qemu_env` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `qemu_use_agent` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `qemu_use_session` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `qemuargs` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `qemuenv` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `random` | No | By design: display, input and host devices: an environment is reached over its networks |
| `random_hostname` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `redirdev` | No | By design: display, input and host devices: an environment is reached over its networks |
| `redirdevs` | No | By design: display, input and host devices: an environment is reached over its networks |
| `redirfilter` | No | By design: display, input and host devices: an environment is reached over its networks |
| `redirfilters` | No | By design: display, input and host devices: an environment is reached over its networks |
| `rng` | No | By design: display, input and host devices: an environment is reached over its networks |
| `serial` | No | By design: display, input and host devices: an environment is reached over its networks |
| `serials` | No | By design: display, input and host devices: an environment is reached over its networks |
| `shares` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `smartcard` | No | By design: display, input and host devices: an environment is reached over its networks |
| `smartcard_dev` | No | By design: display, input and host devices: an environment is reached over its networks |
| `snapshot_pool_name` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `socket` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `sound_type` | No | By design: display, input and host devices: an environment is reached over its networks |
| `storage` | No | Not yet: machines.*.resources.disk_gb |
| `storage_pool_name` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `storage_pool_path` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `suspend_mode` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `sysinfo` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `system_uri` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `title` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `tpm_model` | No | Not yet: with Windows guests |
| `tpm_path` | No | Not yet: with Windows guests |
| `tpm_type` | No | Not yet: with Windows guests |
| `tpm_version` | No | Not yet: with Windows guests |
| `uri` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `usb` | No | By design: display, input and host devices: an environment is reached over its networks |
| `usb_controller` | No | By design: display, input and host devices: an environment is reached over its networks |
| `usbctl_dev` | No | By design: display, input and host devices: an environment is reached over its networks |
| `usbs` | No | By design: display, input and host devices: an environment is reached over its networks |
| `username` | No | Not needed: the user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `uuid` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `video_accel3d` | No | By design: display, input and host devices: an environment is reached over its networks |
| `video_type` | No | By design: display, input and host devices: an environment is reached over its networks |
| `video_vram` | No | By design: display, input and host devices: an environment is reached over its networks |
| `volume_cache` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `watchdog` | No | By design: display, input and host devices: an environment is reached over its networks |
| `watchdog_dev` | No | By design: display, input and host devices: an environment is reached over its networks |

## Vagrant: Hyper-V

From Vagrant 2.4.9 (the provider's config class).

### Settings

| Key | Implemented | Notes |
| --- | --- | --- |
| `auto_start_action` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `auto_stop_action` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `cpus` | No | Not yet: with Hyper-V hosts |
| `differencing_disk` | No | Not yet: faster starts from one image (no change in behavior) |
| `enable_automatic_checkpoints` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `enable_checkpoints` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `enable_enhanced_session_mode` | No | By design: display, input and host devices: an environment is reached over its networks |
| `enable_virtualization_extensions` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `ip_address_timeout` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `linked_clone` | No | Not yet: faster starts from one image (no change in behavior) |
| `mac` | No | By design: networks come from `config.vm.network private_network`, the same on every provider |
| `maxmemory` | No | By design: hypervisor tuning: no meaning for the same machine as a container or on another provider |
| `memory` | No | Not yet: with Hyper-V hosts |
| `vlan_id` | No | By design: networks come from `config.vm.network private_network`, the same on every provider |
| `vm_integration_services` | No | Not needed: Vagrant tooling, not the environment's behavior |
| `vmname` | No | Not yet: with Hyper-V hosts |

## Terraform: Proxmox

From bpg/proxmox 0.115.0 (its resource types).

### Resource types

| Key | Implemented | Notes |
| --- | --- | --- |
| `proxmox_acl` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_acme_account` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_acme_certificate` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_acme_dns_plugin` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_apt_repository` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_apt_standard_repository` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_backup_job` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_ceph_pool` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_cloned_vm` | No | Not yet: linked clones: faster starts from one image (no change in behavior) |
| `proxmox_cluster_options` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_download_file` | No | Not yet: with the Proxmox generator |
| `proxmox_hagroup` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_hardware_mapping_dir` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_hardware_mapping_pci` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_hardware_mapping_usb` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_haresource` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_harule` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_metrics_server` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_network_applier` | No | By design: changes the server's own network; an environment gets its own SDN networks |
| `proxmox_network_linux_bond` | No | By design: changes the server's own network; an environment gets its own SDN networks |
| `proxmox_network_linux_bridge` | No | By design: changes the server's own network; an environment gets its own SDN networks |
| `proxmox_network_linux_vlan` | No | By design: changes the server's own network; an environment gets its own SDN networks |
| `proxmox_node_config` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_node_disk_zfs` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_node_firewall` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_oci_image` | No | Not yet, undecided: LXC containers: a way to run `docker:` machines on Proxmox |
| `proxmox_pool_membership` | No | Not yet: with the Proxmox generator |
| `proxmox_realm_ldap` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_realm_openid` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_realm_sync` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_replication` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_sdn_applier` | No | Not yet: with the Proxmox generator |
| `proxmox_sdn_controller_evpn` | No | Not yet, undecided: multi-node clusters (a simple zone covers one node) |
| `proxmox_sdn_fabric_node_openfabric` | No | Not yet, undecided: multi-node clusters (a simple zone covers one node) |
| `proxmox_sdn_fabric_node_ospf` | No | Not yet, undecided: multi-node clusters (a simple zone covers one node) |
| `proxmox_sdn_fabric_openfabric` | No | Not yet, undecided: multi-node clusters (a simple zone covers one node) |
| `proxmox_sdn_fabric_ospf` | No | Not yet, undecided: multi-node clusters (a simple zone covers one node) |
| `proxmox_sdn_subnet` | No | By design: a subnet's gateway lives on the host bridge, which would route between networks; the router VM holds it |
| `proxmox_sdn_vnet` | No | Not yet: with the Proxmox generator |
| `proxmox_sdn_zone_evpn` | No | Not yet, undecided: multi-node clusters (a simple zone covers one node) |
| `proxmox_sdn_zone_qinq` | No | Not yet, undecided: multi-node clusters (a simple zone covers one node) |
| `proxmox_sdn_zone_simple` | No | Not yet: with the Proxmox generator |
| `proxmox_sdn_zone_vlan` | No | Not yet, undecided: multi-node clusters (a simple zone covers one node) |
| `proxmox_sdn_zone_vxlan` | No | Not yet, undecided: multi-node clusters (a simple zone covers one node) |
| `proxmox_storage_cifs` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_storage_directory` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_storage_lvm` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_storage_lvmthin` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_storage_nfs` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_storage_pbs` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_storage_zfspool` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_user_token` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_virtual_environment_acl` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_acme_account` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_acme_certificate` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_acme_dns_plugin` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_apt_repository` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_apt_standard_repository` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_certificate` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_virtual_environment_cloned_vm` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_cluster_firewall` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_virtual_environment_cluster_firewall_security_group` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_virtual_environment_cluster_options` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_container` | No | Not yet, undecided: LXC containers: a way to run `docker:` machines on Proxmox |
| `proxmox_virtual_environment_dns` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_virtual_environment_download_file` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_file` | No | Not yet: with the Proxmox generator |
| `proxmox_virtual_environment_firewall_alias` | No | Not yet, undecided: the Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_firewall_ipset` | No | Not yet, undecided: the Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_firewall_options` | No | Not yet, undecided: the Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_firewall_rules` | No | Not yet, undecided: the Proxmox firewall: reach rules enforced outside the VMs, besides the router |
| `proxmox_virtual_environment_group` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_virtual_environment_hagroup` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_hardware_mapping_dir` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_hardware_mapping_pci` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_hardware_mapping_usb` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_haresource` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_harule` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_hosts` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_virtual_environment_metrics_server` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_network_linux_bridge` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_network_linux_vlan` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_node_firewall` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_oci_image` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_pool` | No | Not yet: with the Proxmox generator |
| `proxmox_virtual_environment_pool_membership` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_realm_ldap` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_realm_openid` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_realm_sync` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_replication` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_role` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_virtual_environment_sdn_applier` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_fabric_node_openfabric` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_fabric_node_ospf` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_fabric_openfabric` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_fabric_ospf` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_subnet` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_vnet` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_zone_evpn` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_zone_qinq` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_zone_simple` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_zone_vlan` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_sdn_zone_vxlan` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_cifs` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_directory` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_lvm` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_lvmthin` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_nfs` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_pbs` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_storage_zfspool` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_time` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_virtual_environment_user` | No | By design: administers the Proxmox server itself, not an environment on it |
| `proxmox_virtual_environment_user_token` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_vm` | No | Not needed: older name of the resource without `virtual_environment_` |
| `proxmox_virtual_environment_vm2` | No | Not needed: an older, experimental VM resource |
| `proxmox_vm` | No | Not yet: with the Proxmox generator |
