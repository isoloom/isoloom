//! The verdict on one case: its run in the container form and its run in the VM form, each the
//! check results the runner reported. A case passes when every check the contract runs in it
//! passed in both forms and every fact read the same in both. A form that wasn't run leaves the
//! case unproven, never passed.

use super::lab::{ConformanceLab, FACT_PREFIX};

/// A machine's form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Form {
    /// A container: the entry's official image, or a build from the OS's image.
    Container,
    /// A VM, set up by the recipe.
    Vm,
}

impl Form {
    /// How a report names it.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Form::Container => "container",
            Form::Vm => "vm",
        }
    }
}

/// One form's run: each check the runner reported (its name, whether it passed, why not).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Run {
    /// The checks, in the order they ran.
    pub checks: Vec<(String, bool, String)>,
}

/// What became of a check in one form.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckOutcome {
    /// It passed.
    Passed,
    /// It failed, and why.
    Failed(String),
    /// The runner never reported it.
    Missing,
    /// The form wasn't run.
    NotRun,
}

/// A check in both forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckVerdict {
    /// The check.
    pub name: String,
    /// In the container form.
    pub container: CheckOutcome,
    /// In the VM form.
    pub vm: CheckOutcome,
}

/// A fact in both forms: its value in each, when the form reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactVerdict {
    /// The fact.
    pub name: String,
    /// Its value in the container form.
    pub container: Option<String>,
    /// Its value in the VM form.
    pub vm: Option<String>,
}

impl FactVerdict {
    /// Both forms reported it, and it reads the same.
    #[must_use]
    pub fn equal(&self) -> bool {
        self.container.is_some() && self.container == self.vm
    }
}

/// One case's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseVerdict {
    /// Each check.
    pub checks: Vec<CheckVerdict>,
    /// Each fact.
    pub facts: Vec<FactVerdict>,
}

impl CaseVerdict {
    /// Every check passed in both forms and every fact reads the same.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.container == CheckOutcome::Passed && c.vm == CheckOutcome::Passed) && self.facts.iter().all(FactVerdict::equal)
    }

    /// Every check passed in the forms that ran (one may not have): what a run on one form
    /// proves of it.
    #[must_use]
    pub fn passed_where_run(&self) -> bool {
        let ok = |o: &CheckOutcome| matches!(o, CheckOutcome::Passed | CheckOutcome::NotRun);
        self.checks.iter().all(|c| ok(&c.container) && ok(&c.vm))
    }
}

/// The verdict on `lab` from its runs (`None`: the form wasn't run).
#[must_use]
pub fn verdict(lab: &ConformanceLab, container: Option<&Run>, vm: Option<&Run>) -> CaseVerdict {
    let checks = lab
        .checks
        .iter()
        .map(|name| CheckVerdict {
            name: name.clone(),
            container: outcome(container, name),
            vm: outcome(vm, name),
        })
        .collect();
    let facts = lab
        .facts
        .iter()
        .map(|name| FactVerdict {
            name: name.clone(),
            container: fact(container, name),
            vm: fact(vm, name),
        })
        .collect();
    CaseVerdict { checks, facts }
}

/// What became of check `name` in `run`.
fn outcome(run: Option<&Run>, name: &str) -> CheckOutcome {
    let Some(run) = run else { return CheckOutcome::NotRun };
    match run.checks.iter().find(|(n, _, _)| n == name) {
        Some((_, true, _)) => CheckOutcome::Passed,
        Some((_, false, why)) => CheckOutcome::Failed(why.clone()),
        None => CheckOutcome::Missing,
    }
}

/// Fact `name`'s value in `run`: what its check reported, trimmed.
fn fact(run: Option<&Run>, name: &str) -> Option<String> {
    let check = format!("{FACT_PREFIX}{name}");
    run?.checks.iter().find(|(n, _, _)| *n == check).map(|(_, _, value)| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lab() -> ConformanceLab {
        ConformanceLab {
            spec: String::new(),
            project: indexmap::IndexMap::new(),
            checks: vec!["answers".into(), "version".into()],
            facts: vec!["os".into()],
        }
    }

    fn run(checks: &[(&str, bool, &str)]) -> Run {
        Run {
            checks: checks.iter().map(|(n, ok, d)| ((*n).to_owned(), *ok, (*d).to_owned())).collect(),
        }
    }

    #[test]
    fn a_case_passes_when_both_forms_pass_and_their_facts_agree() {
        let both = run(&[("answers", true, ""), ("version", true, ""), ("fact: os", false, "debian 12 ")]);
        let v = verdict(&lab(), Some(&both), Some(&both));
        assert!(v.passed() && v.passed_where_run());
        assert_eq!(v.facts[0].container.as_deref(), Some("debian 12"));
    }

    #[test]
    fn a_differing_fact_or_a_failed_check_fails_the_case() {
        let container = run(&[("answers", true, ""), ("version", true, ""), ("fact: os", false, "ubuntu 24.04")]);
        let vm = run(&[("answers", true, ""), ("version", false, "nginx/1.30.5"), ("fact: os", false, "debian 12")]);
        let v = verdict(&lab(), Some(&container), Some(&vm));
        assert!(!v.passed() && !v.facts[0].equal());
        assert_eq!(v.checks[1].vm, CheckOutcome::Failed("nginx/1.30.5".into()));
    }

    #[test]
    fn one_form_run_proves_its_own_checks_but_never_parity() {
        let container = run(&[("answers", true, ""), ("version", true, ""), ("fact: os", false, "debian 12")]);
        let v = verdict(&lab(), Some(&container), None);
        assert!(v.passed_where_run() && !v.passed());
        assert_eq!(v.checks[0].vm, CheckOutcome::NotRun);
        let missing = verdict(&lab(), Some(&run(&[("answers", true, "")])), None);
        assert_eq!(missing.checks[1].container, CheckOutcome::Missing);
        assert!(!missing.passed_where_run());
    }
}
