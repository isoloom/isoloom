//! A module's first lines: the generated-file header, then how to start and stop it.

use isoloom_render::header;

/// The operator's SSH key, which the modules that set machines up over SSH take.
const SSH_KEY: &str = "-var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519";

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
        }
    }

    /// The header, then the lines.
    pub(crate) fn text(&self) -> String {
        let chdir = format!("terraform -chdir=.isoloom/{}", self.folder);
        let key = self.key.map(|settings| format!(" \\\n#           {settings}{SSH_KEY}")).unwrap_or_default();
        format!(
            "{}{}# Start:  {chdir} init && {chdir} apply{}{key}\n# Stop:   {chdir} destroy (same variables)\n{}",
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
        };
        assert!(
            usage.text().ends_with("# Needs:  a server\n# Start:  terraform -chdir=.isoloom/proxmox init && terraform -chdir=.isoloom/proxmox apply -var a=b\n# Stop:   terraform -chdir=.isoloom/proxmox destroy (same variables)\n# Auth:   none\n"),
            "{}",
            usage.text()
        );
    }
}
