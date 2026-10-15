//! The capability matrix (`isoloom matrix`, `docs/matrix.md`): spec fields against targets, each
//! cell what the target decides about a lab using the field.
//!
//! Nothing in it is written by hand. Each row is a change to a small lab that uses one field (its
//! value, or fields together): `probes.yml`, and one row per OS of the spec's own table. Each
//! column ([`Column`]) decides on the changed lab exactly as it decides on any spec ([`decide`]):
//! lowering, the backend's declared capabilities, its own limits, a Vagrant provider's, a
//! cloud's. A cell is generated, refused with the step that refused it and why, or not
//! applicable. Tests hold every feature of the plan to a row, and every example's refusals to
//! the matrix's, so it can't claim more than the generators do.

mod probe;
mod render;
#[cfg(test)]
mod tests;
mod uses;

use isoloom_plan::{Environment, Feature, Lab};
use isoloom_spec::model::Target;
use serde_yaml_ng::Value;

use crate::Result;
use crate::decision::{Code, Column, Decision, Refused, columns, decide};
use crate::tiers::{TierEntry, tiers};
use probe::{Probe, Probes, Uses, merge, with_hosts};

/// The matrix: its columns, the tiers of the targets they are, and its rows.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Matrix {
    /// The columns, in the order targets are listed.
    pub columns: Vec<Column>,
    /// Every tier entry (a target, or one of its providers or clouds).
    pub tiers: Vec<TierEntry>,
    /// The rows.
    pub rows: Vec<Row>,
}

/// A row: a spec field (or a value of it), and what each column decides.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Row {
    /// The field, `*` for any name and `[]` for any item (`machines.*.hardware.tpm`).
    pub field: String,
    /// The value the row is about, when it is one value of the field (`required`).
    pub value: Option<String>,
    /// What the field (or value) asks for, in a few words.
    pub about: String,
    /// The plan's features it asks for (what a backend declares it can render).
    pub features: Vec<Feature>,
    /// What each column decides, in the order of [`Matrix::columns`].
    pub cells: Vec<Cell>,
    uses: Uses,
}

/// What a column decides about a lab using a row's field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Cell {
    /// It generates the lab.
    Supported,
    /// It refuses it: the step, and why.
    Refused(Refused),
    /// The field means nothing on that target (another target's settings, machines on
    /// `cloud-services`).
    NotApplicable,
}

/// A spec as its author wrote it, read as YAML: what [`Row::used_by`] looks into.
#[derive(Debug, Clone, PartialEq)]
pub struct Authored(Value);

impl Authored {
    /// The spec written as `yaml`, when it is YAML.
    #[must_use]
    pub fn parse(yaml: &str) -> Option<Authored> {
        serde_yaml_ng::from_str(yaml).ok().map(Authored)
    }
}

impl Row {
    /// The row's name: its field, then `= <value>` when it is about one value.
    #[must_use]
    pub fn name(&self) -> String {
        match &self.value {
            Some(v) => format!("{} = {v}", self.field),
            None => self.field.clone(),
        }
    }

    /// Whether the field `path` as written in a spec (`machines.web.app.image`) is the row's
    /// field (`machines.*.app.image`).
    #[must_use]
    pub fn is_field(&self, path: &str) -> bool {
        uses::path_matches(&self.field, path)
    }

    /// Whether the spec `written` (whose lab is `lab`) uses the row on `target`: the row's
    /// field written in it (with its value, its count, the fields beside it), or, for a row
    /// about a combination the spec's text can't show, one of its features among what the lab
    /// requires of `target`.
    #[must_use]
    pub fn used_by(&self, written: &Authored, lab: &Lab, target: Target) -> bool {
        match &self.uses {
            Uses::Written(w) => uses::written_in(w, &written.0),
            Uses::Features => {
                let env = Environment::build(&lab.for_target(target), target, None);
                env.requirements.iter().any(|r| self.features.contains(&r.feature()))
            }
            Uses::Refusal => lab.refusal(target).is_some_and(|p| uses::path_matches(&self.field, p.path.as_str())),
        }
    }
}

/// The matrix, worked out from the rows and what every column decides about each.
///
/// # Errors
///
/// [`crate::Error::Probe`] when a row of `probes.yml` names what doesn't exist, [`crate::Error::Tiers`] or
/// [`crate::Error::TiersJson`] when the tier list can't be read (tests keep both readable).
pub fn matrix() -> Result<Matrix> {
    let probes = probe::probes()?;
    let columns = columns();
    let rows = probes.rows.iter().map(|p| row(&probes, p, &columns)).collect();
    Ok(Matrix {
        columns,
        tiers: tiers()?,
        rows,
    })
}

/// The labs a row's columns decide on: the plain one, `hybrid`'s (with a container-only and a
/// VM-only machine beside), `external`'s (each machine an existing host), and the cloud lab.
struct Labs {
    plain: std::result::Result<Lab, String>,
    hybrid: std::result::Result<Lab, String>,
    external: std::result::Result<Lab, String>,
    cloud: std::result::Result<Lab, String>,
}

/// The lab of `start`, with `patch` merged in (and each machine an existing host, with
/// `hosts`), or why it isn't a spec.
fn lab(start: &Value, patch: &Value, hosts: bool) -> std::result::Result<Lab, String> {
    let mut v = start.clone();
    merge(&mut v, patch);
    if hosts {
        with_hosts(&mut v);
    }
    let text = serde_yaml_ng::to_string(&v).map_err(|e| e.to_string())?;
    isoloom_spec::parse(&text).map(Lab::new).map_err(|e| e.to_string())
}

/// A row: its labs, then each column's decision about the one it reads.
fn row(probes: &Probes, p: &Probe, columns: &[Column]) -> Row {
    let labs = Labs {
        plain: lab(&probes.base, &p.patch, false),
        hybrid: lab(&probes.hybrid, &p.patch, false),
        external: lab(&probes.base, &p.patch, p.hosts),
        cloud: lab(&probes.cloud, &p.patch, false),
    };
    Row {
        field: p.field.clone(),
        value: p.value.clone(),
        about: p.about.clone(),
        features: p.features.clone(),
        cells: columns.iter().map(|c| cell(p, &labs, c)).collect(),
        uses: p.uses.clone(),
    }
}

/// What `column` decides about the row's lab, or not applicable when the row is about another
/// target or another kind of lab.
fn cell(p: &Probe, labs: &Labs, column: &Column) -> Cell {
    let target = column.target;
    if (!p.only.is_empty() && !p.only.contains(&target)) || p.cloud != (target == Target::CloudServices) {
        return Cell::NotApplicable;
    }
    let lab = match target {
        Target::Hybrid => &labs.hybrid,
        Target::External => &labs.external,
        Target::CloudServices => &labs.cloud,
        _ => &labs.plain,
    };
    match lab {
        Err(why) => Cell::Refused(Refused {
            code: Code::Invalid,
            field: None,
            reason: why.clone(),
        }),
        Ok(lab) => match decide(lab, column) {
            Decision::Generated => Cell::Supported,
            Decision::Refused(r) => Cell::Refused(r),
            Decision::NotApplicable => Cell::NotApplicable,
        },
    }
}

impl Matrix {
    /// The cell of `row` in `column`.
    #[must_use]
    pub fn cell<'a>(&self, row: &'a Row, column: &Column) -> Option<&'a Cell> {
        self.columns.iter().position(|c| c == column).and_then(|i| row.cells.get(i))
    }

    /// The tiers of `column`: its entry's, or every entry's of its target when it stands for
    /// them all (`docker-vm`), each tier once, lowest first.
    #[must_use]
    pub fn column_tiers(&self, column: &Column) -> Vec<crate::tiers::Tier> {
        let mut found: Vec<_> = self
            .tiers
            .iter()
            .filter(|e| e.target == column.target && (column.variant.is_none() || e.variant == column.variant))
            .map(TierEntry::tier)
            .collect();
        found.sort();
        found.dedup();
        found
    }

    /// The matrix as text, for a terminal: the tiers, then each row with the columns that
    /// generate it, those that refuse it and why, and those it means nothing to.
    #[must_use]
    pub fn text(&self) -> String {
        render::text(self)
    }

    /// The matrix as Markdown: `docs/matrix.md`.
    #[must_use]
    pub fn markdown(&self) -> String {
        render::markdown(self)
    }

    /// The matrix as JSON (`isoloom matrix --json`).
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        render::json(self)
    }
}
