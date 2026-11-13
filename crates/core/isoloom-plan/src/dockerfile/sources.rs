//! Base images the service matrix doesn't have, as their published Dockerfiles: where each one's
//! source is (an official image's entry in docker-library's `official-images`, or a publisher's
//! repository Isoloom knows), and the sources once fetched and vendored in the project, which a
//! translation reads offline. Fetching is the caller's (`isoloom bases fetch`): it resolves each
//! image to an [`Origin`] here, checks the repository out at the pinned commit, copies the
//! Dockerfile's folder under [`VENDOR_DIR`] and records it in [`INDEX`]; generating reads only
//! what is vendored.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::instruction::{self, Instruction};
use super::matrix::{self, NoEntry};

/// The project folder fetched base images are vendored in.
pub const VENDOR_DIR: &str = "vendor/bases";

/// The vendored bases' index, in [`VENDOR_DIR`].
pub const INDEX: &str = "vendor/bases/index.yml";

/// Where a base image's Dockerfile is published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    /// The git repository.
    pub repo: String,
    /// The commit, or a tag a publisher releases at (resolved to its commit when fetched).
    pub commit: String,
    /// The folder of the repository holding the build context.
    pub directory: String,
    /// The Dockerfile, under `directory`.
    pub file: String,
    /// Build arguments the publisher builds that tag with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<(String, String)>,
}

/// A base image's sources, vendored in the project: one entry of [`INDEX`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vendored {
    /// Where it was fetched from, its commit resolved.
    pub origin: Origin,
    /// The project folder holding its build context (under [`VENDOR_DIR`]).
    pub context: String,
}

/// The vendored bases' index: each image (`name:tag`) and its sources.
pub type Index = BTreeMap<String, Vendored>;

/// A vendored base image, ready to translate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// The Dockerfile's text.
    pub dockerfile: String,
    /// The project folder of its build context.
    pub context: String,
    /// Its build arguments.
    pub args: Vec<(String, String)>,
    /// Where it came from, as problems name it (`https://github.com/jenkinsci/docker@1a2b3c
    /// debian/Dockerfile`).
    pub origin: String,
}

/// The vendored base images a translation may start `FROM`, by `name:tag`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sources(BTreeMap<String, Fetched>);

/// `image`'s key: its name (without `docker.io/library/`) and tag.
#[must_use]
pub fn key(image: &str) -> String {
    let (name, tag) = matrix::split(image);
    format!("{name}:{tag}")
}

impl Sources {
    /// The sources `index` lists, each Dockerfile's text from `read` (its project path → its
    /// text; `None` leaves the image out).
    pub fn of(index: &Index, mut read: impl FnMut(&str) -> Option<String>) -> Sources {
        let mut out = BTreeMap::new();
        for (image, v) in index {
            let path = format!("{}/{}", v.context.trim_end_matches('/'), v.origin.file);
            if let Some(dockerfile) = read(&path) {
                out.insert(
                    key(image),
                    Fetched {
                        dockerfile,
                        context: v.context.clone(),
                        args: v.origin.args.clone(),
                        origin: format!("{}@{} {}", v.origin.repo, v.origin.commit, path_in_repo(&v.origin)),
                    },
                );
            }
        }
        Sources(out)
    }

    /// The vendored image `image`, if it is.
    #[must_use]
    pub fn get(&self, image: &str) -> Option<&Fetched> {
        self.0.get(&key(image))
    }

    /// Add `image`'s sources.
    pub fn insert(&mut self, image: &str, fetched: Fetched) {
        self.0.insert(key(image), fetched);
    }
}

/// The Dockerfile's path in its repository.
fn path_in_repo(o: &Origin) -> String {
    if o.directory.is_empty() || o.directory == "." { o.file.clone() } else { format!("{}/{}", o.directory.trim_end_matches('/'), o.file) }
}

/// The images `dockerfile` starts `FROM` that neither the matrix nor `sources` gives (stage
/// names and refused images aside): what a fetch must vendor before the translation can run.
#[must_use]
pub fn wanted(dockerfile: &str, sources: &Sources) -> Vec<String> {
    let Ok(lines) = instruction::read(dockerfile) else { return Vec::new() };
    let mut args: Vec<(String, String)> = Vec::new();
    let mut stages: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for line in &lines {
        match &line.instruction {
            Instruction::Arg { name, default } if stages.is_empty() => args.push((name.clone(), default.clone().unwrap_or_default())),
            Instruction::From { image, stage } => {
                let image = super::translate::substitute(image, &args).replace(['"', '\''], "");
                let unknown = matches!(matrix::of(&image, None), Err(NoEntry::Unknown(_)));
                if !stages.contains(&image.to_ascii_lowercase()) && unknown && sources.get(&image).is_none() && !out.contains(&image) {
                    out.push(image);
                }
                stages.extend(stage.clone());
            }
            _ => {}
        }
    }
    out
}

/// Where publisher image `name:tag` is built from, for the images published outside docker-library
/// that the catalogue starts from.
#[must_use]
pub fn publisher(image: &str) -> Option<Origin> {
    let (name, tag) = matrix::split(image);
    match name.as_str() {
        // jenkinsci/docker builds every `jenkins/jenkins` tag from one Dockerfile per OS; its
        // release tags carry the version, its bake file the Java release (`jdk21` in the tag).
        "jenkins/jenkins" => {
            let java = tag.split('-').find_map(|w| w.strip_prefix("jdk")).unwrap_or("21").to_owned();
            let version = if tag.starts_with("lts") || tag == "latest" { JENKINS_LTS.to_owned() } else { tag.split('-').next().unwrap_or(&tag).to_owned() };
            let slim = tag.contains("slim");
            Some(Origin {
                repo: "https://github.com/jenkinsci/docker".to_owned(),
                commit: version.clone(),
                directory: ".".to_owned(),
                file: "debian/Dockerfile".to_owned(),
                args: vec![
                    ("JENKINS_VERSION".to_owned(), version),
                    ("JAVA_RELEASE".to_owned(), java),
                    ("DEBIAN_VARIANT".to_owned(), if slim { "-slim" } else { "" }.to_owned()),
                ],
            })
        }
        _ => None,
    }
}

/// The Jenkins LTS release `jenkins/jenkins:lts*` resolves to.
const JENKINS_LTS: &str = "2.580.2";

/// Where official image `name:tag` is built from, by docker-library's manifest of `name`
/// (`library/<name>` in `official-images`): the entry whose `Tags` or `SharedTags` list `tag`.
#[must_use]
pub fn official(manifest: &str, tag: &str) -> Option<Origin> {
    let mut repo = String::new();
    let mut file = "Dockerfile".to_owned();
    let mut entry: BTreeMap<&str, &str> = BTreeMap::new();
    let mut found = None;
    let mut check = |fields: &BTreeMap<&str, &str>, default_repo: &str, default_file: &str| {
        let tags = |k: &str| fields.get(k).is_some_and(|v| v.split(',').map(str::trim).any(|t| t == tag));
        if found.is_none() && (tags("Tags") || tags("SharedTags")) {
            found = Some(Origin {
                repo: fields.get("GitRepo").copied().unwrap_or(default_repo).trim_end_matches(".git").to_owned(),
                commit: fields.get("GitCommit").copied().unwrap_or_default().to_owned(),
                directory: fields.get("Directory").copied().unwrap_or(".").to_owned(),
                file: fields.get("File").copied().unwrap_or(default_file).to_owned(),
                args: Vec::new(),
            });
        }
    };
    for line in manifest.lines().chain(std::iter::once("")) {
        let line = line.trim();
        if line.is_empty() {
            if entry.contains_key("Tags") {
                check(&entry, &repo, &file);
            } else {
                // The header: the defaults every entry inherits.
                if let Some(r) = entry.get("GitRepo") {
                    (*r).clone_into(&mut repo);
                }
                if let Some(f) = entry.get("File") {
                    (*f).clone_into(&mut file);
                }
            }
            entry.clear();
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            entry.insert(k.trim(), v.trim());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "# Eclipse Temurin\n\nMaintainers: someone\nGitRepo: https://github.com/adoptium/containers.git\n\nTags: 8u512-b09-jdk-noble, 8-jdk-noble, 8-noble\nArchitectures: amd64\nGitCommit: df1e9fc3\nDirectory: 8/jdk/ubuntu/noble\n\nTags: 17.0.1-jdk-jammy\nSharedTags: 17-jdk, 17\nGitCommit: abc\nDirectory: 17/jdk/ubuntu/jammy\nFile: Dockerfile.releases\n";

    #[test]
    fn an_official_tag_resolves_to_its_entry() {
        let o = official(MANIFEST, "8-jdk-noble").unwrap();
        assert_eq!((o.repo.as_str(), o.commit.as_str(), o.directory.as_str(), o.file.as_str()), ("https://github.com/adoptium/containers", "df1e9fc3", "8/jdk/ubuntu/noble", "Dockerfile"));
        assert_eq!(official(MANIFEST, "17").unwrap().file, "Dockerfile.releases");
        assert_eq!(official(MANIFEST, "11"), None);
    }

    #[test]
    fn jenkins_resolves_to_jenkinsci_docker_with_the_tags_java() {
        let o = publisher("jenkins/jenkins:lts-jdk21").unwrap();
        assert_eq!((o.commit.as_str(), o.file.as_str()), (JENKINS_LTS, "debian/Dockerfile"));
        assert!(o.args.contains(&("JAVA_RELEASE".to_owned(), "21".to_owned())));
        assert_eq!(publisher("gitea/gitea:1.22"), None);
    }

    #[test]
    fn a_fetch_is_wanted_for_unknown_bases_only() {
        let text = "ARG V=lts-jdk21\nFROM jenkins/jenkins:${V} AS j\nFROM php:8.2-apache\nFROM j\nFROM scratch\n";
        assert_eq!(wanted(text, &Sources::default()), ["jenkins/jenkins:lts-jdk21"]);
    }
}
