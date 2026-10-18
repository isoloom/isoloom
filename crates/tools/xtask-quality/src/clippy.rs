//! Clippy over every crate and its tests, read as JSON: each warning is a finding (counted by
//! the ratchet), each error fails the run (a lint at deny, or code that doesn't build).

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

use super::{Counts, Crate, add, crate_of};
use crate::{Res, process};

/// One warning: its rule and where it points (a file relative to the root, line, column).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Finding {
    rule: String,
    file: String,
    line: u64,
    column: u64,
}

/// Runs clippy and counts its warnings per crate; fails on an error.
pub(crate) fn measure(root: &Path, crates: &[Crate], counts: &mut Counts) -> Res<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.args(["clippy", "--workspace", "--all-targets", "--message-format=json", "--manifest-path"]);
    cmd.arg(root.join("Cargo.toml")).stderr(Stdio::inherit());
    let out = cmd.output().map_err(|e| format!("`{}` couldn't start: {e}", process::describe(&cmd)))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (findings, errors) = read(&text);
    if !errors.is_empty() {
        return Err(format!("clippy found errors:\n{}", errors.join("\n")).into());
    }
    if !out.status.success() {
        return Err(format!("`{}` failed ({})", process::describe(&cmd), out.status).into());
    }
    for f in &findings {
        add(counts, &f.rule, &crate_of(crates, &f.file));
    }
    Ok(())
}

/// The warnings in cargo's JSON messages, each place once (a library is checked as itself and
/// as its tests), and the rendered errors.
pub(crate) fn read(json_lines: &str) -> (BTreeSet<Finding>, Vec<String>) {
    let mut findings = BTreeSet::new();
    let mut errors = Vec::new();
    for msg in json_lines.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        let Some(diagnostic) = msg.get("message") else { continue };
        let level = diagnostic.get("level").and_then(Value::as_str).unwrap_or("");
        if level == "error" {
            errors.push(diagnostic.get("rendered").and_then(Value::as_str).unwrap_or("error").to_owned());
        }
        if level != "warning" {
            continue;
        }
        let code = diagnostic.get("code").and_then(|c| c.get("code")).and_then(Value::as_str);
        if let (Some(code), Some(span)) = (code, own_span(diagnostic)) {
            findings.insert(Finding {
                rule: rule_name(code),
                file: span.get("file_name").and_then(Value::as_str).unwrap_or("").replace('\\', "/"),
                line: span.get("line_start").and_then(Value::as_u64).unwrap_or(0),
                column: span.get("column_start").and_then(Value::as_u64).unwrap_or(0),
            });
        }
    }
    (findings, errors)
}

/// `clippy::too_many_lines` as `clippy/too_many_lines`, a rustc lint as `rustc/<lint>`.
fn rule_name(code: &str) -> String {
    code.strip_prefix("clippy::")
        .map_or_else(|| format!("rustc/{code}"), |lint| format!("clippy/{lint}"))
}

/// The span in the workspace's own sources a diagnostic points at, through macro expansions.
fn own_span(diagnostic: &Value) -> Option<&Value> {
    let spans = diagnostic.get("spans")?.as_array()?;
    let mut span = spans
        .iter()
        .find(|s| s.get("is_primary").and_then(Value::as_bool) == Some(true))
        .or_else(|| spans.first())?;
    while span.get("file_name").and_then(Value::as_str).is_some_and(|f| Path::new(f).is_absolute()) {
        match span.get("expansion").and_then(|e| e.get("span")) {
            Some(outer) => span = outer,
            None => break,
        }
    }
    Some(span)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warning_counts_once_and_an_error_is_kept_whole() {
        let warning = r#"{"reason":"compiler-message","message":{"level":"warning","code":{"code":"clippy::indexing_slicing"},"spans":[{"file_name":"crates/core/a/src/lib.rs","line_start":3,"column_start":5,"is_primary":true}]}}"#;
        let error =
            r#"{"reason":"compiler-message","message":{"level":"error","code":{"code":"E0308"},"rendered":"error[E0308]: mismatched types","spans":[]}}"#;
        let (findings, errors) = read(&format!("{warning}\n{warning}\n{error}\nnot json\n"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings.first().map(|f| f.rule.as_str()), Some("clippy/indexing_slicing"));
        assert_eq!(errors, ["error[E0308]: mismatched types"]);
        assert_eq!(rule_name("dead_code"), "rustc/dead_code");
    }
}
