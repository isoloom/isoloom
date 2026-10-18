//! A service Isoloom defines (a `service` step with `run`) on a Linux VM: a systemd unit, or an
//! OpenRC script on Alpine. Each runs the step's program in the foreground (OpenRC backgrounds
//! it itself), as its user, in its folder, restarted when it fails.

use isoloom_plan::configure::Family;
use isoloom_spec::shell::quote;
use isoloom_spec::syntax::{AbsPath, Argv, UnitName, UserName};

/// A Linux service unit's file for `family`: the same text the shell renderer writes and the
/// Ansible backend copies, so a defined service runs the same under either carrier. An OpenRC
/// script on Alpine, else a systemd unit.
#[must_use]
pub fn linux_unit(family: Family, name: &UnitName, run: &Argv, user: Option<&UserName>, cwd: Option<&AbsPath>) -> String {
    linux(family, name, run, user, cwd)
}

/// The unit's file for `family`: an OpenRC script on Alpine, else a systemd unit.
#[must_use]
pub(super) fn linux(family: Family, name: &UnitName, run: &Argv, user: Option<&UserName>, cwd: Option<&AbsPath>) -> String {
    if family == Family::Alpine {
        openrc(name, run, user, cwd)
    } else {
        systemd(name, run, user, cwd)
    }
}

/// A systemd unit.
fn systemd(name: &UnitName, run: &Argv, user: Option<&UserName>, cwd: Option<&AbsPath>) -> String {
    let exec: Vec<String> = run.args().iter().map(|a| systemd_word(a)).collect();
    let user = user.map(|u| format!("User={u}\n")).unwrap_or_default();
    // A bare path: systemd doesn't unquote `WorkingDirectory=`, but expands its `%` specifiers.
    let cwd = cwd.map(|d| format!("WorkingDirectory={}\n", d.as_str().replace('%', "%%"))).unwrap_or_default();
    format!(
        "[Unit]\nDescription={name}, defined by Isoloom\nWants=network-online.target\nAfter=network-online.target\n\n[Service]\nExecStart={}\n{user}{cwd}Restart=on-failure\n\n[Install]\nWantedBy=multi-user.target\n",
        exec.join(" ")
    )
}

/// One word of a systemd command line, quoted: systemd unescapes C-style in double quotes and
/// expands `%` specifiers and `$` variables, each doubled to stay literal.
#[must_use]
pub fn systemd_word(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '%' => out.push_str("%%"),
            '$' => out.push_str("$$"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// An OpenRC script: OpenRC runs `command` with `command_args` (read by `sh`, so each word is
/// quoted for it) in the background.
fn openrc(name: &UnitName, run: &Argv, user: Option<&UserName>, cwd: Option<&AbsPath>) -> String {
    let args: Vec<String> = run.args().iter().skip(1).map(|a| quote(a)).collect();
    let user = user.map(|u| format!("command_user={}\n", quote(u.as_str()))).unwrap_or_default();
    let cwd = cwd.map(|d| format!("directory={}\n", quote(d.as_str()))).unwrap_or_default();
    format!(
        "#!/sbin/openrc-run\ndescription={}\ncommand={}\ncommand_args={}\ncommand_background=true\npidfile=\"/run/${{RC_SVCNAME}}.pid\"\n{user}{cwd}\ndepend() {{\n\tneed net\n}}\n",
        quote(&format!("{name}, defined by Isoloom")),
        quote(run.program()),
        quote(&args.join(" "))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Argv {
        Argv::try_from(args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn a_systemd_unit_keeps_every_word_literal() {
        let unit = linux(
            Family::Debian,
            &"api".parse().unwrap(),
            &argv(&["/srv/api", "--name", "a \"b\" $HOME 100%"]),
            "api".parse().ok().as_ref(),
            "/srv".parse().ok().as_ref(),
        );
        assert!(
            unit.contains("ExecStart=\"/srv/api\" \"--name\" \"a \\\"b\\\" $$HOME 100%%\"\nUser=api\nWorkingDirectory=/srv\nRestart=on-failure\n"),
            "{unit}"
        );
        assert!(unit.ends_with("[Install]\nWantedBy=multi-user.target\n"));
    }

    /// systemd takes `WorkingDirectory=` as a bare path (quoted, it isn't absolute: a real
    /// VirtualBox run refused the unit), with `%` its only specifier.
    #[test]
    fn a_working_directory_is_a_bare_path_its_percent_signs_doubled() {
        let unit = linux(
            Family::Debian,
            &"api".parse().unwrap(),
            &argv(&["/srv/api"]),
            None,
            "/srv/my app/100%".parse().ok().as_ref(),
        );
        assert!(unit.contains("\nWorkingDirectory=/srv/my app/100%%\n"), "{unit}");
    }

    #[test]
    fn alpine_gets_an_openrc_script_its_shell_reads_back() {
        let unit = linux(Family::Alpine, &"api".parse().unwrap(), &argv(&["/srv/api", "it's"]), None, None);
        assert!(unit.starts_with("#!/sbin/openrc-run\n"), "{unit}");
        assert!(
            unit.contains("command='/srv/api'\ncommand_args=''\\''it'\\''\\'\\'''\\''s'\\'''\ncommand_background=true\n"),
            "{unit}"
        );
    }
}
