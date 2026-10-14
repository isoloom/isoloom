# Secrets

A secret is a value a machine needs but that must not be written down: a database password, an
API key, a private certificate. The spec says **where each value lives** on the runner and
**which machines get it**, as files. `isoloom run` reads the values and hands them to each
target's own mechanism, or writes them over the machine's own connection. No value is ever in a
generated file, the resolved snapshot, Terraform's state or a log.

```yaml
# isoloom.yml
version: 1
name: shop

networks:
  lan: { cidr: 10.20.0.0/24 }

secrets:
  db-password: { env: DB_PASSWORD }
  api-key: { file: ~/.lab/api-key }

machines:
  db:
    networks: { lan: 10 }
    services: [{ port: 5432 }]
    secrets: [db-password]                       # /run/secrets/db-password
    docker: { image: "postgres:16" }
  web:
    networks: { lan: 11 }
    services: [{ port: 80 }]
    secrets:
      - api-key
      - { source: db-password, target: /etc/app/db, uid: 101, gid: 101, mode: 0400 }
    docker: { image: "nginx:1.27" }
```

## Stores

`secrets:` maps a kebab-case name (up to 63 characters) to where `isoloom run` reads its value
on the runner. Each entry has exactly one key:

| Store | Spec | Read with |
|---|---|---|
| Environment | `{ env: DB_PASSWORD }` | The runner's environment variable, else the profile's `inputs` of that name. |
| File | `{ file: ~/.lab/db }` | A file on the runner: an absolute path, `~/...`, or a path relative to the project. UTF-8. Keep it out of version control. |
| HashiCorp Vault | `{ vault: 'secret/app#db_password' }` | `vault kv get -field=<field> <path>`, with your `VAULT_ADDR` and `VAULT_TOKEN`. |
| SOPS | `{ sops: 'secrets.enc.yaml#db.password' }` | `sops --decrypt --extract` on the file, the dotted key as its path; the trailing newline is dropped. |

Every secret is read before anything starts, and every one that can't be read is reported at
once, without its value. A value holding a NUL byte is refused. `isoloom doctor` reads each
store the same way and drops the value at once, to say which ones would work. `isoloom down`
never reads them.

`isoloom validate` checks the references (a valid variable name, `<path>#<key>` for Vault and
SOPS), that every declared secret goes to some machine, and that a machine only lists declared
secrets.

## A machine's secrets

A machine lists the secrets it gets in its own `secrets:`. Each is a file, never an environment
variable:

- **By name** (`db-password`): the file `/run/secrets/db-password`, read-only, owned by root, with
  the default mode of the machine's kind.
- **In full** (`{ source, target, uid, gid, mode }`): `source` is the secret's name, `target` the
  file's path (absolute, or a name under `/run/secrets/`), `uid` and `gid` its owner, and `mode`
  its permissions in octal (`0400` or `"0400"`).

Default modes:

| Where | Default | Why |
|---|---|---|
| A container | `0444` | One process and one user, as Compose and Kubernetes do. |
| A Linux VM | `0400`, owned by root (or the spec's `uid`) | A VM has other users, who shouldn't read it. |
| A Windows machine | No mode: readable by SYSTEM and Administrators only | An access list, with inheritance cut. `uid`, `gid` and `mode` are refused. |

On Windows a path starting with `/` lands on drive C: (`/run/secrets/db` is
`C:\run\secrets\db`), and a drive path (`D:\app\db`) is taken as written.

Two secrets of one machine can't share a path. A Linux path can't hold `..`, `:` or spaces.

## Delivery per target

| Target | How the values arrive |
|---|---|
| `docker`, `hosted` | Compose's own secrets: the Compose file declares each one from the variable `ISOLOOM_SECRET_<NAME>` (the name upper-cased, `-` as `_`), and `isoloom run` sets those variables for `docker compose up` only. To run the file by hand, set them yourself; its header says so. |
| `kubernetes` | `isoloom run` creates the Secret `isoloom-secrets` in the environment's namespace (`kubectl apply --server-side` from standard input, so no annotation keeps a copy). Each pod mounts its secrets from it, read-only. A Secret volume's files belong to root, so `uid` and `gid` are refused. |
| Helm chart, Kustomize | The same objects, but nothing in them creates `isoloom-secrets`: `environment.yaml` carries the `kubectl create secret generic` command to run first. |
| `vagrant`, `hybrid`, `proxmox`, `esxi`, `cloud-vm`, `external` (Linux) | Over the machine's own SSH (`vagrant ssh`, SSH through the Proxmox router, SSH through the ESXi router to the address the machine's set-up announces, SSH to the address a cloud VM's set-up announces, SSH to an existing host). See below. |
| Windows machines on those targets | Over WinRM (through an SSH tunnel by the router on Proxmox and ESXi), or over OpenSSH when the image says so (`vm.image.winssh`, and always on an existing host). See below. |
| `docker-vm`, `cloud-docker`, hybrid's Docker host | The Docker host receives its containers' secrets over SSH like a VM, then hands them to `docker compose up` as `ISOLOOM_SECRET_<NAME>`. |
| A Proxmox LXC container (`docker.lxc`) | Refused: nothing delivers them into it. |
| `cloud-services` | No machines, so no secrets. |

### On a VM

The set-up of a VM runs inside the target's own tool (Vagrant, Terraform, cloud-init), so its
`Secrets` step is a gate: it comes after the machine's files and before its `depends_on` waits,
`provision` steps, `before` steps and command, and it waits for a delivery mark while
`isoloom run` delivers beside the tool.

1. `isoloom run` connects over the machine's own SSH and runs a receiving script that holds no
   value. The values travel on the connection's standard input, base64-encoded.
2. The script, as root and with `umask 077`, keeps each value under `/var/lib/isoloom/secrets/`
   (a folder only root reads), then writes the mark `.delivered`.
3. Each file is placed at its path with its owner and mode (`install -D -o -g -m`), and the
   machine's set-up goes on.

When delivery happens depends on the target: while the tool runs, retried every few seconds
(Vagrant, the clouds, the Docker hosts, ESXi's Linux machines); after Terraform is done on
Proxmox, whose cloud-init runs later, and for an ESXi Windows machine, which cloudbase-init sets
up; before provisioning on an existing host. A cloud VM's or an ESXi Linux machine's Terraform
set-up writes its address to `.isoloom-reach/<machine>` first (on ESXi with the router to jump
through), since the module's outputs only come once every set-up is done.

`/run` is emptied at every boot, so a boot service puts the files back from the kept copies
before the machine's `before` steps and command run. Running `isoloom run` again refreshes
the values.

### On Windows

The same, in PowerShell. The command line is one fixed bootstrap
(`powershell -EncodedCommand`) that reads the receiving script, then the values, from its
standard input.

- **Over OpenSSH** (`vm.image.winssh`, an existing host): `ssh` or `vagrant ssh`.
- **Over WinRM**: the runner speaks WinRM itself with `python3` and `pywinrm`, whose `Send`
  operation carries the input (`vagrant winrm -c` and Terraform feed no input to a remote
  command). `isoloom run` checks for `pywinrm` before the tool starts. The endpoint is
  `vagrant winrm-config`'s, the address a cloud's set-up announces with the password from
  Terraform's state, or on Proxmox and ESXi an SSH tunnel through the router.

The values are kept in `C:\ProgramData\isoloom\secrets`, and each file is written with an access
list of SYSTEM and Administrators only. A startup task (`isoloom-apply-secrets`) puts them back
at every boot, and the `before` and command scripts run the same placing script first, since
Task Scheduler orders no startup task.

## What never happens

- No value is written to `.isoloom/`: not in a Compose file, a manifest, a Vagrantfile, a
  Terraform module, cloud-init, or `resolved.json`.
- No value reaches Terraform's state or variables: a VM's set-up only waits for the mark.
- No value is put on a command line: values travel on standard input. None is printed either:
  a delivery's own output is discarded, and a value's debug form is `<redacted>`.
- A container never gets a secret as an environment variable, only as a file.

The leak tests hold Isoloom to this. `crates/isoloom/tests/secrets.rs` generates every target
for specs whose secrets have sentinel values, from the environment and from a file, and fails
if either value appears in any file under `.isoloom/`:

- `no_secret_value_reaches_a_generated_file`: the containers' targets (Compose, Kubernetes,
  docker-vm, cloud-docker) and the resolved snapshot;
- `no_secret_value_reaches_a_vm_target_file`: Vagrant, hybrid, Proxmox and the cloud VMs, each
  waiting for the delivery mark;
- `no_secret_value_reaches_a_windows_machines_file`: Windows machines on Vagrant, Proxmox, AWS
  and Azure, which register the startup task and never announce the password.

The receiving scripts have their own tests (`crates/isoloom-render/src/secrets/tests.rs`):
`no_value_is_in_a_script`, and a delivery that keeps each value as sent, then marks it.
