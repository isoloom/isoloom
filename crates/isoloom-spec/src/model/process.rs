//! What a machine runs and when it counts as healthy: its own command, the directory it runs
//! in, the steps run before it at every start, and a healthcheck of its own.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::shell;

/// A command line: a text the shell runs, or a list of arguments run as they are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum CommandLine {
    /// A text run by the machine's shell (`/bin/sh -c`; PowerShell on a Windows VM), with its
    /// variables, pipes and `&&`.
    Shell(String),
    /// Arguments run as they are, the program first (no shell in between).
    Exec(Vec<String>),
}

impl CommandLine {
    /// The command as a program and its arguments: a text goes through `/bin/sh -c`.
    pub fn argv(&self) -> Vec<String> {
        match self {
            CommandLine::Shell(text) => vec!["/bin/sh".into(), "-c".into(), text.clone()],
            CommandLine::Exec(args) => args.clone(),
        }
    }

    /// The command as one line of shell: a text as it is, arguments each quoted.
    pub fn shell_line(&self) -> String {
        match self {
            CommandLine::Shell(text) => text.clone(),
            CommandLine::Exec(args) => args.iter().map(|a| shell::quote(a)).collect::<Vec<_>>().join(" "),
        }
    }

    /// Whether there is nothing to run (an empty text or list), which validation rejects.
    pub fn is_empty(&self) -> bool {
        match self {
            CommandLine::Shell(text) => text.trim().is_empty(),
            CommandLine::Exec(args) => args.first().is_none_or(|a| a.trim().is_empty()),
        }
    }
}

/// When a machine counts as healthy, besides its services' ports answering: a command that
/// succeeds, or a page that answers. Each target waits for it where it waits for the machine
/// (Compose's `depends_on` and `up --wait`, Kubernetes' readiness, the end of a VM's set-up).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Healthcheck {
    /// A command run inside the machine: healthy when it exits with 0 (exactly one of `test`
    /// and `http`). The image's or the VM's own tools run it (`pg_isready`, `redis-cli ping`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<CommandLine>,
    /// A page the machine serves: healthy when it answers with a 2xx or 3xx status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpCheck>,
    /// The time between two tries (`5s`, `1m`, `1m30s`). Default: 5s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    /// How long one try may take before it counts as failed. Default: 3s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Failed tries in a row before the machine counts as unhealthy (a VM's set-up then
    /// fails). Default: 60.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<u32>,
    /// A grace period after start before the first try counts (`30s`). Default: 10s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_period: Option<String>,
}

/// A page a healthcheck requests, on the machine itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HttpCheck {
    /// The port the page is served on.
    pub port: u16,
    /// The page's path. Default: `/`.
    #[serde(default = "root")]
    pub path: String,
}

fn root() -> String {
    "/".into()
}

/// Seconds in a duration written `<number><unit>`, one or more times (`90s`, `1m30s`, `2h`):
/// units `h`, `m` and `s`. `None` when it isn't one.
pub fn seconds(duration: &str) -> Option<u32> {
    let mut total: u32 = 0;
    let mut number = String::new();
    for c in duration.trim().chars() {
        if c.is_ascii_digit() {
            number.push(c);
            continue;
        }
        let unit = match c {
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => return None,
        };
        let n: u32 = number.parse().ok()?;
        total = total.checked_add(n.checked_mul(unit)?)?;
        number.clear();
    }
    (number.is_empty() && !duration.trim().is_empty()).then_some(total)
}

#[cfg(test)]
mod tests {
    use super::{CommandLine, seconds};

    #[test]
    fn durations_read_in_seconds() {
        assert_eq!(seconds("5s"), Some(5));
        assert_eq!(seconds("1m30s"), Some(90));
        assert_eq!(seconds("2h"), Some(7200));
        assert_eq!(seconds("0s"), Some(0));
        for bad in ["", "5", "s", "5x", "1.5s", "-1s"] {
            assert_eq!(seconds(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_command_as_arguments_or_a_line() {
        let text = CommandLine::Shell("echo $HOME && true".into());
        assert_eq!(text.argv(), ["/bin/sh", "-c", "echo $HOME && true"]);
        assert_eq!(text.shell_line(), "echo $HOME && true");
        let args = CommandLine::Exec(vec!["nginx".into(), "-g".into(), "daemon off;".into()]);
        assert_eq!(args.argv(), ["nginx", "-g", "daemon off;"]);
        assert_eq!(args.shell_line(), "'nginx' '-g' 'daemon off;'");
        assert!(CommandLine::Exec(Vec::new()).is_empty() && CommandLine::Shell(" ".into()).is_empty());
    }
}
