# Existing hosts: `external`

The `external` target creates nothing. It takes machines that already exist (servers, VMs built by
hand) and writes an Ansible inventory and playbook that set each one up as its spec says, plus
check scripts that prove the environment behaves as the spec says from each machine.

Tier 2. There is no run manifest: run the playbook with Ansible.

## Prerequisites

- Ansible (`ansible-playbook`) and an OpenSSH client on the machine that runs it.
- SSH access to every machine, with a user that can `sudo` (the playbook runs with `become`).
  Windows hosts are reached over WinRM (HTTP on port 5985, basic authentication: see
  [security considerations](../../explanation/security.md#winrm)).
- The machines on the networks and at the addresses the spec gives them: Isoloom doesn't change
  your switches or firewalls; the derived checks tell you whether they match the spec's `reach`.

## Point the spec at the hosts

Each machine needs its address under `overrides.external`:

```yaml
overrides:
  external:
    machines:
      web: { ssh: { address: 192.168.1.20, user: deploy, key: ~/.ssh/id_ed25519 } }
```

The [`existing-hosts`](../../reference/examples.md) example is a complete one. A machine with
no host is refused (`isoloom targets` names it).

## Generate and run

```sh
isoloom generate --target external
ansible-galaxy collection install -r .isoloom/external/requirements.yml   # when it is written
ansible-playbook -i .isoloom/external/inventory.ini .isoloom/external/site.yml
```

`requirements.yml` is written when a role needs a collection (`ansible.posix`, for a `firewall`
step). It names the archive by URL, and above it, in a comment, the SHA-256 the catalogue pins.
`ansible-galaxy` doesn't check that hash: to check it, download the archive, compare
`sha256sum <archive>` with the pin, and install the file you checked
(`ansible-galaxy collection install <archive>`).

Then run the checks from each machine: `.isoloom/external/checks/<machine>.sh` prints a `PASS` or
`FAIL` line per check (copy it to the machine and run it there, or over `ssh`). The checks are
probes (`http`, `tcp`, `exec`), each run from one of the hosts: its `from:`, else the access
machine. The target refuses a check it can't run that way, a project script or an Ansible
playbook (an existing host has no copy of the project) or a check with no host to run from.

## Credentials

Your SSH keys, as the inventory names them; Isoloom stores none and writes no secret value into
the inventory or the playbook.

## Teardown and cost

Nothing to tear down: the hosts are yours. The playbook changes them as the spec says (packages,
users, files, services); undoing that is up to you.
