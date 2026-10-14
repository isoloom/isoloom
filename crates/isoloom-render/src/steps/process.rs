//! A VM's own process: its `before` scripts and its command as services kept across restarts
//! ([`boot::service`]), and its healthcheck tried in the `Ready` step, on Linux and Windows.
//! Each runs where the machine's process runs (its working directory, by default the project
//! folder) with its environment and its inputs, as a container's process would.

use std::fmt::Write as _;

use isoloom_plan::{Environment, Health, HealthTest, MachinePlan};
use isoloom_spec::model::CommandLine;

use super::{PROJECT_DIR, inputs_file};
use crate::boot::{self, Kind};
use crate::ps;
use crate::shell::quote;

/// Where the project is on a Windows VM, and the machine's scripts with it.
const WINDOWS_DIR: &str = "C:\\isoloom";

/// The lines every script of the process starts with: its inputs, its environment, its
/// directory.
fn prelude(env: &Environment, machine: &str, mp: &MachinePlan) -> String {
    let mut sh = String::new();
    if let Some(file) = inputs_file(env, machine) {
        let _ = writeln!(sh, "set -a; . {file}; set +a");
    }
    for (k, v) in &mp.settings.environment {
        let _ = writeln!(sh, "export {k}={}", quote(v));
    }
    let dir = mp.process.working_dir.as_deref().unwrap_or(PROJECT_DIR);
    let _ = writeln!(sh, "mkdir -p {d} && cd {d}", d = quote(dir));
    sh
}

/// The `before` scripts, run in order now (a failure fails the set-up) and at every boot,
/// before the command.
pub(super) fn before(env: &Environment, machine: &str, paths: &[String]) -> Option<String> {
    let mp = env.machines.get(machine)?;
    let mut script = format!("set -e\n{}", prelude(env, machine, mp));
    for p in paths {
        let _ = writeln!(script, "sh {}", quote(&format!("{PROJECT_DIR}/{p}")));
    }
    Some(boot::service(
        "before",
        "The machine's before steps, ahead of its command",
        &script,
        Kind::Once,
        &[],
    ))
}

/// The command, as a service restarted when it fails, after the `before` scripts.
pub(super) fn command(env: &Environment, machine: &str) -> Option<String> {
    let mp = env.machines.get(machine)?;
    let line = mp.process.command.as_ref()?.argv().iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
    let script = format!("{}exec {line}\n", prelude(env, machine, mp));
    let after: &[&str] = if mp.process.before.is_empty() { &[] } else { &["before"] };
    Some(boot::service("command", "The machine's own command", &script, Kind::Daemon, after))
}

/// The healthcheck, tried until it passes before the machine is marked ready: the set-up fails
/// when it hasn't passed after its retries.
pub(super) fn healthy(env: &Environment, machine: &str) -> Option<String> {
    let mp = env.machines.get(machine)?;
    let h = mp.process.health.as_ref()?;
    let test = match &h.test {
        // The VM's curl, else its wget: images ship one or the other.
        HealthTest::Http { port, path } => format!(
            "curl -fsS -o /dev/null --max-time {} http://127.0.0.1:{port}{path} || {}",
            h.timeout,
            h.shell_test("wget")
        ),
        HealthTest::Command(_) => format!("timeout {} sh -c {}", h.timeout, quote(&h.shell_test("wget"))),
    };
    let failed = quote(&format!("{machine}: its healthcheck didn't pass after {} tries", h.retries));
    Some(format!(
        "isoloom_healthy() {{ (\n{}{test}\n) >/dev/null 2>&1; }}\nsleep {}; n=0; until isoloom_healthy; do n=$((n+1)); if [ \"$n\" -ge {} ]; then echo {failed} >&2; exit 1; fi; sleep {}; done\n",
        prelude(env, machine, mp),
        h.start_period,
        h.retries,
        h.interval
    ))
}

/// A command line in PowerShell: a text as it is, arguments each quoted after `&`.
fn powershell(c: &CommandLine) -> String {
    match c {
        CommandLine::Shell(text) => text.clone(),
        CommandLine::Exec(args) => format!("& {}", args.iter().map(|a| ps(a)).collect::<Vec<_>>().join(" ")),
    }
}

/// A script under the project folder, written from `lines`.
fn windows_script(name: &str, lines: &[String]) -> String {
    let body = lines.join("\n");
    format!("Set-Content -Path '{WINDOWS_DIR}\\{name}.ps1' -Value {}", ps(&body))
}

/// A startup task running `name.ps1` as SYSTEM, started now.
fn startup_task(name: &str) -> String {
    format!(
        "$a = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File {WINDOWS_DIR}\\{name}.ps1'; Register-ScheduledTask -TaskName 'isoloom-{name}' -Action $a -Trigger (New-ScheduledTaskTrigger -AtStartup) -User 'SYSTEM' -RunLevel Highest -Force | Out-Null; Start-ScheduledTask -TaskName 'isoloom-{name}'"
    )
}

/// The working directory, on Windows.
fn windows_dir(mp: &MachinePlan) -> String {
    ps(mp.process.working_dir.as_deref().unwrap_or(WINDOWS_DIR))
}

/// The `before` scripts on Windows: `before.ps1`, run now; the command's task runs it first at
/// every boot, or a startup task of its own when there is no command.
pub(super) fn windows_before(env: &Environment, machine: &str, paths: &[String]) -> Option<String> {
    let mp = env.machines.get(machine)?;
    let mut lines = vec![format!(
        "New-Item -ItemType Directory -Force {d} | Out-Null; Set-Location {d}",
        d = windows_dir(mp)
    )];
    lines.extend(paths.iter().map(|p| {
        format!(
            "& powershell -NoProfile -ExecutionPolicy Bypass -File {}; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}",
            ps(&format!("{WINDOWS_DIR}\\{}", p.replace('/', "\\")))
        )
    }));
    let mut out = format!(
        "{}; & powershell -NoProfile -ExecutionPolicy Bypass -File {WINDOWS_DIR}\\before.ps1; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}",
        windows_script("before", &lines)
    );
    if mp.process.command.is_none() {
        let _ = write!(out, "; {}", startup_task("before"));
    }
    Some(out)
}

/// The command on Windows: a startup task (as SYSTEM) running `command.ps1`, after `before.ps1`.
pub(super) fn windows_command(env: &Environment, machine: &str) -> Option<String> {
    let mp = env.machines.get(machine)?;
    let mut lines = Vec::new();
    if !mp.process.before.is_empty() {
        lines.push(format!("& powershell -NoProfile -ExecutionPolicy Bypass -File {WINDOWS_DIR}\\before.ps1"));
    }
    lines.push(format!(
        "New-Item -ItemType Directory -Force {d} | Out-Null; Set-Location {d}",
        d = windows_dir(mp)
    ));
    lines.push(powershell(mp.process.command.as_ref()?));
    Some(format!("{}; {}", windows_script("command", &lines), startup_task("command")))
}

/// The healthcheck on Windows, tried before the machine is marked ready.
pub(super) fn windows_healthy(env: &Environment, machine: &str) -> Option<String> {
    let mp = env.machines.get(machine)?;
    let h: &Health = mp.process.health.as_ref()?;
    let test = match &h.test {
        HealthTest::Http { port, path } => format!(
            "(Invoke-WebRequest -UseBasicParsing -TimeoutSec {} -Uri 'http://127.0.0.1:{port}{}').StatusCode -lt 400",
            h.timeout,
            path.replace('\'', "''")
        ),
        HealthTest::Command(c) => format!("Set-Location {}; {} | Out-Null; $LASTEXITCODE -eq 0", windows_dir(mp), powershell(c)),
    };
    let failed = ps(&format!("{machine}: its healthcheck didn't pass after {} tries", h.retries));
    Some(format!(
        "$ok = {{ try {{ {test} }} catch {{ $false }} }}; Start-Sleep -Seconds {}; $n = 0; while (-not (& $ok)) {{ $n++; if ($n -ge {}) {{ throw {failed} }}; Start-Sleep -Seconds {} }}; ",
        h.start_period, h.retries, h.interval
    ))
}

#[cfg(test)]
mod tests {
    use isoloom_plan::Environment;
    use isoloom_plan::lifecycle::Step;
    use isoloom_spec::model::Target;

    const SPEC: &str = r"
version: 1
name: own-process
inputs: [FLAG]
networks:
  lan: { cidr: 10.84.0.0/24 }
machines:
  web:
    networks: { lan: 10 }
    inputs: [FLAG]
    environment: { MODE: lab }
    services: [{ port: 8000 }]
    vm: { os: debian-12, provision: [setup.sh] }
    command: [python3, -m, http.server, '8000']
    working_dir: /srv/site
    before: [flag.sh]
    healthcheck: { http: { port: 8000, path: /health }, interval: 2s, retries: 30, start_period: 0s }
  win:
    networks: { lan: 11 }
    vm: { os: windows-server-2022, provision: [setup.ps1] }
    command: 'python -m http.server 80'
    before: [flag.ps1]
    healthcheck: { test: [python, check.py] }
";

    fn env() -> Environment {
        let spec = isoloom_spec::parse(SPEC).expect("the test spec parses");
        assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
        Environment::build(&spec, Target::Vagrant, None)
    }

    fn step(env: &Environment, machine: &str, pick: impl Fn(&Step) -> bool) -> Step {
        env.machines[machine].lifecycle.iter().find(|s| pick(s)).expect("the step is planned").clone()
    }

    /// `sh -n`: the script parses.
    fn parses(sh: &str) {
        let out = std::process::Command::new("sh").arg("-n").arg("-c").arg(sh).output().expect("sh runs");
        assert!(out.status.success(), "{}\n{sh}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn the_before_steps_then_the_command_run_as_services() {
        let env = env();
        let lifecycle = &env.machines["web"].lifecycle;
        let at = |s: &Step| lifecycle.iter().position(|x| x == s).expect("planned");
        let before = step(&env, "web", |s| matches!(s, Step::Before { .. }));
        assert!(at(&before) < at(&Step::Command) && at(&Step::Command) < at(&Step::Ready));
        assert!(
            lifecycle.contains(&Step::Inputs { persist: true }),
            "the services read the inputs at every boot"
        );
        let sh = super::super::linux(&env, "web", &before).expect("renders");
        assert!(
            sh.contains(
                "set -a; . /var/lib/isoloom/inputs.env; set +a\nexport MODE='lab'\nmkdir -p '/srv/site' && cd '/srv/site'\nsh '/opt/isoloom/flag.sh'\n"
            ),
            "{sh}"
        );
        assert!(sh.contains("Type=oneshot") && sh.contains("systemctl restart isoloom-before.service"), "{sh}");
        parses(&sh);
        let sh = super::super::linux(&env, "web", &Step::Command).expect("renders");
        assert!(
            sh.contains("exec 'python3' '-m' 'http.server' '8000'\n") && sh.contains("Requires=isoloom-before.service"),
            "{sh}"
        );
        parses(&sh);
    }

    #[test]
    fn the_healthcheck_is_tried_before_the_machine_is_ready() {
        let env = env();
        let sh = super::super::linux(&env, "web", &Step::Ready).expect("renders");
        assert!(sh.contains("curl -fsS -o /dev/null --max-time 3 http://127.0.0.1:8000/health || wget"), "{sh}");
        assert!(sh.contains("sleep 0; n=0; until isoloom_healthy; do n=$((n+1)); if [ \"$n\" -ge 30 ]"), "{sh}");
        assert!(sh.ends_with("echo ready > /var/lib/isoloom/ready\n"), "{sh}");
        parses(&sh);
    }

    #[test]
    fn windows_runs_them_as_startup_tasks() {
        let env = env();
        let before = step(&env, "win", |s| matches!(s, Step::Before { .. }));
        let ps = super::super::windows(&env, "win", &before).expect("renders");
        assert!(
            ps.contains("C:\\isoloom\\flag.ps1") && !ps.contains("Register-ScheduledTask"),
            "the command's task runs it: {ps}"
        );
        let ps = super::super::windows(&env, "win", &Step::Command).expect("renders");
        assert!(
            ps.contains("before.ps1") && ps.contains("python -m http.server 80") && ps.contains("-TaskName 'isoloom-command'"),
            "{ps}"
        );
        let ps = super::super::windows(&env, "win", &Step::Ready).expect("renders");
        assert!(ps.contains("& 'python' 'check.py' | Out-Null; $LASTEXITCODE -eq 0"), "{ps}");
        assert!(ps.contains("Set-Content C:\\isoloom\\ready ready"), "{ps}");
    }
}
