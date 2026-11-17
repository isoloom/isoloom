//! The steps that place files: `copy` (a project file or folder as it is), `template` (a project
//! file with its `{{ path }}` placeholders filled in at generation) and `file` (a folder, a
//! removal, a link or a pinned download, one for every machine or one per CPU architecture).

use std::fmt;
use std::marker::PhantomData;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};

use super::super::files::Mode;
use super::super::mistake::Mistake;
use super::super::problem::Code;
use super::words::{AbsPath, GroupName, HttpsUrl, ProjectPath, Sha256, UserName};
use crate::model::Arch;

/// A `copy` step: a project file or folder copied into the machine as it is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Copy {
    /// A file or folder of the project (a folder ends with `/`).
    pub from: ProjectPath,
    /// Where it goes in the machine (absolute).
    pub to: AbsPath,
    /// The owner, a user name (default root). Not on Windows.
    #[serde(default)]
    pub owner: Option<UserName>,
    /// The mode, octal (default: as in the project). Not on Windows.
    #[serde(default)]
    pub mode: Option<Mode>,
}

/// A `template` step: a project file whose `{{ path }}` placeholders Isoloom fills in when it
/// generates, so every CM tool installs the same bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "TemplateInput")]
pub struct Template {
    /// The project file.
    pub from: ProjectPath,
    /// Where the filled-in file goes in the machine.
    pub to: AbsPath,
    /// Its owner (default root).
    pub owner: Option<UserName>,
    /// Its group (default the owner's).
    pub group: Option<GroupName>,
    /// Its mode (default `0644`).
    pub mode: Option<Mode>,
}

schema_as!(Template, TemplateInput);

/// A `template` step as written.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TemplateInput {
    /// A file of the project holding `{{ path }}` placeholders: `{{ machine.name }}`,
    /// `{{ machine.addresses.<network> }}`, `{{ machines.<m>.addresses.<network> }}` and
    /// `{{ machines.<m>.ports.<name>.port }}`. Filled in when Isoloom generates; no logic.
    from: ProjectPath,
    /// Where the filled-in file goes in the machine (absolute).
    to: AbsPath,
    /// Its owner, a user name (default root). Not on Windows.
    #[serde(default)]
    owner: Option<UserName>,
    /// Its group (default the owner's). Not on Windows.
    #[serde(default)]
    group: Option<GroupName>,
    /// Its mode, octal (default `0644`). Not on Windows.
    #[serde(default)]
    mode: Option<Mode>,
}

impl TryFrom<TemplateInput> for Template {
    type Error = Mistake;

    fn try_from(t: TemplateInput) -> Result<Template, Mistake> {
        if t.from.as_str().ends_with('/') {
            return Err(Mistake::new(Code::InvalidValue, format!("`{}`: a template is one file", t.from))
                .at("from")
                .fix("a `copy` step copies a folder"));
        }
        Ok(Template {
            from: t.from,
            to: t.to,
            owner: t.owner,
            group: t.group,
            mode: t.mode,
        })
    }
}

/// A `file` step: a path made a folder, removed, made a link, or downloaded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "FileInput")]
pub struct FileStep {
    /// The path in the machine.
    pub path: AbsPath,
    /// What it becomes.
    pub state: FileState,
    /// Its owner (default root).
    pub owner: Option<UserName>,
    /// Its group (default the owner's).
    pub group: Option<GroupName>,
    /// Its mode.
    pub mode: Option<Mode>,
}

schema_as!(FileStep, FileInput);

/// What a `file` step makes of its path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FileState {
    /// A folder (with its parents).
    Directory,
    /// Nothing: the path is removed. The model's only removal.
    Absent,
    /// A symbolic link to this target.
    Link(AbsPath),
    /// A file downloaded from `url`, kept only when its SHA-256 is `sha256`.
    Download {
        /// Where it comes from.
        url: HttpsUrl,
        /// What it must hash to.
        sha256: Sha256,
    },
    /// One file per CPU architecture (a vendor's build for each), in `amd64`, `arm64` order: the
    /// machine's own architecture's, picked where the step runs. A machine of an architecture
    /// none is for fails the step.
    Downloads(Vec<ArchDownload>),
}

/// One architecture's file of a `file` step's download.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ArchDownload {
    /// The architecture it is for.
    pub arch: Arch,
    /// Where it comes from.
    pub url: HttpsUrl,
    /// What it must hash to.
    pub sha256: Sha256,
}

/// `state:` as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum StateWord {
    /// A folder.
    Directory,
    /// Removed.
    Absent,
}

/// A `file` step as written.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FileInput {
    /// The path in the machine (absolute).
    path: AbsPath,
    /// `directory` (a folder, with its parents) or `absent` (removed: the model's only removal).
    /// Not with `link` or `url`.
    #[serde(default)]
    state: Option<StateWord>,
    /// A symbolic link to this target (absolute). Not on Windows.
    #[serde(default)]
    link: Option<AbsPath>,
    /// A file to download (`https`), with its `sha256`: every fetch is pinned. One URL, or one per
    /// CPU architecture (`{ amd64: <url>, arm64: <url> }`), the machine's own picked where the
    /// step runs.
    #[serde(default)]
    url: Option<ByArch<HttpsUrl>>,
    /// The SHA-256 of what `url` fetches (64 hex digits, as `sha256sum` prints it): one, or one
    /// per architecture `url` names.
    #[serde(default)]
    sha256: Option<ByArch<Sha256>>,
    /// Its owner, a user name (default root). Not on Windows.
    #[serde(default)]
    owner: Option<UserName>,
    /// Its group (default the owner's). Not on Windows.
    #[serde(default)]
    group: Option<GroupName>,
    /// Its mode, octal. Not on Windows, nor with `state: absent` or `link`.
    #[serde(default)]
    mode: Option<Mode>,
}

impl TryFrom<FileInput> for FileStep {
    type Error = Mistake;

    fn try_from(f: FileInput) -> Result<FileStep, Mistake> {
        let state = state(f.state, f.link, f.url, f.sha256)?;
        let owned = f.owner.is_some() || f.group.is_some() || f.mode.is_some();
        match state {
            FileState::Absent if owned => {
                return Err(Mistake::new(
                    Code::Conflict,
                    "`state: absent` removes the path, so it has no owner, group or mode",
                ));
            }
            FileState::Link(_) if f.mode.is_some() => {
                return Err(Mistake::new(Code::Conflict, "a link has no mode of its own").at("mode"));
            }
            _ => {}
        }
        Ok(FileStep {
            path: f.path,
            state,
            owner: f.owner,
            group: f.group,
            mode: f.mode,
        })
    }
}

/// A download's `url` or `sha256` as written: one value, or one per CPU architecture.
#[derive(JsonSchema)]
#[serde(untagged)]
enum ByArch<T> {
    /// The same for every machine.
    One(T),
    /// One per architecture.
    PerArch(PerArch<T>),
}

/// A value per CPU architecture: each one the download has a build for.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PerArch<T> {
    /// For an x86-64 machine.
    amd64: Option<T>,
    /// For a 64-bit ARM machine.
    arm64: Option<T>,
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for ByArch<T> {
    /// A string is read as `T` itself, so its own rule's mistake reaches the author; a mapping as
    /// one value per architecture.
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<ByArch<T>, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = ByArch<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("one value, or one per architecture (`{ amd64: ..., arm64: ... }`)")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<ByArch<T>, E> {
                T::deserialize(de::value::StrDeserializer::new(s)).map(ByArch::One)
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<ByArch<T>, A::Error> {
                PerArch::deserialize(de::value::MapAccessDeserializer::new(map)).map(ByArch::PerArch)
            }
        }
        d.deserialize_any(V(PhantomData))
    }
}

/// A download: one file for every machine, or one per architecture, each with its SHA-256.
fn download(url: ByArch<HttpsUrl>, sha256: ByArch<Sha256>) -> Result<FileState, Mistake> {
    let (urls, sums) = match (url, sha256) {
        (ByArch::One(url), ByArch::One(sha256)) => return Ok(FileState::Download { url, sha256 }),
        (ByArch::PerArch(urls), ByArch::PerArch(sums)) => (urls, sums),
        _ => {
            return Err(
                Mistake::new(Code::Conflict, "`url` and `sha256` are both one value, or both one per architecture")
                    .at("sha256")
                    .fix("`sha256: { amd64: <hex>, arm64: <hex> }` beside `url: { amd64: <url>, arm64: <url> }`"),
            );
        }
    };
    let mut all = Vec::new();
    for (arch, url, sha256) in [(Arch::Amd64, urls.amd64, sums.amd64), (Arch::Arm64, urls.arm64, sums.arm64)] {
        match (url, sha256) {
            (Some(url), Some(sha256)) => all.push(ArchDownload { arch, url, sha256 }),
            (None, None) => {}
            _ => {
                return Err(Mistake::new(
                    Code::Conflict,
                    format!("`url` and `sha256` name the same architectures: `{}` is in one only", arch.id()),
                )
                .at("sha256"));
            }
        }
    }
    if all.is_empty() {
        return Err(Mistake::new(Code::MissingField, "a download per architecture names at least one")
            .at("url")
            .fix("`url: { amd64: <url>, arm64: <url> }`"));
    }
    Ok(FileState::Downloads(all))
}

/// What the step makes of its path: exactly one of `state`, `link` and `url` (with `sha256`).
fn state(word: Option<StateWord>, link: Option<AbsPath>, url: Option<ByArch<HttpsUrl>>, sha256: Option<ByArch<Sha256>>) -> Result<FileState, Mistake> {
    let given = usize::from(word.is_some()) + usize::from(link.is_some()) + usize::from(url.is_some());
    if given > 1 {
        return Err(Mistake::new(Code::Conflict, "a file step does one thing: one of `state`, `link` and `url`"));
    }
    match (word, link, url, sha256) {
        (_, _, Some(_), None) => Err(
            Mistake::new(Code::MissingField, "every fetch is pinned: a download needs the SHA-256 of what it fetches")
                .at("url")
                .fix("`sha256: <hex>` (`sha256sum <file>` prints it)"),
        ),
        (_, _, None, Some(_)) => Err(Mistake::new(Code::InvalidValue, "`sha256` checks a download: it goes with `url`").at("sha256")),
        (_, _, Some(url), Some(sha256)) => download(url, sha256),
        (Some(StateWord::Directory), ..) => Ok(FileState::Directory),
        (Some(StateWord::Absent), ..) => Ok(FileState::Absent),
        (None, Some(target), ..) => Ok(FileState::Link(target)),
        (None, None, None, None) => Err(Mistake::new(Code::MissingField, "a file step says what to make of its `path`")
            .fix("`state: directory`, `state: absent`, `link: <target>`, or `url` with `sha256`")),
    }
}
