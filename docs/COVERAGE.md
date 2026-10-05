# Coverage

Everything each format can do, and whether Isoloom produces it from a spec.

**What belongs in the format:** a machine feature the VM targets can do (local VMs, Proxmox,
cloud VMs). Containers are the lightweight option, used when they can produce the same machine:
a feature they can't makes an environment VM-only (like Windows) instead of staying out.
Container mechanics (capabilities, cgroups) aren't machine features: Isoloom sets them itself.
**100% coverage** means every portable feature is produced.

| Format | Coverage | Portable features | Done | Partly | To do |
| --- | ---: | ---: | ---: | ---: | ---: |
| [Docker Compose](#docker-compose) | 82% | 56 | 42 | 8 | 6 |
| [Vagrant](#vagrant) | 82% | 35 | 29 | 0 | 6 |
| [Vagrant: VirtualBox](#vagrant-virtualbox) | 100% | 5 | 5 | 0 | 0 |
| [Vagrant: VMware Desktop](#vagrant-vmware-desktop) | 90% | 5 | 4 | 1 | 0 |
| [Vagrant: Parallels](#vagrant-parallels) | 100% | 5 | 5 | 0 | 0 |
| [Vagrant: libvirt](#vagrant-libvirt) | 10% | 19 | 2 | 0 | 17 |
| [Vagrant: Hyper-V](#vagrant-hyper-v) | 25% | 4 | 1 | 0 | 3 |
| [Vagrant: UTM](#vagrant-utm) | 75% | 4 | 3 | 0 | 1 |
| [Vagrant: QEMU](#vagrant-qemu) | 14% | 14 | 2 | 0 | 12 |
| [Vagrant: ESXi](#vagrant-esxi) | 47% | 19 | 9 | 0 | 10 |
| [Terraform: Proxmox](#terraform-proxmox) | 43% | 16 | 7 | 0 | 9 |
| [Terraform: ESXi](#terraform-esxi) | 0% | 4 | 0 | 0 | 4 |
| [Terraform: AWS](#terraform-aws) | 63% | 19 | 12 | 0 | 7 |
| [Terraform: Azure](#terraform-azure) | 44% | 18 | 8 | 0 | 10 |
| [Terraform: Google Cloud](#terraform-google-cloud) | 40% | 10 | 4 | 0 | 6 |
| [Terraform: DigitalOcean](#terraform-digitalocean) | 50% | 8 | 4 | 0 | 4 |
| [Terraform: Linode](#terraform-linode) | 22% | 9 | 2 | 0 | 7 |
| [Terraform: Oracle Cloud](#terraform-oracle-cloud) | 46% | 13 | 6 | 0 | 7 |

Done includes features produced another way, or set in the image. The lists come from the
formats themselves (every key of the Compose schema, every setting of Vagrant and each provider
plugin, every resource type of each Terraform provider); tests fail on an unclassified entry, or
when a table disagrees with what Isoloom really generates.

## Docker Compose

82% of 56 portable features (42 done, 8 partly, 6 to do; 118 features in all). From the compose-spec schema, commit 914ec15d1fa4 (crates/isoloom-core/coverage/compose-spec.json).

### Top level

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `version` | n/a | No | Obsolete in Compose |
| `name` | Yes | Yes | From `name` |
| `include` | n/a | No | Compose tooling, not the environment's behavior |
| `services` | Yes | Yes | From `machines` |
| `networks` | Yes | Yes | From `networks` |
| `volumes` | Yes | Yes | From `machines.*.volumes` |
| `secrets` | Yes | Another way | Via inputs (values given at launch, never baked into images) |
| `configs` | Yes | In the image | Set it in the image (`docker.build`) or the VM's provisioning |
| `models` | n/a | No | Compose tooling, not the environment's behavior |
| `jobs` | Yes | Another way | Via docker.init (one-shot jobs) |

### Services

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `annotations` | n/a | No | Compose tooling, not the environment's behavior |
| `attach` | n/a | No | Compose tooling, not the environment's behavior |
| `blkio_config` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `build` | Yes | Yes | From `machines.*.docker.build` |
| `cap_add` | Yes | Partly | From `networks.*.gateway`; not yet: nothing more: capabilities only exist for containers (root on a VM has them all) |
| `cap_drop` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `cgroup` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `cgroup_parent` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `command` | Yes | In the image | Set it in the image (`docker.build`) or the VM's provisioning |
| `configs` | Yes | In the image | Set it in the image (`docker.build`) or the VM's provisioning |
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
| `depends_on` | Yes | Yes | From `machines.*.depends_on` |
| `deploy` | Yes | Partly | From `machines.*.resources`; not yet: replicas (see `scale`) |
| `develop` | n/a | No | Compose tooling, not the environment's behavior |
| `device_cgroup_rules` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `devices` | No | No | Host devices: hosting services and cloud VMs have none to pass |
| `dns` | Yes | Yes | From `machines.*.dns.servers` (and the Kubernetes dnsConfig) |
| `dns_opt` | Yes | In the image | Resolver options (resolv.conf) belong to the machine's provisioning |
| `dns_search` | Yes | Yes | From `machines.*.dns.search` (and the Kubernetes dnsConfig) |
| `domainname` | Yes | Yes | From `machines.*.dns.domain` |
| `entrypoint` | Yes | Partly | Isoloom's own containers and init jobs; not yet: a machine's own: set it in its image |
| `env_file` | Yes | Another way | Via inputs |
| `environment` | Yes | Partly | From `machines.*.inputs`; not yet: fixed values: set them in the image or the provisioning |
| `expose` | Yes | Another way | Via machines.*.services (every port is reachable on the machine's networks) |
| `extends` | n/a | No | Compose tooling, not the environment's behavior |
| `external_links` | n/a | No | Legacy; replaced by networks |
| `extra_hosts` | Yes | Yes | From `machines.*.networks` (names of machines on other networks) |
| `gpus` | No | No | Local VMs can't use the host's GPU |
| `group_add` | Yes | In the image | Set it in the image (`docker.build`) or the VM's provisioning |
| `healthcheck` | Yes | Yes | From `machines.*.services` (a probe of every port) |
| `hostname` | Yes | Yes | From the machine's name |
| `image` | Yes | Yes | From `machines.*.docker.image` |
| `init` | n/a | No | Container mechanics: a VM always runs its own init |
| `ipc` | No | No | Shares a kernel namespace with the host or another machine: separate VMs can't |
| `isolation` | No | No | Windows containers only |
| `label_file` | n/a | No | Compose tooling, not the environment's behavior |
| `labels` | Yes | Yes | From `machines.*.services` (isoloom.service.<name>, for tools reading the containers) |
| `links` | n/a | No | Legacy; replaced by networks |
| `logging` | n/a | No | Where the runner collects logs |
| `mac_address` | No | No | Cloud VMs get their MAC address from the provider |
| `mem_limit` | Yes | Another way | Via machines.*.resources.memory_mb, written as deploy.resources.limits |
| `mem_reservation` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `mem_swappiness` | No | No | Cloud VMs only choose a size: no CPU or memory scheduling knobs |
| `memswap_limit` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `models` | n/a | No | Compose tooling, not the environment's behavior |
| `network_mode` | Yes | Partly | Isoloom's own sidecars and check runner; not yet: nothing more: sharing another machine's network isn't possible between VMs |
| `networks` | Yes | Yes | From `machines.*.networks` (fixed addresses) |
| `oom_kill_disable` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `oom_score_adj` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `pid` | No | No | Shares a kernel namespace with the host or another machine: separate VMs can't |
| `pids_limit` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `platform` | Yes | Yes | From `machines.*.arch` (pinned on every container, so the machine runs the same on an x86-64 or an ARM host) |
| `ports` | Yes | Yes | From `machines.*.services[].publish` (on the host's loopback) |
| `post_start` | Yes | Another way | Via machines.*.docker.init (runs once the machine answers) |
| `pre_start` | Yes | Another way | Via machines.*.depends_on and docker.init |
| `pre_stop` | n/a | No | Compose tooling, not the environment's behavior |
| `privileged` | Yes | Yes | From `machines.*.privileged` (the container runs privileged; a VM already has it, its workload is root) |
| `profiles` | Yes | Yes | From `checks` (a `check` profile) |
| `provider` | n/a | No | Compose tooling, not the environment's behavior |
| `pull_policy` | n/a | No | Compose tooling, not the environment's behavior |
| `pull_refresh_after` | n/a | No | Compose tooling, not the environment's behavior |
| `read_only` | Yes | Yes | From `machines.*.read_only` (a read-only root filesystem; `volumes` stay writable) |
| `restart` | Yes | Yes | Always unless-stopped: machines stay up like VMs |
| `runtime` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `scale` | Yes | Not yet | Several identical machines (replicas), each with its own address |
| `secrets` | Yes | Another way | Via machines.*.inputs |
| `security_opt` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `shm_size` | Yes | Yes | From `machines.*.shm_size` (and a Memory emptyDir on Kubernetes) |
| `stdin_open` | n/a | No | Compose tooling, not the environment's behavior |
| `stop_grace_period` | n/a | No | Compose tooling, not the environment's behavior |
| `stop_signal` | Yes | In the image | Set it in the image (`docker.build`) or the VM's provisioning |
| `storage_opt` | n/a | No | Container mechanics: a VM has none; Isoloom sets them itself when a machine needs them |
| `sysctls` | Yes | Partly | From `networks.*.gateway`; not yet: a machine's own network sysctls (net.*): containers only allow those, VMs allow them too |
| `tmpfs` | Yes | Yes | From `machines.*.tmpfs` (and Memory emptyDirs on Kubernetes) |
| `tty` | n/a | No | Compose tooling, not the environment's behavior |
| `ulimits` | Yes | Not yet | Process limits (limits.conf or systemd on VMs) |
| `use_api_socket` | No | No | Hands the host's Docker to a machine: VMs and the cloud have none |
| `user` | Yes | Partly | Isoloom's check runner, as root to drop its default route on offline networks; not yet: a machine's own user: set it in its image |
| `userns_mode` | No | No | Shares a kernel namespace with the host or another machine: separate VMs can't |
| `uts` | No | No | Shares a kernel namespace with the host or another machine: separate VMs can't |
| `volumes` | Yes | Partly | From `machines.*.volumes` (and read-only mounts of init and check scripts); not yet: data shared between machines |
| `volumes_from` | Yes | Not yet | With volumes (data shared between machines) |
| `working_dir` | Yes | In the image | Set it in the image (`docker.build`) or the VM's provisioning |

### Networks

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `attachable` | n/a | No | Compose tooling, not the environment's behavior |
| `driver` | No | No | Docker network drivers (overlay, macvlan) have no VM equivalent |
| `driver_opts` | No | No | Docker network driver options |
| `enable_ipv4` | n/a | No | Networks are IPv4 (the default) |
| `enable_ipv6` | Yes | Not yet | IPv6 networks |
| `external` | Yes | Not yet | Joining a network outside the environment (a Docker network, a host bridge, a VPC) |
| `internal` | Yes | Yes | From `networks.*.internet: false` (when nothing routes) |
| `ipam` | Yes | Yes | From `networks.*.cidr` |
| `labels` | n/a | No | Compose tooling, not the environment's behavior |
| `name` | n/a | No | Compose scopes names to the environment |

### Volumes

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `driver` | No | No | Docker volume drivers have no VM equivalent |
| `driver_opts` | No | No | Docker volume driver options |
| `external` | Yes | Not yet | With volumes (data that outlives the environment) |
| `labels` | n/a | No | Compose tooling, not the environment's behavior |
| `name` | n/a | No | Compose scopes names to the environment |

## Vagrant

82% of 35 portable features (29 done, 0 partly, 6 to do; 61 features in all). From Vagrant 2.4.9 (config.vm, network types, provisioners).

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
| `disk` | Yes | Not yet | Machines.*.resources.disk_gb |
| `disks` | Yes | Not yet | Machines.*.resources.disk_gb |
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
| `forwarded_port` | Yes | Yes | From `machines.*.services[].publish` (on the host's loopback) |
| `public_network` | Yes | Not yet | A network bridged to the outside (Compose `external`, a host bridge, a VPC) |

### Provisioners

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `shell` | Yes | Yes | From `machines.*.vm.provision` (.sh), and Isoloom's own steps |
| `file` | Yes | Yes | The project, copied into each VM |
| `ansible_local` | Yes | Yes | From `machines.*.vm.provision` (.yml, .yaml) |
| `ansible` | Yes | Another way | Via ansible_local: the same playbooks, run inside the VM |
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

100% of 5 portable features (5 done, 0 partly, 0 to do; 15 features in all). From Vagrant 2.4.9 (the provider's config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `auto_nat_dns_proxy` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `check_guest_additions` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | Yes | From `machines.*.resources` |
| `customizations` | No | No | Raw commands for one hypervisor: no other target understands them |
| `customize` | No | No | Raw commands for one hypervisor: no other target understands them |
| `default_nic_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `destroy_unused_network_interfaces` | n/a | No | Vagrant tooling, not the environment's behavior |
| `functional_vboxsf` | n/a | No | Shared folders are off: the project is copied into each VM |
| `gui` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `linked_clone` | n/a | No | Faster starts from one image: no change in behavior |
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

100% of 5 portable features (5 done, 0 partly, 0 to do; 15 features in all). From vagrant-parallels 2.4.7 (its config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `check_guest_tools` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | Yes | From `machines.*.resources` |
| `customizations` | No | No | Raw commands for one hypervisor: no other target understands them |
| `customize` | No | No | Raw commands for one hypervisor: no other target understands them |
| `destroy_unused_network_interfaces` | n/a | No | Vagrant tooling, not the environment's behavior |
| `functional_psf` | n/a | No | Shared folders are off: the project is copied into each VM |
| `linked_clone` | n/a | No | Faster starts from one image: no change in behavior |
| `linked_clone_snapshot` | n/a | No | Vagrant tooling, not the environment's behavior |
| `memory` | Yes | Yes | From `machines.*.resources` |
| `name` | Yes | Yes | The environment and machine names, as the VM's display name |
| `network_adapter` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |
| `network_adapters` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |
| `optimize_power_consumption` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `regen_src_uuid` | n/a | No | Vagrant tooling, not the environment's behavior |
| `update_guest_tools` | n/a | No | Vagrant tooling, not the environment's behavior |

## Vagrant: libvirt

10% of 19 portable features (2 done, 0 partly, 17 to do; 146 features in all). From vagrant-libvirt 0.12.2 (its config class).

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
| `default_prefix` | n/a | No | Vagrant tooling, not the environment's behavior |
| `description` | n/a | No | Vagrant tooling, not the environment's behavior |
| `disk_address_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disk_bus` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disk_controller_model` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disk_device` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disk_driver` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disk_driver_opts` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `disks` | Yes | Not yet | Machines.*.resources.disk_gb |
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
| `machine_arch` | Yes | Not yet | A CPU architecture field (amd64, arm64) |
| `machine_type` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `machine_virtual_size` | Yes | Not yet | Machines.*.resources.disk_gb |
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
| `storage` | Yes | Not yet | Machines.*.resources.disk_gb |
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

25% of 4 portable features (1 done, 0 partly, 3 to do; 16 features in all). From Vagrant 2.4.9 (the provider's config class).

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `auto_start_action` | n/a | No | Vagrant tooling, not the environment's behavior |
| `auto_stop_action` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpus` | Yes | Not yet | With Hyper-V hosts |
| `differencing_disk` | n/a | No | Faster starts from one image: no change in behavior |
| `enable_automatic_checkpoints` | n/a | No | Vagrant tooling, not the environment's behavior |
| `enable_checkpoints` | n/a | No | Vagrant tooling, not the environment's behavior |
| `enable_enhanced_session_mode` | No | No | Display, input and host devices: containers and cloud VMs have none |
| `enable_virtualization_extensions` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `ip_address_timeout` | n/a | No | Vagrant tooling, not the environment's behavior |
| `linked_clone` | n/a | No | Faster starts from one image: no change in behavior |
| `mac` | No | No | Cloud VMs get their MAC address from the provider |
| `maxmemory` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `memory` | Yes | Not yet | With Hyper-V hosts |
| `vlan_id` | Yes | Another way | Via `config.vm.network private_network`, the same on every provider |
| `vm_integration_services` | n/a | No | Vagrant tooling, not the environment's behavior |
| `vmname` | Yes | Not yet | With Hyper-V hosts |

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

14% of 14 portable features (2 done, 0 partly, 12 to do; 34 features in all). From vagrant-qemu 0.6.3 (its config class), for Apple Silicon Macs.

### Settings

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `advanced_network` | Yes | Not yet | Private networks: QEMU gives a machine one private network (`advanced_network`), and needs vmnet or socket_vmnet on the Mac |
| `arch` | Yes | Not yet | A CPU architecture field (amd64, arm64) |
| `control_port` | n/a | No | Vagrant tooling, not the environment's behavior |
| `cpu` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `debug_port` | n/a | No | Vagrant tooling, not the environment's behavior |
| `default_qemu_dir` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `disk_resize` | Yes | Not yet | Machines.*.resources.disk_gb |
| `drive_interface` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `extra_drive_args` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `extra_image_opts` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `extra_netdev_args` | Yes | Not yet | With private networks on QEMU |
| `extra_qemu_args` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `firmware_format` | Yes | Not yet | Custom images with their own kernel and boot (makes an environment VM-only) |
| `graceful_timeout` | n/a | No | Vagrant tooling, not the environment's behavior |
| `homebrew_prefix` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `image_path` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `machine` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `mcast_addr` | Yes | Not yet | With private networks on QEMU |
| `memory` | Yes | Yes | From `machines.*.resources` |
| `net_device` | No | No | Hypervisor tuning: containers and cloud VMs have no such knob |
| `net_mode` | Yes | Not yet | Private networks: QEMU gives a machine one private network (`advanced_network`), and needs vmnet or socket_vmnet on the Mac |
| `no_daemonize` | n/a | No | Vagrant tooling, not the environment's behavior |
| `other_default` | n/a | No | Vagrant tooling, not the environment's behavior |
| `qemu_bin` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `qemu_dir` | n/a | No | The user's own setup (how Vagrant reaches the hypervisor, where it stores things) |
| `smp` | Yes | Yes | From `machines.*.resources.cpus` |
| `socket_opts` | Yes | Not yet | With private networks on QEMU |
| `socket_vmnet_client` | Yes | Not yet | With private networks on QEMU |
| `socket_vmnet_socket` | Yes | Not yet | With private networks on QEMU |
| `ssh_auto_correct` | n/a | No | Vagrant tooling, not the environment's behavior |
| `ssh_host` | n/a | No | Vagrant tooling, not the environment's behavior |
| `ssh_port` | n/a | No | Vagrant tooling, not the environment's behavior |
| `tap_device` | Yes | Not yet | With private networks on QEMU |
| `vmnet_interface` | Yes | Not yet | With private networks on QEMU |

## Vagrant: ESXi

47% of 19 portable features (9 done, 0 partly, 10 to do; 53 features in all). From vagrant-vmware-esxi 2.5.2 (its config class), for standalone ESXi hosts.

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
| `guest_boot_disk_size` | Yes | Not yet | Machines.*.resources.disk_gb |
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
| `guest_storage` | Yes | Not yet | Machines.*.resources.disk_gb |
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

43% of 16 portable features (7 done, 0 partly, 9 to do; 116 features in all). From bpg/proxmox 0.115.0 (its resource types).

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
| `proxmox_cloned_vm` | Yes | Not yet | Linked clones: faster starts from one image (no change in behavior) |
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
| `aws_eip` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_eip_association` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_instance` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_internet_gateway` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_key_pair` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_nat_gateway` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_network_interface` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_network_interface_attachment` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_route` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_route_table` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_route_table_association` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_security_group` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_subnet` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_volume_attachment` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_vpc` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_vpc_ipv4_cidr_block_association` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `aws_vpc_security_group_egress_rule` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `aws_vpc_security_group_ingress_rule` | Yes | Not yet | With the cloud generator (one VM per machine) |

And 1709 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Azure

44% of 18 portable features (8 done, 0 partly, 10 to do; 1106 features in all). From hashicorp/azurerm 5.8.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `azurerm_linux_virtual_machine` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `azurerm_managed_disk` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_nat_gateway` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_network_interface` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `azurerm_network_interface_security_group_association` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `azurerm_network_security_group` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `azurerm_network_security_rule` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_public_ip` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `azurerm_resource_group` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `azurerm_route` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_route_table` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_subnet` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `azurerm_subnet_nat_gateway_association` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_subnet_network_security_group_association` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_subnet_route_table_association` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_virtual_machine_data_disk_attachment` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `azurerm_virtual_network` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `azurerm_windows_virtual_machine` | Yes | Not yet | With Windows guests (which make an environment VM-only) |

And 1088 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Google Cloud

40% of 10 portable features (4 done, 0 partly, 6 to do; 1369 features in all). From hashicorp/google 8.5.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `google_compute_address` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_attached_disk` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_disk` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_firewall` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `google_compute_instance` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `google_compute_network` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `google_compute_route` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_router` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_router_nat` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `google_compute_subnetwork` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |

And 1359 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: DigitalOcean

50% of 8 portable features (4 done, 0 partly, 4 to do; 79 features in all). From digitalocean/digitalocean 2.103.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `digitalocean_droplet` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `digitalocean_firewall` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `digitalocean_reserved_ip` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `digitalocean_reserved_ip_assignment` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `digitalocean_ssh_key` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `digitalocean_volume` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `digitalocean_volume_attachment` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `digitalocean_vpc` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |

And 71 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Linode

22% of 9 portable features (2 done, 0 partly, 7 to do; 47 features in all). From linode/linode 4.7.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `linode_firewall` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `linode_instance` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `linode_instance_config` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `linode_instance_disk` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `linode_instance_ip` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `linode_sshkey` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `linode_volume` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `linode_vpc` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `linode_vpc_subnet` | Yes | Not yet | With the cloud generator (one VM per machine) |

And 38 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.

## Terraform: Oracle Cloud

46% of 13 portable features (6 done, 0 partly, 7 to do; 1020 features in all). From oracle/oci 9.8.0 (its resource types).

### Resource types

| Key | Every target | Implemented | Notes |
| --- | --- | --- | --- |
| `oci_core_instance` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `oci_core_internet_gateway` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `oci_core_nat_gateway` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_network_security_group` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_network_security_group_security_rule` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_public_ip` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_route_table` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `oci_core_security_list` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `oci_core_subnet` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `oci_core_vcn` | Yes | Yes | From the cloud-docker target (one VM running the Compose file), and on AWS the cloud-vm target (a VM per machine) |
| `oci_core_vnic_attachment` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_volume` | Yes | Not yet | With the cloud generator (one VM per machine) |
| `oci_core_volume_attachment` | Yes | Not yet | With the cloud generator (one VM per machine) |

And 1007 other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them.
