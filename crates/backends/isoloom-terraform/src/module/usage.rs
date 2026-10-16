//! A module's first lines: the generated-file header, then how to start and stop it.

use isoloom_render::header;

/// The operator's SSH key, which the modules that set machines up over SSH take.
const SSH_KEY: &str = "-var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519";

/// The key the machines let in, for a module whose machines set themselves up (the clouds).
const PUBLIC_KEY: &str = "-var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\"";

/// What follows a module whose machines wait for the project.
const THEN_RUNNER: &str = "# Then:   the machines wait for the project: run the lab through the Isothrum runner (run.json\n#         beside this file), which delivers the project over SSH, waits for each set-up and runs the checks.\n";

/// How to start and stop a module, as its first comment lines say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Usage<'a> {
    /// Its folder under `.isoloom/` (`cloud-vm/aws`).
    pub folder: &'a str,
    /// A whole line before the start (`# Needs:` or `# Auth:`), or nothing.
    pub before: &'a str,
    /// What `apply` takes on its own line.
    pub apply: &'a str,
    /// With the operator's SSH key: what the next line takes before it. `None`: no key.
    pub key: Option<&'a str>,
    /// A whole line after the stop, or nothing.
    pub after: &'a str,
    /// The machines set themselves up from their user data, the Isothrum runner delivering the
    /// project: only the public key, and a line saying so.
    pub runner: bool,
}

impl<'a> Usage<'a> {
    /// The usual: `apply`'s settings on the next line, `settings` then the SSH key.
    pub(crate) const fn with_key(folder: &'a str, settings: &'a str) -> Usage<'a> {
        Usage {
            folder,
            before: "",
            apply: "",
            key: Some(settings),
            after: "",
            runner: false,
        }
    }

    /// The machines set themselves up: `settings` then the key they let in, and what follows.
    pub(crate) const fn with_runner_key(folder: &'a str, settings: &'a str) -> Usage<'a> {
        Usage {
            runner: true,
            ..Usage::with_key(folder, settings)
        }
    }

    /// The header, then the lines.
    pub(crate) fn text(&self) -> String {
        let chdir = format!("terraform -chdir=.isoloom/{}", self.folder);
        let ssh = if self.runner { PUBLIC_KEY } else { SSH_KEY };
        let key = self.key.map(|settings| format!(" \\\n#           {settings}{ssh}")).unwrap_or_default();
        let then = if self.runner { THEN_RUNNER } else { "" };
        format!(
            "{}{}# Start:  {chdir} init && {chdir} apply{}{key}\n# Stop:   {chdir} destroy (same variables)\n{}{then}",
            header("#"),
            self.before,
            self.apply,
            self.after
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_go_on_the_next_line_before_the_key() {
        let text = Usage::with_key("cloud-vm/aws", "-var allowed_cidr=<your IP>/32 ").text();
        assert!(
            text.ends_with("# Start:  terraform -chdir=.isoloom/cloud-vm/aws init && terraform -chdir=.isoloom/cloud-vm/aws apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/aws destroy (same variables)\n"),
            "{text}"
        );
    }

    #[test]
    fn without_a_key_apply_takes_everything_on_its_line() {
        let usage = Usage {
            folder: "proxmox",
            before: "# Needs:  a server\n",
            apply: " -var a=b",
            key: None,
            after: "# Auth:   none\n",
            runner: false,
        };
        assert!(
            usage.text().ends_with("# Needs:  a server\n# Start:  terraform -chdir=.isoloom/proxmox init && terraform -chdir=.isoloom/proxmox apply -var a=b\n# Stop:   terraform -chdir=.isoloom/proxmox destroy (same variables)\n# Auth:   none\n"),
            "{}",
            usage.text()
        );
    }
}
