# Isoloom API migration: the spec switch (`e9a9dd66`) and the contract follow-up (`7b93fef6`)

From the spec unit, for every consumer (Isothrum's runner and compiler, the launcher, Isoloom's own crates).

1. **The spec types:** `isoloom_spec::Spec` is now the authored spec (`syntax::Spec`). The internal model is `isoloom_spec::model::Spec`.
2. **Loading:** `parse`, `load` and `load_profiled` keep their names and return the authored spec.
   - An invalid spec gives `Err(LoadError::Invalid { file, problems })`.
   - Each `syntax::Problem` has a code, path, span, reason and fix, plus `.line()` and `.render(file)`.
   - Old-format YAML (`docker:`, `vm:`, port lists, a machine's `inputs:`) is refused, never read.
3. **Validation:** `isoloom_spec::validate(&spec)` becomes `isoloom_targets::Lab::new(spec).problems()`, or `.problems_with_files(dir)`.
   - `isoloom_plan::problems_in(yaml)` and `Lab::parse(yaml)` cover the parse stage too.
   - Paths are as the author wrote them (`machines.web.app.steps[0]`).
4. **Generation:** `generate`, `generate_all`, `generate_instance`, `refusal`, `refusal_on`, `cloud_refusals`, `resolved::*` and `render_message*` keep their names and take `&Lab`. `lab.possible()` lists the targets, and `lab.refusal(t)` gives the field that rules a target out.
5. **The environment:** `Environment::build(&model, t, None)` is unchanged. The model comes from `lab.for_target(t)`, or from `lab.spec()` lab-wide.
6. **Writing a spec** (Isothrum's compiler):
   - `os: debian@12`
   - `app: { image | build | appliance, steps: [{ run }, { copy }, { packages }, { playbook }, reboot] }`
   - `ports: { <name>: <port> | { port, protocol: tcp|udp|http|https|tls, publish } }`
   - `files: [{ path, content | source | secret | input }]`
   - `setup: [..]`
   - `hardware: { arch, tpm, boot, ... }`
   - target-specific settings go under `overrides.<target>`.

   `docs/spec.md` and `isoloom schema` are the reference.
7. **Run manifest v0, published ports:**
   - `Placeholder::Published { machine, port: Name }` replaces the port number and protocol;
   - `${published.web.site}` replaces `${published.web.80[.udp]}`;
   - an output's source is `{ kind: published_port, machine, port: <name> }`;
   - every machine port needs a unique `name`.
8. **Run manifest secrets:** they are delivered to `up` and to `start`, because Compose re-reads the variable on every start. Every input slot has `default: ""` (`SecretSlot.default`), so the runner delivers `""` whenever it has no value. An input declared `sensitive: false` keeps that flag (`model::Spec::plain_inputs`).
9. **`resolved.json` is version 3:**
   - a machine's `ports` is a map keyed by name (`port`, `protocol`, `publish`, `published`, `url`);
   - `published[]` entries carry `name` and `protocol`;
   - message placeholders are `{{ machines.<m>.ports.<name>.published|port|url }}`.
10. **Labels and outputs:**
    - the Compose label is `isoloom.port.<name>`;
    - Terraform's published outputs are keyed `<machine>/<name>`;
    - the minimum Compose version is 2.17.0.

## Phase 1c, `isoloom-spec` (the 1c-spec unit)

11. **One error for the crate, `isoloom_spec::Error`** (and `isoloom_spec::Result<T>`):
    - `LoadError` is `Error` (`Error::Invalid { file, problems }`, `NotFound`, `Ambiguous`, `Read` as before); `Stage` and `LoadError::Expand` are gone (an override that doesn't apply is `Error::SetShape` / `SetKey`, YAML that doesn't read `Error::Yaml`).
    - `os::OsError` and `syntax::Invalid` are gone: `os::resolve`, `OsRef`, `Mode` and the name types' `FromStr` return `Error` (`Error::UnknownOs`, `UnknownOsVersion`, `Mode`, `Name`).
    - What returned `Result<_, String>` returns `Result<_, Error>`, each case its own variant with the same message: `profile::{path, load, parse}` and `Profile::pin` (`Error::Profile*`), `defaults::{resolve, parse_set, override_spec, override_spec_with}` (`Error::Layer`, `Defaults`, `Set*`), `instance::apply` (`Error::Instance*`), `images::Table::parse` (`Error::Images`). `.to_string()` gives the old text.
    - `Profile::check` returns `Vec<profile::ProfileProblem>` (`Display` is the old line); `Error::ProfileMismatch { problems }` replaces `LoadError::Profile`.
12. **The environment is passed in** (`host::Host`, from `std::env::vars_os()` at the program's edge):
    - `defaults::load(&defaults::Sources { project, images_file, profile, sets, host })` replaces `defaults::load` and `defaults::load_profiled`;
    - `defaults::user_file(&host)`; `home::home()` is `host.home()`.
13. **`syntax::Healthcheck`** holds one `probe: HealthProbe::{Test(CommandLine), Http(HealthPage)}`; both or neither is refused when the spec is parsed (`invalid-value`, or `conflict` for both).
14. **Gone, unused outside the crate:** `addresses`, `model::from_yaml` (now the crate's own test helper), `os::{fill_vm, canonical}`, `groups::expand_shared` (`groups::expand` is the authored spec's), `model::Ipv6Addresses::is_static`.
15. **`model::SecretMount.mode`** is an `Option<syntax::Mode>` (`.bits()` gives the `u32`), written `"0440"` as before.
16. **Changed shape, same meaning:** `clones::expand`, `groups::expand` and `vlans::flatten` refuse with a placed `syntax::Problem` (was a `"path: reason"` string); `syntax::NoForm::refusal(machine)` is the `refused` problem a target reports.
17. **A network block's text is written in one place:** `validate::Cidr` and `validate::Cidr6` implement `Display` (`10.20.0.0/24`, `fd00:20::/64`) and `FromStr` (`Error::Cidr`, `Error::Cidr6`); `Cidr6::text()` is `to_string()`.

## Phase 1c, `isoloom-manifest` (the 1c-spec unit)

18. **One error for the crate, `isoloom_manifest::Error`** (was `ManifestError`), with `isoloom_manifest::Result<T>`. Variants and `code()` are unchanged, plus `Error::PortProtocol` (`port_protocol`). Each `X::parse(&str)` has `FromStr` and `TryFrom<String>` beside it.
19. **`PortName`** (`[a-z][a-z0-9-]{0,30}`, the spec's rule) is the type of `Port.name`, `OutputSource::PublishedPort.port` and `Placeholder::Published.port` (was `Name`). A port's name has no dot, so `${published.a.b.c}` reads one way only: machine `a.b`, port `c`. `${published.<machine>.80}` and capitals or `_` in a port's name are refused (`unknown_placeholder`); the schema's template pattern says the same.
20. **`Port.kind: PortKind`** (`Tcp`, `Udp`, `Http`, `Https`, `Tls`, as the spec's `protocol:`) replaces `Port.protocol`, `Port.http` and `Port.tls`. `kind.transport()`, `kind.http()` and `kind.tls()` give the old values, and `PortKind::from_parts(protocol, http, tls)` builds one. The JSON is unchanged (`protocol`, `http`, `tls`); a UDP port with `http` or `tls` is refused (`port_protocol`). `Port`, `PortKind`, `Protocol`, `Publish` and `PublishMode` live in the crate's `port` module, re-exported at the root as before.

## Phase 3, Server mode on ESXi (the server unit)

21. **Run manifest v0, additive (Server mode on ESXi):** existing manifests and the runner's reading of them don't change.
    - `Step.provider_timeout_s: { <provider>: seconds }` replaces `timeout_s` on the Vagrant providers it lists, and only there. The vagrant target's `lifecycle.status` sets `vmware_esxi` to 60 s plus 30 s per VM, because the plugin asks the ESXi host once per VM. The runner picks it from the run's provider (`Scope`).
    - `Placeholder::Host(machine)`, written `${host.<machine>}`, is where the machine's published ports answer, as the driver reports it. A forwarded port answers at this computer's loopback. On a provider that forwards nothing (`vmware_esxi`) it is the VM's own address, with the guest port as the published one. An unknown machine is refused (`unknown_machine`), and the machine part is a machine `Name`. The vagrant target's outputs are `http://${host.<machine>}:${value}/` (`isoloom_render::run_manifest::outputs_on_their_hosts`). Compose and Proxmox outputs keep their fixed host.

## Phase 1c, `isoloom-render` and `isoloom-plan` (the core2 unit)

None of this reaches the launcher or Isothrum, which read `isoloom-targets` (its `GenerateError`, `generate*`, `exec_runner` and the rest keep their names). For crates that use render or plan directly:

22. **One error for render, `isoloom_render::Error`** (and `isoloom_render::Result<T>`): `Refused { target, refusal }` and `Unsupported { target, what }`, with `target()` and `reason()`. `backend::RenderError` is gone; `GenerateError` (`NotPossible`, `NoGenerator`) is `isoloom-targets`' own, with `From<isoloom_render::Error>`. `isoloom_targets::RenderError` names `isoloom_render::Error`.
23. **Render's root holds no helpers:** the plan's addresses come from `isoloom_plan::addressing` (`address`, `netmask`, `names_of`, `start_order`, ...); `ansible_runs`/`provision_script` are `controller::provision_script`; `rb`/`ps` are `ruby::quote`/`powershell::quote` (`UNTAR_PS1` is `powershell::UNTAR`); `hcl()` is gone (write strings through `hcl_builder`); the controller's size is `isoloom_plan::Controller::of`; `machine_setup`, `disks` and `machine_files` live under `steps` (crate-private); `ignore` is `project::ignore`, pure (`Rules::read` and `entries` are gone: the caller reads and walks the project).
24. **Typed arguments:** `boot::at_boot` takes a `boot::Trigger` (was `on_link_up: bool`); `project::lists` takes a `project::Copy`; `trunks::trunks` is `trunks::containers` (beside `trunks::vms`), a trunk's VLANs are `trunks::Vlan { network, id, octet }`, and `trunks::mac` takes a `trunks::End`.
25. **Plan:** a machine's `before` scripts are gone (`Step::Before`, `Process.before`), as the spec's design says. `Controller` has flat `cpus`, `memory_mb`, `disk_gb`, with `Controller::of(spec)` and `Controller::needed(spec)`. What only the plan reads is crate-private (`tag_of`, `cloud_routes`, `with_domain`, `mixed_shapes`, `container_checks`, `generated_dockerfile`).

## Phase 4.1, the configuration model (the 4.1 unit)

26. **Phase 4.1, the configuration model** (for Isothrum's compiler and anyone matching on steps):
    - `syntax::Step` gains `Repository`, `File`, `Template`, `User`, `Group`, `Service`, `Command` and `Firewall` (it is `#[non_exhaustive]`: keep a `_` arm). `Step::kind()` names a step's key.
    - Step paths and names are newtypes with `as_str()` and `Display`: `Run { script: ProjectPath, creates: Option<AbsPath> }`, `Playbook(ProjectPath)`, `Copy { from: ProjectPath, to: AbsPath, owner: Option<UserName>, mode }`. Isothrum's `is_ps1(script)` becomes `is_ps1(script.as_str())`.
    - `Step::Packages(Vec<Package>)`: each `Package { name: PackageName, version: Option<PackageVersion> }`.
    - `syntax::placeholder` reads `{{ path }}` (the message's, a template's, an entry's) into a `Reference`; `syntax::placeholder_problems` resolves them against a spec. `syntax::Warning` is a warning's shape (path, line, reason, fix), and `Lab::warnings()` lists them.
    - `isoloom_plan::configure` is the resolved model: `recipe()` gives a machine's `Recipe` (or its `Unportable` steps), `ContainerProcess::of` what its container runs, `fill` a template's text, `Configure` and `Handoff` the CM tools. `isoloom_plan::Error` is the plan's one error type.

## Phase 4.2, the floor renderers (the 4.2 unit)

27. **The recipe travels on the plan:** lowering hands a machine's configuration to the plan as `model::Machine.configuration` (`model::Configuration { os, install, configure, start, files }`, steps still in the authored types); `Environment::build` resolves it once into `MachinePlan.recipe: Option<configure::Recipe>` (`Recipe::of`), which every CM renderer reads. `model::Generated` loses `steps`; `Build::Generated` is `{ dockerfile, base, process: ContainerProcess }` (`ContainerProcess::resolve` and `is_runner()`). `Recipe` gains `os` and `files` (template texts by their `from`).

## Phase 1c, `isoloom-spec` follow-ups (the 1c-spec unit)

28. **A lowered spec's problem has a code:** `isoloom_spec::Problem` (`validate::Problem`) carries `code: syntax::Code` (`UnknownReference` for a name that names nothing, else `InvalidValue`), and `Problem::new(code, at, message)` takes it first. `groups::problems` returns these problems instead of `(path, message)` pairs. The plan places them with their own code instead of reading their wording.
29. **`model::Machine.before` is gone**, with its checks: the authored spec has no `before`, and lowering always left it empty.

## Phase 4.3, the catalogue engine (the 4.3 unit)

30. **The catalogue resolves `service:`:** `isoloom_catalogue::Catalogue` (`Catalogue::embedded()`, `resolve(&syntax::Spec, machine)` → `Vec<Resolved>`) is the new core crate at rank 1. `Lab` carries it (`Lab::catalogue()`), and `isoloom_plan::lower::lower(source, catalogue, scope)` and `lower::missing_form(source, catalogue, target)` take it. `isoloom_plan::configure::Family` and `codename` are the catalogue's, re-exported; `DaemonTable::common()` is the catalogue's daemon table (`DaemonTable::from(&Catalogue)`).
31. **`model::Generated` gains `from: BaseImage`** (`Os`, or `Entry`: an entry's official image, whose container keeps its own process, `ContainerProcess::Image`, and whose recipe skips `install`). `model::Configuration::entry(os, install, configure, start, files)` builds an entry's configuration. `resolved.json` lists a service machine's `services` (`entry`, `release`, `source`, `image`).

## Phase 3, `docker-vm` (the docker-vm unit)

32. **Run manifest v0, additive (`docker-vm`):** existing manifests read the same.
    - `Step.stdin` is a `PathTemplate` (was `RelPath`): `PathTemplate::relative(&rel)` for a file beside the manifest, `PathTemplate::in_state_dir(&rel)` for one of the run's state folder; render it like any template, then join it to the manifest's folder (a state-folder path is absolute). The JSON is unchanged.
    - `Action::ProjectArchive` (`project.archive`) takes `with.root` (the project's folder, relative to the manifest's) and writes `${run.state_dir}/project.tgz`; `Action::VagrantCheckForwardedPorts` (`vagrant.check_forwarded_ports`) is new.
    - `Manifest.guest: Option<Guest>` (`Guest { machine: Name, host: Option<Template>, status: Step }`); `guest.status` is in `step_lists()` (`"guest.status"`), `guest.host` in `templates()`, and every step's `stdin` in `templates()` (`<list>[i].stdin`).
    - The `.isoloomignore` matcher is `isoloom_manifest::ignore` (`Rules::parse`, `ignored`, `brings_back`, `is_empty`, `FILE`); `isoloom_render::project::ignore` re-exports it.

33. **The floor renderers (Phase 4.2):** `isoloom_render::dockerfile` is `isoloom_render::configure::dockerfile` (`files(env)`, `render(&Configured, base, &ContainerProcess)`); `configure::{shell, powershell, runner}` render a machine's recipe, `configure::files(env)` lists what they add (the libraries, a container's runner and filled templates), and `configure::Configured::of(env, machine)` is a machine's recipe with what its templates read. The plan's lifecycle `Step::Run` gains `creates`, and `Step::Configure { resources, fresh }` is a run of the recipe's other steps between its scripts and reboots (`lifecycle::of` takes the machine's `Option<&Recipe>`). `model::VmImpl::copies` is gone. `Lab::with_templates` takes the texts of the project's templates (`Lab::template_paths()` names them), read by the caller that generates.
