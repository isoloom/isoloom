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

## Phase 1c, `isoloom-manifest` (the 1c-spec unit)

17. **One error for the crate, `isoloom_manifest::Error`** (was `ManifestError`), with `isoloom_manifest::Result<T>`. Variants and `code()` are unchanged, plus `Error::PortProtocol` (`port_protocol`). Each `X::parse(&str)` has `FromStr` and `TryFrom<String>` beside it.
18. **`PortName`** (`[a-z][a-z0-9-]{0,30}`, the spec's rule) is the type of `Port.name`, `OutputSource::PublishedPort.port` and `Placeholder::Published.port` (was `Name`). A port's name has no dot, so `${published.a.b.c}` reads one way only: machine `a.b`, port `c`. `${published.<machine>.80}` and capitals or `_` in a port's name are refused (`unknown_placeholder`); the schema's template pattern says the same.
19. **`Port.kind: PortKind`** (`Tcp`, `Udp`, `Http`, `Https`, `Tls`, as the spec's `protocol:`) replaces `Port.protocol`, `Port.http` and `Port.tls`. `kind.transport()`, `kind.http()` and `kind.tls()` give the old values, and `PortKind::from_parts(protocol, http, tls)` builds one. The JSON is unchanged (`protocol`, `http`, `tls`); a UDP port with `http` or `tls` is refused (`port_protocol`). `Port`, `PortKind`, `Protocol`, `Publish` and `PublishMode` live in the crate's `port` module, re-exported at the root as before.
