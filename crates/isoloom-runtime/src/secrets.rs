//! Reading secrets from their stores, on the runner, when an environment starts. Each store
//! (`env`, `file`, `vault`, `sops`) is a [`SecretStore`]; adding one is a variant of the spec's
//! `SecretSource` and a store here. A value is held as a [`SecretValue`], which never prints:
//! it goes to the target's own secret mechanism and nowhere else (no file, no log, no event).

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use isoloom_spec::model::{SecretSource, Spec};

/// A secret's value. Its `Debug` and `Display` never show it.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretValue(String);

impl SecretValue {
    /// The value itself, for the tool that delivers it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue(<redacted>)")
    }
}

/// Why a secret couldn't be read. Never holds the value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("secret `{name}` ({store}: {reference}): {why}")]
pub struct SecretError {
    /// The secret, as the spec names it.
    pub name: String,
    /// Its store (`env`, `file`, `vault`, `sops`).
    pub store: &'static str,
    /// What the store was asked for.
    pub reference: String,
    /// What went wrong, and what to do.
    pub why: String,
}

/// What a store reads with: the project folder (relative paths) and the runner's variables
/// (this process's environment, then the profile's `inputs`).
pub struct Lookup<'a> {
    /// The project folder.
    pub project: &'a Path,
    /// A variable's value, when set.
    pub var: &'a dyn Fn(&str) -> Option<String>,
}

/// A place secrets are read from.
pub trait SecretStore {
    /// The value `reference` names in this store, or why it can't be read (never the value).
    fn read(&self, reference: &str, at: &Lookup) -> Result<String, String>;
}

/// The runner's environment variables (and the profile's `inputs`).
pub struct EnvStore;

impl SecretStore for EnvStore {
    fn read(&self, reference: &str, at: &Lookup) -> Result<String, String> {
        (at.var)(reference).ok_or_else(|| format!("`{reference}` isn't set in the environment (or the profile's `inputs`)"))
    }
}

/// A file on the runner: absolute, `~/...`, or relative to the project folder.
pub struct FileStore;

impl SecretStore for FileStore {
    fn read(&self, reference: &str, at: &Lookup) -> Result<String, String> {
        let path = file_path(reference, at);
        let bytes = std::fs::read(&path).map_err(|e| format!("{} can't be read: {e}", path.display()))?;
        String::from_utf8(bytes).map_err(|_| format!("{} isn't text (UTF-8)", path.display()))
    }
}

/// Where a file reference points: `~/` is the home folder, a relative path the project's.
fn file_path(reference: &str, at: &Lookup) -> PathBuf {
    match reference.strip_prefix("~/") {
        Some(rest) => (at.var)("HOME").map_or_else(|| PathBuf::from(reference), |home| Path::new(&home).join(rest)),
        None => at.project.join(reference),
    }
}

/// HashiCorp Vault's key/value store, through its command (`vault kv get -field=<field> <path>`),
/// with its usual settings (`VAULT_ADDR`, `VAULT_TOKEN`) from the environment.
pub struct VaultStore {
    /// The `vault` command.
    pub program: PathBuf,
}

impl SecretStore for VaultStore {
    fn read(&self, reference: &str, at: &Lookup) -> Result<String, String> {
        let (path, field) = split(reference)?;
        let mut c = Command::new(&self.program);
        c.args(["kv", "get", &format!("-field={field}"), path]).current_dir(at.project);
        output(&mut c, "vault")
    }
}

/// A value in a SOPS-encrypted file (`<file>#<dotted.key>`), through `sops --decrypt --extract`.
/// The value's last line break, which `sops` adds, is dropped.
pub struct SopsStore {
    /// The `sops` command.
    pub program: PathBuf,
}

impl SecretStore for SopsStore {
    fn read(&self, reference: &str, at: &Lookup) -> Result<String, String> {
        let (file, key) = split(reference)?;
        let extract = key.split('.').fold(String::new(), |mut out, k| {
            let _ = write!(out, "[\"{k}\"]");
            out
        });
        let mut c = Command::new(&self.program);
        c.args(["--decrypt", "--extract", &extract]).arg(file_path(file, at)).current_dir(at.project);
        output(&mut c, "sops").map(|v| v.strip_suffix('\n').map(str::to_string).unwrap_or(v))
    }
}

/// `<where>#<key>`, split.
fn split(reference: &str) -> Result<(&str, &str), String> {
    reference
        .split_once('#')
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .ok_or_else(|| "not `<where>#<key>`".to_string())
}

/// A store command's standard output, or why it failed (its own message, never its output).
fn output(c: &mut Command, program: &str) -> Result<String, String> {
    let out = c.output().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("`{program}` isn't installed"),
        _ => format!("`{program}` couldn't start: {e}"),
    })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("`{program}` failed: {}", err.trim()));
    }
    String::from_utf8(out.stdout).map_err(|_| format!("`{program}` gave a value that isn't text (UTF-8)"))
}

/// The standard input a machine's delivery script reads (`isoloom_render::secrets::receive`):
/// one line per secret of `names`, in that order, its value in base64. `None` when a name has no
/// value (the spec's secrets were all read, so only a plan out of step with the spec does).
pub fn delivery_input(values: &[(String, SecretValue)], names: &[String]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for name in names {
        let (_, v) = values.iter().find(|(n, _)| n == name)?;
        out.extend(isoloom_spec::shell::base64(v.0.as_bytes()).into_bytes());
        out.push(b'\n');
    }
    Some(out)
}

/// The store a source reads from, with the commands found on the `PATH`.
pub fn store(source: &SecretSource) -> Box<dyn SecretStore> {
    match source {
        SecretSource::Env { .. } => Box::new(EnvStore),
        SecretSource::File { .. } => Box::new(FileStore),
        SecretSource::Vault { .. } => Box::new(VaultStore { program: "vault".into() }),
        SecretSource::Sops { .. } => Box::new(SopsStore { program: "sops".into() }),
    }
}

/// Every secret of `spec`, read from its store, in the order the spec declares them; or every
/// one that couldn't be read (all of them, so one run says everything that is missing). A value
/// with a NUL character is refused: the tools take secrets as text.
pub fn resolve(spec: &Spec, at: &Lookup) -> Result<Vec<(String, SecretValue)>, Vec<SecretError>> {
    resolve_with(spec, at, &|s| store(s))
}

/// [`resolve`] with the stores `store_of` gives (tests swap the commands).
pub fn resolve_with(
    spec: &Spec,
    at: &Lookup,
    store_of: &dyn Fn(&SecretSource) -> Box<dyn SecretStore>,
) -> Result<Vec<(String, SecretValue)>, Vec<SecretError>> {
    let mut values = Vec::new();
    let mut errors = Vec::new();
    for (name, source) in &spec.secrets {
        let read = store_of(source).read(source.reference(), at).and_then(|v| {
            if v.contains('\0') {
                Err("the value holds a NUL character".to_string())
            } else {
                Ok(v)
            }
        });
        match read {
            Ok(v) => values.push((name.clone(), SecretValue(v))),
            Err(why) => errors.push(SecretError {
                name: name.clone(),
                store: source.store(),
                reference: source.reference().to_string(),
                why,
            }),
        }
    }
    if errors.is_empty() { Ok(values) } else { Err(errors) }
}

#[cfg(test)]
mod tests;
