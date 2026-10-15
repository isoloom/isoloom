//! The user documentation stays true: docs/spec.md is what `isoloom docs --spec` prints, every
//! spec in the docs (a YAML block whose first line is `# isoloom.yml`) validates, and every link
//! between the docs reaches a file and, with an anchor, a heading in it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::{Path, PathBuf};

use isoloom_spec::reference::{markdown, slug};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The README, the contributor and community guides, the pull request template, and every page
/// under docs/.
fn pages() -> Vec<PathBuf> {
    let top = [
        "README.md",
        ".github/CONTRIBUTING.md",
        ".github/CODE_OF_CONDUCT.md",
        ".github/SECURITY.md",
        ".github/pull_request_template.md",
    ];
    let mut out: Vec<PathBuf> = top.iter().map(|p| root().join(p)).collect();
    let mut dirs = vec![root().join("docs")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn shown(path: &Path) -> String {
    path.strip_prefix(root()).unwrap_or(path).display().to_string()
}

#[test]
fn docs_spec_is_up_to_date() {
    let on_disk = std::fs::read_to_string(root().join("docs/spec.md")).unwrap_or_default();
    assert!(on_disk == markdown(), "docs/spec.md is stale; run `cargo run -q -- docs --spec > docs/spec.md`");
}

/// The fenced code blocks of a page: (info string, first line number, body).
fn blocks(text: &str) -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    let mut open: Option<(String, usize, String)> = None;
    for (n, line) in text.lines().enumerate() {
        let fence = line.trim_start().strip_prefix("```");
        match (&mut open, fence) {
            (None, Some(info)) => open = Some((info.trim().to_string(), n + 1, String::new())),
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
fn every_spec_in_the_docs_validates() {
    let mut problems = Vec::new();
    let mut seen = 0;
    for page in pages() {
        let text = std::fs::read_to_string(&page).unwrap();
        for (info, line, body) in blocks(&text) {
            if !matches!(info.as_str(), "yaml" | "yml") || body.lines().next().map(str::trim) != Some("# isoloom.yml") {
                continue;
            }
            seen += 1;
            let at = format!("{}:{line}", shown(&page));
            match isoloom_spec::parse(&body) {
                Err(e) => problems.push(format!("{at}: {e}")),
                Ok(spec) => problems.extend(isoloom_spec::validate(&spec).iter().map(|p| format!("{at}: {p}"))),
            }
        }
    }
    assert!(seen >= 1, "no spec found: mark a spec's block with `# isoloom.yml` as its first line");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
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
        for (i, _) in prose.match_indices("](") {
            let rest = &prose[i + 2..];
            let Some(end) = rest.find(')') else { continue };
            let target = rest[..end].split_whitespace().next().unwrap_or_default();
            if !target.is_empty() && !target.contains("://") && !target.starts_with("mailto:") {
                out.push(target.to_string());
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
fn links_between_the_docs_resolve() {
    let mut broken = Vec::new();
    for page in pages() {
        let text = std::fs::read_to_string(&page).unwrap();
        for link in local_links(&text) {
            let (file, anchor) = link.split_once('#').unwrap_or((link.as_str(), ""));
            let target = if file.is_empty() { page.clone() } else { page.parent().unwrap().join(file) };
            if !target.exists() {
                broken.push(format!("{}: {link}: no such file", shown(&page)));
                continue;
            }
            let is_page = target.extension().is_some_and(|e| e == "md");
            if !anchor.is_empty() && is_page && !anchors(&std::fs::read_to_string(&target).unwrap()).iter().any(|a| a == anchor) {
                broken.push(format!("{}: {link}: no such heading", shown(&page)));
            }
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

#[test]
fn the_community_pages_are_checked() {
    let checked = pages();
    for page in [".github/SECURITY.md", ".github/CODE_OF_CONDUCT.md", ".github/CONTRIBUTING.md", "docs/spec.md"] {
        assert!(checked.iter().any(|p| p.ends_with(page)), "{page} isn't among the checked pages");
    }
}

#[test]
fn the_checks_read_blocks_links_and_headings() {
    let page = "# T\n\nSee [a](a.md#x-y), [web](https://x.y), `[not](here)`.\n\n```yaml\n# isoloom.yml\n[no](link)\n```\n## X y\n";
    assert_eq!(local_links(page), ["a.md#x-y"]);
    assert_eq!(anchors(page), ["t", "x-y"]);
    let b = blocks(page);
    assert_eq!((b.len(), b[0].0.as_str(), b[0].1), (1, "yaml", 5));
}
