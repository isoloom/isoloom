# 0006: How a coverage row is classified

## Status

Accepted. Extends [0005](0005-coverage-methodology.md), which says what is measured; this one
says how each row of a table gets its class.

## Context

ADR 0005 defines seven classes (`Emitted`, `Partial`, `Equivalent`, `InImage`, `Planned`,
`NotPortable`, `Tooling`) and the number they produce. The first tables used them loosely:
`configs` and `command` were "in the image" although Isoloom can write them, `secrets` was
"the same as inputs" although inputs are environment values, the healthcheck was "emitted"
although only a port probe is, and GPUs were "not portable" because Compose lacks them on some
hosts. Each loose call moved the number without any support being added, which is what 0005
exists to prevent.

## Decision

Classify a row by asking these questions in order; the first "yes" decides.

1. **Does Isoloom write it?** `Emitted` when the key appears in some generated output (a
   committed example or a fixture of the written-features test) for every use the format has.
   `Partial` when it appears but covers only some uses: name the gap. A key Isoloom writes only
   for its own containers (sidecars, check runners, tools) is `Emitted` with a parenthesized
   origin when the machine has no use for it, and `Partial` when a machine would have one too.
2. **Does another key Isoloom writes give the same effect?** `Equivalent`, naming that key.
   Only for the same effect on the machine: the short form of a long-form key, a limit written
   in another place. Not for a different mechanism that serves a similar purpose.
3. **Does it belong to the software inside the machine?** `InImage`: the user, signals and
   groups an image's own program uses. Never for a machine setting Isoloom could write itself
   (a file, a command, a working directory, a resolver option): those are `Planned`.
4. **Can every kind of target produce it, at least where the hardware exists?** `Planned`.
   Hardware passthrough (GPUs, devices) is portable: a target with the hardware passes it, a
   target without it refuses the environment, as Windows machines do on containers. A feature
   isn't unportable because one Compose host or one cloud lacks it.
5. **Do some VM targets have no way to produce it?** `NotPortable`, naming them and why (cloud
   VMs choose a size, not CPU shares; Windows containers only; Docker's own drivers).
6. **Does it change nothing about how the environment behaves?** `Tooling`: build and pull
   mechanics, Swarm scheduling across hosts, labels for tools, container mechanics a VM has no
   counterpart for.

A key whose value is a whole service definition (Compose `jobs`, `pre_start` hooks) is one row:
its keys are the service rows again. Every other nested key is a row of its own, read from the
pinned source, so a new upstream key is a failing test until it is classified.

## Re-decided rows (Compose)

| Row | Was | Now | Why |
| --- | --- | --- | --- |
| `configs`, `services.*.configs` | In image | Planned | Isoloom can place a file on every target: machine files. |
| `services.*.command` | In image | Partial | Isoloom writes it for its tools; a machine's own command is the gap. |
| `services.*.working_dir` | In image | Planned | Comes with a machine's own command. |
| `services.*.dns_opt` | In image | Planned | A resolver setting beside `dns`, which Isoloom already writes. |
| `secrets`, `services.*.secrets` | Equivalent (inputs) | Planned | Inputs are environment values; secrets are files from a store (#147). |
| `services.*.healthcheck` | Emitted | Partial | Only a port probe; a custom check command is the gap. |
| `services.*.gpus` | Not portable | Planned | Passed through where the target has a GPU, refused elsewhere. |
| `services.*.devices` | Not portable | Planned | Passed through where the target has the device, refused elsewhere. |
| `models`, `services.*.models` | Tooling | Not portable | They change what the machine can call, but only Docker's model runner serves them. |

## Re-decided rows (Kubernetes and the Vagrant providers)

| Row | Was | Now | Why |
| --- | --- | --- | --- |
| Kubernetes `PodSpec: hostAliases` | Planned | Not portable | It points names at fixed addresses, and a pod has none. The spec's `aliases` name a machine, which Kubernetes reaches only by its Service's name (no dots), so Kubernetes refuses `aliases` instead. |
| Kubernetes `PodSpec: setHostnameAsFQDN` | Planned | Equivalent | `subdomain` puts the pod's full name in its hosts file (`hostname -f`); its hostname stays the short name, as on every other target. |
| QEMU `mcast_addr`, `tap_device`, `socket_vmnet_*` | Planned | Equivalent | A hub (a QEMU process with no machine) links any number of VMs without root, on a Mac too: multicast doesn't send there, a TAP device and the socket_vmnet daemon need root. |
| QEMU `extra_netdev_args` | Planned | Tooling | Options of the NAT adapter Vagrant reaches the VM through, not of the lab's networks. |
| QEMU `vmnet_interface` | Planned | Planned | Bridged to the host's LAN: it comes with `public_network`. |
| ESXi `numvcpus`, `memsize`, `vmname`, `virtual_network`, `*name_prefix` | Planned | Equivalent | The plugin's older names for what Isoloom writes; the prefix only names a VM Isoloom leaves unnamed, and it names them all. |
| UTM `private_network` | Planned | Not portable | vagrant_utm 0.1.6 sets up no private network (it only forwards ports): `refusal_on` with `utm` refuses a spec whose VMs are on one. |

## Terraform: families of resource types and arguments

A provider has hundreds of resource types and each type dozens of arguments, so a Terraform
table classifies by family: a rule names a type or argument exactly, by prefix (`rds_*`, which
also names `rds`), as a block and everything inside it (`launch_template.*`) or by suffix
(`*_custom_endpoint`). The exact name wins, then the longest pattern. A family is a reviewed
decision under the questions above, never a default: a type or argument no rule names is
unclassified and fails the tests, and a rule naming nothing in the pinned schema fails them too.
The utility providers (random, tls, local, time, null, cloudinit, http, Terraform's built-in) are
the one exception: they act on Terraform's side only, so what Isoloom doesn't write of them is
`Tooling` as a whole.

| Family | Class | Why |
| --- | --- | --- |
| What one VM per machine on its own networks needs (VMs, networks, routes, NAT, addresses, rules, keys, disks) | Planned | Every VM target builds these. |
| Custom cloud images (`aws_ami`, `azurerm_image`, `google_compute_image`, `oci_core_image`...) | Planned | A machine's own image, as on the local targets. |
| Private DNS (`aws_route53_zone`, `azurerm_private_dns_*`, `google_dns_managed_zone`, `oci_dns_*`...) | Planned | The environment's names served by the platform: local targets serve them too. |
| Rule resources whose effect the written inline rules give (`aws_security_group_rule`, `azurerm_network_security_rule`...) | Equivalent | The same rules on the same interfaces. |
| Managed services, the cloud's network services (VPN, peering, load balancers, CDN), public DNS, agents run in the VMs | Not portable | No other VM target has them. |
| The account (identities, billing, quotas, auditing, monitoring), placement and billing of VMs (templates, scaling groups, reservations, spot), backups, image pipelines, the default network | Tooling | No change in how the environment behaves. |
| Arguments: IPv6, data disks, nested virtualization, device and GPU passthrough, TPM, VLANs, MTU, several addresses, egress and ICMP rules, routing through a machine | Planned | Features every VM target can have. |
| Arguments: names, tags, timeouts, locations, credentials, how Terraform creates or deletes, another spelling of what Isoloom sets | Tooling | No change in how the environment behaves. |
| Arguments: a cloud's agents, metadata service, encryption keys, performance tiers; Proxmox's hardware tuning | Not portable | Cloud VMs or local targets have no counterpart. |

Re-decided rows: custom cloud images and private DNS were swept into "not portable" with every
managed service; they are Planned. Azure `azurerm_windows_virtual_machine`, Google Cloud
`google_project` and `google_project_service`, and Linode `linode_vpc` and `linode_vpc_subnet`
are written and now checked against every module that writes them.

## The spec table's cells

The spec table runs the other way: a row per field of an Isoloom spec, a column per output (each
cloud of `cloud-vm` its own). A cell is classified the same way, with its own classes:

1. **Does the output's backend refuse a spec using the field?** Refused, with the backend's own
   reason. This is never written by hand. Each field has a probe spec (the smallest spec that
   uses it), and every backend is asked to render it. So a column follows the backend's
   capabilities and limits, and a refusal lifted in a backend shows up without editing a table.
2. **Does the output produce it?** Done, or Partial with the gap named. The claim names an
   example that uses the field and generates for the output. When no example does, it names the
   field's probe, which must then render. A claim is made once per rendering (the Compose file,
   the cloud VM core) and inherited by every output that shares it. An inherited claim still has
   to generate on that output, through its probe or an example.
3. **Could it, and doesn't yet?** Planned.
4. **Does it mean nothing for the output** (a `vm:` field in a Compose file, a VM's size on an
   existing host)? Not applicable. A field Isoloom reads itself is "core", and a field only
   people and runners read is "info".

A refusal outranks a claim: a backend that refuses what its own rendering claims fails the
tests.

## Consequences

- The Compose table lists nested keys (`deploy.*`, `healthcheck.*`, `build.*`, long-form
  `ports` and `volumes`, `services.*.networks.*`, `ipam.config[]`, the items of `configs`,
  `secrets` and `models`), and its number drops to what Isoloom really writes.
- Several rows become features to build (files, a machine's command, secrets, GPUs, devices,
  a custom healthcheck), tracked as `Planned` until they land.
- The other tables follow the same questions when they are rebuilt.
- The spec table's Proxmox column lost its stale "planned" rows (`tc`, `aliases`, `dns.servers`,
  `dns.domain` and `access` are generated; `gateway` is refused by the backend).
