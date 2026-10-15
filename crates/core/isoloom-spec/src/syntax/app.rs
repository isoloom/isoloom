//! `app:`, a custom machine's one recipe: a container-only one (`image`, `build`, `appliance`),
//! portable `steps`, or both (each form its own recipe).

use std::fmt;

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::files::Mode;
use crate::model::Appliance;

/// A custom machine's recipe.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "AppInput")]
pub struct App {
    /// The container form's own recipe, when it has one beside (or instead of) the steps.
    pub container: Option<Recipe>,
    /// The portable recipe: a generated Dockerfile on containers, set-up steps on VMs.
    pub steps: Vec<Step>,
}

/// A recipe only a container has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipe {
    /// A published image, by tag or digest.
    Image(String),
    /// The project's own Dockerfile.
    Build(Build),
    /// A network appliance's image (or a Dynamips router's firmware).
    Appliance(ApplianceRecipe),
}

/// A network appliance (`app.appliance`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplianceRecipe {
    /// Which appliance it is.
    pub kind: Appliance,
    /// Its image (every appliance but `cisco-dynamips`, which Isoloom builds itself).
    pub image: Option<String>,
    /// Its own configuration, added to the one Isoloom generates.
    pub config: Option<String>,
    /// The firmware `cisco-dynamips` boots.
    pub firmware: Option<String>,
}

/// The project's own Dockerfile: its folder, or the folder with the file, stage and arguments.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Build {
    /// The build context: a folder of the project.
    pub context: String,
    /// The Dockerfile, anywhere in the project (default `<context>/Dockerfile`).
    #[serde(default)]
    pub dockerfile: Option<String>,
    /// The stage of a multi-stage Dockerfile to build, when not its last.
    #[serde(default)]
    pub target: Option<String>,
    /// Build arguments (the Dockerfile's `ARG`s), fixed values.
    #[serde(default)]
    pub args: IndexMap<String, String>,
}

/// `app:` as written.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AppInput {
    /// A published image, by tag or digest (`example/clamd:1.4`): the container form. Not with
    /// `build`.
    #[serde(default)]
    image: Option<String>,
    /// The project's own Dockerfile: a folder holding a `Dockerfile`, or `{ context, dockerfile,
    /// target, args }`. The container form; translating it to a VM is never attempted, so add
    /// `os` and `steps` for a VM form.
    #[serde(default, deserialize_with = "build")]
    #[schemars(with = "Option<BuildInput>")]
    build: Option<Build>,
    /// Portable steps (they need `os`): a Dockerfile Isoloom generates from the OS's official
    /// image on containers, set-up steps on VMs. With `image` or `build`, the VM form's recipe.
    #[serde(default)]
    steps: Vec<Step>,
    /// A network appliance Isoloom wires (a router or switch OS in a container): with `image`, or
    /// `firmware` for `cisco-dynamips`.
    #[serde(default)]
    appliance: Option<Appliance>,
    /// An appliance's own configuration (a project file), added to the one Isoloom generates.
    #[serde(default)]
    config: Option<String>,
    /// The firmware `cisco-dynamips` boots (the IOS `.bin`, a project file).
    #[serde(default)]
    firmware: Option<String>,
}

/// `build:` as written: a folder, or in full.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)] // the schema's shape only.
enum BuildInput {
    /// A folder of the project holding a `Dockerfile`.
    Folder(String),
    /// The folder, the Dockerfile, the stage and the arguments.
    Full(Build),
}

fn build<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Build>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<Build>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a project folder, or `{ context, dockerfile, target, args }`")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(Some(Build {
                context: v.to_string(),
                dockerfile: None,
                target: None,
                args: IndexMap::new(),
            }))
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            Build::deserialize(de::value::MapAccessDeserializer::new(map)).map(Some)
        }
    }
    d.deserialize_any(V)
}

impl TryFrom<AppInput> for App {
    type Error = String;

    fn try_from(a: AppInput) -> Result<App, String> {
        if a.config.is_some() && a.appliance.is_none() {
            return Err("`config` is an appliance's: it goes with `appliance`".into());
        }
        let container = match (a.image, a.build, a.appliance) {
            (Some(_), Some(_), _) => return Err("`image` and `build` are both given: a container comes from one".into()),
            (_, Some(_), Some(_)) => return Err("an appliance comes from its `image` (or `firmware`), not a `build`".into()),
            (image, None, Some(kind)) => Some(Recipe::Appliance(appliance(kind, image, a.config, a.firmware)?)),
            (_, _, None) if a.firmware.is_some() => return Err("`firmware` is `cisco-dynamips`'s: it goes with `appliance: cisco-dynamips`".into()),
            (Some(image), None, None) => Some(Recipe::Image(image)),
            (None, Some(b), None) => Some(Recipe::Build(b)),
            (None, None, None) => None,
        };
        if container.is_none() && a.steps.is_empty() {
            return Err("an app needs a recipe: `image`, `build`, `steps` or `appliance`".into());
        }
        Ok(App { container, steps: a.steps })
    }
}

fn appliance(kind: Appliance, image: Option<String>, config: Option<String>, firmware: Option<String>) -> Result<ApplianceRecipe, String> {
    let dynamips = kind == Appliance::CiscoDynamips;
    match (dynamips, image.is_some(), firmware.is_some()) {
        (true, true, _) => Err("`cisco-dynamips` boots its `firmware`; Isoloom builds its image, so no `image`".into()),
        (true, false, false) => Err("`cisco-dynamips` needs the `firmware` it boots (the IOS `.bin`)".into()),
        (false, _, true) => Err("`firmware` is `cisco-dynamips`'s; this appliance runs its `image`".into()),
        (false, false, false) => Err("an appliance runs its `image` (`vrnetlab/cisco_iol:<version>`)".into()),
        (true, false, true) | (false, true, false) => Ok(ApplianceRecipe { kind, image, config, firmware }),
    }
}

/// One step of the portable recipe. Steps install and configure: they run as root from the
/// project copy at `/opt/isoloom`, with no network to the other machines and no inputs or
/// secrets (on containers they run at image build time). What needs those goes in `setup`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A project script (`.sh`, or `.ps1` on Windows), skipped when `creates` exists.
    Run {
        /// The script.
        script: String,
        /// A path whose existence means the step already ran.
        creates: Option<String>,
    },
    /// A project file or folder copied into the machine.
    Copy(Copy),
    /// Packages installed with the OS's own installer.
    Packages(Vec<String>),
    /// The machine restarts, and the next step runs once it is back (no container form).
    Reboot,
    /// An Ansible playbook of the project, run in the machine (no container form in 1.0).
    Playbook(String),
}

impl Step {
    /// Why the step has no container form, when it has none.
    pub fn container_refusal(&self) -> Option<&'static str> {
        match self {
            Step::Reboot => Some("a reboot needs a machine with its own kernel and boot: a container has neither"),
            Step::Playbook(_) => Some("a playbook step runs in a VM only in 1.0"),
            Step::Run { script, .. } if has_extension(script, "ps1") => Some("a PowerShell script means Windows, which has no container form in 1.0"),
            _ => None,
        }
    }
}

/// Whether `path` ends in `.<extension>`, in any case.
pub(crate) fn has_extension(path: &str, extension: &str) -> bool {
    std::path::Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

/// A `copy` step.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Copy {
    /// A file or folder of the project (a folder ends with `/`).
    pub from: String,
    /// Where it goes in the machine (absolute).
    pub to: String,
    /// The owner, a user name (default root).
    #[serde(default)]
    pub owner: Option<String>,
    /// The mode, octal (default: as in the project).
    #[serde(default)]
    pub mode: Option<Mode>,
}

/// A step as written: `reboot`, or a map with one of `run`, `copy`, `packages` and `playbook`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StepInput {
    /// A project script (`.sh`; `.ps1` on Windows).
    #[serde(default)]
    run: Option<String>,
    /// With `run`: a path whose existence means the step already ran.
    #[serde(default)]
    creates: Option<String>,
    /// A project file or folder copied into the machine.
    #[serde(default)]
    copy: Option<Copy>,
    /// Packages to install with the OS's installer (`apt-get`, `apk`, `dnf`).
    #[serde(default)]
    packages: Option<Vec<String>>,
    /// An Ansible playbook of the project, run in the machine (`ansible-playbook -c local`).
    #[serde(default)]
    playbook: Option<String>,
}

/// A step as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)] // the schema's shape only.
enum StepSchema {
    /// `reboot`: the machine restarts, and the next step runs once it is back.
    Reboot(RebootWord),
    /// A step that runs, copies or installs something.
    Map(StepInput),
}

/// The word `reboot`.
#[derive(JsonSchema)]
#[allow(dead_code)] // the schema's shape only.
enum RebootWord {
    /// Restarts the machine.
    #[serde(rename = "reboot")]
    Reboot,
}

impl JsonSchema for Step {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Step".into()
    }

    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        StepSchema::json_schema(g)
    }
}

impl<'de> Deserialize<'de> for Step {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Step, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Step;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("`reboot`, or one of `{ run }`, `{ copy }`, `{ packages }` and `{ playbook }`")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Step, E> {
                if v == "reboot" {
                    Ok(Step::Reboot)
                } else {
                    Err(E::custom(format!("a step is `reboot` or a map: write `{{ run: {v} }}`")))
                }
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Step, A::Error> {
                let s = StepInput::deserialize(de::value::MapAccessDeserializer::new(map))?;
                step(s).map_err(de::Error::custom)
            }
        }
        d.deserialize_any(V)
    }
}

fn step(s: StepInput) -> Result<Step, String> {
    if s.creates.is_some() && s.run.is_none() {
        return Err("`creates` goes with `run`".into());
    }
    let mut given: Vec<Step> = [
        s.run.map(|script| Step::Run { script, creates: s.creates }),
        s.copy.map(Step::Copy),
        s.packages.map(Step::Packages),
        s.playbook.map(Step::Playbook),
    ]
    .into_iter()
    .flatten()
    .collect();
    match given.len() {
        1 => Ok(given.remove(0)),
        0 => Err("a step needs one of `run`, `copy`, `packages` and `playbook`".into()),
        _ => Err("a step does one thing: one of `run`, `copy`, `packages` and `playbook` per step".into()),
    }
}

impl JsonSchema for App {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "App".into()
    }

    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        AppInput::json_schema(g)
    }
}

#[cfg(test)]
mod tests;
