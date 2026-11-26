//! Guardrails over the product's sources, for what clippy can't express (clippy.toml holds the
//! rest: a process starts only in its tool's adapter, and `unwrap` stays in tests):
//!
//! - a shell script handed to `sh -c` (or `bash -c`) is quoted by `isoloom_spec::shell::quote`,
//!   never wrapped in hand-typed single quotes around an interpolated value, which a `'` in the
//!   value breaks out of;
//! - a root script's commands are separate lines, not `&&` chains: under `set -e` (the shell
//!   library, cloud-init's step scripts) a failure on the left of `&&` doesn't stop the script.
//!   The chains already there are counted per file ([`CHAINS`]), so a new one fails;
//! - a tool's JSON output is parsed only in the adapter that runs the tool.
//!
//! The product's sources are the crates' `src/` folders, tests left out (`tests/`, `tests.rs`, a
//! file from its first `#[cfg(test)]` on, the tests' helpers). The tests' own sources have theirs
//! in [`flakes`]: no wait on the clock, no free port let go before it is used.

mod flakes;

use std::path::{Path, PathBuf};

use crate::repo;

/// The crates that are tests' or maintainers' tools, not the product.
const NOT_PRODUCT: [&str; 2] = ["crates/tools/isoloom-test-support/", "crates/tools/xtask-quality/"];

/// Files of the product holding test helpers only (compiled for tests or the `test-support`
/// feature).
const TEST_FILES: [&str; 3] = ["tests.rs", "test_support.rs", "testing.rs"];

/// The root scripts' sources: the shell library a recipe sources under `set -eu`, and the Linux
/// steps cloud-init runs under `set -e` (or one `sh -c` each over SSH).
const ROOT_SCRIPTS: [&str; 4] = [
    "crates/core/isoloom-render/src/configure/shell/configure.sh",
    "crates/core/isoloom-render/src/steps.rs",
    "crates/core/isoloom-render/src/steps/",
    "crates/backends/isoloom-terraform/src/machine/controller.rs",
];

/// The `&&` chains the root scripts have, per file: each a chain whose left side's failure a
/// `set -e` script would not stop at. Rewriting one as separate lines changes the generated
/// files, so it is a change of its own (with its snapshots); until then each count is exact: a new
/// chain fails, and a removed one asks for the count to go down.
const CHAINS: [(&str, usize, &str); 8] = [
    (
        "crates/core/isoloom-render/src/configure/shell/configure.sh",
        16,
        "the library's functions end in `a && b` (their status is the function's) and its package installs chain `update && install && clean`",
    ),
    (
        "crates/core/isoloom-render/src/steps.rs",
        11,
        "the project unpack, sshd's AcceptEnv, the sysctl file, the state folder and the os-release reads are one-line steps or substitutions, each run as one `sh -c` whose status is the chain's",
    ),
    ("crates/core/isoloom-render/src/steps/disks.rs", 1, "awk's own `&&`, inside its program"),
    (
        "crates/core/isoloom-render/src/steps/files.rs",
        2,
        "a file's folder, write, owner and mode, one line per file",
    ),
    (
        "crates/core/isoloom-render/src/steps/process.rs",
        1,
        "`mkdir -p <dir> && cd <dir>`: the folder a process step runs in",
    ),
    (
        "crates/core/isoloom-render/src/steps/settings.rs",
        2,
        "the limits file and the fstab entry, each written in one line",
    ),
    (
        "crates/core/isoloom-render/src/steps/wait.rs",
        1,
        "the port probes of a wait, joined into the `until` condition",
    ),
    (
        "crates/backends/isoloom-terraform/src/machine/controller.rs",
        4,
        "the controller's apt install, authorized key and config folder, one-line steps run as one `sh -c` each",
    ),
];

/// The modules that run a tool and so may parse its output.
const TOOL_ADAPTERS: [&str; 3] = [
    "crates/cli/isoloom/src/commands/bases.rs",
    "crates/cli/isoloom/src/commands/graph.rs",
    "crates/cli/isoloom/src/commands/import/vagrant.rs",
];

/// A product source: its path from the repository root (with `/`) and its text, tests cut off.
struct Source {
    path: String,
    text: String,
}

/// Every product source under `crates/` with one of `extensions`.
fn sources(extensions: &[&str]) -> Vec<Source> {
    let root = repo::root();
    let mut files = Vec::new();
    walk(&root.join("crates"), &mut files);
    let mut out: Vec<Source> = files
        .into_iter()
        .filter(|f| f.extension().is_some_and(|e| extensions.iter().any(|x| e == *x)))
        .filter_map(|f| {
            let path = f.strip_prefix(&root).ok()?.to_string_lossy().replace('\\', "/");
            let name = f.file_name()?.to_string_lossy().into_owned();
            let product =
                path.contains("/src/") && !path.contains("/tests/") && !TEST_FILES.contains(&name.as_str()) && !NOT_PRODUCT.iter().any(|p| path.starts_with(p));
            if !product {
                return None;
            }
            let text = std::fs::read_to_string(&f).ok()?;
            let text = text.split("#[cfg(test)]").next().unwrap_or_default().to_owned();
            Some(Source { path, text })
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Every file under `dir`, `target` folders left out.
pub(super) fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !path.ends_with("target") {
                walk(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

/// Whether `line` wraps an interpolated value (`{`) in hand-typed single quotes after `-c`.
fn hand_quoted_script(line: &str) -> bool {
    line.match_indices("-c '").any(|(at, m)| {
        let rest = line.get(at + m.len()..).unwrap_or("");
        rest.split('\'').next().is_some_and(|quoted| quoted.contains('{'))
    })
}

/// Whether `line` of a root script chains commands with `&&` as a statement: not in a condition
/// (`if`, `while`, …), not a guard (`[ … ] &&`, `command -v … &&`, `grep -q … &&`, `… && return`),
/// not with its failure handled (`|| …`). In Rust sources only string text counts (a line with a `"`).
fn statement_chain(line: &str, rust: bool) -> bool {
    let trimmed = line.trim_start();
    if !line.contains("&&") || line.contains("||") || trimmed.starts_with("//") || (rust && !line.contains('"')) {
        return false;
    }
    if ["if ", "elif ", "while ", "until ", "[ ", "test "].iter().any(|k| trimmed.starts_with(k)) {
        return false;
    }
    let right = line.rsplit("&&").next().unwrap_or("").trim_start();
    if ["return", "continue", "break", "exit"].iter().any(|k| right.starts_with(k)) {
        return false;
    }
    let left = line.split("&&").next().unwrap_or("");
    let last = left.rsplit([';', '(']).next().unwrap_or(left);
    let last = last.rsplit("then").next().unwrap_or(last);
    !["[ ", "command -v", "grep -q", "test "].iter().any(|guard| last.contains(guard))
}

/// Whether `text` parses JSON from a process's output: `serde_json::from_str`/`from_slice` within
/// a few lines after a `.stdout`.
fn parses_tool_output(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("serde_json::from_str") || l.contains("serde_json::from_slice"))
        .filter(|(i, _)| lines.get(i.saturating_sub(6)..=*i).is_some_and(|w| w.iter().any(|l| l.contains(".stdout"))))
        .map(|(i, _)| i + 1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every symbolic link under `crates/` resolves: a crate's `LICENSE` and `NOTICE` link to the
    /// workspace's, and `cargo publish` can't package a link that leads nowhere (a crate moved to
    /// another depth keeps its old links).
    #[test]
    fn every_link_under_the_crates_resolves() {
        fn broken(dir: &Path, out: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
                let path = e.path();
                let kind = e.file_type().unwrap();
                if kind.is_symlink() && !path.exists() {
                    out.push(format!("{} -> {}", path.display(), std::fs::read_link(&path).unwrap().display()));
                } else if kind.is_dir() && e.file_name() != "target" {
                    broken(&path, out);
                }
            }
        }
        let mut found = Vec::new();
        broken(&repo::root().join("crates"), &mut found);
        assert_eq!(found, Vec::<String>::new(), "links that lead nowhere");
    }

    #[test]
    fn a_shell_script_is_quoted_by_the_quoting_module_not_by_hand() {
        let found: Vec<String> = sources(&["rs"])
            .iter()
            .flat_map(|s| {
                s.text
                    .lines()
                    .enumerate()
                    .filter(|(_, l)| hand_quoted_script(l))
                    .map(|(i, l)| format!("{}:{}: {}", s.path, i + 1, l.trim()))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            found.is_empty(),
            "an interpolated value in hand-typed quotes after `-c`: quote the script with isoloom_spec::shell::quote:\n{}",
            found.join("\n")
        );
    }

    #[test]
    fn the_hand_quoted_script_rule_fires_on_interpolation_only() {
        assert!(hand_quoted_script(r#"format!("sudo sh -c 'nft -f {SEALED} && cp {SEALED} x'")"#));
        assert!(hand_quoted_script(r#"format!("(bash -c '</dev/tcp/{host}/{p}' 2>/dev/null)")"#));
        assert!(!hand_quoted_script(r#"format!("sudo sh -c {}", quote(&script))"#));
        assert!(!hand_quoted_script(r#""sh -c 'exit 0'""#));
    }

    #[test]
    fn root_scripts_add_no_and_chain() {
        let sources: Vec<Source> = sources(&["rs", "sh"])
            .into_iter()
            .filter(|s| ROOT_SCRIPTS.iter().any(|r| s.path == *r || (r.ends_with('/') && s.path.starts_with(r))))
            .collect();
        assert!(
            sources.len() >= 4,
            "the root scripts' sources: {:?}",
            sources.iter().map(|s| &s.path).collect::<Vec<_>>()
        );
        let mut wrong = Vec::new();
        for s in &sources {
            let rust = Path::new(&s.path).extension().is_some_and(|e| e == "rs");
            let chains: Vec<String> = s
                .text
                .lines()
                .enumerate()
                .filter(|(_, l)| statement_chain(l, rust))
                .map(|(i, l)| format!("  {}:{}: {}", s.path, i + 1, l.trim()))
                .collect();
            let allowed = CHAINS.iter().find(|(p, ..)| *p == s.path).map_or(0, |(_, n, _)| *n);
            if chains.len() != allowed {
                wrong.push(format!(
                    "{}: {} `&&` chains, {allowed} allowed (xtask/src/guardrails.rs, CHAINS): write each command on its own line (`set -e` stops at it), or lower the count when one went away:\n{}",
                    s.path,
                    chains.len(),
                    chains.join("\n")
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn the_chain_rule_skips_conditions_guards_and_handled_failures() {
        assert!(statement_chain("  mkdir -p \"$d\" && cp a b", false));
        assert!(statement_chain(r#"format!("rm -rf {DIR} && mkdir -p {DIR}")"#, true));
        assert!(!statement_chain("  if [ -f a ] && [ -f b ]; then", false));
        assert!(!statement_chain("  [ \"$#\" -eq 0 ] && return 0", false));
        assert!(!statement_chain("  command -v x >/dev/null && return 0", false));
        assert!(!statement_chain("  ufw status | grep -Eq x && continue", false));
        assert!(!statement_chain("  mkdir a && cp b c || return 1", false));
        assert!(!statement_chain("    let both = a && b;", true));
    }

    #[test]
    fn only_a_tools_adapter_parses_its_output() {
        let found: Vec<String> = sources(&["rs"])
            .iter()
            .filter(|s| !TOOL_ADAPTERS.contains(&s.path.as_str()))
            .flat_map(|s| parses_tool_output(&s.text).into_iter().map(|line| format!("{}:{line}", s.path)))
            .collect();
        assert!(
            found.is_empty(),
            "a tool's JSON output parsed outside its adapter ({}): move the parsing into the module that runs the tool:\n{}",
            TOOL_ADAPTERS.join(", "),
            found.join("\n")
        );
    }

    #[test]
    fn the_output_rule_fires_on_json_read_from_stdout() {
        let code = "let out = cmd.output()?;\nlet v: Value = serde_json::from_slice(&out.stdout)?;\n";
        assert_eq!(parses_tool_output(code), [2]);
        assert_eq!(parses_tool_output("let v: Value = serde_json::from_str(text)?;\n"), Vec::<usize>::new());
    }
}
