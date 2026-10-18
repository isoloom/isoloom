//! `app:`, a custom machine's one recipe: a container-only one (`image`, `build`, `appliance`),
//! portable `steps`, or both (each form its own recipe).

use std::fmt;

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::step::Step;
use crate::Error;
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

// What the schema shows and the reader returns.
/// `build:` as written: a folder, or in full.
#[derive(JsonSchema)]
#[serde(untagged)]
enum BuildInput {
    /// A folder of the project holding a `Dockerfile`.
    Folder(String),
    /// The folder, the Dockerfile, the stage and the arguments.
    Full(Build),
}

impl<'de> Deserialize<'de> for BuildInput {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<BuildInput, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = BuildInput;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a project folder, or `{ context, dockerfile, target, args }`")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<BuildInput, E> {
                Ok(BuildInput::Folder(v.to_owned()))
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<BuildInput, A::Error> {
                Build::deserialize(de::value::MapAccessDeserializer::new(map)).map(BuildInput::Full)
            }
        }
        d.deserialize_any(V)
    }
}

impl From<BuildInput> for Build {
    fn from(written: BuildInput) -> Build {
        match written {
            BuildInput::Folder(context) => Build {
                context,
                dockerfile: None,
                target: None,
                args: IndexMap::new(),
            },
            BuildInput::Full(build) => build,
        }
    }
}

fn build<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Build>, D::Error> {
    BuildInput::deserialize(d).map(|written| Some(written.into()))
}

impl TryFrom<AppInput> for App {
    type Error = Error;

    fn try_from(a: AppInput) -> Result<App, Error> {
        if a.config.is_some() && a.appliance.is_none() {
            return Err(Error::ConfigWithoutAppliance);
        }
        let container = match (a.image, a.build, a.appliance) {
            (Some(_), Some(_), _) => return Err(Error::ImageAndBuild),
            (_, Some(_), Some(_)) => return Err(Error::ApplianceBuild),
            (image, None, Some(kind)) => Some(Recipe::Appliance(appliance(kind, image, a.config, a.firmware)?)),
            (_, _, None) if a.firmware.is_some() => return Err(Error::FirmwareWithoutDynamips),
            (Some(image), None, None) => Some(Recipe::Image(image)),
            (None, Some(b), None) => Some(Recipe::Build(b)),
            (None, None, None) => None,
        };
        if container.is_none() && a.steps.is_empty() {
            return Err(Error::NoRecipe);
        }
        Ok(App { container, steps: a.steps })
    }
}

fn appliance(kind: Appliance, image: Option<String>, config: Option<String>, firmware: Option<String>) -> Result<ApplianceRecipe, Error> {
    let dynamips = kind == Appliance::CiscoDynamips;
    match (dynamips, image.is_some(), firmware.is_some()) {
        (true, true, _) => Err(Error::DynamipsImage),
        (true, false, false) => Err(Error::DynamipsFirmware),
        (false, _, true) => Err(Error::ApplianceFirmware),
        (false, false, false) => Err(Error::ApplianceImage),
        (true, false, true) | (false, true, false) => Ok(ApplianceRecipe { kind, image, config, firmware }),
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
