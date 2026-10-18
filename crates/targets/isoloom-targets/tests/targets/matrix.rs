//! The capability matrix against what the generators decide about the repository's examples:
//! every refusal a generator gives an example shows in the matrix, on a row the example uses, with
//! the same step and reason; and no row an example uses is refused where that example is
//! generated. So the matrix never claims more than the code does, and its refusals are real.

use std::path::Path;

use isoloom_spec::model::Target;
use isoloom_targets::decision::{Code, Column, Decision, Refused, decide};
use isoloom_targets::matrix::{Authored, Cell, Matrix, Row, matrix};
use isoloom_targets::{Lab, cloud_refusals, refusal, refusal_on};

use crate::testing::{Outcome, examples};

/// An example: its name, its lab and its spec as written.
struct Example {
    name: String,
    lab: Lab,
    written: Authored,
}

/// Every example of the repository.
fn every_example() -> Outcome<Vec<Example>> {
    let mut dirs: Vec<_> = std::fs::read_dir(examples())?.map(|e| e.map(|e| e.path())).collect::<Result<_, _>>()?;
    dirs.retain(|d| d.join("isoloom.yml").is_file());
    dirs.sort();
    dirs.iter().map(|d| example(d)).collect()
}

fn example(dir: &Path) -> Outcome<Example> {
    let text = std::fs::read_to_string(dir.join("isoloom.yml"))?;
    Ok(Example {
        name: dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        lab: Lab::new(isoloom_spec::load(dir)?),
        written: Authored::parse(&text).ok_or("not YAML")?,
    })
}

/// A reason with what changes from one spec to another left out: the names in backquotes and
/// the numbers.
fn shape(reason: &str) -> String {
    let mut out = String::new();
    let mut quoted = false;
    let mut digits = false;
    for c in reason.chars() {
        if c == '`' {
            quoted = !quoted;
            if quoted {
                out.push_str("`…`");
            }
        } else if !quoted && c.is_ascii_digit() {
            if !digits {
                out.push('#');
            }
        } else if !quoted {
            out.push(c);
        }
        digits = !quoted && c.is_ascii_digit();
    }
    out
}

/// Whether the matrix shows `given` in `column` on a row the example uses: a lowering refusal
/// at the row's field, a capability's on a row asking for it, any other on a row the spec uses;
/// in each case with the same step and reason.
fn shown(matrix: &Matrix, example: &Example, column: &Column, given: &Refused) -> bool {
    matrix.rows.iter().any(|row| {
        let Some(Cell::Refused(shows)) = matrix.cell(row, column) else {
            return false;
        };
        let attributed = match given.code {
            Code::Lowering => given.field.as_deref().is_some_and(|field| row.is_field(field)),
            Code::Capability(feature) => row.features.contains(&feature),
            _ => row.used_by(&example.written, &example.lab, column.target),
        };
        attributed && shows.code == given.code && shape(&shows.reason) == shape(&given.reason)
    })
}

/// The rows `e` uses on `column` that the matrix shows refused there.
fn refused_but_used<'a>(m: &'a Matrix, e: &Example, column: &Column) -> Vec<&'a Row> {
    m.rows
        .iter()
        .filter(|row| matches!(m.cell(row, column), Some(Cell::Refused(_))) && row.used_by(&e.written, &e.lab, column.target))
        .collect()
}

#[test]
fn every_refusal_a_generator_gives_an_example_shows_in_the_matrix() -> Outcome {
    let m = matrix()?;
    let mut missing = Vec::new();
    let mut checked = 0;
    for e in every_example()? {
        for column in &m.columns {
            if let Decision::Refused(r) = decide(&e.lab, column) {
                checked += 1;
                if !shown(&m, &e, column, &r) {
                    missing.push(format!("{} on {}: {} {:?}: {}", e.name, column.id(), r.code.id(), r.field, r.reason));
                }
            }
        }
    }
    assert!(missing.is_empty(), "refusals the matrix doesn't show:\n{}", missing.join("\n"));
    assert!(checked > 200, "only {checked} refusals among the examples");
    Ok(())
}

#[test]
fn no_row_an_example_uses_is_refused_where_the_example_is_generated() -> Outcome {
    let m = matrix()?;
    let mut wrong = Vec::new();
    let mut used = 0;
    for e in every_example()? {
        for column in m.columns.iter().filter(|c| decide(&e.lab, c) == Decision::Generated) {
            used += m.rows.iter().filter(|row| row.used_by(&e.written, &e.lab, column.target)).count();
            for row in refused_but_used(&m, &e, column) {
                wrong.push(format!("{} on {}: {} ({})", e.name, column.id(), row.name(), row.about));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "rows the matrix refuses where an example using them is generated:\n{}",
        wrong.join("\n")
    );
    assert!(used > 1000, "the examples use only {used} rows where they are generated");
    Ok(())
}

/// The generator's own answer for `column`: `None` when it generates the lab there.
fn generators(lab: &Lab, column: &Column) -> Option<String> {
    match (column.target, column.variant) {
        (Target::Vagrant, Some(p)) => refusal_on(lab, Target::Vagrant, Some(p)),
        (Target::CloudVm, Some(cloud)) => refusal(lab, Target::CloudVm)
            .or_else(|| cloud_refusals(lab, Target::CloudVm).and_then(|c| c.refused.into_iter().find(|r| r.cloud == cloud).map(|r| r.reason))),
        (t, _) => refusal(lab, t),
    }
}

#[test]
fn each_column_decides_what_its_generator_does() -> Outcome {
    let m = matrix()?;
    for e in every_example()? {
        for column in &m.columns {
            let theirs = generators(&e.lab, column);
            let at = format!("{} on {}", e.name, column.id());
            match decide(&e.lab, column) {
                Decision::Generated => {
                    assert_eq!(theirs, None, "{at}");
                    assert!(e.lab.possible().contains(&column.target), "{at}");
                }
                Decision::NotApplicable => assert!(theirs.is_none() && !e.lab.possible().contains(&column.target), "{at}"),
                Decision::Refused(r) => {
                    let theirs = theirs.ok_or_else(|| format!("{at}: the generator generates it"))?;
                    // Every cloud refusing reads `no cloud can run it (<cloud>: <reason>; ...)`.
                    assert!(theirs.ends_with(&r.reason) || theirs.contains(&r.reason), "{at}: {theirs} / {}", r.reason);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn docs_matrix_md_is_what_isoloom_docs_matrix_prints() -> Outcome {
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/matrix.md");
    let on_disk = std::fs::read_to_string(page).unwrap_or_default();
    assert!(
        on_disk == matrix()?.markdown(),
        "docs/matrix.md is stale; run `cargo run -q -p isoloom -- docs --matrix > docs/matrix.md`"
    );
    Ok(())
}
