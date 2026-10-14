//! The spec's side of coverage: for every field of an Isoloom spec, what each output does with
//! it, one column per output (each cloud of `cloud-vm` its own).
//!
//! A cell starts from what the field's rendering claims ([`claim`]): the Compose file's
//! claim holds for every target that runs it, the cloud VM core's for every cloud. Then each
//! output's backend is asked to render the field's probe spec ([`probes`]); a backend that
//! refuses it turns the cell into a refusal with the backend's own reason. So a column follows
//! the backends' capabilities and limits without anyone keeping it in step. The tests keep the
//! claims honest: every field of the format has a row, every claim names an example (or its
//! probe) that generates for the output, and no backend refuses what its own family claims.

mod families;
pub mod output;
mod paths;
pub mod probes;
mod render;
pub mod run;
pub mod status;

pub use families::fields::FIELDS;
pub use families::{parts, shared, status as stated};
pub use output::{Family, Output};
pub use paths::paths;
pub use render::{Score, score, section};
pub use status::Status;

/// What a field's rendering hasn't stated yet (the tests reject it).
const UNSTATED: Status = Status::Planned { note: "not stated yet" };

/// One cell of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    /// What the output's rendering states.
    Is(Status),
    /// The output's backend refuses a spec using the field, for this reason.
    Refused(String),
}

impl Cell {
    /// The mark the table shows for it.
    pub fn symbol(&self) -> &'static str {
        match self {
            Cell::Is(s) => s.symbol(),
            Cell::Refused(_) => "✗",
        }
    }

    /// How it's produced, or why not.
    pub fn note(&self) -> String {
        match self {
            Cell::Is(s) => s.note(),
            Cell::Refused(why) => format!("refused: {why}"),
        }
    }
}

/// One field of the spec (a path, `*` for names you choose, `[]` for list items) and its cell in
/// every output, in [`Output::all`] order.
#[derive(Debug, Clone)]
pub struct Row {
    /// The field's path (`machines.*.vm.os`).
    pub path: &'static str,
    /// Its cell in each output.
    pub cells: Vec<(Output, Cell)>,
}

impl Row {
    /// The field's cell in output `o`.
    pub fn cell(&self, o: Output) -> Option<&Cell> {
        self.cells.iter().find(|(x, _)| *x == o).map(|(_, c)| c)
    }
}

/// What the field's rendering states for `output`, before asking its backend: hybrid combines
/// Compose (its containers) and Vagrant (its VMs). `None` when the tables don't state it.
pub fn claim(path: &str, output: Output) -> Option<Status> {
    if output == Output::Esxi
        && let Some((_, s)) = families::links::ESXI.iter().find(|(p, _)| *p == path)
    {
        return Some(*s);
    }
    match output.family() {
        Some(f) => families::status(f, path),
        None => Some(families::status(Family::Compose, path)?.combine(families::status(Family::Vagrant, path)?)),
    }
}

/// The cell of `path` in `output`: its claim, unless the backend refuses the field's probe.
pub fn cell(path: &str, output: Output) -> Cell {
    let claim = claim(path, output).unwrap_or(UNSTATED);
    if !claim.applies() {
        return Cell::Is(claim);
    }
    match run::outcome(path, output) {
        run::Outcome::Refused(why) => Cell::Refused(why),
        _ => Cell::Is(claim),
    }
}

/// The table, in spec-reference order.
pub fn table() -> Vec<Row> {
    let outputs = Output::all();
    FIELDS
        .iter()
        .map(|path| Row {
            path,
            cells: outputs.iter().map(|o| (*o, cell(path, *o))).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_cell_says_why_and_an_unknown_output_has_no_cell() {
        let refused = Cell::Refused("no tc".to_string());
        assert_eq!((refused.symbol(), refused.note()), ("✗", "refused: no tc".to_string()));
        let row = Row {
            path: "name",
            cells: Vec::new(),
        };
        assert!(row.cell(Output::Docker).is_none());
        assert_eq!(cell("no.such.field", Output::Docker), Cell::Is(UNSTATED));
        assert!(claim("no.such.field", Output::Hybrid).is_none());
    }
}
