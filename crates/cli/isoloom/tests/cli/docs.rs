//! What the docs and the code's own docs say about the command line and the generated names,
//! against what they are.

use std::path::{Path, PathBuf};

use crate::testing::{Outcome, isoloom, project, scratch};

/// The repository's root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The commands `isoloom` has, as the command-line reference lists them (`validate`,
/// `import compose`...).
fn commands() -> Outcome<Vec<String>> {
    let reference = std::fs::read_to_string(root().join("docs/reference/cli.md"))?;
    Ok(reference
        .lines()
        .filter_map(|l| l.strip_prefix("## `isoloom ")?.strip_suffix('`'))
        .map(str::to_owned)
        .collect())
}

/// Every `.rs` file under `dir`.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) -> Outcome {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// The commands a doc comment line names: `` `isoloom generate` ``, and each one in a list after
/// it (a comma, `and` or `or` before each).
fn named(line: &str) -> Vec<String> {
    let is_word = |w: &str| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    let mut out = Vec::new();
    let mut rest = line;
    while let Some((_, after)) = rest.split_once("`isoloom ") {
        let Some((command, tail)) = after.split_once('`') else { break };
        if let Some(first) = command.split_whitespace().next().filter(|w| is_word(w)) {
            out.push(first.to_owned());
        }
        rest = tail;
        // A list after it: `, `run` and `check``.
        while let Some(next) = rest
            .strip_prefix(", `")
            .or_else(|| rest.strip_prefix(" and `"))
            .or_else(|| rest.strip_prefix(" or `"))
        {
            let Some((word, after_word)) = next.split_once('`') else { break };
            if !is_word(word) {
                break;
            }
            out.push(word.to_owned());
            rest = after_word;
        }
    }
    out
}

#[test]
fn the_code_docs_name_only_commands_the_cli_has() -> Outcome {
    let known = commands()?;
    assert!(known.iter().any(|c| c == "generate"), "{known:?}");
    let mut files = Vec::new();
    sources(&root().join("crates"), &mut files)?;
    let mut wrong = Vec::new();
    for file in files {
        for (n, line) in std::fs::read_to_string(&file)?.lines().enumerate() {
            let doc = line.trim_start();
            if !(doc.starts_with("//!") || doc.starts_with("///")) {
                continue;
            }
            for command in named(doc) {
                if !known.contains(&command) {
                    wrong.push(format!("{}:{}: `{command}`", file.display(), n + 1));
                }
            }
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
    assert_eq!(named("`isoloom generate`, `run` and `check` do"), ["generate", "run", "check"]);
    Ok(())
}

#[test]
fn the_changelog_and_the_migration_guide_name_the_published_service_kubernetes_writes() -> Outcome {
    let home = scratch("service-name-home")?;
    let dir = project(
        "service-name",
        "version: 1\nname: s\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, ports: { http: { port: 80, publish: 8080 } }, app: { image: nginx } }\n",
    )?;
    let o = isoloom(&home, &[&"generate", &dir, &"--target", &"kubernetes"])?;
    assert_eq!(o.code, Some(0), "{o:?}");
    let services = std::fs::read_to_string(dir.join(".isoloom/kubernetes/services.yaml")).or_else(|_| {
        // The Services are wherever the manifests put them.
        let mut all = String::new();
        for entry in std::fs::read_dir(dir.join(".isoloom/kubernetes"))? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "yaml") {
                all.push_str(&std::fs::read_to_string(path)?);
            }
        }
        Ok::<String, std::io::Error>(all)
    })?;
    assert!(services.contains("name: isoloom-web\n"), "{services}");
    let pattern = "`isoloom-<machine>`";
    let changelog = std::fs::read_to_string(root().join("CHANGELOG.md"))?;
    let entry = changelog
        .split("\n- ")
        .find(|e| e.contains("published port is a cluster-internal Service"))
        .ok_or("the changelog's entry")?;
    assert!(entry.contains(pattern), "{entry}");
    let guide = std::fs::read_to_string(root().join("docs/guide/migrating-to-2.0.md"))?;
    assert!(guide.contains(&format!("Service is {pattern}")), "the migration guide");
    Ok(())
}

#[test]
fn the_matrix_states_each_targets_facts_not_a_roadmap() -> Outcome {
    let matrix = std::fs::read_to_string(root().join("docs/matrix.md"))?;
    for promise in ["come later", "comes later", "comes next", "for now", "so far", " yet"] {
        let found: Vec<&str> = matrix.lines().filter(|l| l.contains(promise)).collect();
        assert!(found.is_empty(), "`{promise}`: {found:#?}");
    }
    Ok(())
}
