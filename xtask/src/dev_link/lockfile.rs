//! What a Cargo.lock says about Isoloom: whether it pins it by git, and which of its crates the
//! workspace's own packages depend on directly.

use std::collections::BTreeSet;
use std::path::Path;

/// Whether a `source` value is `url` by git: `git+<url>` then `?`, `#` or the end.
fn is_git_source(source: &str, url: &str) -> bool {
    source
        .strip_prefix("git+")
        .and_then(|s| s.strip_prefix(url))
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['?', '#']))
}

/// Whether the lockfile pins a crate of `url` by git.
pub fn pins_git(lockfile: &str, url: &str) -> bool {
    packages(lockfile).iter().any(|p| p.source.as_deref().is_some_and(|s| is_git_source(s, url)))
}

struct Package {
    name: String,
    source: Option<String>,
    dependencies: Vec<String>,
}

fn packages(lockfile: &str) -> Vec<Package> {
    let Ok(doc) = lockfile.parse::<toml::Table>() else { return Vec::new() };
    let list = doc.get("package").and_then(toml::Value::as_array).cloned().unwrap_or_default();
    list.iter()
        .filter_map(|p| {
            let field = |k: &str| p.get(k).and_then(toml::Value::as_str).map(str::to_string);
            let dependencies = p.get("dependencies").and_then(toml::Value::as_array).map_or_else(Vec::new, |deps| {
                deps.iter()
                    .filter_map(toml::Value::as_str)
                    .filter_map(|d| d.split_whitespace().next().map(str::to_string))
                    .collect()
            });
            Some(Package {
                name: field("name")?,
                source: field("source"),
                dependencies,
            })
        })
        .collect()
}

/// The crates of `url`, pinned by git, that the workspace's own packages (those without a
/// source) depend on directly, sorted.
pub fn direct_git_crates(lockfile: &str, url: &str) -> Vec<String> {
    let all = packages(lockfile);
    let git: BTreeSet<&str> = all
        .iter()
        .filter(|p| p.source.as_deref().is_some_and(|s| is_git_source(s, url)))
        .map(|p| p.name.as_str())
        .collect();
    let direct: BTreeSet<&str> = all
        .iter()
        .filter(|p| p.source.is_none())
        .flat_map(|p| p.dependencies.iter().map(String::as_str))
        .collect();
    git.intersection(&direct).map(|s| (*s).to_string()).collect()
}

/// The `package.name` of a Cargo.toml.
pub fn package_name(manifest: &Path) -> Option<String> {
    let doc = std::fs::read_to_string(manifest).ok()?.parse::<toml::Table>().ok()?;
    doc.get("package")?.get("name")?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://github.com/isoloom/isoloom";
    const LOCK: &str = r#"
version = 4

[[package]]
name = "app"
version = "0.1.0"
dependencies = [
 "isoloom-spec",
 "isoloom-targets 0.10.1 (git+https://github.com/isoloom/isoloom?rev=abc#abc)",
 "serde",
]

[[package]]
name = "isoloom-spec"
version = "0.10.1"
source = "git+https://github.com/isoloom/isoloom?rev=abc#abc"

[[package]]
name = "isoloom-targets"
version = "0.10.1"
source = "git+https://github.com/isoloom/isoloom?rev=abc#abc"
dependencies = ["isoloom-plan"]

[[package]]
name = "isoloom-plan"
version = "0.10.1"
source = "git+https://github.com/isoloom/isoloom?rev=abc#abc"

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;

    #[test]
    fn only_the_crates_the_workspace_names_itself_are_direct() {
        assert!(pins_git(LOCK, URL));
        assert_eq!(direct_git_crates(LOCK, URL), ["isoloom-spec", "isoloom-targets"]);
    }

    #[test]
    fn another_repository_of_the_same_prefix_is_not_isoloom() {
        assert!(!is_git_source("git+https://github.com/isoloom/isoloom-website#x", URL));
        assert!(is_git_source("git+https://github.com/isoloom/isoloom", URL));
        assert!(!pins_git("[[package]]\nname = \"a\"\nversion = \"1\"\n", URL));
    }
}
