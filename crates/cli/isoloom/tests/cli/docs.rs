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

/// What a roadmap says where a fact belongs: what a target, a check or an import does today.
const PROMISES: [&str; 7] = ["come later", "comes later", "comes next", "for now", "so far", " yet", "(yet)"];

/// Every Markdown page under `dir`.
fn pages(dir: &Path, out: &mut Vec<PathBuf>) -> Outcome {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            pages(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    Ok(())
}

/// The README and every page of the docs, the generated matrix among them, state facts, not a
/// roadmap (the changelog, a record of what changed when, aside).
#[test]
fn the_docs_state_facts_not_a_roadmap() -> Outcome {
    let mut files = vec![root().join("README.md")];
    pages(&root().join("docs"), &mut files)?;
    assert!(files.iter().any(|f| f.ends_with("docs/matrix.md")), "the matrix is a page of the docs");
    let mut found = Vec::new();
    for file in &files {
        for (n, line) in std::fs::read_to_string(file)?.lines().enumerate() {
            if let Some(promise) = PROMISES.iter().find(|p| line.contains(**p)) {
                found.push(format!("{}:{}: `{promise}` in {line:?}", file.display(), n + 1));
            }
        }
    }
    assert!(found.is_empty(), "{found:#?}");
    Ok(())
}

/// Rust source `src` up to its inline test module, which the sources keep last.
fn outside_tests(src: &str) -> &str {
    src.find("\n#[cfg(test)]\nmod tests {").map_or(src, |at| src.get(..at).unwrap_or(src))
}

/// A walk over Rust source, collecting its string literals with the line each starts on.
struct Literals {
    chars: Vec<char>,
    at: usize,
    line: usize,
    found: Vec<(usize, String)>,
}

impl Literals {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    /// Moves past `n` characters, counting the lines it crosses.
    fn skip(&mut self, n: usize) {
        for _ in 0..n {
            self.line += usize::from(self.peek(0) == Some('\n'));
            self.at += 1;
        }
    }

    /// Moves to just past `end` (or to the end of the source).
    fn skip_past(&mut self, end: &str) {
        let end: Vec<char> = end.chars().collect();
        while self.at < self.chars.len() && !self.chars.get(self.at..).unwrap_or_default().starts_with(&end) {
            self.skip(1);
        }
        self.skip(end.len());
    }

    /// A `"…"` string from here, its escapes as written.
    fn string(&mut self) {
        let (start, mut text) = (self.line, String::new());
        self.skip(1);
        while let Some(c) = self.peek(0) {
            match c {
                '"' => break,
                '\\' => {
                    text.extend(self.chars.get(self.at..self.at + 2).unwrap_or_default());
                    self.skip(2);
                }
                _ => {
                    text.push(c);
                    self.skip(1);
                }
            }
        }
        self.skip(1);
        self.found.push((start, text));
    }

    /// A raw string (`r"…"`, `r#"…"#`) from here, when one starts here.
    fn raw_string(&mut self) -> bool {
        let hashes = self.chars.get(self.at + 1..).unwrap_or_default().iter().take_while(|c| **c == '#').count();
        if self.peek(1 + hashes) != Some('"') {
            return false;
        }
        let start = self.line;
        self.skip(2 + hashes);
        let from = self.at;
        let close: String = std::iter::once('"').chain(std::iter::repeat_n('#', hashes)).collect();
        self.skip_past(&close);
        let end = self.at.saturating_sub(close.chars().count()).max(from);
        self.found.push((start, self.chars.get(from..end).unwrap_or_default().iter().collect()));
        true
    }

    /// One step from here: past a comment, a char literal or a string, else one character.
    fn step(&mut self) {
        let identifier_before = self.at.checked_sub(1).and_then(|i| self.chars.get(i)).is_some_and(|c| c.is_alphanumeric());
        match (self.peek(0), self.peek(1), self.peek(2)) {
            (Some('/'), Some('/'), _) => self.skip_past("\n"),
            (Some('/'), Some('*'), _) => self.skip_past("*/"),
            (Some('\''), Some('\\'), _) => {
                self.skip(2);
                self.skip_past("'");
            }
            (Some('\''), _, Some('\'')) => self.skip(3),
            (Some('"'), _, _) => self.string(),
            (Some('r'), Some('"' | '#'), _) if !identifier_before && self.raw_string() => {}
            _ => self.skip(1),
        }
    }
}

/// The string literals of Rust source `src`, outside comments, char literals and the inline
/// test module, each with the line it starts on. Enough of Rust's lexing for the repository's
/// own sources.
fn string_literals(src: &str) -> Vec<(usize, String)> {
    let mut walk = Literals {
        chars: outside_tests(src).chars().collect(),
        at: 0,
        line: 1,
        found: Vec::new(),
    };
    while walk.at < walk.chars.len() {
        walk.step();
    }
    walk.found
}

#[test]
fn the_string_literals_are_found_outside_comments_and_tests() {
    let src =
        "let a = \"one yet\"; // \"not this\"\nlet c = '\"';\nlet r = r#\"raw \"two\"\"#;\n/* \"no\" */\n#[cfg(test)]\nmod tests {\n    \"in a test\"\n}\n";
    let found: Vec<String> = string_literals(src).into_iter().map(|(_, s)| s).collect();
    assert_eq!(found, ["one yet", "raw \"two\""]);
}

/// What the code says to its users (validation and refusal messages, an import's notes and
/// drafts, the generated files' text) states a fact, not a roadmap, as the matrix does: every
/// string literal of the crates' and xtask's sources, their tests aside.
#[test]
fn what_the_code_says_states_facts_not_a_roadmap() -> Outcome {
    let mut files = Vec::new();
    sources(&root().join("crates"), &mut files)?;
    sources(&root().join("xtask/src"), &mut files)?;
    let is_test = |p: &Path| {
        p.components().any(|c| c.as_os_str() == "tests") || p.file_name().is_some_and(|n| n == "tests.rs" || n == "testing.rs" || n == "test_support.rs")
    };
    let mut found = Vec::new();
    for file in files.iter().filter(|f| !is_test(f)) {
        for (n, text) in string_literals(&std::fs::read_to_string(file)?) {
            if let Some(promise) = PROMISES.iter().find(|p| text.contains(**p)) {
                found.push(format!("{}:{n}: `{promise}` in {text:?}", file.display()));
            }
        }
    }
    assert!(found.is_empty(), "{found:#?}");
    Ok(())
}

/// Each line of the `files` that aren't tests (outside their test module) that `finds` names
/// something in, each as its file, its line and what it found.
fn scanned(files: &[PathBuf], is_test: impl Fn(&Path) -> bool, finds: impl Fn(&str) -> Option<&'static str>) -> Outcome<Vec<String>> {
    let mut found = Vec::new();
    for file in files.iter().filter(|f| !is_test(f)) {
        let text = std::fs::read_to_string(file)?;
        for (n, line) in outside_tests(&text).lines().enumerate() {
            if let Some(what) = finds(line) {
                found.push(format!("{}:{}: `{what}`", file.display(), n + 1));
            }
        }
    }
    Ok(found)
}

/// The backends and the renderers they share read each network's block, a machine's address and
/// the router's from the plan (`Environment::block`, `address_on`, `router6_on`, a machine's
/// `addresses`), as `isoloom_plan::addressing::cidr`'s doc says: none works them out again from
/// the spec, their tests aside.
#[test]
fn the_backends_read_the_blocks_and_the_router_from_the_plan() -> Outcome {
    let mut files = Vec::new();
    sources(&root().join("crates/backends"), &mut files)?;
    sources(&root().join("crates/core/isoloom-render/src"), &mut files)?;
    let is_test = |p: &Path| p.components().any(|c| c.as_os_str() == "tests") || p.file_name().is_some_and(|n| n == "tests.rs");
    let from_the_spec = [
        "addressing::address",
        "addressing::cidr",
        "addressing::{",
        "router::address(",
        "router_address(",
    ];
    let found = scanned(&files, is_test, |line| from_the_spec.iter().find(|w| line.contains(**w)).copied())?;
    assert!(found.is_empty(), "{found:#?}");
    Ok(())
}

/// The backends read what the plan holds from the plan, not the spec: a VM and its OS
/// (`Environment::vm`, `Vm`; the spec's `vm` block only for the provisioning steps it writes),
/// the checks and where each runs (`Environment::checks`, `checks_by_position`,
/// `default_position`) and whether the controller keeps running (`Controller::keep_running`).
#[test]
fn the_backends_read_vms_checks_and_the_controller_from_the_plan() -> Outcome {
    let mut files = Vec::new();
    sources(&root().join("crates/backends"), &mut files)?;
    let is_test = |p: &Path| p.components().any(|c| c.as_os_str() == "tests") || p.file_name().is_some_and(|n| n == "tests.rs" || n == "testing.rs");
    let from_the_spec = [
        ".vm.is_some()",
        "spec.checks",
        "spec().checks",
        "checks::by_position(",
        "checks::default_position(",
        "spec.controller.as_ref().is_some_and",
        "spec().controller.as_ref().is_some_and",
    ];
    let found = scanned(&files, is_test, |line| {
        let vm_block = line.contains(".vm.as_ref()") && !line.contains("provision");
        from_the_spec
            .iter()
            .find(|w| line.contains(**w))
            .copied()
            .or(vm_block.then_some(".vm.as_ref()"))
    })?;
    assert!(found.is_empty(), "{found:#?}");
    Ok(())
}

/// The code's docs say "environment" for what a spec describes, as every user-facing text does:
/// no doc comment calls it "the environment" or "an environment" outside a code span (`Lab`, the type, keeps its
/// name in `isoloom_plan::lab`).
#[test]
fn the_code_s_docs_say_environment_not_lab() -> Outcome {
    let mut files = Vec::new();
    sources(&root().join("crates"), &mut files)?;
    let lab = |prose: &str| {
        let words: Vec<&str> = prose.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
        words
            .windows(2)
            .any(|w| matches!(w[0], "the" | "The" | "a" | "A") && matches!(w[1], "lab" | "labs"))
    };
    let mut found = Vec::new();
    for file in files.iter().filter(|f| !f.ends_with("isoloom-plan/src/lab.rs")) {
        for (n, line) in std::fs::read_to_string(file)?.lines().enumerate() {
            let Some(doc) = line.trim_start().strip_prefix("///").or_else(|| line.trim_start().strip_prefix("//!")) else {
                continue;
            };
            if doc.split('`').step_by(2).any(lab) {
                found.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(found.is_empty(), "{found:#?}");
    Ok(())
}
