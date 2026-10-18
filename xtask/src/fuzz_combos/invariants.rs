//! The invariants `cargo xtask fuzz-combos` asserts for every generated spec, each a small pure
//! function over what the pipeline produced, so they are unit-testable on their own:
//!
//! - the spec the generator built is valid (its problems are empty);
//! - generating every target twice is byte-identical (determinism);
//! - every target left out carries a reason (a refusal is never silent);
//! - no sentinel secret value appears in any generated file (values never reach a file).

use isoloom_plan::Lab;
use isoloom_spec::model::Target;
use isoloom_targets::{GenerateError, GeneratedFile, generate_all};

use super::generator::GeneratedSpec;

/// What one spec produced, for the run's summary.
pub(crate) struct Outcome {
    /// How many targets generated at least one file.
    pub(crate) generated: usize,
    /// How many targets were left out, each with a reason.
    pub(crate) skipped: usize,
}

/// Checks every invariant for `generated`; `Err` names the first that failed, with enough of the
/// spec to reproduce it.
pub(crate) fn check(generated: &GeneratedSpec) -> Result<Outcome, String> {
    let lab = parsed(&generated.yaml)?;
    let problems = lab.problems();
    if !problems.is_empty() {
        let lines: Vec<String> = problems.iter().map(isoloom_spec::syntax::Problem::line).collect();
        return Err(format!("the generated spec is not valid:\n  {}", lines.join("\n  ")));
    }
    for target in Target::ALL {
        if let Some(refusal) = lab.refusal(target)
            && refusal.line().trim().is_empty()
        {
            return Err(format!("{}: a target refusal carries no reason", target.id()));
        }
    }
    let (files, skipped) = generate_all(&lab);
    let (again, skipped_again) = generate_all(&lab);
    if let Some(diff) = determinism(&files, &again) {
        return Err(diff);
    }
    if messages(&skipped) != messages(&skipped_again) {
        return Err("generation is not deterministic: a target was left out only on one run".to_owned());
    }
    if let Some(silent) = reasonless(&skipped) {
        return Err(silent);
    }
    if let Some(leak) = secret_leak(&files, &generated.secret_values) {
        return Err(leak);
    }
    Ok(Outcome {
        generated: targets_with_files(&files),
        skipped: skipped.len(),
    })
}

/// The lab, or why the generated text didn't parse (a generator bug).
fn parsed(yaml: &str) -> Result<Lab, String> {
    let spec = isoloom_spec::parse(yaml).map_err(|e| format!("the generated spec does not parse: {e}"))?;
    Ok(Lab::new(spec))
}

/// How many distinct target folders have a file (`.isoloom/<target>/...`), the resolved snapshot
/// aside.
fn targets_with_files(files: &[GeneratedFile]) -> usize {
    let mut seen: Vec<&str> = files
        .iter()
        .filter_map(|f| f.path.strip_prefix(".isoloom/"))
        .filter_map(|p| p.split_once('/'))
        .map(|(target, _)| target)
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

/// The reasons the targets were left out, sorted, so two runs compare regardless of order.
fn messages(skipped: &[GenerateError]) -> Vec<String> {
    let mut out: Vec<String> = skipped.iter().map(ToString::to_string).collect();
    out.sort();
    out
}

/// Where two generations differ, if they do: a missing file, an extra one, or different bytes.
fn determinism(a: &[GeneratedFile], b: &[GeneratedFile]) -> Option<String> {
    if a.len() != b.len() {
        return Some(format!("generation is not deterministic: {} files, then {}", a.len(), b.len()));
    }
    for (first, second) in a.iter().zip(b) {
        if first.path != second.path {
            return Some(format!(
                "generation is not deterministic: file order changed ({} vs {})",
                first.path, second.path
            ));
        }
        if first.contents != second.contents {
            return Some(format!("generation is not deterministic: `{}` changed between two runs", first.path));
        }
    }
    None
}

/// A target left out without a reason, if there is one: every refusal reads `<target>: <reason>`,
/// so the text after the first `: ` must not be empty.
fn reasonless(skipped: &[GenerateError]) -> Option<String> {
    skipped
        .iter()
        .find(|e| e.to_string().split_once(": ").is_none_or(|(_, reason)| reason.trim().is_empty()))
        .map(|e| format!("a target was left out with no reason: `{e}`"))
}

/// A sentinel secret value found in a generated file, if any leaked.
fn secret_leak(files: &[GeneratedFile], values: &[String]) -> Option<String> {
    for file in files {
        for value in values {
            if file.contents.contains(value.as_str()) {
                return Some(format!("a secret value leaked into `{}`", file.path));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, contents: &str) -> GeneratedFile {
        GeneratedFile {
            path: path.to_owned(),
            contents: contents.to_owned(),
        }
    }

    #[test]
    fn a_leaked_secret_value_is_caught_and_a_hashed_one_is_not() {
        let value = "P6FUZZ-SENTINEL-deadbeef".to_owned();
        let leaked = [file(".isoloom/docker/compose.yaml", "password: P6FUZZ-SENTINEL-deadbeef")];
        assert!(secret_leak(&leaked, std::slice::from_ref(&value)).is_some());
        let safe = [file(".isoloom/docker/compose.yaml", "password: $6$rounds$abc")];
        assert!(secret_leak(&safe, std::slice::from_ref(&value)).is_none());
    }

    #[test]
    fn a_byte_difference_or_a_changed_file_set_breaks_determinism() {
        let one = [file("a", "x"), file("b", "y")];
        assert!(determinism(&one, &one).is_none());
        let changed = [file("a", "x"), file("b", "z")];
        assert!(determinism(&one, &changed).is_some());
        let shorter = [file("a", "x")];
        assert!(determinism(&one, &shorter).is_some());
    }

    #[test]
    fn a_refusal_without_a_reason_is_reported() {
        let silent = [GenerateError::Unsupported {
            target: Target::Docker,
            what: "   ".to_owned(),
        }];
        assert!(reasonless(&silent).is_some());
        let with_reason = [GenerateError::NotPossible(Target::Vagrant)];
        assert!(reasonless(&with_reason).is_none());
    }

    #[test]
    fn target_folders_are_counted_once_each() {
        let files = [
            file(".isoloom/docker/compose.yaml", ""),
            file(".isoloom/docker/run.json", ""),
            file(".isoloom/vagrant/Vagrantfile", ""),
            file(".isoloom/resolved.json", ""),
        ];
        // docker and vagrant are the two target folders; the resolved snapshot is not one.
        assert_eq!(targets_with_files(&files), 2);
    }
}
