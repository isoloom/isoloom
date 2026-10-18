//! `quality-baseline.json`: each rule's count per crate, the crates at zero left out. Read, compared
//! with today's counts, and written back when they only went down.

use std::path::Path;

use super::Counts;
use crate::Res;

/// What changed since the baseline.
#[derive(Debug, Default)]
pub(crate) struct Changes {
    /// The counts that went up: `(rule, crate)`.
    pub(crate) raised: Vec<(String, String)>,
    /// The counts that went down.
    pub(crate) lowered: Vec<(String, String)>,
    /// One line per change, to print.
    pub(crate) lines: Vec<String>,
}

/// The baseline, or `None` when there is none yet.
pub(crate) fn read(file: &Path) -> Res<Option<Counts>> {
    match std::fs::read_to_string(file) {
        Ok(text) => {
            let doc: serde_json::Value = serde_json::from_str(&text)?;
            Ok(Some(serde_json::from_value(doc.get("rules").cloned().unwrap_or_default())?))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Writes the counts as the baseline.
pub(crate) fn write(file: &Path, counts: &Counts) -> Res<()> {
    let doc = serde_json::json!({ "rules": counts });
    std::fs::write(file, format!("{}\n", serde_json::to_string_pretty(&doc)?))?;
    Ok(())
}

/// Each count that differs between the baseline and today's.
pub(crate) fn compare(before: &Counts, now: &Counts) -> Changes {
    let mut changes = Changes::default();
    let rules: std::collections::BTreeSet<&String> = before.keys().chain(now.keys()).collect();
    for rule in rules {
        let (was, is) = (before.get(rule), now.get(rule));
        let crates: std::collections::BTreeSet<&String> = was.into_iter().flat_map(|m| m.keys()).chain(is.into_iter().flat_map(|m| m.keys())).collect();
        for krate in crates {
            let old = was.and_then(|m| m.get(krate)).copied().unwrap_or(0);
            let new = is.and_then(|m| m.get(krate)).copied().unwrap_or(0);
            let key = (rule.clone(), krate.clone());
            if new > old {
                changes.lines.push(format!("up    {rule} in {krate}: {old} -> {new}"));
                changes.raised.push(key);
            } else if new < old {
                changes.lines.push(format!("down  {rule} in {krate}: {old} -> {new}"));
                changes.lowered.push(key);
            }
        }
    }
    changes
}

/// Every count, one line per rule and crate, with each rule's total.
pub(crate) fn table(counts: &Counts) -> String {
    counts
        .iter()
        .map(|(rule, per_crate)| {
            let total: u64 = per_crate.values().sum();
            let crates: Vec<String> = per_crate.iter().map(|(c, n)| format!("{c} {n}")).collect();
            format!("{rule:<48} {total:>5}  {}\n", crates.join(", "))
        })
        .collect::<Vec<_>>()
        .concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(entries: &[(&str, &str, u64)]) -> Counts {
        let mut c = Counts::new();
        for (rule, krate, n) in entries {
            c.entry((*rule).to_owned()).or_default().insert((*krate).to_owned(), *n);
        }
        c
    }

    #[test]
    fn a_count_that_goes_up_is_raised_and_a_new_rule_starts_at_zero() {
        let before = counts(&[("clippy/a", "x", 3), ("clippy/b", "x", 1)]);
        let now = counts(&[("clippy/a", "x", 2), ("clippy/c", "y", 1)]);
        let changes = compare(&before, &now);
        assert_eq!(changes.raised, [("clippy/c".to_owned(), "y".to_owned())]);
        assert_eq!(changes.lowered.len(), 2);
    }
}
