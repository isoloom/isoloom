//! The user documentation stays true: docs/spec.md is what `isoloom docs --spec` prints, every
//! spec in the docs (a YAML block whose first line is `# isoloom.yml`) validates, every
//! configuration file the docs and the crate's own docs show is read as that file is, and every
//! link between the docs reaches a file and, with an anchor, a heading in it. The docs are user
//! documentation: no decision records, maintainer notes or test-host material (`DEV_MARKERS`).

use std::error::Error;
use std::path::{Path, PathBuf};

use isoloom_spec::reference::{markdown, slug};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// Every file under `dir` with the extension `ext`, at any depth.
fn files_under(dir: &Path, ext: &str) -> Outcome<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(next) = dirs.pop() {
        for entry in std::fs::read_dir(&next)? {
            let path = entry?.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == ext) {
                out.push(path);
            }
        }
    }
    Ok(out)
}

/// The README, the contributor and community guides, the pull request template, and every page
/// under docs/.
fn pages() -> Outcome<Vec<PathBuf>> {
    let top = [
        "README.md",
        ".github/CONTRIBUTING.md",
        ".github/CODE_OF_CONDUCT.md",
        ".github/SECURITY.md",
        ".github/pull_request_template.md",
    ];
    let mut out: Vec<PathBuf> = top.iter().map(|p| root().join(p)).collect();
    out.extend(files_under(&root().join("docs"), "md")?);
    out.sort();
    Ok(out)
}

fn shown(path: &Path) -> String {
    path.strip_prefix(root()).unwrap_or(path).display().to_string()
}

#[test]
fn docs_spec_is_up_to_date() -> Outcome {
    let on_disk = std::fs::read_to_string(root().join("docs/spec.md"))?;
    assert!(on_disk == markdown(), "docs/spec.md is stale; run `cargo run -q -- docs --spec > docs/spec.md`");
    Ok(())
}

/// The fenced code blocks of a page: (info string, first line number, body).
fn blocks(text: &str) -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    let mut open: Option<(String, usize, String)> = None;
    for (n, line) in text.lines().enumerate() {
        let fence = line.trim_start().strip_prefix("```");
        match (&mut open, fence) {
            (None, Some(info)) => open = Some((info.trim().to_owned(), n + 1, String::new())),
            (Some(_), Some(_)) => out.extend(open.take()),
            (Some((_, _, body)), None) => {
                body.push_str(line);
                body.push('\n');
            }
            (None, None) => {}
        }
    }
    out
}

#[test]
fn every_spec_in_the_docs_validates() -> Outcome {
    let mut problems = Vec::new();
    let mut seen = 0;
    for page in pages()? {
        let text = std::fs::read_to_string(&page)?;
        for (info, line, body) in blocks(&text) {
            if !matches!(info.as_str(), "yaml" | "yml") || body.lines().next().map(str::trim) != Some("# isoloom.yml") {
                continue;
            }
            seen += 1;
            let at = format!("{}:{line}", shown(&page));
            match isoloom_spec::parse(&body) {
                Err(e) => problems.push(format!("{at}: {e}")),
                Ok(spec) => problems.extend(isoloom_spec::syntax::check(&spec).iter().map(|p| format!("{at}: {p}"))),
            }
        }
    }
    assert!(seen >= 1, "no spec found: mark a spec's block with `# isoloom.yml` as its first line");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

/// The configuration file a block's first line names (`# images.yml`, `# isoloom.defaults.yml`,
/// `# profiles/<name>.yml`), checked as that file is read; `None` for another block.
fn config_problem(first: &str, body: &str) -> Option<Result<(), String>> {
    let file = first.trim().strip_prefix("# ")?;
    let checked = if file.ends_with("defaults.yml") {
        serde_yaml_ng::from_str(body).map_err(|e| e.to_string()).and_then(|value| {
            let layer = isoloom_spec::defaults::Layer {
                source: file.to_owned(),
                value,
            };
            isoloom_spec::defaults::resolve(&[layer]).map(drop).map_err(|e| e.to_string())
        })
    } else if file == "images.yml" {
        isoloom_spec::images::Table::parse(body).map(drop).map_err(|e| e.to_string())
    } else {
        let name = file.strip_prefix("profiles/").and_then(|f| f.strip_suffix(".yml"))?;
        isoloom_spec::profile::parse(name, PathBuf::from(file), body)
            .map(drop)
            .map_err(|e| e.to_string())
    };
    Some(checked)
}

/// The YAML blocks of the spec crate's own docs (`//!` and `///` comments), as a page's.
fn rustdoc_blocks() -> Outcome<Vec<(String, usize, String)>> {
    let mut out = Vec::new();
    for path in files_under(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), "rs")? {
        let docs: String = std::fs::read_to_string(&path)?
            .lines()
            .map(|l| {
                let l = l.trim_start();
                l.strip_prefix("//! ")
                    .or_else(|| l.strip_prefix("/// "))
                    .unwrap_or(if l == "//!" || l == "///" { "" } else { "\u{0}" })
            })
            .collect::<Vec<_>>()
            .join("\n");
        let label = |(info, line, body): (String, usize, String)| (info, line, format!("{}: {body}", shown(&path)));
        out.extend(blocks(&docs).into_iter().map(label));
    }
    Ok(out)
}

/// Every configuration file the docs and the spec crate's own docs show (a YAML block whose first
/// line names it: `# images.yml`, `# ~/.isoloom/defaults.yml`, `# profiles/<name>.yml`) is read
/// as that file is: the defaults module's example once had an OS name the image table refuses.
#[test]
fn every_configuration_file_in_the_docs_is_read() -> Outcome {
    let mut found = Vec::new();
    for page in pages()? {
        let text = std::fs::read_to_string(&page)?;
        found.extend(
            blocks(&text)
                .into_iter()
                .map(|(info, line, body)| (info, format!("{}:{line}", shown(&page)), body)),
        );
    }
    for (info, line, labelled) in rustdoc_blocks()? {
        let (file, body) = labelled.split_once(": ").ok_or("a labelled block")?;
        found.push((info, format!("{file}:{line}"), body.to_owned()));
    }
    let mut problems = Vec::new();
    let mut seen = 0;
    for (info, at, body) in found {
        if !matches!(info.as_str(), "yaml" | "yml") {
            continue;
        }
        let Some(checked) = body.lines().next().and_then(|first| config_problem(first, &body)) else {
            continue;
        };
        seen += 1;
        if let Err(e) = checked {
            problems.push(format!("{at}: {e}"));
        }
    }
    assert!(seen >= 6, "{seen} configuration files found");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

/// The links of a page that point inside the repository, outside code blocks and code spans.
fn local_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let prose: String = line.split('`').step_by(2).collect::<Vec<_>>().join(" ");
        // Each `](` opens a link's target, up to its `)`.
        for after in prose.split("](").skip(1) {
            let Some((inside, _)) = after.split_once(')') else { continue };
            let target = inside.split_whitespace().next().unwrap_or_default();
            if !target.is_empty() && !target.contains("://") && !target.starts_with("mailto:") {
                out.push(target.to_owned());
            }
        }
    }
    out
}

/// The anchors of a page's headings.
fn anchors(text: &str) -> Vec<String> {
    let mut in_code = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        } else if !in_code && let Some(h) = line.strip_prefix('#') {
            out.push(slug(h.trim_start_matches('#').trim()));
        }
    }
    out
}

#[test]
fn links_between_the_docs_resolve() -> Outcome {
    let mut broken = Vec::new();
    for page in pages()? {
        let text = std::fs::read_to_string(&page)?;
        for link in local_links(&text) {
            let (file, anchor) = link.split_once('#').unwrap_or((link.as_str(), ""));
            let target = if file.is_empty() {
                page.clone()
            } else {
                page.parent().ok_or("a page has a folder")?.join(file)
            };
            if !target.exists() {
                broken.push(format!("{}: {link}: no such file", shown(&page)));
                continue;
            }
            let is_page = target.extension().is_some_and(|e| e == "md");
            if !anchor.is_empty() && is_page && !anchors(&std::fs::read_to_string(&target)?).iter().any(|a| a == anchor) {
                broken.push(format!("{}: {link}: no such heading", shown(&page)));
            }
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
    Ok(())
}

#[test]
fn the_community_pages_are_checked() -> Outcome {
    let checked = pages()?;
    for page in [".github/SECURITY.md", ".github/CODE_OF_CONDUCT.md", ".github/CONTRIBUTING.md", "docs/spec.md"] {
        assert!(checked.iter().any(|p| p.ends_with(page)), "{page} isn't among the checked pages");
    }
    Ok(())
}

#[test]
fn the_checks_read_blocks_links_and_headings() {
    let page = "# T\n\nSee [a](a.md#x-y), [web](https://x.y), `[not](here)`.\n\n```yaml\n# isoloom.yml\n[no](link)\n```\n## X y\n";
    assert_eq!(local_links(page), ["a.md#x-y"]);
    assert_eq!(anchors(page), ["t", "x-y"]);
    let b = blocks(page);
    assert_eq!((b.len(), b[0].0.as_str(), b[0].1), (1, "yaml", 5));
}

/// Words that mark the project's own process rather than user documentation, lowercase: decision
/// records, maintainer material and test-host or workflow vocabulary. Kept short and precise so
/// a page explaining a feature never trips one; `.github/CONTRIBUTING.md` lists them.
const DEV_MARKERS: [&str; 18] = [
    "decision record",
    "status: accepted",
    "status: proposed",
    "status: superseded",
    "we decided",
    "we chose",
    "docs/dev/",
    "real-host",
    "self-hosted runner",
    "test rig",
    "the rig",
    "worktree",
    "dev branch",
    "test gate",
    "release gate",
    "coding agent",
    "ai agent",
    "subagent",
];

/// The folders under docs/ that hold no user documentation.
const DEV_FOLDERS: [&str; 2] = ["adr", "dev"];

/// Whether `text` at byte `at` starts a word (no letter or digit just before it).
fn starts_word(text: &str, at: usize) -> bool {
    !text[..at].chars().next_back().is_some_and(char::is_alphanumeric)
}

/// Whether `marker`, found in `text` at byte `at`, is whole words there: a word starts at it and,
/// when it ends with a letter, a word (or its plural) ends with it.
fn is_whole(text: &str, at: usize, marker: &str) -> bool {
    let rest = &text[at + marker.len()..];
    let rest = rest.strip_prefix('s').unwrap_or(rest);
    let ends = !marker.ends_with(char::is_alphanumeric) || !rest.starts_with(char::is_alphanumeric);
    starts_word(text, at) && ends
}

/// The developer artefacts a page's text holds: each marker found, a decision record's number
/// (`ADR 0004`, `adr-0001`) and a GitHub link pinned to a commit (`/blob/<hex>/`, `/tree/<hex>/`).
fn dev_markers(text: &str) -> Vec<String> {
    let lower = text.to_lowercase().replace('*', "");
    let mut found: Vec<String> = DEV_MARKERS
        .iter()
        .filter(|m| lower.match_indices(*m).any(|(at, _)| is_whole(&lower, at, m)))
        .map(|m| (*m).to_owned())
        .collect();
    for (at, _) in lower.match_indices("adr") {
        let rest = &lower[at + 3..];
        let number = rest.strip_prefix([' ', '-']).unwrap_or_default();
        if starts_word(&lower, at) && number.len() >= 4 && number[..4].bytes().all(|b| b.is_ascii_digit()) {
            found.push(format!("adr {}", &number[..4]));
        }
    }
    for kind in ["/blob/", "/tree/"] {
        for (at, _) in lower.match_indices(kind) {
            let rest = &lower[at + kind.len()..];
            let hex = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
            if (7..=40).contains(&hex) && rest[hex..].starts_with('/') {
                found.push(format!("a link pinned to commit {}", &rest[..hex]));
            }
        }
    }
    found
}

#[test]
fn the_docs_hold_no_developer_artefacts() -> Outcome {
    let docs = root().join("docs");
    let mut problems = Vec::new();
    let mut dirs = vec![docs.clone()];
    while let Some(next) = dirs.pop() {
        for entry in std::fs::read_dir(&next)? {
            let path = entry?.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| DEV_FOLDERS.iter().any(|d| n == *d)) {
                    problems.push(format!("{}: a folder of maintainer material; keep it out of docs/", shown(&path)));
                }
                dirs.push(path);
            }
        }
    }
    for page in files_under(&docs, "md")? {
        for marker in dev_markers(&std::fs::read_to_string(&page)?) {
            problems.push(format!("{}: `{marker}` is the project's process, not user documentation", shown(&page)));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

#[test]
fn the_markers_find_decision_records_and_spare_user_text() {
    let record = "# 0004 — Seal\n\n**Status:** Accepted (2026-10).\n\nSee [ADR 0001](adr/0001-x.md) and \
                  [the CI](https://github.com/o/r/blob/a0a628feeda71c6891e821d9fbc32c1c8de34907/docs/dev/ci.md).";
    assert_eq!(
        dev_markers(record),
        [
            "status: accepted",
            "docs/dev/",
            "adr 0001",
            "a link pinned to commit a0a628feeda71c6891e821d9fbc32c1c8de34907"
        ]
    );
    assert_eq!(
        dev_markers("The real-host CI runs on a self-hosted runner, in a worktree of the dev branch."),
        ["real-host", "self-hosted runner", "worktree", "dev branch"]
    );
    let user = "Install the guest agent (`qemu-guest-agent`). Clone with `--branch v3.0.0`, see \
                [the release](https://github.com/o/r/blob/v3.0.0/README.md) and a cheaper (ADRs aside) padr 0001, the right worktreeish gate.";
    assert_eq!(dev_markers(user), Vec::<String>::new());
}
