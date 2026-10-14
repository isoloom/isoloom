# Targets

A target is where an environment runs: containers on your Docker, a Kubernetes cluster, local
VMs, a Proxmox or ESXi server, a cloud account, or hosts that already exist. Isoloom derives the
targets a spec can run on from its machines (every machine has `docker:`, so the container
targets; every machine has `vm:`, so the VM targets), and a `targets:` list in the spec can only
narrow them.

```sh
isoloom targets           # where this spec can run, and why not elsewhere
isoloom targets --host    # and whether this machine can run each one now
isoloom doctor            # every target this machine could run, and what is missing
```

Every target follows the same flow:

```sh
isoloom run TARGET        # generate .isoloom/TARGET/, then bring it up with the target's tool
isoloom test TARGET       # the checks, from inside the environment
isoloom down TARGET       # tear it down
```

`run`, `test` and `down` also take `--instance N` (a second copy with its own names, blocks and
ports, written to `.isoloom-N/`), `-s KEY=VALUE` (a spec value), `--profile NAME` and `--json`
([JSON output](json.md)). Before the tool starts, `run` checks the host (as `targets --host`
does) and reads every [secret](secrets.md). While an environment runs, `isoloom status`,
`connect`, `exec`, `capture` and `provision` work on it.

A target refuses a spec it can't produce, with the reason, rather than dropping a field. The
[spec reference](spec.md) lists, for every field, the targets that render it and those that
refuse it, in the backends' own words. "Not run on a real host" below means the target's files
are generated, checked by `terraform validate` or snapshot tests, and run nowhere for real yet;
the workflows that will run them are described in [Real targets in CI](real-targets.md).

Volumes, `tmpfs` and data disks (`disks`) differ the most between targets: each section below
says what its target does with them, and [Storage](storage.md) has the whole table.

| Target | Builds | Tool |
|---|---|---|
| [`docker`](#docker) | Containers on your Docker | Docker Compose |
| [`hosted`](#hosted) | The same Compose file, for a hosting service | Docker Compose |
| [`kubernetes`](#kubernetes) | Manifests for a cluster you already have | kubectl |
| [`helm`](#helm) | The same environment as a Helm chart (an output of `kubernetes`) | Helm |
| [`kustomize`](#kustomize) | The same environment as a Kustomize overlay (an output of `kubernetes`) | kubectl |
| [`vagrant`](#vagrant) | One local VM per machine | Vagrant |
| [`docker-vm`](#docker-vm) | The containers on one VM | Vagrant (or Terraform on Proxmox) |
| [`hybrid`](#hybrid) | Containers and VMs on the same networks | Vagrant |
| [`proxmox`](#proxmox) | One VM (or LXC container) per machine on Proxmox VE | Terraform |
| [`esxi`](#esxi) | One VM per machine on a standalone ESXi host | Terraform |
| [`cloud-vm`](#cloud-vm) | One cloud VM per machine | Terraform |
| [`cloud-docker`](#cloud-docker) | The containers on one cloud VM | Terraform |
| [`cloud-services`](#cloud-services) | Your Terraform module, in your cloud account | Terraform |
| [`external`](#external) | Nothing: hosts that already exist, set up over SSH | SSH, Ansible |

## docker

**Builds** `.isoloom/docker/compose.yml` (and the files it mounts): a container per machine, a
Docker network per network, routers that enforce `reach` between networks and apply `tc`, the
published ports on the host's loopback, and a `check` profile with the check runners.

**The host needs** Docker with its daemon running and Compose v2 (`docker compose version`).

**Run, test, down:**

- `run`: `docker compose up -d --build --wait`. One-shot jobs run once the rest is up.
- `test`: `docker compose --profile check run --rm isoloom-check` for each place a check runs
  from; `exec` checks run with `docker compose exec`.
- `down`: `docker compose --profile check down -v`, volumes included.

By hand: `docker compose -f .isoloom/docker/compose.yml up -d --wait`.

**Limits.** Refused: a network's `mtu` or `bridged`, a machine's `rate_limit` or
`nested_virtualization`, and environment-level `provision:` (it runs on the VM targets). A
Windows machine has no `docker:`, so an environment with one never gets this target.

**Storage.** Volumes are named volumes; `persistent` ones survive `down` (`down --volumes`
removes them) and `external` ones are existing volumes. A data disk becomes a named volume at
its mount; a `raw` disk is refused (a container gets no block device).

**Run for real** in CI at every push: hello-stack, air-gapped, segmented, edge-firewall,
slow-link, udp-dns, tuned-web and shared-files are brought up, tested and torn down.

## hosted

**Builds** the same Compose file as `docker`, for a service that runs Docker Compose for you
(the Docker shape, on networks laid out on Docker's own address blocks).

**The host needs** the same as `docker`: `run`, `test` and `down` run the same commands. Point
`docker` at the service the way it documents (a context or `DOCKER_HOST`); Isoloom checks no
credentials of its own.

**Limits:** the same as `docker`.
A data disk is refused: Isoloom creates no machine to attach one to.

**Not run on a real host.**

## kubernetes

**Builds** everything under `.isoloom/kubernetes/`:

- the manifests: a Namespace, a Deployment and Service per machine, NetworkPolicies for the
  networks and `reach`, claims for the volumes, and a Job per check position;
- `kustomization.yaml`, with the project's scripts as a ConfigMap;
- a Helm chart in `chart/`, with `values.yaml`, its `values.schema.json`, the checks as
  `helm test` pods and the inputs as a Secret. `isoloom chart --version 1.2.0` packages it.

**The host needs** `kubectl`, a current context, and a cluster that answers
(`kubectl cluster-info`). `KUBECONFIG` works as kubectl reads it. Helm is needed only for the
chart.

**Run, test, down:**

- `run`: creates the [secrets](secrets.md) Secret when the spec has secrets, then
  `kubectl kustomize --load-restrictor LoadRestrictionsNone .isoloom/kubernetes | kubectl apply -f -`.
- `test`: runs the check Jobs and reads their logs; `exec` checks run with `kubectl exec`.
- `down`: the same pipe into `kubectl delete`.

**Limits.** Refused: link impairment (`tc`) and networks with a `gateway` machine (Kubernetes
has no router in the path), a network's `mtu`, `bridged` or `external`, a machine's
`rate_limit`, `nested_virtualization` or `devices`, `gpus.ids` (give a count) and
`gpus.options`, environment-level `provision:`, a secret's `uid` or `gid`, and `dns.private`
(a cluster names Services under its own domain). A network's `cidr` and a machine's address
on it are kept as intent only: Kubernetes picks pod addresses. The manifests, the chart and the
overlay refuse the same things.

**Storage.** Volumes are claims (`persistent` ones kept by `down`, `external` ones existing
claims in the namespace). A data disk is a claim of its size, a `raw` one in block mode. A
`tmpfs` `mode` is refused (memory volumes take a size only).

**Run for real** in CI on kind (with Calico for NetworkPolicies): hello-stack, segmented and
shared-files, tested with `isoloom test kubernetes`.

## helm

Not a target of its own: `isoloom run kubernetes` writes the chart beside the manifests, and the
[spec reference](spec.md) lists its column apart because a chart can lag behind the manifests.

**Builds** `.isoloom/kubernetes/chart/`: `Chart.yaml`, the templates, `values.yaml` (each
machine's image, and the launch-time `inputs`, stored as a Secret), its `values.schema.json`,
and the checks as `helm test` pods. `isoloom chart --version 1.2.0` writes it and packages it
with `helm package` as the artifact to hand over or push to a chart registry.

**The host needs** Helm 3 and what [`kubernetes`](#kubernetes) needs. Images built from the
project must be pushed where the cluster pulls from, and named in `values.yaml`.

**Install, test, uninstall,** by hand:

```sh
helm install env .isoloom/kubernetes/chart -n env --create-namespace
helm test env -n env
helm uninstall env -n env
```

**Limits:** those of [`kubernetes`](#kubernetes).

**Run for real** in CI on kind: `helm lint`, `helm install` and `helm test` on hello-stack.

## kustomize

Not a target of its own either: `.isoloom/kubernetes/kustomization.yaml` is what
`isoloom run kubernetes` applies. It lists the manifests (`environment.yaml`) and turns the
project's scripts (init jobs, checks) into a ConfigMap, read from the project at apply time.

**The host needs** `kubectl` (its built-in Kustomize). The overlay reads files above its own
folder, so it needs `--load-restrictor LoadRestrictionsNone`:

```sh
kubectl kustomize --load-restrictor LoadRestrictionsNone .isoloom/kubernetes | kubectl apply -f -
```

Use it as a base of your own overlay to patch what Isoloom doesn't set (an ingress, a storage
class, node selectors). **Limits:** those of [`kubernetes`](#kubernetes).

## vagrant

**Builds** `.isoloom/vagrant/Vagrantfile`: one VM per machine, a private network per network, a
router VM that enforces `reach` and `tc`, and a controller VM when the spec has `provision:`
playbooks. One Vagrantfile carries a block for every provider below.

**The host needs** Vagrant and at least one usable provider:

| Provider | Found by | Notes |
|---|---|---|
| VirtualBox | `VBoxManage --version` | The one run for real. |
| VMware (`vmware_desktop`) | `vmrun`, or the `vagrant-vmware-desktop` plugin | |
| Parallels | `prlctl --version` | |
| libvirt | the `vagrant-libvirt` plugin and `virsh`; on Linux a usable `/dev/kvm` | Boots a machine's own `vm.boot` (kernel, UEFI loader, network boot). |
| UTM | the `vagrant_utm` plugin | No private networks yet. |
| QEMU | the `vagrant-qemu` plugin | One network per VM. |
| ESXi (`vmware_esxi`) | the `vagrant-vmware-esxi` plugin | Reads `ESXI_HOSTNAME`, `ESXI_USERNAME`, `ESXI_PASSWORD`, `ESXI_DATASTORE`, `ESXI_VIRTUAL_NETWORK`. See [the `esxi` target or Vagrant's ESXi provider?](esxi.md#the-esxi-target-or-vagrants-esxi-provider) |
| Hyper-V | Windows only: the `vmms` service running, and an elevated shell or a member of Hyper-V Administrators | See below. |

Pick one with `vagrant.provider` in the defaults (`isoloom defaults` shows where it comes
from: `~/.isoloom/defaults.yml`, `isoloom.defaults.yml`, `ISOLOOM_VAGRANT__PROVIDER`, or
`-s defaults.vagrant.provider=libvirt`); `run` then passes `--provider`. Without it, Vagrant
chooses.

**Run, test, down:** `vagrant up`, then `vagrant provision --provision-with checks`, then
`vagrant destroy -f`, in `.isoloom/vagrant`. `isoloom provision web --target vagrant` runs a
machine's set-up again in place.

**Hyper-V.** Run Vagrant from an elevated shell. Each VM's first adapter is on
`ISOLOOM_HYPERV_SWITCH` (default: the Default Switch, which Windows Server doesn't have); each
network becomes a private switch `isoloom-<env>-<network>`, and each machine's fixed address
there is set inside the guest. Published ports aren't forwarded to the host's loopback (Vagrant
can't): `vagrant up` warns with the VM's address to use instead. OS names without a Hyper-V box
(debian-13, ubuntu-14.04, fedora-42) need `vm.image.hyperv`.

**Limits.** Refused on every provider: GPUs, `devices`, a network's `mtu` or `external`, a
VLAN on a bridged network, `ipv6.addresses: auto`, `ipv6.only`, and a machine's `rate_limit`.
Per provider, when it is the one picked: QEMU gives a VM one network and no bridge; UTM no
private network; Hyper-V and libvirt no bridged network; only VirtualBox, libvirt and Hyper-V
give a TPM; only libvirt boots a machine's own `vm.boot` (QEMU takes its `firmware_format`).

**Storage.** A data disk is an empty disk (`config.vm.disk` on VirtualBox, VMware and Hyper-V,
`storage` on libvirt, `guest_storage` on ESXi), formatted and mounted by the machine's set-up;
QEMU, Parallels and UTM refuse it. `persistent` and `external` volumes are refused: a VM's
storage goes with it.

**Run by hand** on VirtualBox (several examples, and Windows Server 2019). The other providers
are **not run on a real host**. The weekly VirtualBox and libvirt workflow starts once its
runners exist ([Real targets in CI](real-targets.md)).

## docker-vm

**Builds** the `docker` Compose file and one Debian VM to run it on, sized for every container
at once:

- `.isoloom/docker-vm/Vagrantfile`, on this machine or on an ESXi host through
  vagrant-vmware-esxi;
- `.isoloom/docker-vm/proxmox/main.tf`, the same VM on a Proxmox server.

The VM installs Docker, receives the project and the [secrets](secrets.md), then runs
`docker compose up --wait`.

**The host needs** what [`vagrant`](#vagrant) needs.

**Run, test, down:** `vagrant up`, `vagrant provision --provision-with checks` and
`vagrant destroy -f` in `.isoloom/docker-vm`. `isoloom run` always uses the Vagrantfile; the
Proxmox module is run by hand with Terraform (its header lists its variables).

**Limits:** what the Compose file can do, except host GPUs and `devices` (the containers run on
a VM Isoloom creates) and `external` networks. Also refused: `bridged`, `mtu`, `rate_limit`,
`nested_virtualization` and `provision:`.
Data disks are named volumes on the VM (`raw` refused); `persistent` and `external` volumes are
refused, since the storage goes with the VM.

**Not run on a real host.**

## hybrid

**Builds** `.isoloom/hybrid/Vagrantfile` and a Compose file: each machine as a container when it
can be one, else as a VM (a Windows machine, say), all on the same networks, with the
containers on a Docker host VM (`isoloom-docker`). The target exists only for a spec that mixes
the two.

**The host needs** Vagrant with VirtualBox (the Docker host's interfaces are set promiscuous,
for VirtualBox only for now).

**Run, test, down:** as [`vagrant`](#vagrant), in `.isoloom/hybrid`.

**Limits.** Refused: network appliances, networks with a `gateway` machine, IPv6 networks, a
volume shared by containers and VMs, a machine's own `vm.boot`, GPUs, `devices`, `bridged` and
`external` networks, `mtu` and `rate_limit`.
Data disks are empty disks on its VMs and named volumes on its containers (`raw` refused).

**Not run on a real host.**

## proxmox

**Builds** `.isoloom/proxmox/main.tf` (the bpg/proxmox provider) and the check runners: an SDN
zone with a VNet per network (VLANs as tags, a network's `mtu` and a machine's `rate_limit` on
each device), a router VM that enforces `reach`, forwards the published ports and applies `tc`,
the firewall, one VM per machine (arm64 VMs emulated), and everything in a pool. A machine with
only `docker:` and `docker.lxc: true` runs as an LXC container (Proxmox VE 9.1 or later).

**The host needs** Terraform, `PROXMOX_VE_ENDPOINT`, and `PROXMOX_VE_API_TOKEN` (or
`PROXMOX_VE_USERNAME` and `PROXMOX_VE_PASSWORD`): that is what `targets --host` checks. The
module itself reads Terraform variables, so set those too:

```sh
export PROXMOX_VE_ENDPOINT=https://pve.lab:8006/ PROXMOX_VE_API_TOKEN='isoloom@pve!ci=...'
export TF_VAR_proxmox_endpoint=$PROXMOX_VE_ENDPOINT TF_VAR_proxmox_api_token=$PROXMOX_VE_API_TOKEN
export TF_VAR_proxmox_ssh_private_key_file=~/.ssh/pve TF_VAR_ssh_public_key="$(cat ~/.ssh/id_ed25519.pub)"
```

Optional variables: `node` (default `pve`), `datastore` (`local-lvm`), `image_datastore`
(`local`), `slot` (unique per environment on the cluster) and `proxmox_insecure`.

**Run, test, down:** `terraform init && terraform apply -auto-approve` in `.isoloom/proxmox`;
`test` pipes each check runner over SSH through the router (`isoloom@<router>`); `down` runs
`terraform destroy`. `isoloom connect web` goes through the router too.

**Limits.** Linux machines need an OS with a cloud image (Debian, Ubuntu, Rocky, AlmaLinux,
CentOS 7); a Windows machine needs a template VM of yours (`vm.image.proxmox`). Refused:
networks with a `gateway` machine, `ipv6.addresses: auto`, `ipv6.only`, a machine's own
kernel, loader or network boot (Proxmox boots the disk through its own firmware), `devices`,
and `gpus` as a count (list them in `gpus.ids`). An LXC container takes no `files` or `secrets`.
A data disk is a `disk` block on the VM, set up by cloud-init; an LXC container refuses data
disks. `persistent` and `external` volumes are refused.

**Not run on a real host.** The weekly workflow (hello-stack, segmented) starts once a runner
that reaches a cluster exists ([Real targets in CI](real-targets.md#proxmox)).

## esxi

**Builds** `.isoloom/esxi/main.tf` (the josenk/esxi provider) for a standalone ESXi host with no
vCenter: a vSwitch and port group per network, a router VM, and one VM per Linux machine from an
OVA, set up over SSH through the router. [The `esxi` target](esxi.md) has its variables,
outputs and how it compares with Vagrant's ESXi provider.

**The host needs** Terraform, VMware's `ovftool`, `TF_VAR_esxi_hostname` and
`TF_VAR_esxi_password`, plus `TF_VAR_ssh_public_key` and `TF_VAR_ssh_private_key_file`. SSH
must be on on the ESXi host.

**Run, test, down:** Terraform in `.isoloom/esxi`, the checks through the router, as on
`proxmox`.

**Limits.** Refused: Windows machines, arm64, IPv6, networks with a `gateway` machine,
`secrets`, `external` and `bridged` networks, a machine's own image or boot, a TPM, GPUs,
nested virtualization, `mtu`, `rate_limit`, and more than nine networks. Data disks are
refused too. A Linux OS other than
Ubuntu needs an OVA of yours.

**Not run on a real host.**

## cloud-vm

**Builds** one Terraform module per cloud that accepts the environment, under
`.isoloom/cloud-vm/<cloud>/`: AWS, Azure, Google Cloud, DigitalOcean, Linode and Oracle Cloud.
Each creates the networks and firewall rules from `reach`, one VM per machine, a controller for
`provision:` playbooks, and an auto-stop. A cloud that refuses the environment gets no module;
`isoloom targets` lists it with its reason. The target is refused only when every cloud refuses
it. [Cloud modules](cloud-modules.md#cloud-docker-and-cloud-vm) has the variables, outputs and
tags.

**The host needs** Terraform and the chosen cloud's credentials:

| Cloud | Credentials found in |
|---|---|
| `aws` | `AWS_ACCESS_KEY_ID`, `AWS_PROFILE`, `~/.aws/credentials` or `~/.aws/config` (`aws login`, SSO) |
| `azure` | `ARM_CLIENT_ID`, or a signed-in Azure CLI (`az account show`) |
| `gcp` | `GOOGLE_APPLICATION_CREDENTIALS`, or gcloud's application default credentials |
| `digitalocean` | `DIGITALOCEAN_TOKEN` |
| `linode` | `LINODE_TOKEN` |
| `oci` | `~/.oci/config`, or `OCI_CLI_USER` |

Every module also needs `TF_VAR_allowed_cidr` (who may reach the VMs), `TF_VAR_ssh_public_key`
and `TF_VAR_ssh_private_key_file`; Azure a subscription, Google Cloud a project (or a billing
account) and Oracle Cloud a compartment.

**Which cloud.** `run`, `test`, `down`, `exec`, `connect` and `capture` work on one cloud
([ADR 0007](adr/0007-one-cloud-vm-target-many-clouds.md)):

1. `--cloud` when given;
2. else the cloud `run` recorded for the environment;
3. else AWS, when it accepts the environment;
4. else the only cloud that does.

When AWS refuses and several clouds accept, the command stops and names them
(`--cloud gcp, --cloud azure`). `run` on a cloud that refuses stops with that cloud's reason.

**Run, test, down:** `terraform init && terraform apply` in the cloud's module; `test` reads the
module's `checks` output and runs each runner over SSH; `down` runs `terraform destroy`.

**Limits.** Refused by every cloud: `tc`, a machine's own `vm.boot`, `devices`, `bridged` and
`external` networks, `mtu`, `rate_limit`, arm64, `ipv6.only`, and `gpus.ids` (give a count). `dns.private` uses the cloud's own private zone (Route 53, Azure
private DNS, Cloud DNS, an Oracle Cloud private view), or the hosts file where the cloud has
none. Then each cloud:

- **AWS:** a network with a `gateway` machine routes through it, though the network's
  machines still reach the internet at their own public addresses. AWS assigns a VPC's IPv6
  block itself, so the spec's can't be kept, and keeps a subnet's first addresses (.1 to .3).
- **Azure:** no networks with a `gateway` machine and no nested virtualization yet; IPv6
  addresses are the derived ones (`ipv6.addresses: auto` is refused).
- **Google Cloud:** no networks with a `gateway` machine (a route can't go between subnetworks
  of one VPC through a machine), no Windows yet, and the VPC's IPv6 block is Google's.
- **Oracle Cloud:** several networks with a VNIC on each, `gateway` machines as on AWS;
  dual-stack networks keep the spec's IPv6 block as a private prefix of the VCN, and with
  `ipv6.addresses: auto` Oracle Cloud picks each VNIC's address. It keeps a prefix's first four
  addresses (`::0` to `::3`). Linux machines only, no Ansible controller and no GPUs yet.
- **Linode:** several networks, but a machine on one of them only (a Linode has one VPC
  interface); Linux machines only, no Ansible controller, no TPM, no nested virtualization.
- **DigitalOcean:** a single Linux machine on a single network (a droplet's private address is
  DigitalOcean's pick); no TPM and no nested virtualization.

**Storage.** A data disk is a block volume attached to the VM ([each cloud's
resource](cloud-modules.md#data-disks-disks)); `persistent` and `external` volumes are refused.

**Run by hand** on AWS (one VM per machine, with Ansible provisioning). The other clouds are
**not run on a real host**.

## cloud-docker

**Builds** the `docker` Compose file and, for each of the six clouds, a module under
`.isoloom/cloud-docker/<cloud>/` with one VM that installs Docker, receives the project and the
[secrets](secrets.md), and runs `docker compose up --wait`. [Cloud
modules](cloud-modules.md#cloud-docker-and-cloud-vm) has its variables and outputs.

**The host needs** Terraform and the chosen cloud's credentials, as for
[`cloud-vm`](#cloud-vm), and the same `TF_VAR_*` variables. The VM's size is a variable per
cloud (`instance_type`, `size`, `machine_type`, `type`).

**Which cloud.** `--cloud` is required (`isoloom run cloud-docker --cloud azure`): there is no
automatic pick.

**Run, test, down:** Terraform in the cloud's module; `test` runs the check profile on the VM
over SSH (`docker compose --profile check run`); `down` runs `terraform destroy`.

**Limits:** what the Compose file can do, except host GPUs, `devices` and `external` networks.
Also refused: arm64, `bridged`, `nested_virtualization`, `provision:`, `mtu` and `rate_limit`.
Data disks are named volumes on the VM (`raw` refused); `persistent` and `external` volumes are
refused.

**Run by hand** on AWS and Azure. The weekly workflow runs solo-web on all six clouds once
their secrets are provided ([Real targets in CI](real-targets.md#clouds)); Google Cloud,
DigitalOcean, Linode and Oracle Cloud are **not run on a real host** yet.

## cloud-services

**Builds** the scripts that apply a Terraform module of your project into your own cloud
account, under `.isoloom/cloud-services/`: `up.sh`, `down.sh`, `outputs.sh` and
`terraform.tfvars.json`. The spec has `cloud:` instead of machines (IAM users and roles,
functions, buckets, queues). [Cloud modules](cloud-modules.md#cloud-services) has the details.

**The host needs** Terraform, the credentials of the spec's `cloud.provider`, and every entry of
`cloud.requires`: a command name looked up on the PATH, or a shell command that must succeed
(run by `isoloom run` only, never by `doctor`).

**Run, test, down:**

- `run`: `sh up.sh`, which runs the spec's `cloud.before` steps, then `terraform apply` on a
  copy of the module, retried `cloud.apply_retries` times.
- `test`: runs the spec's checks from this machine, with each module output as
  `ISOLOOM_OUTPUT_<NAME>`.
- `down`: `sh down.sh`; on AWS with the `aws` CLI, it then lists resources left with the
  environment's tag.

**Limits.** Machine fields don't apply: no machines, networks or secrets.

**Run by hand** on AWS (cloud-bucket and thirteen AWS labs). Azure and Google Cloud are **not
run on a real host**.

## external

**Builds** under `.isoloom/external/`: an Ansible inventory, a check runner per machine and
`machines.json`. Isoloom creates nothing: each machine has `external: { address, user, port,
key }`, an SSH endpoint that already exists.

**The host needs** OpenSSH (`ssh`).

**Run, test, down:**

- `run`: delivers the [secrets](secrets.md), then over SSH copies the project to
  `/opt/isoloom`, runs each machine's `.sh` steps and its `.yml` playbooks from this machine,
  then the `provision:` playbooks. A Windows host skips its own steps.
- `test`: pipes each machine's check runner over SSH.
- `down`: forgets the environment; the hosts stay as they are.

**Limits.** Refused: a machine's own process (`command`, `working_dir`, `before`,
`healthcheck`), `files` (place them with a `provision:` playbook), `ipv6.only`, `mtu` and
`rate_limit`. `reach` is checked, not enforced: the hosts' own firewall decides.
Data disks are refused: Isoloom creates no machine to attach one to.

**Not run on a real host.**
