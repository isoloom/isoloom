//! What configures a machine, as lowering hands it to the plan: its steps in the configuration
//! model's three phases, for its OS, with the texts of the templates they fill in. The plan
//! resolves it once per machine into its recipe (`isoloom_plan::configure::Recipe`), which every
//! CM renderer reads; nothing below the plan renders it.

use std::collections::BTreeMap;

use crate::syntax::{OsRef, ProjectPath, Step};

/// A machine's configuration: an app's `steps` (all of them `install`), or a catalogue entry's
/// phases, resolved from what the author wrote. Set by lowering, never written in a spec.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Configuration {
    /// The OS it is resolved for: its family installs and starts things.
    pub os: OsRef,
    /// Install: an app's steps; an entry's `install` (which its official image skips).
    pub install: Vec<Step>,
    /// Configure: an entry's `configure`, run in every form.
    pub configure: Vec<Step>,
    /// Start: an entry's per-start preparation (an app has none in 1.0).
    pub start: Vec<Step>,
    /// The text of each template its steps fill in, by the template's `from`: an entry's own
    /// (its placeholders filled in, the lab's left for the plan), or a project file read by the
    /// caller that generates (`isoloom_plan::Lab::with_templates`).
    pub files: BTreeMap<ProjectPath, String>,
}

impl Configuration {
    /// An app's configuration on `os`: its steps, all of them installing.
    #[must_use]
    pub fn app(os: OsRef, steps: Vec<Step>) -> Configuration {
        Configuration {
            os,
            install: steps,
            configure: Vec::new(),
            start: Vec::new(),
            files: BTreeMap::new(),
        }
    }

    /// Its steps, phase after phase.
    pub fn steps(&self) -> impl Iterator<Item = &Step> {
        self.install.iter().chain(&self.configure).chain(&self.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_apps_steps_all_install_and_read_back_in_order() {
        let os: OsRef = "debian@12".parse().unwrap();
        let steps: Vec<Step> = serde_yaml_ng::from_str("[{ run: a.sh }, reboot]").unwrap();
        let c = Configuration::app(os, steps.clone());
        assert_eq!(c.install, steps);
        assert!(c.configure.is_empty() && c.start.is_empty() && c.files.is_empty());
        assert_eq!(c.steps().map(Step::kind).collect::<Vec<_>>(), ["run", "reboot"]);
    }
}
