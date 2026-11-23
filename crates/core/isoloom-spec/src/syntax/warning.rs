//! Something a valid spec says that is likely not what its author means: shown beside the
//! problems, never failing validation.
//!
//! ```text
//! warning: machines.web.app.steps[1]  (isoloom.yml:9:11)
//!   `nginx` installs a daemon that starts on Debian VMs only: ...
//!   fix: `service: nginx`, or `{ service: { name: nginx, enabled: false } }`
//! ```

use std::fmt;

use serde::Serialize;

use super::problem::{FieldPath, Span, lines};

/// A warning about a field of the spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    /// The field it is about.
    pub path: FieldPath,
    /// Where that field is in the file, when it is written there.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// What is likely wrong.
    pub reason: String,
    /// What to write instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

impl Warning {
    /// A warning at `path`, not yet placed.
    #[must_use]
    pub fn new(path: FieldPath, reason: impl Into<String>) -> Warning {
        Warning {
            path,
            span: None,
            reason: reason.into(),
            fix: None,
        }
    }

    /// The warning, with its fix.
    #[must_use]
    pub fn fix(mut self, fix: impl Into<String>) -> Warning {
        self.fix = Some(fix.into());
        self
    }

    /// The three lines an author reads, naming `file` (`isoloom.yml`) on the first.
    #[must_use]
    pub fn render(&self, file: &str) -> String {
        lines(&format!("warning: {}", self.path), self.span, file, &self.reason, self.fix.as_deref())
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render("isoloom.yml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warning_reads_as_three_lines() {
        let mut w = Warning::new(FieldPath::new("machines.web.app.steps[1]"), "`nginx` starts on Debian VMs only").fix("`service: nginx`");
        w.span = Some(Span { line: 9, column: 11 });
        assert_eq!(
            w.render("isoloom.yml"),
            "warning: machines.web.app.steps[1]  (isoloom.yml:9:11)\n  `nginx` starts on Debian VMs only\n  fix: `service: nginx`"
        );
        let json = serde_json::to_value(&w).unwrap();
        assert_eq!((json["path"].as_str(), json["line"].as_u64()), (Some("machines.web.app.steps[1]"), Some(9)));
    }
}
