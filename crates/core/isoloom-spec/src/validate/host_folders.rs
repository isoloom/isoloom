//! A machine's `host_folders`: a folder of the computer that runs the lab, at a safe path of its
//! own in a Linux machine. Whether a project folder exists is [`super::validate_files`]'s.

use std::collections::BTreeSet;

use super::Problems;
use super::machines::Subject;
use super::volumes::safe_path;
use crate::model::{HostFolder, HostPath};

/// The folders Isoloom keeps for itself in a machine (the project's copy, its state, its
/// settings) and the kernel's: a host folder there would hide them.
const RESERVED: &[&str] = &[
    crate::linux::PROJECT_DIR,
    crate::linux::STATE_DIR,
    crate::linux::CONFIG_DIR,
    "/proc",
    "/sys",
    "/dev",
];

/// Each host folder of the machine: a Linux machine, a usable `from`, and a path of its own.
pub(super) fn of_machine(s: &Subject, p: &mut Problems) {
    if s.m.host_folders.is_empty() {
        return;
    }
    if s.m.vm.as_ref().is_some_and(|v| crate::images::is_windows(&v.os)) {
        p.add(
            format!("{}.host_folders", s.at),
            "a Windows machine has no host folders: they are mounted in Linux machines",
        );
        return;
    }
    let mut taken: BTreeSet<&str> = s.m.volumes.values().map(|v| v.path().trim_end_matches('/')).collect();
    taken.extend(s.m.tmpfs.iter().map(|t| t.path().trim_end_matches('/')));
    taken.extend(s.m.disks.values().filter_map(|d| d.mount.as_deref()).map(|m| m.trim_end_matches('/')));
    for (i, f) in s.m.host_folders.iter().enumerate() {
        let at = format!("{}.host_folders[{i}]", s.at);
        from(&at, f, p);
        let path = f.path.as_str();
        if !safe_path(path) {
            p.add(
                format!("{at}.path"),
                format!("`{path}` isn't a safe absolute path inside the machine (no spaces or ':'), like /srv/shared"),
            );
        } else if let Some(own) = RESERVED
            .iter()
            .find(|r| path == **r || path.strip_prefix(**r).is_some_and(|rest| rest.starts_with('/')))
        {
            p.add(
                format!("{at}.path"),
                format!("`{path}` is inside `{own}`, Isoloom's own or the kernel's: mount the folder elsewhere"),
            );
        } else if !taken.insert(path.trim_end_matches('/')) {
            p.add(
                format!("{at}.path"),
                format!("`{path}` is already a volume, a disk, a tmpfs or a host folder of this machine"),
            );
        }
    }
}

/// A folder named the way every tool reads it the same: relative to the project, `~` or `~/...`
/// (not another user's `~name`), or absolute; nothing Compose would read as a reference.
fn from(at: &str, f: &HostFolder, p: &mut Problems) {
    let from = f.from.as_str();
    let at = format!("{at}.from");
    if from.trim().is_empty() {
        p.add(
            at,
            "name the folder of the computer: relative to the project folder, `~/...` or an absolute path",
        );
    } else if from.contains(['$', '{', '}']) || from.chars().any(char::is_control) {
        p.add(
            at,
            format!("`{from}`: no `$`, `{{`, `}}` or control characters (Compose would read them as its own)"),
        );
    } else if matches!(f.host(), HostPath::Project(_)) && from.starts_with('~') {
        p.add(
            at,
            format!("`{from}`: `~` or `~/...` is the home folder of the user running the environment; another user's (`~name`) isn't one"),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// The problems of machine `web` (a container and a Debian VM, or `form`) with `host_folders`.
    fn with(form: &str, folders: &str) -> Vec<String> {
        problems(&lab(&format!(
            "machines:\n  web: {{ networks: {{ lan: 10 }}, {form}, host_folders: [{folders}] }}\n"
        )))
    }

    const BOTH: &str = "docker: { image: nginx }, vm: { os: debian@12 }, volumes: { data: /srv/data }, tmpfs: [/run/x]";

    #[test]
    fn a_folder_from_the_project_the_home_folder_or_an_absolute_path_passes() {
        let ok = "{ from: shared, path: /srv/shared }, { from: '~/notes', path: /home/notes, read_only: true }, { from: /srv/share, path: /mnt/share }, { from: 'C:\\share', path: /mnt/c }, { from: ../up, path: /up }";
        assert_eq!(with(BOTH, ok), Vec::<String>::new());
    }

    #[test]
    fn a_path_is_safe_absolute_its_own_and_not_isoloom_s_or_the_kernel_s() {
        let at = "machines.web.host_folders";
        assert_eq!(
            with(BOTH, "{ from: a, path: srv }, { from: a, path: '/a b' }"),
            [
                format!("{at}[0].path: `srv` isn't a safe absolute path inside the machine (no spaces or ':'), like /srv/shared"),
                format!("{at}[1].path: `/a b` isn't a safe absolute path inside the machine (no spaces or ':'), like /srv/shared"),
            ]
        );
        assert_eq!(
            with(
                BOTH,
                "{ from: a, path: /opt/isoloom/x }, { from: a, path: /proc }, { from: a, path: /optional }"
            ),
            [
                format!("{at}[0].path: `/opt/isoloom/x` is inside `/opt/isoloom`, Isoloom's own or the kernel's: mount the folder elsewhere"),
                format!("{at}[1].path: `/proc` is inside `/proc`, Isoloom's own or the kernel's: mount the folder elsewhere"),
            ]
        );
        assert_eq!(
            with(
                BOTH,
                "{ from: a, path: /srv/data }, { from: a, path: /run/x/ }, { from: a, path: /m }, { from: b, path: /m }"
            ),
            [
                format!("{at}[0].path: `/srv/data` is already a volume, a disk, a tmpfs or a host folder of this machine"),
                format!("{at}[1].path: `/run/x/` is already a volume, a disk, a tmpfs or a host folder of this machine"),
                format!("{at}[3].path: `/m` is already a volume, a disk, a tmpfs or a host folder of this machine"),
            ]
        );
    }

    #[test]
    fn from_names_a_folder_every_tool_reads_the_same() {
        let at = "machines.web.host_folders";
        assert_eq!(
            with(BOTH, "{ from: '', path: /a }, { from: '${HOME}/x', path: /b }, { from: '~bob/x', path: /c }"),
            [
                format!("{at}[0].from: name the folder of the computer: relative to the project folder, `~/...` or an absolute path"),
                format!("{at}[1].from: `${{HOME}}/x`: no `$`, `{{`, `}}` or control characters (Compose would read them as its own)"),
                format!("{at}[2].from: `~bob/x`: `~` or `~/...` is the home folder of the user running the environment; another user's (`~name`) isn't one"),
            ]
        );
    }

    #[test]
    fn a_windows_machine_has_none() {
        assert_eq!(
            with("vm: { os: windows-server@2022 }", "{ from: a, path: /a }"),
            ["machines.web.host_folders: a Windows machine has no host folders: they are mounted in Linux machines"]
        );
    }
}
