# The registry: `~/.isoloom/status.yml`

The registry records which environments `isoloom run` brought up on this host, where they run
and on which target. `status`, `connect`, `exec`, `capture` and `test` read it, so you don't have to
repeat the target. Tools that embed Isoloom read and write it too, through
`isoloom_runtime::registry`.

## Where it is

`$ISOLOOM_HOME/status.yml` when `ISOLOOM_HOME` is set, else `~/.isoloom/status.yml`. `~` is
`$HOME`, or `%USERPROFILE%` on Windows.

## Format

```yaml
# Environments `isoloom run` brought up on this host. Written by `isoloom run` and `isoloom down`;
# `isoloom status` reads it.
version: 1
environments:
- name: hello-stack
  dir: /home/me/labs/hello-stack
  target: docker
  started: 2026-10-10T09:12:44Z
- name: hello-stack
  dir: /home/me/labs/hello-stack
  target: cloud-vm
  instance: 2
  cloud: azure
  started: 2026-10-10T10:03:01Z
- name: shop
  dir: /home/me/labs/shop
  target: docker
  project: acme-shop-7
  started: 2026-10-10T11:30:00Z
```

At the top, `version` is the format version (`isoloom_runtime::registry::VERSION`), written on
every save. A file without it (written before the field existed) is version 1. It changes only
when a field is removed or changes meaning; a new optional field doesn't change it, so readers
should ignore fields they don't know.

Each entry of `environments`:

| Field | Required | Meaning |
|---|---|---|
| `name` | yes | The spec's `name`. |
| `dir` | yes | The project folder, as an absolute path. |
| `target` | yes | The target id: `docker`, `hosted`, `docker-vm`, `cloud-docker`, `kubernetes`, `hybrid`, `vagrant`, `proxmox`, `esxi`, `cloud-vm`, `external`, `cloud-services`. |
| `instance` | no | The instance number (1-99), when the environment was run with `--instance`. Its outputs live in `.isoloom-<n>/`. |
| `cloud` | no | The cloud whose module was applied (`aws`, `azure`, ...), for `cloud-vm` and `cloud-docker`. |
| `project` | no | Docker and hosted only: the Compose project name, when the Compose file was brought up under another name than its own `name:` (`docker compose -p <project>`), so the containers are labelled with that name. Whatever started it that way (a script, a tool running several copies under names of its own) records the name here. `isoloom run` never sets it: it uses the file's `name:`, and `--instance` for copies. When it's set, `status`, `connect`, `exec`, `capture` and `down` pass `-p <project>`; without it, Compose would look for an environment that isn't there. A later `isoloom run` of the same environment keeps it. |
| `started` | yes | When `run` finished, in RFC 3339 UTC format (`YYYY-MM-DDTHH:MM:SSZ`). |

An entry is identified by `dir`, `target` and `instance` together. `run` replaces the entry with
the same three values, and `down` removes it. A missing file means an empty registry. Fields not
listed here are ignored on read, and the next write drops them.

## Writing it safely

Several processes can write the registry at once: `isoloom run` and `down` in two terminals, or
a tool and its background workers. To write it without losing anyone's change:

- **Lock.** Hold `status.lock`, a file next to `status.yml` created exclusively, for the whole
  read-change-write. A lock older than 30 seconds belongs to a process that died, and can be
  taken over. A writer that has waited 10 seconds takes it over too.
- **Write atomically.** Write the whole file to a temporary file in the same folder, then rename
  it over `status.yml`. Readers never see a half-written file.

`isoloom_runtime::registry::update` does both. Prefer it to `load` and then `save` whenever more
than one writer may be running.
