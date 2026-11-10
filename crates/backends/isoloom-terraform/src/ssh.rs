//! The way into a VM Terraform made, as every run manifest of this crate writes it: `ssh.run`
//! with the runner's key, to the VM's address as its login user, and how a command there gets
//! root ([`AsRoot`]: `sudo`, unless the user is root).

use isoloom_manifest::{Action, Error as ManifestError, Run, Step, Template};
use isoloom_render::run_manifest::action_with;
use isoloom_render::steps::AsRoot;

use crate::module::manifest::id;

/// A VM's address and the user the runner's key logs in as there.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Login {
    /// Where SSH reaches it (`${outputs.ip}`, an address output).
    pub(crate) host: Template,
    /// The user the runner's key logs in as.
    pub(crate) user: String,
}

impl Login {
    /// `user` at `host`.
    pub(crate) fn new(host: Template, user: &str) -> Login {
        Login { host, user: user.to_owned() }
    }

    /// How a command run as its user gets root.
    pub(crate) fn as_root(&self) -> AsRoot {
        AsRoot::of(&self.user)
    }

    /// `ssh.run` of `command`, its `with` extended by `more` (`wait_s`).
    ///
    /// # Errors
    ///
    /// A key the manifest refuses (a bug: they are fixed).
    pub(crate) fn run(&self, command: &Template, more: &[(&str, Template)]) -> Result<Run, ManifestError> {
        let with = [("host", self.host.clone()), ("user", Template::text(&self.user)), ("command", command.clone())];
        action_with(Action::SshRun, with.into_iter().chain(more.iter().cloned()))
    }

    /// The way in as the Compose-in-a-VM commands take it: [`Login::run`] with nothing more.
    pub(crate) fn way_in(&self) -> impl Fn(&Template) -> Result<Run, ManifestError> + '_ {
        |command| self.run(command, &[])
    }

    /// Step `step` running `command` through [`Login::run`].
    ///
    /// # Errors
    ///
    /// A step id or key the manifest refuses.
    pub(crate) fn step(&self, step: &str, command: &Template, more: &[(&str, Template)]) -> Result<Step, ManifestError> {
        let mut s = Step::action(id(step)?, Action::SshRun);
        s.run = self.run(command, more)?;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_login_runs_its_command_at_its_host_as_its_user() {
        let login = Login::new(Template::text("10.0.0.9"), "root");
        assert_eq!(login.as_root(), AsRoot::Root);
        let s = login.step("go", &Template::text("true"), &[("wait_s", Template::text("9"))]).unwrap();
        assert!(matches!(s.run, Run::Action { action: Action::SshRun, .. }), "{:?}", s.run);
        let Run::Action { with, .. } = &s.run else { return };
        let keys: Vec<&str> = with.keys().map(isoloom_manifest::SnakeName::as_str).collect();
        let values: Vec<String> = with.values().map(ToString::to_string).collect();
        assert_eq!(
            (keys, values),
            (
                vec!["command", "host", "user", "wait_s"],
                vec!["true".to_owned(), "10.0.0.9".to_owned(), "root".to_owned(), "9".to_owned()]
            )
        );
        assert_eq!(Login::new(Template::text("h"), "isoloom").as_root(), AsRoot::Sudo);
    }
}
