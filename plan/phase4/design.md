# Phase 4 design: the configuration model, the CM backends and the catalogue

The design for PLAN.md Phase 4, against §1 (two layers: IaC and CM), §2 (CM tools in 1.0, Catalogue, Secrets, Licences) and §3 (the configuration model, the pairing rules). It builds on the approved syntax (`plan/phase2/spec.md`: `service:`, `with:`, `app.steps` §4.2) and only adds to it: nothing approved changes meaning.

**Inputs:**
- PLAN.md, `plan/phase2/spec.md` and `api-migration.md`;
- the earlier catalogue design and the maintainer's role-sourcing rule;
- the salvage branch `v1/catalogue-engine`: the loader, the checker, build-time embedding, the nginx entry and its install recipes;
- today's code: `syntax::Step`, `lower/machine.rs`, `render::dockerfile`, Terraform's cloud-init document, `xtask/src/deps.rs`;
- the feature counts of the 376 CTF lab specs (counts only, no lab content).

**Status:** design. Nothing here is implemented. The open questions are in §7, each with a recommendation. Made-up specs are in `examples/`, and a full entry is in `example-entry/nginx/`.

---

## 0. Decisions at a glance

| # | Decision | Why |
|---|---|---|
| 1 | Eight step kinds join the approved five: `repository`, `file`, `template`, `user`, `group`, `service`, `command` and `firewall`. A `packages` item may now carry a version. | PLAN §3's model, added to `app.steps` without changing it. |
| 2 | Steps keep list order in every renderer. Each step ensures a state behind a guard. A `service` step restarts its service when an earlier step changed something. | Determinism and convergence, the same in every tool. |
| 3 | Every fetched byte is pinned: images by digest, repository keys and downloads by SHA-256, vendor packages by exact version. | Honest proof and reproducible builds. |
| 4 | One POSIX shell library implements each step kind per OS family. The Dockerfile, shell and cloud-init renderings all call it. Ansible uses official modules, and Windows uses one PowerShell library. | Parity by construction for three of the four CM tools: three implementations to keep equal, not five. |
| 5 | cloud-init carries the shell rendering (`write_files` and `runcmd`). It never uses cloud-init's own `packages:` or `users:` modules for recipe steps. | Those modules run in cloud-init's module order, not the recipe's. |
| 6 | Ansible runs from Isoloom's Linux controller, over SSH and WinRM, as a generated `site.yml`. The IaC tool only bootstraps the machine. | One Ansible install, host-neutral, and it reaches Windows. |
| 7 | **Containers:** one service runs as the foreground process, with no supervisor. Several run under the engine's init (tini) and a generated runner that fails fast. A service with no foreground form is refused. | §3. |
| 8 | Catalogue entries are written in the same model, in phases (`install`, `configure`, `start`). The container form is the official image when that image's OS is the machine's, otherwise a build from the OS image. The VM form is the same recipe, rendered by the chosen CM tool. | Parity by construction, which the contract still proves. |
| 9 | `entry.yml` is written by hand. `lock.yml` (digests, exact package versions, checksums) is written only by `cargo xtask catalogue refresh`. | People review intent; a tool owns the pins. |
| 10 | GPL Galaxy content is referenced through a pinned `requirements.yml` in the generated project. It never enters the repository or the binary. Rendered tasks use only `ansible.builtin`, `ansible.posix` and `ansible.windows` modules, and no vendor roles in 1.0. | PLAN §2 (Licences) and the maintainer's rule. |
| 11 | Some CM renderers write output that an IaC file embeds: shell, PowerShell, Dockerfile and cloud-init. They are core modules (`isoloom-render::configure`). Ansible is a backend, which IaC backends reach only through a `Handoff` from the plan. | The one-way dependency rule. |
| 12 | Parity is checked by real runs through the Isothrum CLI. `cargo xtask catalogue check` runs on Docker and on Vagrant + VirtualBox, and writes `catalogue/conformance.json`. | The Phase 4 exit in PLAN.md. |

---

## 1. The configuration resource model

### 1.1 Steps

The approved five keep their syntax and meaning, and eight are new. A step is still `reboot`, or a map with exactly one kind key. Fields such as `creates`, `unless` and `owner` sit beside that key.

| Step | Fields (type) | Example |
|---|---|---|
| `run` (approved) | a project script; `creates` (absolute path) | `{ run: provision/web.sh, creates: /srv/web/.done }` |
| `copy` (approved) | `from` (project file or folder), `to`, `owner`, `mode` | `{ copy: { from: site/, to: /var/www/html/ } }` |
| `packages` (approved; items extended) | a list of names, or of `{ name, version }` with the family's own version string | `{ packages: [curl, { name: nginx, version: 1.30.6-1~bookworm }] }` |
| `reboot`, `playbook` (approved) | as approved | `reboot` |
| `repository` | `name` (kebab-case), `url` (https), `key: { url, sha256 }`, `suite` (apt only; default: the OS codename), `components` (apt only; default `[main]`), `priority` (integer) | see below |
| `file` | `path` plus exactly one of `state: directory \| absent`, `link: <target>` or `url` + `sha256` (a download); `owner`, `group`, `mode` | `{ file: { path: /opt/tool.tgz, url: "https://example.org/tool.tgz", sha256: 9f2c… } }` |
| `template` | `from` (a project file), `to`, `owner`, `group`, `mode` | `{ template: { from: conf/app.toml, to: /etc/app.toml, mode: "0640" } }` |
| `user` | `name`; `uid`, `group`, `groups`, `home`, `shell`, `system` (bool), `password` (a literal, which is lab content) | `{ user: { name: alice, uid: 1500, groups: [sudo], password: winter } }` |
| `group` | `name`; `gid`, `system` | `{ group: { name: ops, gid: 1600 } }` |
| `service` | a unit name, or `{ name, run, user, cwd, enabled }` | `{ service: nginx }`, `{ service: { name: api, run: [/srv/api/bin/api], user: api } }` |
| `command` | a text (run by `sh -c`, or PowerShell on Windows) or an argv; exactly one of `creates` and `unless` | `{ command: a2enmod rewrite, creates: /etc/apache2/mods-enabled/rewrite.load }` |
| `firewall` | `allow`: the machine's port names, or `<number>[/udp]` | `{ firewall: { allow: [http, 8443] } }` |

```yaml
- repository:
    name: nginx
    url: https://nginx.org/packages/debian
    components: [nginx]
    key: { url: "https://nginx.org/keys/nginx_signing.key", sha256: 4c3f… }
```

**Field rules (parse, don't validate):**
- **`repository`:** `url` is `https`, and the key is required, pinned by the SHA-256 of its file. The repository's kind (apt, rpm or apk) is the OS family's, so `suite` or `components` on a non-apt family is `invalid-value`.
- **`file`:** `url` needs `sha256`, and a `link` target is absolute. `state: absent` is the only removal in the model.
- **`user.password` in `app.steps`** is a literal: lab content, which lands in generated files.
  - It is hashed with a salt derived from the lab, machine and user names, so output stays deterministic. On Windows it stays plain, because Windows takes no hash.
  - `{ secret }` and `{ input }` are refused there, since steps have no secrets (approved §4.2). Catalogue entries set such passwords in their `start` phase (§4.5).
- **`service`:**
  - With `run`, Isoloom defines the service: a generated systemd or OpenRC unit, or a Windows service, on VMs; a runner process in containers.
  - Without `run`, the unit comes from a package, and the container form takes its foreground form from the daemon table (§3.4).
- **`command`:** one without a guard is `missing-field`. The fix: add `creates:` or `unless:`, or make it a `run:` script.
- **`firewall.allow`** names declared ports (or numbers) and opens them in the OS's own firewall when one is running. Deny rules are 1.x; network policy between machines stays in `reach:`.

### 1.2 Types

The authored types live in `isoloom-spec::syntax`. The addition is safe because `Step` is already `#[non_exhaustive]`. The resolved model lives in `isoloom-plan::configure`. Lowering builds it, once per machine and target.

```rust
// isoloom-spec::syntax::app (authored)
pub enum Step { Run { .. }, Copy(Copy), Packages(Vec<Package>), Reboot, Playbook(ProjectPath),
    Repository(Repository), File(FileStep), Template(Template), User(User), Group(Group),
    Service(Service), Command(Command), Firewall(Firewall) }
pub enum Guard { Creates(AbsPath), Unless(CommandLine) }
pub enum FileState { Directory, Absent, Link(AbsPath), Download { url: HttpsUrl, sha256: Sha256 } }
pub enum Service { Unit(UnitName),
    Defined { name: UnitName, run: Argv, user: Option<UserName>, cwd: Option<AbsPath>, enabled: bool } }

// isoloom-plan::configure (resolved: no placeholder, setting name or port name is left)
pub struct Recipe { pub family: Family, pub install: Vec<Resource>, pub configure: Vec<Resource>,
    pub start: Vec<Resource>, pub daemons: Vec<Daemon> }
pub enum Resource { Repository(RepositoryPlan), Packages(Vec<PinnedPackage>), Copy(CopyPlan),
    File(FilePlan), Template(RenderedFile), User(UserPlan), Group(GroupPlan), Service(ServicePlan),
    Command(CommandPlan), Firewall(Vec<PortSpec>), Run(RunPlan), Reboot, Playbook(ProjectPath) }
pub struct Daemon { pub unit: UnitName, pub foreground: Argv, pub user: Option<UserName>, pub logs: Vec<LogLink> }
pub enum ContainerProcess { Image, Idle, Command(Argv), One(Daemon), Supervised(Vec<Daemon>) }
pub enum Configure { Shell, CloudInit, Ansible }            // Shell is PowerShell on Windows
pub enum Handoff { Image { dockerfile: OutPath }, Inline { script: OutPath },
    UserData { document: OutPath }, Controller { playbook: OutPath } }
```

- **Newtypes:** `UnitName`, `UserName`, `GroupName`, `PackageName`, `Sha256`, `HttpsUrl`, `AbsPath`, `ProjectPath`, `Argv`, `Mode`.
- **In the lifecycle,** Phase 2's `Step::Run` and `Step::Reboot` for configuration give way to `Step::Configure(Recipe)`, `Step::Start` and `Step::Services` (§2.4).

### 1.3 Semantics

**Idempotence.** Every step states a desired state and a guard, so applying a recipe twice changes nothing the second time. Each renderer reports what changed: Ansible through its `changed` results, the shell library through `isoloom: changed <step>` lines.

| Step | Already done when | Otherwise |
|---|---|---|
| `repository` | the source file and key file have the expected content | writes both, refreshes the index |
| `packages` | the package manager reports each one installed (at the version, when given) | installs the missing ones |
| `copy`, `template` | content (SHA-256), owner and mode match | writes the file or folder |
| `file` | the state matches (for a download, its SHA-256) | creates, links, removes or downloads |
| `user`, `group` | it exists with the given fields | creates or modifies; never deletes |
| `service` | enabled at boot and running | enables and starts it, and restarts it per the rule below |
| `command` | `creates` exists, or `unless` exits 0 | runs it |
| `firewall` | no firewall runs, or the port is already open in it | opens the port, persistently |
| `run` | `creates` exists; without `creates` the script runs at every apply | runs it |
| `reboot` | this step has already restarted the machine once | restarts, then continues |

**Restart rule.**
- A `service` step restarts its service when any configuration step earlier in the same recipe changed something during this apply. Otherwise it only makes sure the service is enabled and running.
- Restarting too often is harmless; restarting too rarely serves stale configuration, so the rule errs on restarting.
- In Ansible, every earlier task that can change something notifies the handler, flushed at the `service` task.

**Ordering.** List order is the contract: no renderer reorders, groups or parallelizes steps, and there is no dependency graph. The phases always run in this order:

| Phase | What | Container form | VM form | Secrets and inputs | Network to other machines |
|---|---|---|---|---|---|
| `steps` (an entry's `install` and `configure`) | install and configure | image build | first provisioning, after `Secrets` | no | no |
| `start` (entries only) | per-start preparation: host keys, first-start database set-up, passwords from secrets | the runner, at each start, before the services | a boot unit before the services, at each boot | yes, as files | no |
| services | the recipe's `service` steps and the machine's `command` | §3 | enabled units, started | n/a | n/a |
| `setup` (approved) | once, after the machine is ready | the sibling job | in the VM | yes | yes |

**No implicit removal.** Deleting a step from a spec doesn't undo it on a machine that already ran it: the undo is a fresh machine. `file: { state: absent }` is the one explicit removal.

**Every fetch is pinned** (decision 3): a repository key by SHA-256, a download by SHA-256, an image by digest, and a vendor package by exact version (for entries, from `lock.yml`).

### 1.4 Templates and placeholders

There is one placeholder grammar for the whole spec: the message's `{{ path }}`. Isoloom renders templates at generation, never through Jinja2 or envsubst, so every CM tool installs the same bytes.

| Variable | Available in | Value |
|---|---|---|
| `{{ machine.name }}`, `{{ machine.addresses.<net> }}` | app and entry templates | this machine's |
| `{{ machines.<m>.addresses.<net> }}`, `{{ machines.<m>.ports.<name>.port }}` | app and entry templates | another machine's (validated) |
| `{{ version }}`, `{{ os.family }}`, `{{ os.version }}`, `{{ os.codename }}`, `{{ vars.<x> }}` | entries | the resolved release and OS |
| `{{ with.<setting> }}`; `{{ with.<setting>.path }}` for a password given as a secret or input | entries | the setting's value; for a secret or input, its value file's path, never the value |
| `{{ item.<field> }}` | entry steps with `for_each` | the list item |

- **Refused:**
  - `.published` and `.url`, which are run-time values;
  - an unknown path (`unknown-reference`);
  - any secret or input value. Only its file's path is available.
- **Logic-free:** files have no conditionals or loops. Entries choose files with `when:` and repeat steps with `for_each:`.
- **Injection:**
  - A string setting that reaches a template must declare a `pattern` or `values`, and the entry check enforces it.
  - `isoloom-render`'s quoting functions always quote shell and PowerShell arguments. They are property-tested and fuzzed.

### 1.5 Portability and refusals

| Step | Container form | Linux VM | Windows VM |
|---|---|---|---|
| `run` | `.sh` | `.sh` | `.ps1` |
| `copy`, `template` | yes | yes | yes; `owner` and `mode` are refused (ACLs are 1.x) |
| `packages` | yes | yes | Windows features (`Install-WindowsFeature`; optional features on client editions) |
| `repository` | yes | yes | **refused:** Windows has no in-box package repository; use `run` |
| `file` | yes | yes | yes; `link` is refused |
| `user`, `group` | yes, at build | yes | local users and groups |
| `service` | §3 | systemd, OpenRC | Windows services |
| `command` | yes, at build | yes | PowerShell |
| `firewall` | satisfied by construction: a container has no host firewall, and `reach` governs | firewalld when it runs (Rocky, Alma, Fedora), ufw when it's active (Ubuntu), else nothing | Windows Firewall |
| `reboot`, `playbook` | **refused** (approved) | yes | yes |

Everything else that rules out a form is unchanged from the approved spec (§4.3).

### 1.6 New common mistakes

| Mistake | Path | Code and reason | Fix |
|---|---|---|---|
| A `command` without a guard | `machines.web.app.steps[3]` | `missing-field`: a command runs once; say how to tell it already ran | `creates: <path>` or `unless: <check>`, or a `run:` script |
| A download without a checksum | `machines.web.app.steps[2].file.url` | `missing-field`: every fetch is pinned | `sha256: <hex>` |
| A secret password in a step | `machines.web.app.steps[4].user.password` | `sensitive-value`: steps have no secrets | a literal (lab content), or a catalogue entry's `users` setting |
| `repository` on Windows | `vagrant: machines.ws.app.steps[0]` | `refused`: Windows has no package repository | `run: provision/install.ps1` |
| A service with no foreground form | `docker: machines.web.app.steps[5].service` | `refused`: `myd` isn't in the daemon table, so a container can't run it | `{ service: { name: myd, run: [/usr/sbin/myd, --foreground] } }` |
| A daemon package without a `service` step | `machines.web.app.steps[1]` | warning: `nginx` starts on Debian VMs only | `service: nginx`, or `{ service: { name: nginx, enabled: false } }` |

---

## 2. Rendering per CM tool (1.0)

### 2.1 How the four tools share the work

| CM tool | Implementation | Carrier |
|---|---|---|
| Shell (Linux) | the **shell library**, `configure.sh`: one POSIX `sh` function per step kind and family (apt, dnf and apk; systemd and OpenRC), guard included. A recipe is one call per step. The library installs a downloader when it needs one (`curl` and `ca-certificates`; BusyBox `wget` on Alpine). | today's VM set-up script: Vagrant's shell provisioner, the runner's `ssh.run` |
| Dockerfile | the same library: one `COPY` of it, then one `RUN` per step; native `COPY` for `copy` and `template` | the generated build in `.isoloom/docker/build/<m>/` |
| cloud-init | the same library and script, in `write_files` (gzip+base64), run by `runcmd`. Native modules are used only for the bootstrap Isoloom already writes: hostname, users, `bootcmd` and disks. | `user_data`, or a Proxmox snippet |
| Ansible | native tasks from official modules only (`ansible.builtin`, `ansible.posix`, `ansible.windows`): one role per entry (`isoloom_<entry>`) and one per app machine | `site.yml`, run by the controller |
| PowerShell (Windows) | the **PowerShell library**, `configure.ps1`: one function per step kind | Vagrant's `powershell` provisioner, the runner's `winrm.run`, cloud user data |

A resource has three implementations to keep equal (the shell library, Ansible and the PowerShell library), not five. The resource conformance tests (§6.3) keep them equal.

### 2.2 Per step

| Step | Shell library (also each Dockerfile `RUN` and cloud-init) | Ansible | PowerShell |
|---|---|---|---|
| `repository` | `isoloom_repository nginx <url> <suite> <components> <key-url> <sha256>`: the key goes to `/usr/share/keyrings/isoloom-<name>.asc` (checked), then a deb822 source, a `.repo` file or a `repositories` line | `ansible.builtin.get_url` (with `checksum`), then `ansible.builtin.deb822_repository`, `ansible.builtin.yum_repository`, or `ansible.builtin.lineinfile` for apk | refused |
| `packages` | `isoloom_packages 'nginx=1.30.6-1~bookworm'`: checks with `dpkg-query`, `rpm -q` or `apk info -e`, then `apt-get`, `dnf` or `apk add`; caches cleaned | `ansible.builtin.apt`, `ansible.builtin.dnf`; on Alpine `ansible.builtin.command: apk add`, guarded by `apk info -e`, since the `apk` module is in `community.general`; on Windows `ansible.windows.win_feature` | `Install-WindowsFeature`, guarded by `Get-WindowsFeature` |
| `copy` | `isoloom_copy` from `/opt/isoloom`: `cmp`, `chown`, `chmod` (in a Dockerfile: `COPY --chown --chmod`) | `ansible.builtin.copy` / `ansible.windows.win_copy` | `Copy-Item`, guarded by the file hash |
| `template` | `isoloom_copy` of the rendered file (a Dockerfile `COPY`s it) | `ansible.builtin.copy` of the rendered file, not `template` | `Copy-Item` of the rendered file |
| `file` | `isoloom_file`: `mkdir`, `rm`, `ln -s`, or a download checked with `sha256sum` | `ansible.builtin.file`, `ansible.builtin.get_url` (with `checksum`) | `New-Item`, `Remove-Item`, `Invoke-WebRequest` checked with `Get-FileHash` |
| `user`, `group` | `isoloom_user` and `isoloom_group`: `getent`, then `useradd` or `adduser`, and `usermod -p <hash>` | `ansible.builtin.user`, `ansible.builtin.group`; `ansible.windows.win_user`, `ansible.windows.win_group` | `New-LocalUser`, `New-LocalGroup`, `Add-LocalGroupMember` |
| `service` | `isoloom_service nginx`: `systemctl enable --now`, or `rc-update add` and `rc-service start`, restarted when the change flag is set. In a Dockerfile: nothing; it joins the runner's list (§3). | `ansible.builtin.service` (enabled, started), a restart handler and `meta: flush_handlers`; `ansible.windows.win_service` | `Set-Service -StartupType Automatic`, `Start-Service`, `Restart-Service` |
| `command` | `[ -e <creates> ] \|\| sh -c '…'`, or `<unless> \|\| sh -c '…'` | `ansible.builtin.command` with `creates`; for `unless`, a check task (`changed_when: false`) and `when:` | `if (-not (Test-Path …)) { … }`, or the `unless` check first |
| `firewall` | `isoloom_firewall 80/tcp`: `firewall-cmd --permanent` when firewalld runs, `ufw allow` when ufw is active, else nothing. In a Dockerfile: nothing. | `ansible.posix.firewalld` when firewalld runs; `ansible.builtin.command: ufw allow` when ufw is active (`community.general.ufw` isn't allowed) | `New-NetFirewallRule`, guarded by `Get-NetFirewallRule`; through `ansible.windows.win_powershell` under Ansible |
| `run` | `sh /opt/isoloom/<script>` (a Dockerfile `COPY`s and `RUN`s it, as today) | `ansible.builtin.script` (with `creates`) | the script |
| `reboot` | today's continuation; refused in a Dockerfile | `ansible.builtin.reboot`, `ansible.windows.win_reboot` | today's continuation |
| `playbook` | `ansible-playbook -c local` (approved); refused in a Dockerfile | `import_playbook`, limited to the host | from the controller |

### 2.3 One recipe, every rendering

This recipe, from the nginx entry resolved for `debian@12`:

```yaml
- repository: { name: nginx, url: "https://nginx.org/packages/debian", components: [nginx],
                key: { url: "https://nginx.org/keys/nginx_signing.key", sha256: 4c3f… } }
- packages: [{ name: nginx, version: 1.30.6-1~bookworm }]
- template: { from: files/default.conf, to: /etc/nginx/conf.d/default.conf }
- firewall: { allow: [http] }
- service: nginx
```

**Shell**, the VM set-up; cloud-init runs the same file:
```sh
. /opt/isoloom/.isoloom/configure/lib/configure.sh
isoloom_repository nginx https://nginx.org/packages/debian bookworm nginx https://nginx.org/keys/nginx_signing.key 4c3f…
isoloom_packages 'nginx=1.30.6-1~bookworm'
isoloom_copy .isoloom/configure/web/rendered/default.conf /etc/nginx/conf.d/default.conf root:root 0644
isoloom_firewall 80/tcp
isoloom_service nginx
```

**Dockerfile.** The firewall step needs nothing, and the service is the runner's one foreground process, `exec nginx -g 'daemon off;'`:
```dockerfile
FROM debian:12@sha256:2c03…
COPY .isoloom/configure/lib/configure.sh /usr/local/lib/isoloom/configure.sh
RUN . /usr/local/lib/isoloom/configure.sh && isoloom_repository nginx https://nginx.org/packages/debian bookworm nginx https://nginx.org/keys/nginx_signing.key 4c3f…
RUN . /usr/local/lib/isoloom/configure.sh && isoloom_packages 'nginx=1.30.6-1~bookworm'
COPY --chmod=0644 .isoloom/configure/web/rendered/default.conf /etc/nginx/conf.d/default.conf
COPY --chmod=0755 .isoloom/configure/web/run /usr/local/bin/isoloom-run
ENTRYPOINT ["/usr/local/bin/isoloom-run"]
```

**cloud-init:**
```yaml
#cloud-config
write_files:
  - { path: /var/lib/isoloom/configure.sh, encoding: gz+b64, permissions: "0700", content: H4sI… }
runcmd:
  - [sh, /var/lib/isoloom/configure.sh]
```

**Ansible** (`roles/isoloom_nginx/tasks/main.yml`):
```yaml
- name: nginx's repository key
  ansible.builtin.get_url: { url: "https://nginx.org/keys/nginx_signing.key", dest: /usr/share/keyrings/isoloom-nginx.asc,
                             checksum: "sha256:4c3f…", mode: "0644" }
- name: nginx's repository
  ansible.builtin.deb822_repository: { name: isoloom-nginx, uris: "https://nginx.org/packages/debian", suites: bookworm,
                                       components: nginx, signed_by: /usr/share/keyrings/isoloom-nginx.asc }
  notify: isoloom restart nginx
- name: nginx 1.30.6
  ansible.builtin.apt: { name: "nginx=1.30.6-1~bookworm", update_cache: true }
  notify: isoloom restart nginx
- name: /etc/nginx/conf.d/default.conf
  ansible.builtin.copy: { src: rendered/default.conf, dest: /etc/nginx/conf.d/default.conf, mode: "0644" }
  notify: isoloom restart nginx
- name: nginx enabled and running
  ansible.builtin.service: { name: nginx, enabled: true, state: started }
- ansible.builtin.meta: flush_handlers
```

**PowerShell**, an `iis` recipe for comparison:
```powershell
. C:\isoloom\.isoloom\configure\lib\configure.ps1
Isoloom-Feature Web-Server
Isoloom-Copy -From intranet -To C:\inetpub\wwwroot
Isoloom-Firewall -Port 80 -Protocol TCP
Isoloom-Service W3SVC
```

### 2.4 The pairing rules

**The VM lifecycle splits at `Secrets`.**
- **Bootstrap** is everything up to and including `Secrets`: the prelude, disks, hosts, routes, the project copy, files and secrets. The IaC tool writes it in its own carrier, as today.
- **Configuration** is everything after it: the waits, the recipe, `start`, the services, `command`, `setup`, egress, link impairment and the ready marker. The chosen CM tool writes it.
- Isoloom's own plumbing in the configuration part (egress, `tc`, the ready marker) is shell that every CM tool runs verbatim.

| IaC | CM tool (default in bold) | How it hands off |
|---|---|---|
| Compose and Kubernetes (`docker`, `hosted`, `docker-vm`, `cloud-docker`, `kubernetes`) | **Dockerfile**, always | The configuration is baked into the generated image, and `build:` points at it. `--configure` doesn't apply. |
| Vagrant (every provider) | **shell/PowerShell**, Ansible | **Shell:** the inline `shell` or `powershell` provisioner runs on past the bootstrap, as today. **Ansible:** each machine's provisioner stops after the bootstrap. The controller, provisioned last, runs `site.yml` and then the spec's `provision:` playbooks. **cloud-init:** refused: Vagrant's `cloud_init` is experimental and VirtualBox-only, and boxes don't ship cloud-init. |
| Terraform (`proxmox`, `esxi`, `cloud-vm`) | **cloud-init** for Linux, **PowerShell** for Windows (cloud user data; cloudbase-init on Proxmox and ESXi); shell; Ansible | **cloud-init:** one document per machine holds the bootstrap and the configuration, run at first boot. **Shell, PowerShell and Ansible:** a run-manifest step after `apply`: `ssh.run` or `winrm.run` of the rendered script, or the controller's `ansible-playbook`. Never a `local-exec`: output stays host-neutral, and Phase 3 moves guest steps to the runner anyway. |
| Existing hosts (`external`) | **Ansible**, shell | `site.yml` against the external inventory, bootstrap included, since the hosts already exist. |

cloud-init configures at first boot only. A change needs a new machine, because cloud-init doesn't run again.

**Choosing a CM tool:**
- `isoloom generate --configure <shell|cloud-init|ansible>`. Each target's default is the tool in bold above, which is today's carrier, so the snapshots don't move.
- A lab may pin the tool with `overrides.<target>.configure` (Q6).
- `resolved.json` and the run manifest record the choice. When Ansible is chosen, the manifest's `tools[]` gains `ansible-core` on the controller.

---

## 3. Services in containers

### 3.1 The gap

Take a machine with `os: debian@12` and `app.steps: [{ packages: [nginx] }]`. Today it builds an image whose process sits idle: in `lower/machine.rs`, no `command` means `idle`.
- **In the container,** nginx is installed but never starts.
- **On a Debian VM,** the same steps start it, through the package's own postinst.
- **On a Rocky VM,** they don't, since Rocky's packages don't start their services.

The behaviour differs by form and by family, and nothing says so. Phase 4 makes it explicit: in every form, a daemon runs only where a `service` step (or `command`) says so.

### 3.2 Options

| Option | What runs in the container | Health | Logs | Cost | Gaps |
|---|---|---|---|---|---|
| **A.** The first `service` becomes the foreground command | that one daemon, `exec`'d | the engine watches one process; ports and healthcheck as today | the daemon's own stdout and stderr | nothing extra | A second service is dropped, or refused. Refusing it loses common recipes: web + php-fpm on Rocky, an app with its worker, an all-in-one training app. |
| **B1.** tini + a generated runner | every daemon, each in its foreground form, under the engine's init (tini). The runner exits when one daemon exits, and the engine restarts the container. | ports and healthcheck. A dead daemon takes the container down, so a half-running machine never looks ready. | one stream, each line prefixed `<unit> \| ` | one generated POSIX script (about 60 lines), shellchecked and snapshot-tested; nothing downloaded; tini is the engine's own (`init: true`) | no per-daemon restart: the whole container restarts |
| **B2.** s6-overlay | every daemon, supervised, with per-daemon restart and readiness | per daemon | per-daemon log pipelines | release tarballs per architecture, fetched at build and pinned; its own `/init`, environment and readiness conventions to render and verify | heavier; a second supervision model to prove; nothing shared with the VM form |
| **C.** Refusal | — | — | — | none | no container form for any recipe with a service, which is most recipes |

### 3.3 Decision: A for one daemon, B1 for several, C only when there's no foreground form

| The recipe has | The container's process (`ContainerProcess`) |
|---|---|
| no `service` step and no `command` | `Idle` (today) |
| a `command` and no `service` | `Command` (today: `CMD`) |
| exactly one enabled `service`, and no `command` | `One`: the runner runs `start`, then `exec`s the daemon's foreground form |
| two or more, or one plus a `command` | `Supervised`: the runner runs `start`, starts each daemon (and the command) in order, then waits |
| a service with no foreground form | the container form is refused (§1.6) |
| a service with `enabled: false` | not started; on VMs, disabled and stopped |

**The runner** is `/usr/local/bin/isoloom-run`, generated per machine in POSIX `sh`. It:
1. runs the recipe's `start` steps; the secrets and inputs are already files (spec §3.4);
2. links each daemon's declared log files to its stdout or stderr (§3.4);
3. **with one daemon,** `exec`s it, dropping to its `user` (`setpriv`, or BusyBox `su` on Alpine);
4. **with several,** starts each with its line prefix, in recipe order. On `SIGTERM` or `SIGINT` it forwards the signal to every daemon and waits. When any daemon exits, it stops the others and exits with that daemon's status.

**PID 1 is the engine's tini.**
- Compose sets `init: true` on every service whose process is the runner.
- Kubernetes sets `shareProcessNamespace: true`, so the pause process reaps orphans.
- Nothing is installed in the image for this. tini isn't in Rocky's or Alma's base repositories anyway.

**Restart** is the engine's: Compose's `restart: unless-stopped`, Kubernetes' default. Failing fast and letting the engine restart is the container idiom, and a lab never sits half up.

**Health** follows the rules approved in spec §3.3: the machine's `healthcheck`, else the entry's `health`, else its TCP ports answering. Each daemon's ports belong to the machine, so readiness already waits for all of them.

**Logs:**
- **One daemon** writes to the container's output, with its log files linked to `/dev/stdout` and `/dev/stderr`, as official images do.
- **Several** share one stream, each line prefixed by the daemon's unit name. A FIFO per daemon lets the runner still hold each daemon's PID.

**Other forms:**
- **The VM form** never uses the runner. Services are enabled units (systemd, OpenRC, Windows services), and `start` is a boot unit ordered before them.
- **The image form** (an official image) keeps its own entrypoint and process. It uses the runner only when the entry asks for its `start` steps (§4.5).
- **LXC on Proxmox** runs the image's entrypoint, so the runner works there unchanged.

### 3.4 The daemon table

A `service: <unit>` step without `run` needs the unit's foreground form for containers. It comes from the **daemon table**:
- **Sources:** every catalogue entry's `daemons:` (nginx, apache, php-fpm, sshd, mariadb…), plus `catalogue/daemons.yml` for daemons no entry owns (cron, rsyslog, vsftpd, xinetd…).
- **Each daemon:** its unit name per family, its foreground argv, its user and its log files.
- **Unit names differ by family:** `ssh` on Debian, `sshd` on Rocky and Alpine. A step names the table's key, and lowering picks the unit.

A recipe that installs a package the table lists as shipping a daemon (nginx, apache2, openssh-server, mariadb-server…), with no `service` step for that daemon, gets a **warning** with the fix (§1.6).

---

## 4. The catalogue

### 4.1 Layout

```
catalogue/
  daemons.yml                 daemons no entry owns
  collections.yml             the official Galaxy collections the Ansible renderer may name, pinned
  entry.schema.json           generated from the Rust types (stale-file test)
  contract.schema.json        generated from the Rust types (stale-file test)
  conformance.json            written by `cargo xtask catalogue check`
  <entry>/
    entry.yml                 by hand: what it is, releases, settings, ports, data, daemons, recipe
    lock.yml                  written by `cargo xtask catalogue refresh`: digests, package versions, checksums
    contract.yml              by hand: the parity contract
    files/                    the templates and scripts the recipe names (Apache-2.0, Isoloom's own)
```

PLAN's `roles/` folder becomes `files/`: the Ansible role (`isoloom_<entry>`) is rendered from the recipe, not written.

### 4.2 `entry.yml`

| Field | Type | Notes |
|---|---|---|
| `name`, `description`, `upstream` | text | `name` is the folder's name, in kebab-case |
| `licence` | `{ software, notes }` | the software's licence (SPDX), recorded for the generated catalogue page |
| `releases` | a list of `{ version, channel, source, os, vars }` | §4.3 |
| `default` | a release's version | pinned: what `service: <name>` resolves to |
| `images` | a map from OS to an image reference template (`docker.io/library/nginx:{{ version }}-trixie`) | an official image, only where its OS is that OS |
| `settings` | a map from name to setting (§4.4) | the schema of `with:` |
| `ports` | a map from name to `{ port \| setting, protocol }` | the machine's default ports, by name |
| `data` | a map from name to an absolute path | persistent data: the machine's volumes, at one path in both forms |
| `health` | a healthcheck, in the shape of spec §3.3 | optional; otherwise the ports are probed |
| `daemons` | a map from key to `{ unit (per family), foreground, user, logs }` | added to the daemon table |
| `image` | `{ environment, files, command, start }` | how settings reach the official image (§4.5) |
| `recipe` | `{ install, configure, start }`, each a list of steps; entry steps may add `when:` and `for_each:` | §4.5 |

- **A repository key in an entry** gives `{ url, fingerprint }` for a GPG key: the refresh downloads it, checks the fingerprint and writes its SHA-256 into `lock.yml`. Any other key gives `{ url, sha256 }` directly.
- **`when:`** takes `family`, `os`, `release` (a channel or version), `set: <setting>` and `unset: <setting>`. It never takes the form, because the forms must not differ.
- **`for_each: <list setting>`** repeats a step, with `{{ item.* }}` for the item.

### 4.3 Releases, sources and forms

| `source` | VM form installs from | Container form | Version pinned to |
|---|---|---|---|
| `vendor` | the vendor's repository: nginx.org, PGDG, MariaDB, repo.mysql.com, repo.mongodb.org, packages.redis.io, Team RabbitMQ | the official image when `images` has the OS, else a build from the OS image with the same recipe | the exact upstream version (`1.30.6`); `lock.yml` holds each OS's package version string (`1.30.6-1~bookworm`) |
| `distro` | the OS's own packages: Apache, PHP, OpenSSH, Windows features, and older database series an OS shipped | a build from the OS image with the same recipe | the series (`2.4`, `8.2`), since the OS archive moves within it with security updates. The exact version a build got is recorded in the image's labels and the VM's facts. |
| `download` | a pinned archive (a `file` step with `url` and `sha256`) | the official image, else a build | the exact release |

- **The official image is used only when its OS is the machine's OS,** and, for images with a language runtime, its runtime series too. Otherwise the container is built. The parity contract compares `/etc/os-release` across forms, and an Ubuntu-based image behind `os: debian@12` would be a false claim.
- **Version matching:**
  - `service: postgres@16` picks the newest release whose version starts with `16.`, on dot boundaries;
  - `postgres@16.10` is exact;
  - no version means the `default`.

  The result is recorded in `resolved.json`, so it moves only with an Isoloom upgrade.
- **Retention:** a vendor pin must stay installable. An entry uses the vendor's archive where the main repository drops old minor versions (PGDG: `apt-archive.postgresql.org`). The refresh and the weekly conformance run catch a version that has vanished.
- **The OS table** gains `alpine@3.24` (salvaged) and keeps its pinned base images.

### 4.4 Settings: the schema of `with:`

| `type` | Accepts | Extra fields | Reaches the machine as |
|---|---|---|---|
| `string` | text | `pattern` or `values` (required when a template uses it), `default` | the text |
| `integer`, `boolean` | the value | `minimum`, `maximum`, `default` | the value |
| `port` | 1 to 65535 | `default` | the value; a port in `ports:` may name it |
| `endpoint` | `<machine>:<port>` or `<address>:<port>` | | the machine's address on a shared network (validated, like `dns` names) |
| `folder`, `file` | a project path | | its content, copied where the recipe says |
| `password` | a literal (lab content), `{ secret }` or `{ input }` | `required`, `owner` | a literal stays text. Otherwise a value file at `/run/isoloom/with/<setting>` (mode 0400, owned by root or by the setting's `owner`), reached as `{{ with.x.path }}`; never the value itself |
| `list` | a list | `items` (a map of these types), `max` | `for_each` |

- **Every setting** has a `description`, and either `required` or a `default`.
- **The entry check** proves the recipe uses every setting (and so does `image`, when the entry has `images`), so a setting can't silently do nothing in one form.
- **The spec's JSON Schema** gains, for each entry, a generated rule: if `service` names that entry, `with` follows its settings schema. Editors, Isothrum's included, can then complete `with:`.

### 4.5 Phases per form

| Phase | Official image form | Built container form | VM form |
|---|---|---|---|
| `install` | skipped: the image provides it | Dockerfile, at build | provisioning |
| `configure` | a thin generated Dockerfile, `FROM <image>@<digest>` plus these steps, built only when there are any | Dockerfile, at build | provisioning |
| `start` | **By default,** the image's own entrypoint does it, fed by `image.environment` (`POSTGRES_PASSWORD_FILE`, `MARIADB_DATABASE`…). **With `image.start: run`,** Isoloom's runner runs these steps, then `exec`s the image's own entrypoint and command, which the refresh records from the image config. | the runner, at each start | a boot unit before the services, at each boot |
| services | the image's own process; `image.command` may set its arguments | the runner (§3) | enabled units |

`start` steps must be idempotent across restarts. First-start work is guarded by a marker in the entry's `data` path (`creates: /var/lib/mysql/.isoloom-init`), so a container re-created over its volume doesn't redo it.

### 4.6 The parity contract and how it runs

`contract.yml` has three parts:

| Field | What |
|---|---|
| `cases` | named set-ups for the conformance lab: `with:` values, project fixture files (inline) and helper machines (a `php-fpm` machine for nginx's `php_fpm`, a `mariadb` machine for `wordpress`). `default: {}` always runs. |
| `checks` | the spec's own check grammar (`http`, `tcp`, `exec`, `script`, `expect`, `contains`, `wait`), with `{{ machine }}`, `{{ version }}` and `{{ with.* }}`. `case:` limits a check to one case, and `exec` runs in the service machine. |
| `facts` | commands whose output must be **equal** in both forms: the ID and VERSION_ID from `/etc/os-release`, the first line of `nginx -V`, the installed package version. |

**The harness,** `cargo xtask catalogue check [<entry>] [--version …] [--os …] [--form container|vm] [--configure …] [--provider …]`:
1. **Generates,** for each case, a small lab on one isolated network: the service machine, its helpers, and an access machine (`probe`) that runs the checks.
2. **Runs** it on `docker` and on `vagrant`, each through the **Isothrum CLI** (`isothrum up --wait`, `check`, `down`), found on `PATH` or given with `--runner`. There is no Cargo dependency on Isothrum, so the dependency between the repositories stays one-way.
3. **Compares:** it collects each check's result and each fact's output per form. A case passes when every check passes in both forms and every fact is equal.
4. **Records** the outcome in `catalogue/conformance.json`: entry, release, OS, form, CM tool, provider, runner version, host, date and result. The generated catalogue page and `isoloom matrix` read it.

**Scope:**
- **The exit run (PLAN):** each entry's default release on its first OS, in both forms, on Docker and on Vagrant + VirtualBox. That's this Mac now, and libvirt on jarvis once it's back. The VM form runs under the default CM tool **and** under Ansible.
- **Weekly** (`check-all --deep`): every release × OS × form × CM tool.
- **Windows entries,** which have no container form, compare the VM form under PowerShell and under Ansible.
- **User labs** don't get the contract checks. Labs keep their own checks, and readiness comes from the entry's ports and `health`.

### 4.7 Role sourcing and licences

| Content | Licence | Lives in | Generated output uses it as |
|---|---|---|---|
| `catalogue/**`: recipes, files, contracts, the daemon table | Apache-2.0, Isoloom's own | the repository; embedded in the binary | rendered into the generated files |
| rendered roles (`isoloom_<entry>`, `isoloom_machine_<m>`) | generated from Isoloom's own | the generated project | roles that `ansible-playbook` runs |
| `ansible.builtin` | GPL-3.0 (ansible-core) | the controller's Ansible | module names |
| `ansible.posix`, `ansible.windows` | GPL-3.0 | Galaxy | module names in tasks. Pinned in `catalogue/collections.yml`, copied into the generated `requirements.yml`, and installed by `ansible-galaxy` on the controller at provisioning, as `provision:` already does |
| vendor collections (`microsoft.ad`, `nginxinc.nginx_core`) | GPL or Apache | Galaxy | **not used in 1.0** (Q8): a vendor role would be a second implementation beside the recipe the other CM tools need anyway |
| `community.*` and personal roles | | | **never** (the maintainer's rule) |

The rule rules out some modules, each with a replacement:
- **Alpine packages** (`community.general.apk`): a guarded `command`.
- **ufw:** a guarded `command`.
- **The Windows firewall** (`community.windows`): `ansible.windows.win_powershell`.
- **Database users and grants** (`community.mysql`, `community.postgresql`): guarded `command`s calling the database client, the same calls the shell library makes.

A test asserts that no file under `catalogue/` is third-party: there's no `vendor/`, and every script carries `SPDX-License-Identifier: Apache-2.0`.

### 4.8 Embedding and the refresh

**Embedding:**
- `isoloom-catalogue/build.rs` (salvaged) walks `catalogue/` and generates a table of `(path, include_str!)` pairs. Only text files are allowed; a test refuses binaries.
- `catalogue()` loads and checks the catalogue once. A broken entry fails the crate's tests, never a user's spec.
- The budget: the catalogue stays under 1 MB.

**The refresh,** `cargo xtask catalogue refresh [<entry>] [--check] [--propose]`, is an xtask rather than a script, so it runs on every host. It:
1. resolves each image tag to its multi-platform digest (through the registry API, with anonymous tokens) and records the platforms (`amd64`, `arm64`);
2. reads each vendor repository's index for each OS, to confirm the pinned version and record its exact package string;
3. downloads keys and archives, checks each GPG key against the entry's `fingerprint`, and records SHA-256s;
4. records each image's own entrypoint and command, for `image.start: run`;
5. pins `collections.yml` (version and archive SHA-256, from Galaxy) and the OS table's base images;
6. with `--propose`, lists newer upstream releases without adding them.

It writes only `lock.yml`, `collections.yml` and the OS table. With `--check`, it exits 1 on drift and 0 otherwise, the drift-job contract. It is the only code that touches the network: tests and generation stay offline.

### 4.9 How `service:` resolves in lowering

`isoloom-plan::lower(&syntax::Spec, &Catalogue, Target)` resolves a machine with `service:` in eight steps:

1. **Entry:** the name exists; otherwise `unknown-reference`, with a did-you-mean.
2. **Release:** the version matches (§4.3); otherwise `invalid-value`, listing the releases.
3. **OS:** the release supports the machine's `os`; otherwise `invalid-value`, listing its OSes.
4. **Settings:** `with:` parses against the schema into typed `Settings`. It fails on:
   - an unknown key (with a did-you-mean);
   - a wrong type, or a missing `required` setting;
   - `{ secret }` on a setting that isn't a `password`;
   - a project path that doesn't exist;
   - an `endpoint` machine that shares no network.
5. **Recipe:** `when:` and `for_each:` applied, placeholders resolved, templates rendered and `lock.yml` versions filled in, giving one `plan::configure::Recipe`.
6. **Forms, for the target:**
   - **container:** `Image { reference@digest }` (plus a thin build when `configure` has steps), or `Built { recipe }`;
   - **VM:** the OS's VM image and `Step::Configure(recipe)`.
7. **Merge with the machine:**
   - The entry contributes:
     - its ports, keyed by name; the machine may `publish` them but not change `port`;
     - its data volumes;
     - the value files of its `password` settings;
     - its environment (image form);
     - its health and its daemons.
   - The machine's own `files`, `volumes`, `environment` and `healthcheck` add to these. The same path or name twice is a `conflict`.
8. **Origin:** `Origin::Service { entry, version, source, form: Image | Built }`, in `resolved.json` and the run manifest.

**Refusals** use the same three-line format and the `refused` code, prefixed by the target:

| When | Field | Reason | Fix |
|---|---|---|---|
| A list of services on a container target | `service` | one catalogue service is one image (approved) | one machine per service |
| The entry has no container form for the OS (Windows entries) | `service` | `iis` runs on Windows Server VMs only | a VM target |
| The release's image lacks the machine's architecture | `hardware.arch` | `<entry> <version>` publishes no `arm64` image | another release, or `amd64` |
| A `start` step that needs a reboot, on a container target | `service` | a container can't reboot | a VM target |
| Vagrant with cloud-init | `--configure` | §2.4 | `shell` or `ansible` |

Spec-level errors stay errors (approved §9): `service` beside `app`, `command` on a service machine, `ports.<name>.port` set on a service machine.

---

## 5. The first entries

### 5.1 What the corpus uses

These are counts over the 376 lab specs and the build folders they name. The unit is labs, and a lab may count in several rows. Every one of these is an `app.build` or `app.image` today, so the catalogue gives the Phase 7 rewrite a shorter form, not a requirement.

| Service | Labs | How |
|---|---|---|
| mysql, mariadb | 56 | 84 database Dockerfiles (counting postgres and mongo too), mostly 2 to 6 lines: `FROM`, `ENV MYSQL_ROOT_PASSWORD` and `MYSQL_DATABASE`, and a `COPY` of an SQL file. MySQL 5.7 in 34 of 51 `FROM mysql` lines; MariaDB 10.6 or 10.11 in 14 of 16 `FROM mariadb` lines |
| apache | 52 | 35 `FROM php:*-apache` (mod_php); the rest `httpd` or the `apache2` package |
| mongodb | 14 | official `mongo` images, and builds |
| postgres | 9 | official images |
| nginx | 6 | `FROM nginx`, or the package |
| wordpress | 5 | |
| openssh | 3, plus 12 labs that publish port 22, mostly on VMs | `openssh-server` in a build |
| php-fpm | 3 | |
| dvwa, juice-shop, redis, iis | 2 each | one dvwa lab uses an unofficial image; one juice-shop lab uses the project's image |
| ad-ds | at least 5 build a forest; 14 Windows VM labs in all | the GOAD family runs its own Ansible from the controller and keeps `provision:` |
| rabbitmq | 0 | |
| several daemons in one container | 6 | supervisord or `service … start`, in the labs' own Dockerfiles |
| single-process jails (xinetd, socat, kCTF) | 29 | not catalogue material |

**Two findings:**
- **Database machines are the biggest win:** thin Dockerfiles become `service:` plus `with: { database, init }`.
- **The corpus wants old series** (MySQL 5.7, MariaDB 10.6). `distro` releases on the OS that shipped them provide those honestly (§5.2).

### 5.2 Wave 1

| Entry | OS and container source | VM source | Settings | Contract checks |
|---|---|---|---|---|
| **openssh** | `distro` on debian@12, debian@13, ubuntu@22.04, ubuntu@24.04, rocky@9 and alpine@3.24; always built, as there's no official image | the OS's OpenSSH, configured through `sshd_config.d/isoloom.conf`. On VMs, port 22 stays open for management beside `port` | `port` (22), `password_authentication` (true), `permit_root_login` (`prohibit-password`), `users` (a list of `name`, `password`, `groups`, `shell`, `authorized_keys` file, `sudo`), `banner` (file) | the port answers `SSH-2.0-OpenSSH_<series>`; `sshd -T` shows the settings; a listed user logs in with its password (a script from `probe`); host keys differ between two containers (generated in `start`, never baked in) |
| **nginx** | `vendor` (nginx.org): 1.30 stable and 1.31 mainline. Official image on debian@13 and alpine@3.24; built on debian@12, ubuntu@24.04 and rocky@9 | nginx.org's repository | `port` (80), `root` (folder), `config` (file), `php_fpm` (endpoint) | the port answers; `nginx -v` reports the version; `root`'s index is served; a `.php` file runs through a php-fpm helper (case `php`). Facts: os-release, `nginx -V` |
| **apache** | `distro` 2.4 on debian@12, debian@13, ubuntu@24.04, rocky@9 and alpine@3.24; built, because the official `httpd` image compiles from source under `/usr/local/apache2` with no VM equivalent | the distro's `apache2` or `httpd` | `port` (80), `root`, `config` (a site file), `modules` (an enum list: rewrite, headers, ssl, cgi…), `php` (`none`, or `module`: mod_php from the distro; on Rocky, php-fpm with `proxy_fcgi`, so two daemons) | the port answers; `apachectl -v` reports the series; `root` is served; with `php: module`, a fixture (`<?php echo 6*7;`) returns 42; the list of enabled modules is equal (fact) |
| **php-fpm** | `distro`: debian@12 (8.2), debian@13 (8.4), ubuntu@24.04 (8.3), rocky@9 (AppStream's PHP streams), alpine@3.24. Built, because the official `php` image compiles PHP with a different layout and set of extensions | the distro's PHP | `port` (9000), `root` (folder, at `/var/www/html`), `extensions` (an enum list, mapped to each family's package names), `ini` (file), `clients` (a list of machines, for `listen.allowed_clients`) | the port answers; `php-fpm -v` reports the series; a FastCGI request from `probe` runs a fixture; `php -m` lists the extensions (fact) |
| **mariadb** | `vendor` MariaDB 11.4 and 11.8 LTS: official image on ubuntu@24.04 (noble); built on debian@12 and rocky@9. `distro` 10.11 on debian@12, 11.8 on debian@13 and 10.6 on ubuntu@22.04 (built) | MariaDB's repository, or the distro | `port` (3306), `root_password` (password), `database`, `user`, `password`, `init` (a folder of `.sql` and `.sh` files, run once in name order), `config` (file) | the port answers from `probe` (bound to 0.0.0.0, as in the image); `mariadbd --version`; `user` logs into `database`; root needs its password; an `init` table exists (case `init`) |
| **mysql** | `vendor` 8.4 LTS on debian@12, ubuntu@24.04 and rocky@9; built, since the official image is Oracle Linux, which the OS table doesn't have. `distro` 8.0 on ubuntu@22.04 and 5.7 on ubuntu@18.04 | repo.mysql.com, or the distro | as mariadb | as mariadb, with `mysqld --version` |
| **postgres** | `vendor` (PGDG) 16, 17 and 18: official images on debian@12, debian@13 and alpine@3.24; built on ubuntu@24.04 and rocky@9 | PGDG and its archive | `port` (5432), `database`, `user`, `password`, `init` (folder), `config` (file) | the port answers; `postgres --version`; `user` logs in with its password (scram); `listen_addresses` is `*`; `init` ran. Facts: the data path and the version |
| **dvwa** | `download` of a pinned DVWA release on debian@12, debian@13 and ubuntu@24.04. Built, because the project's image has no version tags and needs a separate database machine. Two daemons (mariadb and apache) under the runner | the distro's apache, mod_php and mariadb, plus the release | `port` (80), `security_level` (low, medium, high or impossible; default low), `admin_password` (default: DVWA's own), `allow_url_include` (bool) | `/login.php` names the application; the admin logs in (a script); the database has its tables; the security level applies |
| **juice-shop** | `download` of a pinned release on debian@12: the project's image (`bkimminich/juice-shop`), which is distroless Debian 12 | Node.js from nodejs.org (the image's major version, SHA-256 pinned) plus the release's prebuilt archive, a `juice-shop` user and a defined service | `port` (3000), `config` (one of the bundled configurations), `custom_config` (file) | `/` answers; `/rest/admin/application-version` returns the version; `/api/Challenges` lists the challenges. HTTP checks only: the image has no shell, so there are no `exec` checks |

### 5.3 Wave 2

| Entry | OS and container source | VM source | Settings | Contract checks |
|---|---|---|---|---|
| **wordpress** | `download` of the wordpress.org release on debian@12 and debian@13. The official image (`wordpress:<v>-php<x.y>-apache`) where its Debian release and PHP series match the OS, which the refresh checks; built otherwise | the distro's apache and PHP, plus the release | `port` (80), `database` (endpoint, required: a `mariadb` or `mysql` machine, as the official image expects), `db_name`, `db_user`, `db_password`, `table_prefix`, `content` (a folder added to `wp-content`) | the install page answers with no database error (against a `mariadb` helper); `wp-includes/version.php` reports the version |
| **mongodb** | `vendor` 7.0 and 8.0: official images on ubuntu@22.04 and ubuntu@24.04; built on debian@12 and rocky@9 | repo.mongodb.org | `port` (27017), `root_user`, `root_password`, `database`, `init` (a folder of `.js` and `.sh` files) | the port answers; `mongod --version`; an authenticated `ping`; an `init` collection exists. It needs AVX on x86-64, noted on the catalogue page. Licence: SSPL |
| **redis** | `vendor` (packages.redis.io): official images on debian@12 and alpine@3.24; built on ubuntu@24.04 and rocky@9 | packages.redis.io | `port` (6379), `password`, `appendonly` (bool), `config` (file). `start` writes the password into an include file: the image has no `_FILE` variable, so the entry uses `image.start: run` | `redis-server --version`; `PING` with the password answers `PONG`, and without it is refused. The licence is recorded per release: BSD for 7.2; RSAL or SSPL from 7.4; 8 adds AGPL |
| **rabbitmq** | `vendor` (Team RabbitMQ) 4.x: official image on ubuntu@24.04; built on debian@12 and rocky@9 | Team RabbitMQ's repositories (server and Erlang) | `port` (5672), `management` (bool: the plugin, plus a `management` port with protocol `http`), `user`, `password` (set in `start`), `vhost` | `rabbitmq-diagnostics server_version`; the user authenticates (the management API, from `probe`); the vhost exists |
| **iis** | windows-server@2016 to @2025, VM only | Windows features | `port` (80), `root` (folder, into `C:\inetpub\wwwroot`), `features` (an enum list: asp-net-45, cgi, windows-auth, basic-auth) | the port answers through Windows Firewall; the root's index is served; the features are installed (`Get-WindowsFeature`) |
| **ad-ds** | windows-server@2016 to @2025, VM only | Windows features. `start` promotes the forest (`Install-ADDSForest`, guarded by `Get-ADDomain`, then `reboot`) and creates the users after it | `domain` (FQDN, required), `netbios`, `dsrm_password` (required), `users` (a list of `name`, `password`, `groups`), `groups` (a list), `dns_forwarders` | ports 53, 88, 389 and 445 answer; `(Get-ADDomain).DNSRoot` is the domain; `_ldap._tcp.<domain>` resolves from `probe`; a listed user binds over LDAP |

---

## 6. Crates and phasing

### 6.1 Where the code lives

| Crate (group, rank) | What Phase 4 adds | Depends on |
|---|---|---|
| `isoloom-spec` (core 0) | the eight step kinds, `Guard`, `Package { name, version }`, the shared placeholder grammar | — |
| `isoloom-catalogue` (core 1, salvaged) | `Entry`, `Contract`, typed `Settings` for `with:`; releases and version matching; `when` and `for_each`; the daemon table; `build.rs` embedding; the entry checks; the generated entry and contract schemas | spec |
| `isoloom-plan` (core 2) | `configure`: the resolved `Recipe`, `ContainerProcess`, `Configure` and `Handoff`. Also `lower` resolving `service:` and `with:`, the lifecycle's `Step::Configure`, `Start` and `Services`, and the bootstrap/configuration split | spec, catalogue |
| `isoloom-render` (core 3) | `configure::{shell, powershell, dockerfile, cloud_init, runner}`: the shell and PowerShell libraries and the recipe scripts; the Dockerfile (today's `dockerfile.rs` moves in); the cloud-config document, moved from `isoloom-terraform/machine/cloud_init.rs` so Terraform keeps only its HCL wrapping; the container runner | spec, catalogue, plan |
| `isoloom-ansible` (backend) | the Ansible CM tool: `site.yml`, the rendered roles, `requirements.yml`, and the controller's run of them | core |
| `isoloom-compose`, `isoloom-kubernetes` (backends) | `init: true` and `shareProcessNamespace`; the runner as entrypoint | core |
| `isoloom-vagrant`, `isoloom-terraform` (backends) | the bootstrap/configuration split, and the `Handoff` invocations (the controller step, the manifest step) | core |
| `isoloom-targets` | `--configure`; composing an IaC backend's files with the CM backend's; catalogue rows in the matrix | backends |
| `xtask` | `catalogue refresh`, `catalogue check`, and the resource conformance tests (`--deep`) | anything; nothing depends on it |

**The rule behind the split:**
- **Core:** a CM renderer whose output an IaC file embeds or builds from: shell, PowerShell, Dockerfile, cloud-init. It must be core, because a backend may not depend on another backend.
- **Backend:** a CM tool whose output stands alone and is run by path: Ansible now; Puppet, Chef, Salt and DSC in 1.x. IaC backends reach it only through the plan's `Handoff`, and `isoloom-targets` composes the two sets of files.
- **No change to the dependency rule:** `xtask/src/deps.rs` already places `isoloom-catalogue` at rank 1.

### 6.2 Units, order and parallelism

| # | Unit | Size | Needs | Runs in parallel with |
|---|---|---|---|---|
| 4.1 | **The model:** the step syntax, schema and errors; the `plan::configure` types; lifting today's refusal of `copy` and `packages` on VMs; the daemon-package warning | M | the Phase 2 spec | nothing: spec and plan core are serial |
| 4.2 | **The floor renderers:** the shell and PowerShell libraries, the Dockerfile, the runner, `init: true`; the resource conformance tests | M | 4.1 | 4.3 |
| 4.3 | **The catalogue engine:** the salvaged `isoloom-catalogue` moved to the new format, `lock.yml`, settings, the daemon table, `refresh`; `lower` resolving `service:` | M | 4.1 | 4.2 |
| 4.4 | **cloud-init:** the document moves to render; the recipe goes in `runcmd`; Terraform wraps it | S | 4.2 | 4.5, 4.6 |
| 4.5 | **Ansible:** `site.yml`, the roles, `requirements.yml`; the bootstrap/configuration split in Vagrant and Terraform; the controller and manifest hand-offs | M | 4.2 | 4.4, 4.6 |
| 4.6 | **Conformance:** `catalogue check` through the Isothrum CLI, `conformance.json`, the generated catalogue page and the matrix rows | S–M | 4.3, and Isothrum's Docker and Vagrant drivers (Phase 3) | 4.4, 4.5 |
| 4.7 | **Wave 1 entries,** one worker per entry, each in its own `catalogue/<entry>/` folder | L in all, S each | 4.2, 4.3, 4.6 | each other |
| 4.8 | **Wave 2 entries;** the Windows ones need 4.5 for their Ansible leg | M | 4.7's pattern | each other |
| 4.9 | **Follow-through:** the examples on `service:` (air-gapped, dual-stack, edge-firewall, pivot-dmz, vlan-office), Isothrum's editor forms from the generated schema, the phase's review pass | S | 4.7 | — |

- **Overlap with Phase 3:** units 4.1 to 4.3 can start while Phase 3's backend and runner work runs, because they touch different crates. The entries' real VM parity runs need Phase 3's Vagrant driver in the runner.
- **Shared files:** the ones several entry workers would touch (`conformance.json`, the OS table) are either generated or owned by the 4.3 worker.

### 6.3 Tests and gates

- **Unit tests:** every step's parsing (proptest for the new newtypes), version matching, settings typing, `when` and `for_each`, template rendering and the refusals.
- **Snapshots:** every entry's default release, on its first OS, under each CM tool, plus the three examples; render-twice determinism.
- **`verify`:** hadolint (Dockerfiles), shellcheck (the library, the recipe scripts, the runner), the cloud-init schema, ansible-lint (the site and the roles), PSScriptAnalyzer (the PowerShell library). Every one is already in the pinned validator image.
- **Resource conformance** (`check-all --deep`): each step kind × renderer × Linux family, applied twice in the family's container image. The first apply reaches the state, the second changes nothing, and the shell library and Ansible must leave the same observable state. `service` and `firewall` are covered by the VM runs, since a test container has no init.
- **Parity** (§4.6) for the Phase 4 exit. Quoting stays fuzzed.

---

## 7. Open questions

1. **Services in containers (§3):**
   - one daemon runs as the foreground process (A);
   - several run under the engine's tini and a generated runner that fails fast (B1);
   - refusal only when there's no foreground form;
   - s6-overlay is revisited in 1.x if per-daemon restart is ever needed.

   *Recommended.*
2. **A `start` phase on `app`.** Entries have one and apps don't: approved steps have no secrets. Should `app.start` exist, for app recipes that need a secret at start, such as a user's password? *Recommended: not in 1.0.* The corpus's entrypoint wrappers live in the labs' own Dockerfiles, and entries cover the database and SSH cases.
3. **Several catalogue services on one container.** Should the approved refusal of `service: [a, b]` on container targets stay? *Recommended: keep it.*
   - Two entries can be two images: nginx's official image beside a built php-fpm.
   - Two machines is the container idiom.
   - A one-machine stack is an `app` recipe, or an all-in-one entry like dvwa.
4. **The official image only when its OS is the machine's.** This leaves the official mysql image (Oracle Linux) and mariadb image (Ubuntu) unused on Debian. *Recommended: keep the rule.* The alternative makes `os:` a claim the contract can't confirm. Adding `oraclelinux@9` to the OS table would be a later, separate decision.
5. **Version prefixes.** `postgres@16` resolves to the newest 16.x the installed Isoloom lists, and `resolved.json` records it. *Recommended.* The alternative, exact versions only, makes every spec change on each refresh.
6. **Choosing the CM tool.** A generate option (`--configure`) defaulting to today's carriers, plus `overrides.<target>.configure` for a lab that needs one tool. *Recommended.* The tool is a "how", so it isn't a machine field, but a lab with heavy `playbook` set-up may pin Ansible.
7. **Old, vulnerable versions.** The corpus wants MySQL 5.7 and old MariaDB. *Recommended:*
   - `distro` releases on the OS that shipped them: 5.7 on ubuntu@18.04, MariaDB 10.6 on ubuntu@22.04;
   - never unmaintained third-party images;
   - deliberately vulnerable builds (vulhub's 51 images) stay `app.image`.
8. **Vendor Galaxy content** (`microsoft.ad` for ad-ds, `nginxinc.nginx_core`). *Recommended: not in 1.0.* One recipe per entry, rendered by every tool, is what keeps parity by construction, and `ansible.windows` covers ad-ds through PowerShell.
9. **Where Ansible runs.** From the controller over SSH and WinRM (*recommended*), or with `ansible-playbook -c local` in each Linux VM? The second needs an Ansible install in every VM and can't reach Windows.
10. **The order of wave 1.** The corpus has mongodb in 14 labs, and dvwa and juice-shop in 2 each. *Recommended:* promote mongodb into wave 1, after mariadb and mysql. Keep dvwa and juice-shop, which are training staples beyond the corpus.
11. **`roles/` becomes `files/`** in the catalogue layout, since roles are rendered, not written. *Recommended.*
12. **Redis's licence.** Releases from 7.4 on aren't OSI open source (RSAL or SSPL; 8 adds AGPL). Should the catalogue offer `redis` with its licence recorded, and add `valkey` (BSD, Linux Foundation) beside it? *Recommended: both,* with `valkey` in wave 2 if the maintainer wants an open-source default.
13. **The conformance VM provider for the exit.** *Recommended:* Vagrant + VirtualBox on this Mac now (VirtualBox already runs the Windows test VM here), then libvirt on jarvis when it's back.
