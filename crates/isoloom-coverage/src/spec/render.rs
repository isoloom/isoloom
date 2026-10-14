//! The spec table as Markdown: the grid, each output's score, and the backends' refusals.

use std::fmt::Write;

use super::{Cell, Output, Row, Status};

/// How far an output handles the fields that apply to it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Score {
    /// Produced.
    pub done: usize,
    /// Produced with a gap.
    pub partial: usize,
    /// Not produced yet.
    pub planned: usize,
    /// Refused by the output's backend.
    pub refused: usize,
}

impl Score {
    /// The fields that apply to the output: every one but those it has no use for.
    pub fn applicable(self) -> usize {
        self.done + self.partial + self.planned + self.refused
    }

    /// Done counts fully, partial half.
    pub fn percent(self) -> usize {
        match self.applicable() {
            0 => 100,
            n => (self.done * 2 + self.partial) * 100 / (n * 2),
        }
    }
}

/// Output `o`'s score over `rows`.
pub fn score(rows: &[Row], o: Output) -> Score {
    let mut s = Score::default();
    for cell in rows.iter().filter_map(|r| r.cell(o)) {
        match cell {
            Cell::Is(Status::Done { .. }) => s.done += 1,
            Cell::Is(Status::Partial { .. }) => s.partial += 1,
            Cell::Is(Status::Planned { .. }) => s.planned += 1,
            Cell::Refused(_) => s.refused += 1,
            Cell::Is(_) => {}
        }
    }
    s
}

/// The table as a Markdown section: the grid, the score of each output, then why each backend
/// refuses what it refuses.
pub fn section(rows: &[Row]) -> String {
    let outputs = Output::all();
    let mut md = String::from(
        "\n## From the spec's side\n\nThe other direction: every field of an Isoloom spec, and what each output does with it. A column per\noutput, each cloud of `cloud-vm` its own (aws to oci). An output inherits what its rendering does\n(the Compose file runs on docker, hosted, docker-vm and cloud-docker; one Kubernetes backend writes\nthe manifests, the Helm chart and the Kustomize overlay; hybrid is Compose for its containers and\nVagrant for its VMs), and every backend is asked to render each field's probe spec: what it refuses\nis marked ✗, with its reason below.\n\n✓ done · ◐ partial · planned · ✗ refused by the backend · n/a: means nothing for this output · info: descriptive only · core: read by Isoloom itself\n\n",
    );
    grid(&mut md, rows, &outputs);
    md.push_str("\n| Output | Handled | Done | Partial | Planned | Refused |\n| --- | ---: | ---: | ---: | ---: | ---: |\n");
    for o in &outputs {
        let s = score(rows, *o);
        let _ = writeln!(
            md,
            "| {} | {}% of {} | {} | {} | {} | {} |",
            o.label(),
            s.percent(),
            s.applicable(),
            s.done,
            s.partial,
            s.planned,
            s.refused
        );
    }
    refusals(&mut md, rows);
    md
}

fn grid(md: &mut String, rows: &[Row], outputs: &[Output]) {
    md.push_str("| Field |");
    for o in outputs {
        let _ = write!(md, " {} |", o.label());
    }
    md.push_str("\n| --- |");
    md.push_str(&" :---: |".repeat(outputs.len()));
    md.push('\n');
    for r in rows {
        let _ = write!(md, "| `{}` |", r.path);
        for (_, c) in &r.cells {
            let _ = write!(md, " {} |", c.symbol());
        }
        md.push('\n');
    }
}

/// Each reason a backend gave, with the outputs that gave it and the fields it refused.
fn refusals(md: &mut String, rows: &[Row]) {
    let mut reasons: Vec<(&str, Vec<&str>, Vec<&str>)> = Vec::new();
    for r in rows {
        for (o, c) in &r.cells {
            let Cell::Refused(why) = c else { continue };
            if !reasons.iter().any(|(w, _, _)| w == why) {
                reasons.push((why, Vec::new(), Vec::new()));
            }
            let Some((_, outs, fields)) = reasons.iter_mut().find(|(w, _, _)| w == why) else {
                continue;
            };
            if !outs.contains(&o.label()) {
                outs.push(o.label());
            }
            if !fields.contains(&r.path) {
                fields.push(r.path);
            }
        }
    }
    md.push_str("\n### Refusals\n\nIn the backends' words, from their probes (a machine a reason names is the probe's own).\n\n");
    for (why, outs, fields) in reasons {
        let fields: Vec<String> = fields.iter().map(|f| format!("`{f}`")).collect();
        let _ = writeln!(md, "- **{}**: {} ({})", outs.join(", "), super::super::capitalize(why), fields.join(", "));
    }
}
