//! Secrets: values read from a store on the runner when the environment starts, and handed to
//! the machines as files. A secret's value is never in the spec, the generated files or the
//! resolved snapshot: those only name where it comes from and where it goes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::machine::ScalarSchema;
use crate::syntax::Mode;

/// Where the runner (`isoloom run`) reads a secret's value: one store, written as a map with
/// one key (`{ env: DB_PASSWORD }`). Another store is a new variant here and a reader of its
/// own in the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SecretSource {
    /// An environment variable of the runner (or the profile's `inputs`).
    Env {
        /// The name of the runner's environment variable holding the value, e.g. `DB_PASSWORD`.
        env: String,
    },
    /// A file on the runner. Keep it out of version control.
    File {
        /// Its path: absolute, `~/...`, or relative to the project folder.
        file: String,
    },
    /// A HashiCorp Vault key/value secret, read with the `vault` command and its usual
    /// settings (`VAULT_ADDR`, `VAULT_TOKEN`).
    Vault {
        /// The secret's path in Vault and the field holding the value, as `<path>#<field>`,
        /// e.g. `secret/app#db_password`.
        vault: String,
    },
    /// A value in a SOPS-encrypted file, decrypted with the `sops` command.
    Sops {
        /// `<file>#<key>`, dotted keys for nesting, e.g. `secrets.enc.yaml#db.password`.
        sops: String,
    },
    /// An input's value, given at each start: the secret lowering makes of a file whose source
    /// is an input (`files: [{ path, input }]`). Never written in a spec.
    #[serde(skip_deserializing)]
    #[schemars(skip)]
    Input {
        /// The input's name (`CTF_FLAG_MAIN`): the runner's variable holding the value.
        input: String,
    },
}

impl SecretSource {
    /// The store's name, as the spec writes it (`env`, `file`, `vault`, `sops`).
    #[must_use]
    pub fn store(&self) -> &'static str {
        match self {
            SecretSource::Env { .. } => "env",
            SecretSource::File { .. } => "file",
            SecretSource::Vault { .. } => "vault",
            SecretSource::Sops { .. } => "sops",
            SecretSource::Input { .. } => "input",
        }
    }

    /// What the store is asked for: a variable, a path, a `<path>#<field>` reference.
    #[must_use]
    pub fn reference(&self) -> &str {
        match self {
            SecretSource::Env { env: r }
            | SecretSource::File { file: r }
            | SecretSource::Vault { vault: r }
            | SecretSource::Sops { sops: r }
            | SecretSource::Input { input: r } => r,
        }
    }
}

/// Where a machine's secret goes: by name alone (`/run/secrets/<name>`, read-only, with the
/// default mode of the machine's kind), or with its path, owner and mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MachineSecret {
    /// The secret's name: the file `/run/secrets/<name>`.
    Name(String),
    /// The secret placed as written.
    Mount(SecretMount),
}

/// A secret placed in a machine: which one, at what path, owned by whom, with what mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecretMount {
    /// The secret, from the top-level `secrets`.
    pub source: String,
    /// The file's path in the machine: absolute, or a name under `/run/secrets/`. Default:
    /// `/run/secrets/<source>`. On Windows, a drive path (`C:\app\db-password`) too; a path from
    /// `/` is on drive C: (`/run/secrets/db` is `C:\run\secrets\db`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The file's owner, a user id (default 0, root). Not on Windows, where the file is
    /// readable by SYSTEM and Administrators only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    /// The file's group, a group id (default 0, root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gid: Option<u32>,
    /// The file's mode, in octal (`0440`, or `"0440"`). Default `0444` in a container (as
    /// Compose and Kubernetes do), `0400` on a Linux VM, whose other users shouldn't read it.
    /// Not on Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ScalarSchema>")]
    pub mode: Option<Mode>,
}

/// Whether `p` is an absolute Windows path on a drive (`C:\app\db`, `C:/app/db`).
#[must_use]
pub fn is_drive_path(p: &str) -> bool {
    after_drive(p).is_some()
}

/// The path after its drive (`\app\db` of `C:\app\db`), when `p` is an absolute Windows path
/// on a drive.
#[must_use]
pub fn after_drive(p: &str) -> Option<&str> {
    let rest = p.strip_prefix(|c: char| c.is_ascii_alphabetic())?.strip_prefix(':')?;
    (rest.len() > 1 && rest.starts_with(['\\', '/'])).then_some(rest)
}

/// Where secrets go when a machine names no path.
pub const SECRETS_DIR: &str = "/run/secrets";

impl MachineSecret {
    /// The secret it places.
    #[must_use]
    pub fn source(&self) -> &str {
        match self {
            MachineSecret::Name(n) => n,
            MachineSecret::Mount(m) => &m.source,
        }
    }

    /// The file's absolute path in the machine.
    #[must_use]
    pub fn path(&self) -> String {
        match self {
            MachineSecret::Mount(SecretMount { target: Some(t), .. }) if t.starts_with('/') || is_drive_path(t) => t.clone(),
            MachineSecret::Mount(SecretMount { target: Some(t), .. }) => format!("{SECRETS_DIR}/{t}"),
            _ => format!("{SECRETS_DIR}/{}", self.source()),
        }
    }

    /// Its long form (every field but the source unset for a name alone).
    #[must_use]
    pub fn mount(&self) -> SecretMount {
        match self {
            MachineSecret::Name(n) => SecretMount {
                source: n.clone(),
                target: None,
                uid: None,
                gid: None,
                mode: None,
            },
            MachineSecret::Mount(m) => m.clone(),
        }
    }
}

/// The environment variable a secret's value is handed over in, to the tool that delivers it
/// (`ISOLOOM_SECRET_DB_PASSWORD` for `db-password`). Secret names are kebab-case, so two
/// secrets never share one.
#[must_use]
pub fn secret_variable(name: &str) -> String {
    format!("ISOLOOM_SECRET_{}", name.to_ascii_uppercase().replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_names_its_store_and_what_the_store_is_asked_for() -> Result<(), serde_yaml_ng::Error> {
        let mut read = Vec::new();
        for yaml in [
            "{ env: DB_PASSWORD }",
            "{ file: ~/db.txt }",
            "{ vault: \"secret/app#db\" }",
            "{ sops: \"s.enc.yaml#db.pw\" }",
        ] {
            let source: SecretSource = serde_yaml_ng::from_str(yaml)?;
            read.push((source.store(), source.reference().to_owned()));
        }
        let input = SecretSource::Input { input: "FLAG".into() };
        read.push((input.store(), input.reference().to_owned()));
        let expected = [
            ("env", "DB_PASSWORD"),
            ("file", "~/db.txt"),
            ("vault", "secret/app#db"),
            ("sops", "s.enc.yaml#db.pw"),
            ("input", "FLAG"),
        ];
        assert_eq!(read, expected.map(|(s, r)| (s, r.to_owned())));
        Ok(())
    }

    #[test]
    fn a_drive_path_is_a_letter_a_colon_and_a_rooted_path() {
        let read: Vec<bool> = ["C:\\app\\db", "d:/app", "C:\\", "C:app\\db", "1:\\app", "/run/db"]
            .iter()
            .map(|p| is_drive_path(p))
            .collect();
        assert_eq!(read, [true, true, false, false, false, false]);
    }

    #[test]
    fn a_secret_goes_to_its_target_or_under_run_secrets() -> Result<(), serde_yaml_ng::Error> {
        let mut paths = Vec::new();
        for yaml in [
            "db",
            "{ source: db, target: /etc/db.pw }",
            "{ source: db, target: \"C:\\\\app\\\\db\" }",
            "{ source: db, target: db.pw }",
            "{ source: db }",
        ] {
            paths.push(serde_yaml_ng::from_str::<MachineSecret>(yaml)?.path());
        }
        assert_eq!(paths, ["/run/secrets/db", "/etc/db.pw", "C:\\app\\db", "/run/secrets/db.pw", "/run/secrets/db"]);
        Ok(())
    }

    #[test]
    fn a_secret_is_handed_over_in_a_variable_of_its_name() {
        assert_eq!(secret_variable("db-password"), "ISOLOOM_SECRET_DB_PASSWORD");
    }
}
