//! Waiting for a machine a VM depends on ([`Step::WaitFor`](isoloom_plan::Step::WaitFor)):
//! until it answers on every port it serves, up to [`TIMEOUT`] seconds.

use crate::powershell;

/// Seconds a machine waits for one it depends on.
const TIMEOUT: u32 = 900;

/// A shell loop that waits until `host` answers on every port (bash's `/dev/tcp`, then `nc`),
/// giving up after [`TIMEOUT`] seconds.
pub(super) fn linux(host: &str, ports: &[u16]) -> String {
    let probes = ports
        .iter()
        .map(|p| format!("(bash -c '</dev/tcp/{host}/{p}' 2>/dev/null || nc -z -w 2 {host} {p} 2>/dev/null)"))
        .collect::<Vec<_>>()
        .join(" && ");
    format!(
        "i=0; until {probes}; do i=$((i+2)); if [ $i -ge {TIMEOUT} ]; then echo \"{host} didn't answer within {TIMEOUT}s\" >&2; exit 1; fi; sleep 2; done; echo \"{host} answers\"\n"
    )
}

/// The same in PowerShell; `None` when there is no port to wait on.
pub(super) fn windows(host: &str, ports: &[u16]) -> Option<String> {
    if ports.is_empty() {
        return None;
    }
    let ports = ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ");
    let target = powershell::quote(host);
    let failed = powershell::quote(&format!("{host} didn't answer on port "));
    let answers = powershell::quote(&format!("{host} answers"));
    Some(format!(
        "$t = (Get-Date).AddSeconds({TIMEOUT}); foreach ($p in @({ports})) {{ while (-not (Test-NetConnection {target} -Port $p -WarningAction SilentlyContinue).TcpTestSucceeded) {{ if ((Get-Date) -gt $t) {{ throw ({failed} + $p) }}; Start-Sleep 5 }} }}; {answers}"
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_machine_waits_on_every_port_then_gives_up() {
        let sh = super::linux("db", &[5432, 6379]);
        assert_eq!(
            sh,
            "i=0; until (bash -c '</dev/tcp/db/5432' 2>/dev/null || nc -z -w 2 db 5432 2>/dev/null) && (bash -c '</dev/tcp/db/6379' 2>/dev/null || nc -z -w 2 db 6379 2>/dev/null); do i=$((i+2)); if [ $i -ge 900 ]; then echo \"db didn't answer within 900s\" >&2; exit 1; fi; sleep 2; done; echo \"db answers\"\n"
        );
        let ps = super::windows("db", &[5432, 6379]).expect("ports to wait on");
        assert!(
            ps.starts_with("$t = (Get-Date).AddSeconds(900); foreach ($p in @(5432, 6379)) { while (-not (Test-NetConnection 'db' -Port $p"),
            "{ps}"
        );
        assert!(
            ps.ends_with("throw ('db didn''t answer on port ' + $p) }; Start-Sleep 5 } }; 'db answers'"),
            "{ps}"
        );
        assert_eq!(super::windows("db", &[]), None);
    }
}
