//! The README's targets table against the targets: its "Run manifest" column says yes for a
//! target exactly when that target writes a `run.json` (it once said no for `esxi`, which writes
//! one).

use isoloom_spec::model::Target;
use isoloom_targets::{Lab, generate};

use crate::testing::{Outcome, examples};

/// The README's targets table: each target's id and its "Run manifest" cell.
fn run_manifest_column() -> Outcome<Vec<(String, String)>> {
    let readme = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../README.md"))?;
    let rows = readme
        .lines()
        .skip_while(|l| !l.starts_with("| Target |"))
        .skip(2)
        .take_while(|l| l.starts_with('|'));
    rows.map(|row| {
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        match cells.as_slice() {
            ["", target, _, _, manifest, _, ""] => Ok((target.trim_matches('`').to_owned(), (*manifest).to_owned())),
            _ => Err(format!("a targets row of another shape: {row}").into()),
        }
    })
    .collect()
}

#[test]
fn the_readme_says_which_targets_write_a_run_manifest() -> Outcome {
    let mut labs = Vec::new();
    for entry in std::fs::read_dir(examples())? {
        let dir = entry?.path();
        if dir.join("isoloom.yml").is_file() {
            labs.push(Lab::new(isoloom_spec::load(&dir)?));
        }
    }
    let column = run_manifest_column()?;
    assert_eq!(column.len(), Target::ALL.len(), "{column:?}");
    for (id, cell) in column {
        let target = Target::ALL.into_iter().find(|t| t.id() == id).ok_or_else(|| format!("no target `{id}`"))?;
        let files = labs
            .iter()
            .find_map(|lab| generate(lab, target).ok())
            .ok_or_else(|| format!("no example generates `{id}`"))?;
        let writes = files.iter().any(|f| f.path.ends_with("run.json"));
        assert_eq!(cell.starts_with("yes"), writes, "`{id}`: the README says {cell:?}");
    }
    Ok(())
}
