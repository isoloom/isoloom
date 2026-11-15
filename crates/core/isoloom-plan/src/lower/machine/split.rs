//! A machine's files split by where their content comes from: configuration stays a file, a
//! run-time value (an input, a secret) becomes a secret file.

use isoloom_spec::model::{MachineFile, MachineSecret, SecretMount};
use isoloom_spec::syntax::{self, FileSource};

use super::super::trail::Trail;
use super::input_secret;

/// The machine's files split by source: configuration as files, values as secret files (an
/// input's as the secret `input-<name>`), and the inputs read that way.
pub(super) fn files(name: &str, m: &syntax::Machine, trail: &mut Trail) -> (Vec<MachineFile>, Vec<MachineSecret>, Vec<(String, String)>) {
    let windows = m.os.is_some_and(syntax::OsRef::is_windows);
    let (mut files, mut secrets, mut inputs) = (Vec::new(), Vec::new(), Vec::new());
    for (i, f) in m.files.iter().enumerate() {
        let (source, content) = match &f.source {
            FileSource::Source(p) => (Some(p.clone()), None),
            FileSource::Content(c) => (None, Some(c.clone())),
            FileSource::Input(input) => {
                let secret = input_secret(input.as_str());
                inputs.push((secret.clone(), input.to_string()));
                trail.file(name, "secrets", secrets.len(), i);
                secrets.push(value_file(&secret, f, windows));
                continue;
            }
            FileSource::Secret(secret) => {
                trail.file(name, "secrets", secrets.len(), i);
                secrets.push(value_file(secret, f, windows));
                continue;
            }
        };
        trail.file(name, "files", files.len(), i);
        files.push(MachineFile {
            path: f.path.clone(),
            source,
            content,
            uid: f.uid,
            gid: f.gid,
            mode: f.mode.map(|mode| mode.to_string()),
        });
    }
    (files, secrets, inputs)
}

/// A value's file: the secret at its path, root's and `0400` unless the spec says (no mode on
/// Windows).
fn value_file(secret: &str, f: &syntax::File, windows: bool) -> MachineSecret {
    MachineSecret::Mount(SecretMount {
        source: secret.to_owned(),
        target: Some(f.path.clone()),
        uid: f.uid,
        gid: f.gid,
        mode: (!windows).then(|| f.mode_or_default()),
    })
}
