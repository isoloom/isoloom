//! The workspace's size rule: a source file under `crates/*/src` holds at most 400 lines of code,
//! so a reviewer can read one concept per file. Blank lines, comment-only lines and the inline
//! `#[cfg(test)] mod tests` block don't count, and a separate `tests.rs` is not counted at all.
//! A file over the limit is split into submodules, or its inline tests move to a `tests.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::{Path, PathBuf};

/// Lines of code a source file may have.
const MAX_CODE_LINES: usize = 400;

/// The workspace's `crates` directory.
fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Every `.rs` file under `dir`, recursively.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// The lines of `src` that count: not blank, not a comment, not inside the inline test module.
/// rustfmt closes a top-level module with a `}` alone at column 0, which ends the test block.
fn code_lines(src: &str) -> usize {
    let lines: Vec<&str> = src.lines().collect();
    let mut count = 0;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        let next = lines.get(i + 1).map_or("", |l| l.trim());
        if line == "#[cfg(test)]" && next.starts_with("mod tests") && next.ends_with('{') {
            i += 2;
            while i < lines.len() && lines[i] != "}" {
                i += 1;
            }
        } else if !line.is_empty() && !line.starts_with("//") {
            count += 1;
        }
        i += 1;
    }
    count
}

/// Each counted source file, as `crate/src/path.rs`, and its lines of code.
fn sizes() -> Vec<(String, usize)> {
    let root = crates_dir();
    let mut files = Vec::new();
    for krate in std::fs::read_dir(&root).unwrap().flatten() {
        let src = krate.path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files);
        }
    }
    let mut sizes: Vec<(String, usize)> = files
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|n| n != "tests.rs"))
        .map(|path| {
            let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            (rel, code_lines(&std::fs::read_to_string(&path).unwrap()))
        })
        .collect();
    sizes.sort();
    sizes
}

#[test]
fn source_files_stay_under_the_size_limit() {
    let sizes = sizes();
    assert!(sizes.len() > 50, "only {} source files found", sizes.len());
    let over: Vec<String> = sizes
        .iter()
        .filter(|(_, lines)| *lines > MAX_CODE_LINES)
        .map(|(rel, lines)| {
            format!("crates/{rel} has {lines} lines of code (over {MAX_CODE_LINES}): split it into submodules, or move its tests to a tests.rs")
        })
        .collect();
    assert!(over.is_empty(), "{}", over.join("\n"));
}

#[test]
fn comments_blanks_and_the_test_module_are_not_counted() {
    let src = "//! Doc.\n\nfn a() {}\n    // note\n/// item doc\nfn b() {\n}\n#[cfg(test)]\nmod tests {\n    fn t() {\n    }\n}\nfn after() {}\n";
    assert_eq!(code_lines(src), 4);
}
