//! Hyper-V, on a Windows host: whether Vagrant's Hyper-V provider can be used from this shell.

use super::Host;

/// Hyper-V's management service state (`Running`, `Stopped`, or `Missing` when Hyper-V isn't
/// installed) and the user's rights over it (`admin`: elevated; `hyperv-admin`: a member of
/// Hyper-V Administrators; `user`), in one line. No double quotes: it goes through a command line.
const PROBE: &str = "$s = Get-Service vmms -ErrorAction SilentlyContinue; \
    $p = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent()); \
    $r = if ($p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { 'admin' } \
    elseif ($p.IsInRole([Security.Principal.SecurityIdentifier]'S-1-5-32-578')) { 'hyperv-admin' } else { 'user' }; \
    if ($s) { $s.Status.ToString() + ' ' + $r } else { 'Missing ' + $r }";

/// Why Hyper-V can't be used from here, when it is on but the shell isn't elevated: another
/// provider may still do, so this is a note rather than the reason nothing runs.
pub const NOT_ELEVATED: &str = "Hyper-V is on, but Vagrant needs an elevated shell for it (run as administrator, or join Hyper-V Administrators)";

/// Hyper-V, on a Windows host (`windows`): `Ok` with the provider's name when Vagrant can use it
/// from this shell, `Err` with the reason when it can't (off, its service stopped, or the shell
/// not elevated: Vagrant's Hyper-V provider needs an administrator or a member of Hyper-V
/// Administrators). `None` elsewhere.
pub fn probe(h: &Host, windows: bool) -> Option<Result<String, String>> {
    if !windows {
        return None;
    }
    let out = (h.run)("powershell", &["-NoProfile", "-NonInteractive", "-Command", PROBE]).unwrap_or_default();
    let mut words = out.split_whitespace();
    let (state, rights) = (words.next().unwrap_or("Missing"), words.next().unwrap_or("user"));
    Some(match (state, rights) {
        ("Running", "admin" | "hyperv-admin") => Ok(match (h.env)("ISOLOOM_HYPERV_SWITCH") {
            Some(s) => format!("Hyper-V (switch {s})"),
            None => "Hyper-V".to_string(),
        }),
        ("Running", _) => Err(NOT_ELEVATED.into()),
        ("Missing", _) => Err(
            "Hyper-V isn't turned on (Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V -All, then restart; Windows Pro, Enterprise or Server)".into(),
        ),
        (other, _) => Err(format!("Hyper-V's service (vmms) is {}, not running", other.to_lowercase())),
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{Host, probe};

    /// The probe on a Windows host whose PowerShell answers `answer`, with `switch` as
    /// `ISOLOOM_HYPERV_SWITCH`.
    fn on(answer: &'static str, switch: Option<&'static str>) -> Option<Result<String, String>> {
        let run = move |p: &str, _: &[&str]| (p == "powershell").then(|| answer.to_string());
        let env = move |k: &str| (k == "ISOLOOM_HYPERV_SWITCH").then(|| switch.map(str::to_string)).flatten();
        let no = |_: &Path| false;
        let h = Host {
            run: &run,
            env: &env,
            exists: &no,
            opens: &no,
            home: PathBuf::new(),
        };
        probe(&h, true)
    }

    #[test]
    fn hyperv_counts_on_windows_from_an_elevated_shell() {
        assert_eq!(on("Running admin", None), Some(Ok("Hyper-V".to_string())));
        assert_eq!(on("Running hyperv-admin", Some("LabNAT")), Some(Ok("Hyper-V (switch LabNAT)".to_string())));
        assert!(on("Running user", None).unwrap().unwrap_err().contains("elevated shell"));
        assert!(on("Missing admin", None).unwrap().unwrap_err().contains("isn't turned on"));
        assert!(on("Stopped admin", None).unwrap().unwrap_err().contains("stopped, not running"));
        // PowerShell missing or failing: Hyper-V counts as off.
        assert!(on("", None).unwrap().unwrap_err().contains("isn't turned on"));
    }

    #[test]
    fn nothing_to_probe_elsewhere() {
        let run = |_: &str, _: &[&str]| None;
        let env = |_: &str| None;
        let no = |_: &Path| false;
        let h = Host {
            run: &run,
            env: &env,
            exists: &no,
            opens: &no,
            home: PathBuf::new(),
        };
        assert_eq!(probe(&h, false), None);
    }
}
