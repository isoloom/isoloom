# Changelog

## Unreleased

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
