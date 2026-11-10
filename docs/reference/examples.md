# Examples

Thirty complete projects live under `examples/<name>/` in the repository: each is an
`isoloom.yml` with the files it names (build recipes, set-up scripts, checks). CI validates every
one, generates it twice to prove generation is deterministic, validates its Terraform, and runs a
selection for real on Compose and Kubernetes. Copy one to start your own:

```sh
cp -R isoloom-src/examples/segmented my-env && cd my-env && isoloom targets
```

The targets column is what `isoloom targets` gives each example (`cloud-vm` and `cloud-docker`
may still refuse some clouds).

## Getting started

| Example | Shows | Targets |
|---|---|---|
| `hello-stack` | A web server and a cache, as containers or VMs, with the same checks; no internet | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, proxmox, esxi, cloud-vm |
| `solo-web` | One Linux web server on one network: every cloud's `cloud-vm` takes it | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, proxmox, esxi, cloud-vm |
| `workbench` | A machine to work from (its OS alone) next to a web server | docker, hosted, docker-vm, cloud-docker, kubernetes |
| `configured-app` | An app written in the configuration model (steps rendered for VMs and image builds alike) | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, proxmox, esxi, cloud-vm |
| `supplier-portal-api` | A recipe per form for each machine; a launch token delivered to the database as a file | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, cloud-vm |

## Networks and isolation

| Example | Shows | Targets |
|---|---|---|
| `air-gapped` | A network with no internet and nothing published | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, proxmox, esxi, cloud-vm |
| `segmented` | Three networks and a router; `reach` rules allow some paths and block the rest | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, proxmox, esxi, cloud-vm |
| `dmz-jump` | A gateway in a DMZ, an app and a database behind it, a client that only sees the DMZ | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, proxmox, esxi, cloud-vm |
| `edge-firewall` | A firewall machine of the environment's own routes the DMZ and the LAN with its own rules | docker, hosted, docker-vm, cloud-docker, vagrant, cloud-vm |
| `dual-stack` | Two dual-stack networks and a router, IPv4 and IPv6 alike | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, proxmox, esxi, cloud-vm |
| `vlan-office` | One LAN split into two VLANs, each with its own reach | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, proxmox, esxi, cloud-vm |
| `several-addresses` | Several addresses for one machine on one network | vagrant, proxmox, esxi, cloud-vm |
| `slow-link` | Two sites over a link with delay, jitter, loss and a rate limit | docker, hosted, docker-vm, cloud-docker, vagrant, proxmox, esxi |
| `private-names` | Every machine's name in a private DNS zone (`corp.internal`) | docker, hosted, docker-vm, cloud-docker, vagrant, proxmox, esxi, cloud-vm |
| `udp-dns` | A DNS server answering over UDP and TCP, reachable on port 53 only | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, proxmox, esxi, cloud-vm |

## Machines and data

| Example | Shows | Targets |
|---|---|---|
| `shared-files` | A volume shared between two machines (a Docker volume, or NFS between VMs) | docker, hosted, docker-vm, cloud-docker, kubernetes, vagrant, proxmox, esxi, cloud-vm |
| `tuned-web` | Environment variables, a kernel parameter, process limits and VM hardware (a TPM) | vagrant, proxmox, cloud-vm |
| `arm-lab` | An ARM64 container (`hardware.arch: arm64`) | docker, hosted, docker-vm, kubernetes |
| `arm-vm` | An ARM64 VM, native on Apple Silicon or Graviton | vagrant, proxmox |
| `ansible-pair` | Two machines configured by one Ansible playbook from a controller | vagrant, proxmox, esxi, cloud-vm |

## Windows

| Example | Shows | Targets |
|---|---|---|
| `windows-hello` | A Windows server with IIS, set up in PowerShell | vagrant, proxmox, esxi, cloud-vm |
| `configured-windows` | A Windows server written in the configuration model | vagrant, proxmox, esxi, cloud-vm |
| `corp-ad-basics` | A small Active Directory domain | hybrid, vagrant, proxmox, esxi |
| `mixed-office` | A Windows file server (a VM) next to a web app (a container) | hybrid, vagrant, proxmox, cloud-vm |

## Network appliances

These need Cisco images you bring yourself (from Cisco Modeling Labs, for one): the examples say where each goes.

| Example | Shows | Targets |
|---|---|---|
| `cisco-dynamips` | Two Cisco 7200 routers in Dynamips, OSPF between two sites | docker, hosted, docker-vm, cloud-docker |
| `cisco-iol` | Two sites behind Cisco IOL routers over a WAN | docker, hosted, docker-vm, cloud-docker |
| `cisco-qemu` | Cisco IOSv and CSR1000v in QEMU, packaged by vrnetlab | docker, hosted, docker-vm, cloud-docker |
| `cisco-switch` | Two VLANs switched by a Cisco IOL-L2 switch | docker, hosted, docker-vm, cloud-docker |

## Beyond machines

| Example | Shows | Targets |
|---|---|---|
| `existing-hosts` | Machines that already exist, set up and checked over SSH | vagrant, proxmox, esxi, cloud-vm, external |
| `cloud-bucket` | Cloud services instead of machines: your Terraform module, applied into your AWS account | cloud-services |
