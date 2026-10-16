//! What a resolved entry gives its machine besides the recipe (design §4.9, steps 6 and 7): the
//! official image pinned by digest, the ports by number, and the passwords' value files, with a
//! `start` step that hands each to its setting's owner.

use std::num::NonZeroU16;

use indexmap::IndexMap;
use isoloom_spec::model::Arch;
use isoloom_spec::syntax::{AbsPath, PortName, PortProtocol, ProjectPath, Step, UserName};

use super::fill::{Text, Values};
use super::recipe::Context;
use crate::entry::{ImageStart, PortNumber, SettingType};
use crate::family::Family;
use crate::settings::{Password, SettingValue};

/// The official image a container runs, pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OfficialImage {
    /// The image with its digest (`docker.io/library/nginx:1.30.6-trixie@sha256:…`).
    pub reference: String,
    /// The platforms it publishes.
    pub platforms: Vec<Arch>,
    /// Its own entrypoint.
    pub entrypoint: Vec<String>,
    /// Its own command.
    pub command: Vec<String>,
    /// The command the entry gives it instead, when it gives one.
    pub command_override: Option<Vec<String>>,
    /// Who runs the `start` steps.
    pub start: ImageStart,
    /// The variables it reads, filled in.
    pub environment: IndexMap<String, String>,
    /// Project files and folders placed in it, by path.
    pub files: IndexMap<AbsPath, ProjectPath>,
}

/// A port of the machine, by number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedPort {
    /// Its number.
    pub port: NonZeroU16,
    /// What it speaks.
    pub protocol: PortProtocol,
}

/// A password's value file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ValueFile {
    /// Where it goes in the machine.
    pub path: String,
    /// Where its value comes from.
    pub value: Password,
    /// Who owns it (root when `None`).
    pub owner: Option<UserName>,
}

/// What the entry gives the machine besides its recipe.
pub(super) struct Surface {
    pub image: Option<OfficialImage>,
    pub ports: IndexMap<PortName, ResolvedPort>,
    pub value_files: Vec<ValueFile>,
    /// The `start` steps that give each owned value file to its owner.
    pub owners: Vec<Step>,
}

impl Surface {
    /// The surface of `context`'s entry.
    pub(super) fn of(context: &Context<'_>) -> Result<Surface, String> {
        let value_files = value_files(context);
        let linux = Family::from(context.os) != Family::Windows;
        let owners = value_files
            .iter()
            .filter(|_| linux)
            .filter_map(|f| f.owner.as_ref().map(|o| owner_step(&f.path, o)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Surface {
            image: image(context)?,
            ports: ports(context)?,
            value_files,
            owners,
        })
    }
}

/// The official image for the machine's OS, when the entry has one.
fn image(context: &Context<'_>) -> Result<Option<OfficialImage>, String> {
    let def = context.entry.definition();
    let Some(template) = def.images.get(&context.os) else { return Ok(None) };
    let values = Values {
        release: context.release,
        os: context.os.os(),
        settings: context.settings,
        item: None,
    };
    let tag = values.text(template, Text::Field).map_err(|e| format!("images.{}: {e}", context.os))?;
    let locked = context.entry.lock().image(&context.release.version, context.os).ok_or_else(|| {
        format!(
            "lock.yml pins no image for {} on {}: run `cargo xtask catalogue refresh {}`",
            context.release.version,
            context.os,
            context.entry.name()
        )
    })?;
    let use_ = def.image.clone().unwrap_or_default();
    let fill = |t: &String| values.text(t, Text::Field).map_err(|e| e.to_string());
    let environment = use_.environment.iter().map(|(k, v)| Ok((k.clone(), fill(v)?))).collect::<Result<_, String>>()?;
    let files = use_
        .files
        .iter()
        .filter_map(|(to, from)| match fill(from) {
            // An unset folder or file setting places nothing.
            Err(_) if !from.contains("{{") => None,
            Ok(path) => Some(path.parse::<ProjectPath>().map(|p| (to.clone(), p)).map_err(|e| e.to_string())),
            Err(_) => None,
        })
        .collect::<Result<_, String>>()?;
    let command_override = use_.command.as_ref().map(|c| c.iter().map(fill).collect::<Result<Vec<_>, _>>()).transpose()?;
    Ok(Some(OfficialImage {
        reference: format!("{tag}@{}", locked.digest),
        platforms: locked.platforms.clone(),
        entrypoint: locked.entrypoint.clone(),
        command: locked.command.clone(),
        command_override,
        start: use_.start,
        environment,
        files,
    }))
}

/// The ports, by number.
fn ports(context: &Context<'_>) -> Result<IndexMap<PortName, ResolvedPort>, String> {
    context
        .entry
        .definition()
        .ports
        .iter()
        .map(|(name, p)| {
            let port = match &p.number {
                PortNumber::Fixed(n) => *n,
                PortNumber::Setting(s) => match context.settings.get(s.as_str()) {
                    Some(SettingValue::Port(n)) => *n,
                    _ => return Err(format!("port `{name}`: `{s}` isn't a port setting with a value")),
                },
            };
            Ok((name.clone(), ResolvedPort { port, protocol: p.protocol }))
        })
        .collect()
}

/// Every password's value file, with its setting's owner.
fn value_files(context: &Context<'_>) -> Vec<ValueFile> {
    let def = context.entry.definition();
    let owner_of = |name: &str| {
        def.settings
            .iter()
            .find(|(n, s)| n.as_str() == name && s.kind == SettingType::Password)
            .and_then(|(_, s)| s.owner.clone())
    };
    let mut out = Vec::new();
    for (name, value) in context.settings.iter() {
        let owner = owner_of(name.as_str());
        let mut passwords = Vec::new();
        collect(value, &mut passwords);
        out.extend(passwords.into_iter().map(|(password, path)| ValueFile {
            path: path.to_owned(),
            value: password.clone(),
            owner: owner.clone(),
        }));
    }
    out
}

/// `value`'s passwords, its list items' included.
fn collect<'a>(value: &'a SettingValue, out: &mut Vec<(&'a Password, &'a str)>) {
    match value {
        SettingValue::Password { value, path } => out.push((value, path)),
        SettingValue::List(items) => items.iter().flat_map(IndexMap::values).for_each(|v| collect(v, out)),
        _ => {}
    }
}

/// The `start` step giving value file `path` to `owner`, unless it already has it.
fn owner_step(path: &str, owner: &UserName) -> Result<Step, String> {
    let step = serde_json::json!({
        "command": ["chown", owner.as_str(), path],
        "unless": format!("[ \"$(stat -c %U {path})\" = {owner} ]"),
    });
    serde_json::from_value(step).map_err(|e| e.to_string())
}
