# Docs facts (for the Phase 7 rewrite)

Every phase appends one line per user-visible fact it changed. The facts kept from before the rebuild follow.

# Facts to get right in the final docs rewrite

## From the website sync
- `docs/comparisons/terraform.md:25` and `pulumi.md:21` say "runs on" Proxmox, ESXi and six clouds. Isoloom generates for them and validates them; the real runs are Docker, VirtualBox, QEMU and kind.
- `docs/comparisons/vagrant.md:44` says VirtualBox is the only provider run for real. QEMU also ran by hand (#187, #199).

## From the docs-truth unit (PR #236)
- **cloud-vm limits in `targets.md`:**
  - It says every cloud refuses `mtu`, but Google Cloud sets it (#236).
  - Every cloud refuses Ansible `.yml` checks and a Windows machine on several networks; neither is listed.
  - The Azure and Google Cloud bullets still say there are no gateways; #220 added them.
  - Linode and DigitalOcean also refuse IPv6 and gateways.
- **AWS and Azure hand runs:** they predate 1.0; the v1 rewrites (#171, #174) applied nothing.
- **VirtualBox:**
  - The TPM run (#194) was on a Debian guest, a copy of tuned-web, not on Windows.
  - Windows Server 2019 ran only with the pre-1.0 generator.
  - The NFS run was redone on v1 (#172).
- **QEMU:** the provider table (~line 200) calls VirtualBox the only provider run for real; QEMU ran in #187 and #199.
- **docker-vm:** it is marked "not run on a real host", but #200 ran it on VirtualBox.

## Real runs recorded on 2026-10-14
- **Docker:** hello-stack, segmented, tuned-web, shared-files, dual-stack and private-names; 32 checks passed.
- **Persistent volumes:** a volume survived `down`/`run`, and `down --volumes` removed it.
- **Owned project files (#221):** 101:101 mode 0400 on docker and hosted.
- **Launcher labs:** pass on Isoloom main.
