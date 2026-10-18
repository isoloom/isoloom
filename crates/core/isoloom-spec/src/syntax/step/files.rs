//! The steps that place files: `copy` (a project file or folder as it is), `template` (a project
//! file with its `{{ path }}` placeholders filled in at generation) and `file` (a folder, a
//! removal, a link or a pinned download).

use schemars::JsonSchema;
use serde::Deserialize;

use super::super::files::Mode;
use super::super::mistake::Mistake;
use super::super::problem::Code;
use super::words::{AbsPath, GroupName, HttpsUrl, ProjectPath, Sha256, UserName};

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

impl Copy {
    /// Whether it copies a folder (its `from` ends with `/`).
    #[must_use]
    pub fn is_folder(&self) -> bool {
        self.from.as_str().ends_with('/')
    }
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
    /// A file to download (`https`), with its `sha256`: every fetch is pinned.
    #[serde(default)]
    url: Option<HttpsUrl>,
    /// The SHA-256 of what `url` fetches (64 hex digits, as `sha256sum` prints it).
    #[serde(default)]
    sha256: Option<Sha256>,
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

/// What the step makes of its path: exactly one of `state`, `link` and `url` (with `sha256`).
fn state(word: Option<StateWord>, link: Option<AbsPath>, url: Option<HttpsUrl>, sha256: Option<Sha256>) -> Result<FileState, Mistake> {
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
        (_, _, Some(url), Some(sha256)) => Ok(FileState::Download { url, sha256 }),
        (Some(StateWord::Directory), ..) => Ok(FileState::Directory),
        (Some(StateWord::Absent), ..) => Ok(FileState::Absent),
        (None, Some(target), ..) => Ok(FileState::Link(target)),
        (None, None, None, None) => Err(Mistake::new(Code::MissingField, "a file step says what to make of its `path`")
            .fix("`state: directory`, `state: absent`, `link: <target>`, or `url` with `sha256`")),
    }
}
