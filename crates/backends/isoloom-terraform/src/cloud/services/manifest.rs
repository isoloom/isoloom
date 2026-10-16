//! The `cloud-services` run manifest (`.isoloom/cloud-services/run.json`): the lab's own
//! Terraform module, copied into the run's state folder (`terraform.copy_module`) and applied
//! there with the spec's fixed variables (`terraform.tfvars.json`, beside the manifest) and the
//! launch's values (`inputs.tfvars.json`, written by the runner into the run's state folder and
//! kept for `down`), its state the runner's. It replaces the sh-only `up.sh`, `down.sh` and
//! `outputs.sh`: the spec's `outputs` are the module's outputs, read from the state.
//!
//! - **Credentials:** the cloud's own, as connection slots on Terraform's steps only, as for
//!   every cloud run (`crate::cloud::manifest::connection`); the spec's `env` on those steps.
//! - **Each deployment's name** (`TF_VAR_isoloom_instance`) is the run's, and its end
//!   (`TF_VAR_expires_at`) the runner's, which a module may tag what it creates with.
//! - **`requires`:** each a tool the runner probes before `up`: a bare name with `--version`, a
//!   command line as its argv (no shell).
//!
//! What it can't run, it leaves out (no `run.json`): `before` steps (host scripts, which the
//! host-neutral rule refuses), and a variable holding a launch value inside a larger value (the
//! runner writes whole values only).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use isoloom_manifest::{
    Action, Argv, Checks, Condition, Delivery, Driver, EnvName, Error as ManifestError, Expiry, ExpiryUnit, HostArgv, HostOs, Identifier, Identity, Lifecycle,
    Manifest, ManifestVersion, NonEmpty, Output, OutputRole, OutputSource, Part, Placeholder, Program, Recovery, RelPath, Retries, Run, RunVar, SCHEMA_URL,
    SecretSlot, SlotKind, SnakeName, StateFileFormat, StateOwner, Step, StepId, Template, TerraformIdentity, Tool, ToolName,
};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::{argv, generator, lab, noted, tool};
use isoloom_spec::model::{CloudProvider, CloudServices};

use crate::cloud::Cloud;
use crate::cloud::manifest::connection;

/// From `.isoloom/cloud-services/` back to the project folder.
const ROOT: &str = "../..";

/// The Terraform steps, which get the credentials.
const STEPS: [&str; 3] = ["init", "apply", "destroy"];

/// The launch's values, in the run's state folder.
const INPUTS_FILE: &str = "inputs.tfvars.json";

/// The fixed variables, beside the manifest.
const VARS_FILE: &str = "terraform.tfvars.json";

/// How long a run lives when the embedder sets nothing, in minutes: the reaper's.
const DEFAULT_LIFE_MIN: u32 = 240;

/// The most applies again a manifest takes (`Retries`).
const MAX_RETRIES: u8 = 3;

/// The cloud of `provider`.
fn cloud_of(provider: CloudProvider) -> Cloud {
    match provider {
        CloudProvider::Aws => Cloud::Aws,
        CloudProvider::Azure => Cloud::Azure,
        CloudProvider::Gcp => Cloud::Gcp,
    }
}

/// Why `cloud` can't be run from a manifest, if it can't.
pub(super) fn refusal(cloud: &CloudServices) -> Option<&'static str> {
    if !cloud.before.is_empty() {
        return Some("`before` steps are host scripts, which a run manifest can't hold");
    }
    let embedded = cloud
        .vars
        .values()
        .any(|v| !CloudServices::inputs_in(v).is_empty() && CloudServices::input_of(v).is_none());
    embedded.then_some("a variable holds a launch value inside a larger value; the runner gives whole values only")
}

/// The run manifest of `env`'s cloud services; its message is left to the caller.
pub(super) fn manifest(env: &Environment, cloud: &CloudServices) -> Result<Manifest, ManifestError> {
    let on = cloud_of(cloud.provider);
    let mut secrets = connection::slots(on, &STEPS)?;
    secrets.extend(inputs(cloud)?);
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: generator()?,
        lab: lab(env)?,
        target: isoloom_manifest::Target::CloudServices,
        cloud: Some(crate::cloud::manifest::cloud_of(on)),
        driver: Driver::Terraform,
        hosts: BTreeSet::from([HostOs::Linux, HostOs::Macos, HostOs::Windows]),
        tools: tools(cloud)?,
        identity: Identity::Terraform(TerraformIdentity {
            module: RelPath::parse(".")?,
            state: StateOwner::Runner,
            var_files: vec![RelPath::parse(VARS_FILE)?],
        }),
        params: params(cloud, on)?,
        secrets,
        networks: Vec::new(),
        machines: Vec::new(),
        outputs: outputs(cloud)?,
        lifecycle: lifecycle(cloud)?,
        ready: vec![Condition::StepOk {
            step: StepId::parse("apply")?,
            means: Some("the module's services exist".into()),
        }],
        checks: Checks {
            protocol: isoloom_manifest::CheckProtocol::V1,
            setup: Vec::new(),
            runners: Vec::new(),
            teardown: Vec::new(),
        },
        recovery: Some(Recovery {
            stale_markers: Vec::new(),
            steps: Vec::new(),
            retries: Retries::default(),
            apply_retries: Some(Retries::new(cloud.apply_retries.min(MAX_RETRIES))?),
        }),
        expiry: Some(Expiry {
            tf_var: SnakeName::parse("expires_at")?,
            unit: ExpiryUnit::UnixSeconds,
            default_minutes: Some(DEFAULT_LIFE_MIN),
        }),
        guest: None,
    })
}

/// The profile, then where the module runs (`connection::placement`): a name the spec's `env`
/// sets is its own, and the `region` variable only when the spec's `vars` don't set it.
fn params(cloud: &CloudServices, on: Cloud) -> Result<Vec<isoloom_manifest::Param>, ManifestError> {
    let taken = |name: &str| cloud.env.contains_key(name);
    let mut out = connection::profile(on, &STEPS)?;
    let placement = connection::placement(on, &STEPS, &taken)?;
    out.extend(
        placement
            .into_iter()
            .filter(|p| p.name.as_str() != "region" || !cloud.vars.contains_key("region")),
    );
    Ok(out)
}

/// Terraform, then each of `requires`: a bare name probed with `--version`, a command line run
/// as its argv.
fn tools(cloud: &CloudServices) -> Result<Vec<Tool>, ManifestError> {
    let mut out = vec![tool("terraform", argv("terraform", &["version"])?)?];
    for r in &cloud.requires {
        let words: Vec<&str> = r.split_whitespace().collect();
        let Some((program, args)) = words.split_first() else { continue };
        let probe = if args.is_empty() { vec!["--version"] } else { args.to_vec() };
        let Ok(name) = ToolName::parse(&program.to_ascii_lowercase()) else { continue };
        let Ok(program) = Program::parse(program) else { continue };
        let mut t = tool(
            name.as_str(),
            HostArgv::Every(Argv::new(program, probe.iter().map(|a| Template::text(a)).collect())),
        )?;
        t.name = name;
        out.push(t);
    }
    Ok(out)
}

/// `${run.<var>}` then `after`, with `before` first.
fn run_var(before: &str, var: RunVar, after: &str) -> Template {
    Template::from_parts([Part::Text(before.into()), Part::Placeholder(Placeholder::Run(var)), Part::Text(after.into())])
}

/// A `terraform -chdir=<the module's copy>` step running `args`: its data folder in the run's
/// state folder, no prompt, the spec's `env`, the deployment's name.
fn terraform(cloud: &CloudServices, step: &str, args: Vec<Template>, note: &str) -> Result<Step, ManifestError> {
    let mut all = vec![run_var("-chdir=", RunVar::StateDir, "/module")];
    all.extend(args);
    let mut s = noted(
        isoloom_render::run_manifest::run(step, HostArgv::Every(Argv::new(Program::parse("terraform")?, all)), 3600)?,
        note,
    );
    let mut env: BTreeMap<EnvName, Template> = BTreeMap::new();
    for (k, v) in &cloud.env {
        env.insert(EnvName::parse(k)?, Template::text(v));
    }
    env.insert(EnvName::parse("TF_DATA_DIR")?, run_var("", RunVar::StateDir, "/.terraform"));
    env.insert(EnvName::parse("TF_IN_AUTOMATION")?, Template::text("1"));
    env.insert(EnvName::parse("TF_INPUT")?, Template::text("0"));
    env.insert(EnvName::parse("TF_VAR_isoloom_instance")?, Template::from(Placeholder::Run(RunVar::Name)));
    s.env = env;
    Ok(s)
}

/// The state, the fixed variables, then the launch's values when the spec takes any.
fn state_and_vars(cloud: &CloudServices) -> Vec<Template> {
    let mut out = vec![
        run_var("-state=", RunVar::StateDir, "/terraform.tfstate"),
        run_var("-var-file=", RunVar::Dir, &format!("/{VARS_FILE}")),
    ];
    if cloud.vars.values().any(|v| CloudServices::input_of(v).is_some()) {
        out.push(run_var("-var-file=", RunVar::StateDir, &format!("/{INPUTS_FILE}")));
    }
    out
}

/// The module copied (`when` it is missing, for a `down`).
fn copy(cloud: &CloudServices, when: Option<&str>) -> Result<Step, ManifestError> {
    let mut with = BTreeMap::from([(
        SnakeName::parse("from")?,
        Template::text(&format!("{ROOT}/{}", cloud.terraform.trim_end_matches('/'))),
    )]);
    if let Some(w) = when {
        with.insert(SnakeName::parse("when")?, Template::text(w));
    }
    let mut s = Step::action(StepId::parse("copy-module")?, Action::TerraformCopyModule);
    s.run = Run::Action {
        action: Action::TerraformCopyModule,
        with,
    };
    Ok(noted(s, "Copying the lab's Terraform module…"))
}

/// The lock cleared, the module copied, `init`, then `apply` (or `destroy`).
fn lifecycle(cloud: &CloudServices) -> Result<Lifecycle, ManifestError> {
    let lock = || Ok::<Step, ManifestError>(Step::action(StepId::parse("clear-lock")?, Action::TerraformClearLock));
    let words = |w: &[&str]| w.iter().map(|x| Template::text(x)).collect::<Vec<_>>();
    let init = || {
        terraform(
            cloud,
            "init",
            words(&["init", "-input=false", "-no-color"]),
            "Preparing Terraform and its providers…",
        )
    };
    let mut apply = words(&["apply", "-auto-approve", "-input=false", "-no-color"]);
    apply.extend(state_and_vars(cloud));
    let mut destroy = words(&["destroy", "-auto-approve", "-input=false", "-no-color"]);
    destroy.extend(state_and_vars(cloud));
    Ok(Lifecycle {
        up: NonEmpty::new(
            vec![
                lock()?,
                copy(cloud, None)?,
                init()?,
                terraform(cloud, "apply", apply, "Creating the lab's services…")?,
            ],
            "lifecycle.up",
        )?,
        down: NonEmpty::new(
            vec![
                lock()?,
                copy(cloud, Some("missing"))?,
                init()?,
                terraform(cloud, "destroy", destroy, "Removing the lab's services…")?,
            ],
            "lifecycle.down",
        )?,
        park: None,
        resume: Vec::new(),
        provision: None,
        seal: None,
        status: Step::action(StepId::parse("state")?, Action::TerraformReadState),
    })
}

/// Each launch value the variables take whole: the runner writes it into the launch's file,
/// under each variable's name, kept for `down`.
fn inputs(cloud: &CloudServices) -> Result<Vec<SecretSlot>, ManifestError> {
    let path = RelPath::parse(INPUTS_FILE)?;
    let mut by_input: BTreeMap<&str, Vec<Delivery>> = BTreeMap::new();
    for (var, v) in &cloud.vars {
        if let Some(input) = CloudServices::input_of(v) {
            by_input.entry(input).or_default().push(Delivery::StateFile {
                path: path.clone(),
                format: StateFileFormat::TfvarsJson,
                key: Some(var.clone()),
            });
        }
    }
    by_input
        .into_iter()
        .map(|(input, deliver)| {
            Ok(SecretSlot {
                name: Identifier::parse(input)?,
                kind: SlotKind::Input,
                required: true,
                sensitive: true,
                machines: Vec::new(),
                providers: BTreeSet::new(),
                source: None,
                default: None,
                deliver: NonEmpty::new(deliver, "the variables taking the input")?,
            })
        })
        .collect()
}

/// The spec's `outputs`: each a module output, read from the state (the runner masks one the
/// state marks sensitive).
fn outputs(cloud: &CloudServices) -> Result<Vec<Output>, ManifestError> {
    cloud
        .outputs
        .iter()
        .map(|(name, from)| {
            let snake: String = name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
                .collect();
            Ok(Output {
                name: SnakeName::parse(&snake)?,
                description: Some(format!("The module's `{from}` (the spec's `cloud.outputs.{name}`).")),
                role: OutputRole::Info,
                sensitive: false,
                format: None,
                from: OutputSource::TerraformOutput {
                    name: Identifier::parse(from)?,
                    default: None,
                },
            })
        })
        .collect()
}
