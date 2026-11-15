//! What the sources themselves say: the workspace's crates (from `cargo metadata`), the files
//! over the size limit, and the exceptions (`#[expect]`).

use std::path::{Path, PathBuf};

use super::{Counts, Crate, add, path_in};
use crate::{Res, process, repo};

/// Lines of code a source file may have.
const MAX_CODE_LINES: usize = 400;

/// The workspace's crates, from `cargo metadata`.
pub(crate) fn crates(root: &Path) -> Res<Vec<Crate>> {
    let mut cmd = process::cargo();
    cmd.args(["metadata", "--format-version", "1", "--no-deps", "--offline", "--manifest-path"])
        .arg(root.join("Cargo.toml"));
    Ok(in_workspace(&serde_json::from_str(&process::output(&mut cmd)?)?))
}

/// What this tool reads of `cargo metadata --format-version 1`: the workspace's root and each
/// package's name and manifest. Unknown fields are skipped; a missing one is an error, never an
/// empty path that would put every crate in the wrong folder.
#[derive(Debug, serde::Deserialize)]
struct Metadata {
    workspace_root: String,
    packages: Vec<Package>,
}

/// A package of [`Metadata`].
#[derive(Debug, serde::Deserialize)]
struct Package {
    name: String,
    manifest_path: String,
}

/// The crates of `metadata`, each with its folder relative to the workspace's root (a package
/// outside it is left out).
fn in_workspace(metadata: &Metadata) -> Vec<Crate> {
    let base = repo::slashed(Path::new(&metadata.workspace_root));
    metadata
        .packages
        .iter()
        .filter_map(|p| {
            let manifest = repo::slashed(Path::new(&p.manifest_path));
            let dir = manifest.strip_suffix("/Cargo.toml")?.strip_prefix(&base)?.trim_start_matches('/').to_owned();
            Some(Crate { name: p.name.clone(), dir })
        })
        .collect()
}

/// Counts the files over the size limit (a crate's `src`, its `tests.rs` files aside) and the
/// `#[expect]` exceptions (anywhere in the crate).
pub(crate) fn measure(root: &Path, crates: &[Crate], counts: &mut Counts) -> Res<()> {
    for c in crates {
        let dir = path_in(root, &c.dir);
        for file in rust_files(&dir.join("src"))? {
            let text = std::fs::read_to_string(&file)?;
            if file.file_name().is_some_and(|n| n != "tests.rs") && code_lines(&text) > MAX_CODE_LINES {
                add(counts, "file/over-400-loc", &c.name);
            }
        }
        for file in rust_files(&dir)? {
            let text = std::fs::read_to_string(&file)?;
            for _ in 0..expectations(&text) {
                add(counts, "exception/expect", &c.name);
            }
        }
    }
    Ok(())
}

/// Every `.rs` file under `dir`, recursively, build folders aside; a folder that can't be read
/// is the error, never counted as empty.
fn rust_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                out.extend(rust_files(&path)?);
            }
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
    Ok(out)
}

/// The lines of `src` that count: not blank, not a comment, not inside the inline test module.
/// rustfmt closes a top-level module with a `}` alone at column 0, which ends the test block.
pub(crate) fn code_lines(src: &str) -> usize {
    let mut count = 0;
    let mut in_tests = false;
    let mut previous = "";
    for line in src.lines() {
        let trimmed = line.trim();
        if in_tests {
            in_tests = line != "}";
        } else if previous == "#[cfg(test)]" && trimmed.starts_with("mod tests") && trimmed.ends_with('{') {
            in_tests = true;
            count -= 1;
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            count += 1;
        }
        previous = trimmed;
    }
    count
}

/// How many `#[expect(` and `#![expect(` attributes `src` holds.
fn expectations(src: &str) -> usize {
    src.lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("#[expect(") || l.starts_with("#![expect("))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_metadata_gives_each_crate_with_its_folder() {
        // As `cargo metadata --format-version 1 --no-deps` prints it (fields trimmed to a few).
        let printed = r#"{"packages":[{"name":"isoloom","version":"2.0.0","id":"path+file:///w/crates/cli/isoloom#2.0.0","manifest_path":"/w/crates/cli/isoloom/Cargo.toml","targets":[]},{"name":"xtask","version":"0.0.0","manifest_path":"/w/xtask/Cargo.toml"},{"name":"elsewhere","manifest_path":"/other/Cargo.toml"}],"workspace_members":[],"resolve":null,"target_directory":"/w/target","version":1,"workspace_root":"/w","metadata":null}"#;
        let crates = in_workspace(&serde_json::from_str(printed).unwrap());
        let found: Vec<(&str, &str)> = crates.iter().map(|c| (c.name.as_str(), c.dir.as_str())).collect();
        assert_eq!(found, [("isoloom", "crates/cli/isoloom"), ("xtask", "xtask")]);
        assert!(serde_json::from_str::<Metadata>(r#"{"packages":[]}"#).is_err(), "no workspace root is an error");
    }

    #[test]
    fn comments_blanks_and_the_test_module_are_not_counted() {
        let src = "//! Doc.\n\nfn a() {}\n    // note\n/// item doc\nfn b() {\n}\n#[cfg(test)]\nmod tests {\n    fn t() {\n    }\n}\nfn after() {}\n";
        assert_eq!(code_lines(src), 4);
    }

    #[test]
    fn an_expectation_counts_once() {
        assert_eq!(expectations("#[expect(clippy::x, reason = \"y\")]\nfn a() {}\n    #![expect(dead_code)]\n"), 2);
    }

    #[test]
    fn a_crates_long_files_and_its_exceptions_are_counted_and_its_tests_file_and_build_folder_are_not() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("crates/a/src");
        std::fs::create_dir_all(src.join("deep")).unwrap();
        std::fs::create_dir_all(root.path().join("crates/a/target/x")).unwrap();
        let long = "fn f() {}\n".repeat(MAX_CODE_LINES + 1);
        std::fs::write(src.join("lib.rs"), "#[expect(dead_code, reason = \"r\")]\nfn a() {}\n").unwrap();
        std::fs::write(src.join("deep/long.rs"), &long).unwrap();
        std::fs::write(src.join("tests.rs"), &long).unwrap();
        std::fs::write(root.path().join("crates/a/target/x/built.rs"), "#[expect(x)]\n").unwrap();
        let crates = [Crate {
            name: "a".into(),
            dir: "crates/a".into(),
        }];
        let mut counts = Counts::new();
        measure(root.path(), &crates, &mut counts).unwrap();
        assert_eq!(counts["file/over-400-loc"]["a"], 1, "deep/long.rs, not tests.rs");
        assert_eq!(counts["exception/expect"]["a"], 1, "lib.rs's, not the build folder's");
    }

    #[test]
    fn the_crates_are_read_from_cargo_metadata() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Cargo.toml"), "[workspace]\nmembers = [\"crates/a\"]\nresolver = \"2\"\n").unwrap();
        std::fs::create_dir_all(root.path().join("crates/a/src")).unwrap();
        std::fs::write(
            root.path().join("crates/a/Cargo.toml"),
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(root.path().join("crates/a/src/lib.rs"), "").unwrap();
        let found: Vec<(String, String)> = crates(root.path()).unwrap().into_iter().map(|c| (c.name, c.dir)).collect();
        assert_eq!(found, [("a".to_owned(), "crates/a".to_owned())]);
    }

    #[test]
    fn a_folder_that_cant_be_read_is_an_error_not_an_empty_crate() {
        let missing = std::env::temp_dir().join(format!("xtask-quality-none-{}", std::process::id()));
        assert!(rust_files(&missing).is_err());
    }
}
