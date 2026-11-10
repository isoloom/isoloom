//! The duplicate-code check: jscpd over the Rust sources (every crate and the xtask), each block
//! found twice (at least 50 tokens over 5 lines) counted in the crate of its first copy. It runs
//! a pinned jscpd through `npx`, so it needs Node.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::{Counts, Crate, add, crate_of};
use crate::{Res, process, repo};

/// The jscpd release the counts are measured with.
const JSCPD: &str = "jscpd@5.4.1";

/// Runs jscpd and counts its clones per crate. `slug` prefixes the temporary report folder, so
/// two repositories' runs never share one.
pub(crate) fn measure(root: &Path, crates: &[Crate], counts: &mut Counts, slug: &str) -> Res<()> {
    let out = std::env::temp_dir().join(format!("{slug}-jscpd-{}", std::process::id()));
    let npx = process::program("npx", &std::env::var_os("PATH").unwrap_or_default());
    let mut cmd = Command::new(npx);
    cmd.args(["--yes", JSCPD, "--silent", "--absolute", "-f", "rust", "-r", "json", "-o"]).arg(&out);
    cmd.args(["crates", "xtask/src"]).current_dir(root);
    let ran = process::quiet(&mut cmd).map_err(|e| format!("jscpd (through npx, which needs Node): {e}"));
    let report = ran.and_then(|()| std::fs::read_to_string(out.join("jscpd-report.json")).map_err(|e| e.to_string()));
    // The report folder goes; one left behind in the temporary folder is harmless.
    repo::remove(&out).ok();
    let report: Value = serde_json::from_str(&report?)?;
    let base = format!("{}/", repo::slashed(root));
    for file in first_files(&report)? {
        let file = repo::slashed(Path::new(file));
        add(counts, "dup/clone", &crate_of(crates, file.strip_prefix(&base).unwrap_or(&file)));
    }
    Ok(())
}

/// The file of each clone's first copy in jscpd's `report`. A report without its `duplicates`
/// list, or a clone without its first file, is an error: a jscpd that wrote another format would
/// otherwise count no clone at all, and the gate would pass unseen.
fn first_files(report: &Value) -> Res<Vec<&str>> {
    let clones = report
        .get("duplicates")
        .and_then(Value::as_array)
        .ok_or("jscpd's report has no `duplicates` list: its format changed")?;
    clones
        .iter()
        .map(|clone| {
            clone
                .get("firstFile")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| format!("a clone in jscpd's report has no `firstFile.name`: {clone}").into())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_without_its_clones_list_or_a_clone_without_its_file_is_an_error() {
        let ok = serde_json::json!({ "duplicates": [{ "firstFile": { "name": "/r/crates/a/src/x.rs" } }] });
        assert_eq!(first_files(&ok).unwrap(), ["/r/crates/a/src/x.rs"]);
        assert_eq!(first_files(&serde_json::json!({ "duplicates": [] })).unwrap(), Vec::<&str>::new());
        assert!(first_files(&serde_json::json!({ "clones": [] })).is_err());
        assert!(first_files(&serde_json::json!({ "duplicates": [{ "secondFile": {} }] })).is_err());
    }
}
