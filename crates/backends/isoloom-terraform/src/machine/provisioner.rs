//! One `terraform_data` resource that sets a machine up from where Terraform runs: a connection,
//! files to upload, then commands in one or more `remote-exec` provisioners, the machine
//! restarting between two of them.

use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Template, boolean, call, list, reference, string};
use isoloom_render::secrets::REACH_DIR;

/// How Terraform reaches the machine.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Connection {
    /// SSH, with the operator's key (`var.ssh_private_key_file`).
    Ssh {
        /// The machine's address.
        host: Expression,
        /// The user it logs in as.
        user: String,
    },
    /// SSH through a jump host (the router, the only VM on the host's own network), the same
    /// user and key on both.
    SshVia {
        /// The machine's address, from the jump host.
        host: Expression,
        /// The user it logs in as, on both.
        user: String,
        /// The jump host's address.
        bastion: Expression,
    },
    /// WinRM over HTTP, as `isoloom` with a password.
    WinRm {
        /// The machine's address.
        host: Expression,
        /// The password.
        password: Expression,
        /// NTLM authentication (an image whose WinRM listener doesn't take Basic over HTTP).
        ntlm: bool,
    },
}

/// A file Terraform puts on the machine before the commands.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Upload {
    /// The project, archived on the host (`.isoloomignore` honored) as `<module>/.isoloom-project-<id>.tgz`.
    Project {
        /// The archive's id (the resource's name).
        id: String,
        /// Where it goes on the machine.
        destination: &'static str,
    },
    /// A file made of `content`.
    Content {
        /// What it holds.
        content: Expression,
        /// Where it goes on the machine.
        destination: String,
    },
}

/// A machine's set-up as one `terraform_data` resource.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Provisioner {
    /// The resource's name.
    pub(crate) name: String,
    /// What replaces it when it changes (the instance's id).
    pub(crate) replace_on: Expression,
    /// How Terraform reaches the machine.
    pub(crate) connection: Connection,
    /// The files, uploaded first.
    pub(crate) uploads: Vec<Upload>,
    /// The commands, one `remote-exec` each part; the machine restarts between two parts.
    pub(crate) parts: Vec<Vec<Expression>>,
    /// The resources it waits for.
    pub(crate) depends_on: Vec<Expression>,
    /// The name it tells `isoloom run` how to reach the machine under, before anything else
    /// (`isoloom_render::secrets::REACH_DIR`): a machine whose set-up waits for its secrets.
    pub(crate) announce: Option<String>,
}

impl Provisioner {
    /// The same set-up, reached through `bastion` when it goes over SSH.
    #[must_use]
    pub(crate) fn via(mut self, bastion: Expression) -> Provisioner {
        if let Connection::Ssh { host, user } = self.connection {
            self.connection = Connection::SshVia { host, user, bastion };
        }
        self
    }

    /// The `terraform_data` block.
    pub(crate) fn block(self) -> Block {
        let mut b = Block::builder("resource")
            .add_label("terraform_data")
            .add_label(self.name.as_str())
            .add_attribute(("triggers_replace", list([self.replace_on])))
            .add_block(connection(&self.connection));
        if let Some(name) = &self.announce {
            b = b.add_block(announce(name, &self.connection));
        }
        for u in self.uploads {
            b = b.add_blocks(upload(u));
        }
        for (i, part) in self.parts.into_iter().enumerate() {
            if i > 0 {
                b = b.add_block(restart());
            }
            b = b.add_block(remote_exec(part));
        }
        if !self.depends_on.is_empty() {
            b = b.add_attribute(("depends_on", list(self.depends_on)));
        }
        b.build()
    }
}

/// A `local-exec` writing `<user>@<address>` to `<module>/.isoloom-reach/<name>`, followed on
/// Windows by how WinRM authenticates (`winrm`, or `winrm-ntlm`), or by `via <user>@<address>`
/// through a jump host: an address, never a secret (the password stays in Terraform's state,
/// where `isoloom run` reads it).
fn announce(name: &str, c: &Connection) -> Block {
    let (host, user, suffix) = match c {
        Connection::Ssh { host, user } | Connection::SshVia { host, user, .. } => (host.clone(), user.as_str(), ""),
        Connection::WinRm { host, ntlm, .. } => (host.clone(), "isoloom", if *ntlm { " winrm-ntlm" } else { " winrm" }),
    };
    let mut command = Template::text("mkdir -p \"");
    command.push_expr(reference("path.module"));
    command.push_text(format!("/{REACH_DIR}\" && echo \"{user}@"));
    command.push_expr(host);
    command.push_text(suffix);
    // Through a jump host: ` via <user>@<jump host>`.
    if let Connection::SshVia { bastion, .. } = c {
        command.push_text(format!(" via {user}@"));
        command.push_expr(bastion.clone());
    }
    command.push_text("\" > \"");
    command.push_expr(reference("path.module"));
    command.push_text(format!("/{REACH_DIR}/{name}\""));
    provisioner("local-exec").add_attribute(("command", command.into_expression())).build()
}

/// An SSH connection as `user` to `host`, through `bastion` when there is one (as the same
/// user, with the same key).
fn ssh(host: &Expression, user: &str, bastion: Option<&Expression>) -> BlockBuilder {
    let key = || call("file", [call("pathexpand", [reference("var.ssh_private_key_file")])]);
    let mut b = Block::builder("connection")
        .add_attribute(("type", string("ssh")))
        .add_attribute(("host", host.clone()))
        .add_attribute(("user", string(user)))
        .add_attribute(("private_key", key()));
    if let Some(bastion) = bastion {
        b = b
            .add_attribute(("bastion_host", bastion.clone()))
            .add_attribute(("bastion_user", string(user)))
            .add_attribute(("bastion_private_key", key()));
    }
    b.add_attribute(("timeout", string("10m")))
}

fn connection(c: &Connection) -> Block {
    let b = Block::builder("connection");
    match c {
        Connection::Ssh { host, user } => ssh(host, user, None),
        Connection::SshVia { host, user, bastion } => ssh(host, user, Some(bastion)),
        Connection::WinRm { host, password, ntlm } => {
            let b = b
                .add_attribute(("type", string("winrm")))
                .add_attribute(("host", host.clone()))
                .add_attribute(("user", string("isoloom")))
                .add_attribute(("password", password.clone()))
                .add_attribute(("https", boolean(false)));
            let b = if *ntlm { b.add_attribute(("use_ntlm", boolean(true))) } else { b };
            b.add_attribute(("timeout", string("30m")))
        }
    }
    .build()
}

/// The provisioners of one upload: the project is archived on the host first.
fn upload(u: Upload) -> Vec<Block> {
    match u {
        Upload::Project { id, destination } => {
            let archive = format!("/.isoloom-project-{id}.tgz");
            vec![
                provisioner("local-exec").add_attribute(("command", archive_command(&archive))).build(),
                provisioner("file")
                    .add_attribute((
                        "source",
                        Template::new().with_expr(reference("path.module")).with_text(archive).into_expression(),
                    ))
                    .add_attribute(("destination", string(destination)))
                    .build(),
            ]
        }
        Upload::Content { content, destination } => vec![
            provisioner("file")
                .add_attribute(("content", content))
                .add_attribute(("destination", string(destination)))
                .build(),
        ],
    }
}

/// `tar` of the project's folder, its top folder kept (stripped when unpacked), leaving out
/// version control, the tools' state, the archives themselves and `.isoloomignore`'s files.
fn archive_command(archive: &str) -> Expression {
    let q = "\"";
    Template::text("tar -czf \"")
        .with_expr(reference("path.module"))
        .with_text(format!(
            "{archive}{q} --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project* -X {q}"
        ))
        .with_expr(call("abspath", [reference("local_file.isoloom_project.filename")]))
        .with_text("\" -C \"")
        .with_expr(call("dirname", [reference("local.root")]))
        .with_text("\" \"")
        .with_expr(call("basename", [reference("local.root")]))
        .with_text("\"")
        .into_expression()
}

fn provisioner(kind: &str) -> BlockBuilder {
    Block::builder("provisioner").add_label(kind)
}

fn remote_exec(commands: Vec<Expression>) -> Block {
    provisioner("remote-exec").add_attribute(("inline", list(commands))).build()
}

/// The restart between two parts. New SSH connections are refused first (logins closed with
/// `/run/nologin`, gone at boot, and the SSH listener stopped; the open session stays), so
/// Terraform's next connection waits for the machine to be back rather than reaching it on its
/// way down. The dropped connection is expected (`on_failure = continue`); the next part checks
/// that the boot changed.
fn restart() -> Block {
    let commands = [
        "sudo touch /run/nologin",
        "sudo systemctl stop ssh.socket ssh.service sshd.service 2>/dev/null || true",
        "sudo systemctl --no-block reboot",
        "sleep 600",
    ];
    provisioner("remote-exec")
        .add_attribute(("inline", list(commands.map(string))))
        .add_attribute(("on_failure", reference("continue")))
        .build()
}
