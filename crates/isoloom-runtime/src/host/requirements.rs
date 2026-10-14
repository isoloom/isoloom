//! What a cloud-services module needs on this machine besides Terraform (`cloud.requires`): a
//! command name looked up on the PATH, or a shell command from the spec that must succeed.

use std::path::{Path, PathBuf};

use isoloom_spec::model::{Spec, Target};
use isoloom_spec::shell;

use super::{Readiness, real_host};

/// What the module of `spec`'s cloud services needs here: target `cloud-services`, ready when
/// every entry of `cloud.requires` is there. `None` without `cloud.requires`.
///
/// A bare name is looked up on the PATH (with `PATHEXT` on Windows); anything else is a shell
/// command from the spec (`sh -c`, or PowerShell on Windows), run only when `run_commands` (by
/// `isoloom run`, about to run the project's code anyway; never by `doctor`, which can look at a
/// project nobody has vetted yet).
pub fn requirements(spec: &Spec, run_commands: bool) -> Option<Readiness> {
    let (run, ..) = real_host();
    requirements_with(spec, run_commands, &run)
}

/// [`requirements`], with `run` running a program (as [`Host::run`](super::Host)). On Windows,
/// bare names are looked up on the real PATH, not through `run`.
pub fn requirements_with(spec: &Spec, run_commands: bool, run: &dyn Fn(&str, &[&str]) -> Option<String>) -> Option<Readiness> {
    let on_path = |name: &str| {
        let dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
        let pathext = std::env::var("PATHEXT").ok();
        find_program(name, &dirs, pathext.as_deref(), &|p: &Path| std::fs::metadata(p).is_ok_and(|m| !m.is_dir())).is_some()
    };
    let how = Lookup {
        run,
        windows: cfg!(windows),
        on_path: &on_path,
    };
    requirements_on(spec, run_commands, &how)
}

/// How entries are checked: `run` runs a program; on Windows (`windows`) a bare name is looked
/// up with `on_path` and a command runs in PowerShell (which reads `'...'` as `sh` does);
/// elsewhere both go through `sh -c`.
struct Lookup<'a> {
    run: &'a dyn Fn(&str, &[&str]) -> Option<String>,
    windows: bool,
    on_path: &'a dyn Fn(&str) -> bool,
}

impl Lookup<'_> {
    /// Whether `entry` (a bare name when `bare`) is there.
    fn found(&self, entry: &str, bare: bool) -> bool {
        match (self.windows, bare) {
            (true, true) => (self.on_path)(entry),
            (true, false) => (self.run)("powershell", &["-NoProfile", "-NonInteractive", "-Command", entry]).is_some(),
            (false, true) => (self.run)("sh", &["-c", &format!("command -v {}", shell::quote(entry))]).is_some(),
            (false, false) => (self.run)("sh", &["-c", entry]).is_some(),
        }
    }
}

/// [`requirements_with`], with the lookup spelled out (a Windows host or not).
fn requirements_on(spec: &Spec, run_commands: bool, how: &Lookup) -> Option<Readiness> {
    let cloud = spec.cloud.as_ref().filter(|c| !c.requires.is_empty())?;
    let mut notes = Vec::new();
    for r in &cloud.requires {
        let r = r.trim();
        let bare = !r.contains(char::is_whitespace);
        if !bare && !run_commands {
            notes.push(format!("`{r}` (checked at `isoloom run`)"));
            continue;
        }
        notes.push(match (how.found(r, bare), bare) {
            (true, _) => r.to_string(),
            (false, true) => format!("! `{r}` isn't installed"),
            (false, false) => format!("! `{r}` failed"),
        });
    }
    Some(Readiness {
        target: Target::CloudServices,
        cloud: Some(cloud.provider.id().to_string()),
        ready: notes.iter().all(|n| !n.starts_with('!')),
        notes,
    })
}

/// Where a program is, the way Windows' shells find it: in each of `dirs` in turn, the name with
/// each of `pathext`'s extensions (`.COM;.EXE;.BAT;.CMD` when unset), and the name as given when
/// it already ends in one of them (`jq.exe`). A name with a path (`C:\tools\jq`, `.\jq`) is
/// looked for there, not on the PATH. `is_file` says whether a path is a file.
fn find_program(name: &str, dirs: &[PathBuf], pathext: Option<&str>, is_file: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let exts: Vec<&str> = pathext
        .filter(|e| !e.trim().is_empty())
        .unwrap_or(".COM;.EXE;.BAT;.CMD")
        .split(';')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .collect();
    let lower = name.to_ascii_lowercase();
    let has_ext = exts.iter().any(|e| lower.ends_with(&e.to_ascii_lowercase()));
    let names: Vec<String> = has_ext
        .then(|| name.to_string())
        .into_iter()
        .chain(exts.iter().map(|e| format!("{name}{e}")))
        .collect();
    let in_dir = |dir: &Path| names.iter().map(|n| dir.join(n)).find(|p| is_file(p));
    if name.contains(['/', '\\', ':']) {
        return in_dir(Path::new(""));
    }
    dirs.iter().filter(|d| !d.as_os_str().is_empty()).find_map(|d| in_dir(d))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use super::{Lookup, find_program, requirements_on};

    fn spec(fields: &str) -> isoloom_spec::model::Spec {
        isoloom_spec::parse(&format!("version: 1\nname: t\ncloud:\n  provider: aws\n  terraform: tf\n{fields}")).unwrap()
    }

    #[test]
    fn programs_are_found_on_the_path_with_pathext() {
        let files = [
            "c:\\bin\\jq.exe",
            "c:\\tools\\gsed.bat",
            "c:\\tools\\jq.cmd",
            "c:\\bin\\az.cmd",
            "d:\\x\\tool.exe",
        ];
        // Paths as Windows reads them, on every OS: case aside, `\` (`join` puts a `/` elsewhere).
        let win = |p: &Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
        let is_file = |p: &Path| files.contains(&win(p).as_str());
        let dirs = [PathBuf::from("C:\\bin"), PathBuf::from(""), PathBuf::from("C:\\tools")];
        let find = |n: &str, ext: Option<&str>| find_program(n, &dirs, ext, &is_file).map(|p| win(&p));
        let all = Some(".COM;.EXE;.BAT;.CMD");
        // Each extension in turn, in each folder in turn: the first folder wins.
        assert_eq!(find("jq", all).as_deref(), Some("c:\\bin\\jq.exe"));
        assert_eq!(find("gsed", all).as_deref(), Some("c:\\tools\\gsed.bat"));
        // Without PATHEXT (or an empty one), Windows' default list.
        assert_eq!(find("az", None).as_deref(), Some("c:\\bin\\az.cmd"));
        assert_eq!(find("az", Some(" ")).as_deref(), Some("c:\\bin\\az.cmd"));
        // Only PATHEXT's extensions count.
        assert_eq!(find("gsed", Some(".EXE")), None);
        assert_eq!(find("jq", Some(".CMD")).as_deref(), Some("c:\\tools\\jq.cmd"));
        // A name with one of them is taken as given too; another extension isn't one.
        assert_eq!(find("jq.exe", Some(".exe;.cmd")).as_deref(), Some("c:\\bin\\jq.exe"));
        assert_eq!(find("gsed.bat", all).as_deref(), Some("c:\\tools\\gsed.bat"));
        assert_eq!(find("jq.exe", Some(".CMD")), None);
        // A path is looked at where it says, not on the PATH.
        assert_eq!(find("D:\\x\\tool", None).as_deref(), Some("d:\\x\\tool.exe"));
        assert_eq!(find("D:\\y\\jq", None), None);
        assert_eq!(find("missing", None), None);
        assert_eq!(find("", None), None);
    }

    #[test]
    fn requirements_on_windows_look_up_names_and_run_commands_in_powershell() {
        let s = spec("  requires: [gsed, \"python -c 'import boto3'\", jq]\n");
        let seen = RefCell::new(Vec::new());
        let run = |p: &str, a: &[&str]| {
            seen.borrow_mut().push(format!("{p} {}", a.join(" ")));
            None
        };
        let jq_only = |n: &str| n == "jq";
        let windows = Lookup {
            run: &run,
            windows: true,
            on_path: &jq_only,
        };
        let r = requirements_on(&s, true, &windows).unwrap();
        // Names never go through a shell (there may be no `sh`); commands go to PowerShell.
        assert_eq!(*seen.borrow(), ["powershell -NoProfile -NonInteractive -Command python -c 'import boto3'"]);
        assert_eq!(r.summary(), "not ready: `gsed` isn't installed; `python -c 'import boto3'` failed");
        seen.borrow_mut().clear();
        let everything = |_: &str| true;
        let windows = Lookup {
            run: &run,
            windows: true,
            on_path: &everything,
        };
        let r = requirements_on(&s, false, &windows).unwrap();
        assert!(seen.borrow().is_empty());
        assert!(r.ready, "{r:?}");
        // Elsewhere, `sh` as before.
        let unix = Lookup {
            run: &run,
            windows: false,
            on_path: &everything,
        };
        let r = requirements_on(&s, true, &unix).unwrap();
        assert_eq!(seen.borrow()[0], "sh -c command -v 'gsed'");
        assert!(!r.ready);
    }

    // Through `sh` (Windows has its own lookup, tested above).
    #[cfg(not(windows))]
    #[test]
    fn requires_are_looked_up_or_run() {
        assert!(super::requirements_with(&spec(""), true, &|_, _| Some(String::new())).is_none());
        let s = spec("  requires: [gsed, \"python3 -c 'import boto3'\", jq]\n");
        let seen = RefCell::new(Vec::new());
        // `jq` is there; the others aren't.
        let run = |p: &str, a: &[&str]| {
            seen.borrow_mut().push(format!("{p} {}", a.join(" ")));
            a.last().is_some_and(|c| c.ends_with("'jq'")).then(String::new)
        };
        let r = super::requirements_with(&s, true, &run).unwrap();
        assert!(!r.ready);
        assert_eq!(r.cloud.as_deref(), Some("aws"));
        assert_eq!(
            *seen.borrow(),
            ["sh -c command -v 'gsed'", "sh -c python3 -c 'import boto3'", "sh -c command -v 'jq'"]
        );
        assert_eq!(r.summary(), "not ready: `gsed` isn't installed; `python3 -c 'import boto3'` failed");
        let all = super::requirements_with(&s, true, &|_, _| Some(String::new())).unwrap();
        assert!(all.ready, "{all:?}");
        // Without `run_commands` (`doctor`), the spec's shell commands aren't run: listed for `run`.
        seen.borrow_mut().clear();
        let r = super::requirements_with(&s, false, &run).unwrap();
        assert_eq!(*seen.borrow(), ["sh -c command -v 'gsed'", "sh -c command -v 'jq'"]);
        assert_eq!(r.summary(), "not ready: `gsed` isn't installed");
        assert!(r.notes.contains(&"`python3 -c 'import boto3'` (checked at `isoloom run`)".to_string()), "{r:?}");
    }

    #[cfg(windows)]
    #[test]
    fn requirements_find_real_programs_on_windows() {
        let s = spec("  requires: [cmd, CMD.EXE, isoloom-no-such-tool, \"cmd /C exit 0\", \"cmd /C 'exit 3'\"]\n");
        let r = super::requirements(&s, true).unwrap();
        assert_eq!(
            r.summary(),
            "not ready: `isoloom-no-such-tool` isn't installed; `cmd /C 'exit 3'` failed",
            "{r:?}"
        );
        assert!(r.notes.contains(&"cmd".to_string()) && r.notes.contains(&"CMD.EXE".to_string()), "{r:?}");
        assert!(r.notes.contains(&"cmd /C exit 0".to_string()), "{r:?}");
    }
}
