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
