# Security considerations

What Isoloom does to keep a generated environment as isolated as its spec says, and what you
decide when you run it. To report a vulnerability: [SECURITY.md](../../.github/SECURITY.md).

## Trust model

- **A spec is code.** Its steps run as root in the machines; `cloud.before` steps and a
  `cloud-services` module run on the computer that applies them; a playbook step runs whatever it
  holds. Review a spec, and the scripts it names, as you would a script.
- **`isoloom` itself runs nothing** from the spec. It reads files and writes files, opens no
  network connection and starts no program, except `d2`/`dot` for `isoloom graph -o` and Ruby for
  `isoloom import vagrant`, which evaluates the Vagrantfile it imports (import only Vagrantfiles
  you trust).
- **The generated files and the run manifest are trusted input** for whatever runs them: they name
  the commands that run.

## Secrets never land in a file

A secret or input is declared by name and source (`env`, `file`, Vault, SOPS). Its value is never
in the spec, a generated file, `resolved.json` or the run manifest: the manifest holds a slot (a
name and a destination). A runner reads the value at run time and delivers it as the slot says,
usually as a file in the machine over SSH standard input, never on a command line. Vagrant hands
a value to a provisioner in its environment, which it writes into a shell string: every
Vagrantfile passes it in base64, which the provisioner's script decodes, so a `$(...)` or a
backquote in a value is read as text, never run. Cloud credentials reach only the Terraform
steps' environment.

Two exceptions are by design, and worth knowing:

- **`user` steps' `password` is a literal**, lab content written into the generated files:
  hashed (SHA-512 crypt) for Linux users, in clear in the PowerShell that creates a Windows user.
  A secret or input is refused there. For a real credential, use a catalogue entry's password
  setting with `{ secret: <name> }` (`openssh`'s `password`, `mysql`'s `user_password`), or a
  secret delivered as a file.
- **Terraform state holds what Terraform makes**: a generated Windows administrator password, the
  controller's SSH private key. Keep state where only the people who run the environment can read
  it ([remote state](../guide/remote-terraform-state.md)).

## What a machine gets of the project

A copy of the project into a machine (Vagrant's project step, Proxmox's cloud-init, ESXi's
archive, the runner's `project.archive`) leaves out what the project's `.isoloomignore` lists,
then, whatever that file says: Isoloom's outputs but those the machine reads (a Compose VM, the
`docker` target's folder and `.isoloom/configure/`), `.isoloomignore` itself (every top-level
`.isoloom*` entry is left out, and a machine has no use for the file), version control, and what the tools write at
any depth: Terraform's cache, state, lock and crash logs, Vagrant's machine folders (their private
keys) and the runner's logins (`.isoloom-reach/`). One rule,
`isoloom_manifest::ignore::copy_rules`, which `.isoloom/.gitignore` shares.

## Published ports

- **Compose**: published on `127.0.0.1` only, on a free port, unless you set
  `ISOLOOM_PUBLISH_ADDRESS`.
- **Vagrant**: forwarded ports bind to `127.0.0.1`.
- **Kubernetes**: a published port is a ClusterIP Service by default; exposing it outside the
  cluster is your cluster's decision ([Kubernetes](../guide/targets/kubernetes.md)).
- **Clouds**: reachable only from `allowed_cidr` (below).

## Graphical consoles

A VM's graphical console (its screen, [run manifest](../reference/run-manifest.md#graphical-consoles))
listens only on the loopback of the computer that runs Vagrant: libvirt's and QEMU's VNC, VMware's
VNC server and VirtualBox's remote display all bind `127.0.0.1`. VMware's takes a password the
runner makes for each run and keeps until `down` (VMware writes it in the VM's `.vmx`, as it does
every setting); libvirt's, QEMU's and VirtualBox's take none, so any program of a user of that
computer can reach them while the VM runs, as it can reach the forwarded ports. Proxmox's console
goes through its API, with the connection's own credentials and a ticket per session. A cloud
Windows VM's Remote Desktop is reached only through an SSH tunnel the runner opens from the
loopback, through a Linux machine of the run: Isoloom opens no port for it at the VM's public
address.

## Kubernetes

`reach` and `internet: false` become NetworkPolicies, which only a network plugin that enforces
them (egress included) applies: on a cluster without one they are silently ignored, and the
derived checks fail. The pods run under the Pod Security Standards' `baseline` level, which the
namespace enforces (`privileged` when a machine is privileged or sets a sysctl outside the safe
set); a cluster that admits only `restricted` pods refuses them. Every object is labelled `app.kubernetes.io/managed-by: isoloom`.

## Public IP addresses on cloud VMs

Every `cloud-vm` machine, and the `cloud-docker` host, has a public IP address: the runner reaches
the machines over SSH to deliver the project and secrets and to run the checks. Their security
groups or firewalls accept SSH (22), WinRM (5985, Windows) and the published ports only from
`allowed_cidr`, which has no default: set it to your address or your egress range. Nothing is
open to `0.0.0.0/0` inbound but one ICMP message on Oracle Cloud: "fragmentation needed" (and
ICMPv6's "packet too big"), which path MTU discovery needs. Where policy forbids public addresses, run the environment on
Proxmox, ESXi or Vagrant, or apply the module from inside the cloud network with your own changes
on top.

## The egress seal

An `internet: false` network is a guarantee, not a convention. On Compose, the containers have no
route out and a privileged container on such a network is refused unless it accepts a best-effort
seal. On the clouds and the servers (Proxmox, ESXi), machines need the internet while they set
themselves up, so the runner seals the network once they are ready, where root in a machine can't
undo it: at the cloud's own level (a second apply with `isoloom_sealed`), or at a server's router
(its sealed ruleset applied over SSH). Egress is then limited to the environment's networks. The
seal fails closed, and every later apply keeps it. [ADR 0004](adr/0004-egress-seal.md) explains
the design and its one residual channel (the cloud's DNS resolver).

## Expiry

A cloud run has an expiry (240 minutes by default, set by the runner): every resource is tagged
with it, the machines shut themselves down at that time, and a reaper (`isothrum reap`) destroys
the runs past it. A module applied by hand has no expiry unless you set `expires_at`.

## Proxmox TLS

The Proxmox module's `proxmox_insecure` defaults to `false`: the API's certificate is verified.
Setting it to `true` accepts a self-signed certificate, and relaxes verification of the Proxmox
API endpoint only; prefer giving the server a certificate your machines trust. Use an API token with a role
limited to what the module creates ([Proxmox](../guide/targets/proxmox.md#credentials)).

## WinRM

Windows machines are set up over WinRM. Its default is plain HTTP on port 5985 with basic
authentication, as Vagrant's Windows boxes and cloudbase-init images expect; on AWS and Azure
`cloud-vm` it is HTTP with NTLM, so the password never crosses the network (Basic stays off).
Terraform's own WinRM client doesn't encrypt NTLM messages, so what it sends over HTTP is
readable on the path; the controller's Ansible encrypts its own. It is reached:

- on Vagrant, on the host's private network;
- on AWS and Azure `cloud-vm`, from `allowed_cidr` only;
- on Proxmox and ESXi, cloudbase-init opens 5985 at the Windows firewall, on the environment's
  networks.

On `external`, Windows hosts are reached over SSH instead (OpenSSH on Windows, its commands run by
PowerShell), with the keys you name: Isoloom opens no WinRM port there.

An image whose box answers WinRM over HTTPS takes `winrm: ssl` in the image table (NTLM, the
certificate not verified), or `winssh` for OpenSSH ([golden images](../guide/golden-images.md)).
Vagrant's Windows boxes sign in with the well-known `vagrant`/`vagrant` account: keep those VMs
off networks other people reach.

## Supply chain

- Pinned by digest: the catalogue's container images, the containers' OS base images (`FROM
  debian:12@sha256:…`) and Isoloom's helper images (the check runner, the probe, the utility
  image, the `shell` toolbox and the `tc` sidecar); Proxmox's cloud images by SHA-512, ESXi's
  default OVAs by a SHA-256 the module's `images` description gives you to check (the provider
  checks none). Taken as their source publishes them, not pinned: Vagrant boxes (`box_version`
  pins the Windows boxes; the Linux boxes take the latest), the clouds' VM images (each cloud's
  newest image of the OS: an AMI found by name, Azure's `latest`, a Google Cloud image family, a
  DigitalOcean or Linode image slug, Oracle Cloud's newest Canonical image) and an OVA you give. Ansible
  Galaxy collections are pinned by version and `sha256` in the catalogue (on `external`,
  `requirements.yml` names the pin for you to check: `ansible-galaxy` doesn't); the Docker install
  in VMs uses the official repository with a pinned key.
- Terraform providers are pinned to a patch range ([tool versions](../reference/tool-versions.md)).
- `constraints.registries` and `constraints.mirrors` send every pull through your own mirrors
  ([corporate network](../guide/corporate-network.md)).
- The release binaries come with checksums, a Sigstore signature, build provenance and an SBOM
  ([verify a release](../guide/verify-a-release.md)). The crates forbid `unsafe` code, and
  `cargo deny` checks advisories and licences in CI.
