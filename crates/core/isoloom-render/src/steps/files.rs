//! The `Files` step: a machine's `files` placed on a VM, as a root shell script on Linux and as
//! PowerShell on Windows. A `source` is read from the unpacked project, so the step runs after
//! the `Project` step. Run again, it writes the same files: the step is idempotent.

use isoloom_spec::model::MachineFile;
use isoloom_spec::shell::quote;

use super::{PROJECT_DIR, WINDOWS_PROJECT_DIR};
use crate::powershell;

/// The files as a root shell script: each written (or copied from the project), then given its
/// owner (root unless the spec says otherwise) and its mode.
pub(super) fn linux(files: &[MachineFile]) -> String {
    files
        .iter()
        .map(|f| {
            let path = quote(&f.path);
            let folder = f
                .path
                .rsplit_once('/')
                .map(|(d, _)| d)
                .filter(|d| !d.is_empty())
                .map(|d| format!("mkdir -p {} && ", quote(d)))
                .unwrap_or_default();
            let write = match (&f.source, &f.content) {
                (Some(source), _) => format!("cp {} {path}", quote(&format!("{PROJECT_DIR}/{source}"))),
                (None, content) => format!("printf '%s' {} > {path}", quote(content.as_deref().unwrap_or_default())),
            };
            format!(
                "{folder}{write} && chown {}:{} {path} && chmod {} {path}\n",
                f.uid.unwrap_or(0),
                f.gid.unwrap_or(0),
                quote(f.mode_or_default())
            )
        })
        .collect::<Vec<_>>()
        .concat()
}

/// The files as PowerShell: each folder created, then each file copied from the project or
/// written as UTF-8 without a byte order mark (Windows files have no owner or mode in the spec).
pub(super) fn windows(files: &[MachineFile]) -> String {
    let lines: Vec<String> = files
        .iter()
        .map(|f| {
            let path = powershell::quote(&f.path);
            let dir = format!("New-Item -ItemType Directory -Force -Path (Split-Path -Parent {path}) | Out-Null");
            let write = match (&f.source, &f.content) {
                (Some(source), _) => {
                    let from = format!("{WINDOWS_PROJECT_DIR}\\{}", source.replace('/', "\\"));
                    format!("Copy-Item -Force -Path {} -Destination {path}", powershell::quote(&from))
                }
                (None, content) => format!("[IO.File]::WriteAllText({path}, {})", powershell::quote(content.as_deref().unwrap_or_default())),
            };
            format!("{dir}; {write}")
        })
        .collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::MachineFile;

    fn file(path: &str, source: Option<&str>, content: Option<&str>) -> MachineFile {
        MachineFile {
            path: path.into(),
            source: source.map(Into::into),
            content: content.map(Into::into),
            uid: None,
            gid: None,
            mode: None,
        }
    }

    #[test]
    fn linux_writes_copies_and_sets_owner_and_mode() {
        let owned = MachineFile {
            uid: Some(1000),
            gid: Some(1001),
            mode: Some("0600".into()),
            ..file("/etc/app/a.ini", None, Some("it's\n"))
        };
        let sh = super::linux(&[owned, file("/b.conf", Some("conf/b.conf"), None)]);
        assert_eq!(
            sh,
            "mkdir -p '/etc/app' && printf '%s' 'it'\\''s\n' > '/etc/app/a.ini' && chown 1000:1001 '/etc/app/a.ini' && chmod '0600' '/etc/app/a.ini'\n\
             cp '/opt/isoloom/conf/b.conf' '/b.conf' && chown 0:0 '/b.conf' && chmod '0644' '/b.conf'\n"
        );
    }

    #[test]
    fn the_mode_is_quoted_so_it_cannot_break_out_of_chmod() {
        // Validation keeps a mode to octal digits, but the renderer must not depend on it: a
        // library consumer rendering an unvalidated lowered spec still gets a safe word.
        let crafted = MachineFile {
            mode: Some("0644 /etc/shadow; rm -rf /".into()),
            ..file("/b.conf", None, Some("x"))
        };
        let sh = super::linux(&[crafted]);
        assert!(sh.contains("chmod '0644 /etc/shadow; rm -rf /' '/b.conf'"), "{sh}");
        assert!(!sh.contains("chmod 0644 "), "the mode reached the shell unquoted: {sh}");
    }

    #[test]
    fn windows_creates_the_folder_then_copies_or_writes() {
        let ps = super::windows(&[file("C:\\app\\a.ini", None, Some("x'y")), file("C:\\app\\b.ini", Some("conf/b.ini"), None)]);
        assert!(ps.contains("[IO.File]::WriteAllText('C:\\app\\a.ini', 'x''y')"), "{ps}");
        assert!(
            ps.contains("Copy-Item -Force -Path 'C:\\isoloom\\conf\\b.ini' -Destination 'C:\\app\\b.ini'"),
            "{ps}"
        );
        assert!(
            ps.starts_with("New-Item -ItemType Directory -Force -Path (Split-Path -Parent 'C:\\app\\a.ini')"),
            "{ps}"
        );
    }
}
