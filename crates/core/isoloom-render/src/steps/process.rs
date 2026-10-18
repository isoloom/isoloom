//! A VM's own process: its command as a service kept across restarts
//! ([`boot::service`]), and its healthcheck tried in the `Ready` step, on Linux and Windows.
//! Each runs where the machine's process runs (its working directory, by default the project
//! folder) with its environment and its inputs, as a container's process would.

use isoloom_plan::{Environment, Health, HealthTest, MachinePlan};
use isoloom_spec::model::CommandLine;
use isoloom_spec::shell::quote;

use super::{PROJECT_DIR, WINDOWS_PROJECT_DIR, inputs_file};
use crate::boot::{self, Kind};
use crate::powershell;
use crate::secrets;

/// The lines every script of the process starts with: its inputs, its environment, its
/// directory. The inputs file exists only on the machine, so shellcheck is told not to look for
/// it.
fn prelude(env: &Environment, machine: &str, mp: &MachinePlan) -> String {
    let inputs = inputs_file(env, machine)
        .map(|file| format!("set -a\n# shellcheck source=/dev/null # the inputs file, written on the machine when it starts\n. {file}\nset +a\n"))
        .unwrap_or_default();
    let exports: String = mp
        .settings
        .environment
        .iter()
        .map(|(k, v)| format!("export {k}={}\n", quote(v)))
        .collect::<Vec<_>>()
        .concat();
    let dir = quote(mp.process.working_dir.as_deref().unwrap_or(PROJECT_DIR));
    format!("{inputs}{exports}mkdir -p {dir} && cd {dir}\n")
}

/// The scripts at `paths` run in order with `sh`, from the project folder.
fn scripts(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| format!("sh {}\n", quote(&format!("{PROJECT_DIR}/{p}"))))
        .collect::<Vec<_>>()
        .concat()
}

/// The command, as a service restarted when it fails, after its secrets are in place.
pub(super) fn command(env: &Environment, machine: &str) -> Option<String> {
    let mp = env.machines.get(machine)?;
    let line = mp.process.command.as_ref()?.argv().iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
    let script = format!("{}exec {line}\n", prelude(env, machine, mp));
    Some(boot::service("command", "The machine's own command", &script, Kind::Daemon, &secrets_first(mp)))
}

/// Marks a machine's `setup` scripts as run: they run once, not at every provisioning.
const SETUP_DONE: &str = "/var/lib/isoloom/setup.done";

/// The `setup` scripts, run once in order with the machine's service up (a failure fails the
/// set-up), as root, with its inputs and environment.
pub(super) fn setup(env: &Environment, machine: &str, paths: &[String]) -> Option<String> {
    let mp = env.machines.get(machine)?;
    Some(format!(
        "[ -f {SETUP_DONE} ] || (\nset -e\n{}{}mkdir -p /var/lib/isoloom && touch {SETUP_DONE}\n)\n",
        prelude(env, machine, mp),
        scripts(paths)
    ))
}

/// The `setup` scripts on Windows: each run once in order, from the project folder.
pub(super) fn windows_setup(paths: &[String]) -> String {
    let runs: Vec<String> = paths
        .iter()
        .map(|p| {
            format!(
                "& powershell -NoProfile -ExecutionPolicy Bypass -File {}; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}",
                powershell::quote(p)
            )
        })
        .collect();
    format!(
        "if (-not (Test-Path '{WINDOWS_PROJECT_DIR}\\setup.done')) {{ Set-Location {WINDOWS_PROJECT_DIR}; {}; Set-Content '{WINDOWS_PROJECT_DIR}\\setup.done' done }}",
        runs.join("; ")
    )
}

/// At boot, a machine's secrets are back in place before its command runs.
fn secrets_first(mp: &MachinePlan) -> Vec<&'static str> {
    if mp.secrets.is_empty() { Vec::new() } else { vec![secrets::PLACE_SERVICE] }
}

/// On Windows, the secrets put in place by the script itself: Task Scheduler starts the
/// startup tasks in no fixed order, so the placing task may not have run yet.
fn windows_secrets_first(mp: &MachinePlan) -> Vec<String> {
    if mp.secrets.is_empty() {
        Vec::new()
    } else {
        vec![secrets::windows::place_first()]
    }
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
        CommandLine::Exec(args) => format!("& {}", args.iter().map(|a| powershell::quote(a)).collect::<Vec<_>>().join(" ")),
    }
}

/// A script under the project folder, written from `lines`.
fn windows_script(name: &str, lines: &[String]) -> String {
    let body = lines.join("\n");
    format!("Set-Content -Path '{WINDOWS_PROJECT_DIR}\\{name}.ps1' -Value {}", powershell::quote(&body))
}

/// A startup task running `name.ps1` as SYSTEM, started now.
fn startup_task(name: &str) -> String {
    format!(
        "$a = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File {WINDOWS_PROJECT_DIR}\\{name}.ps1'; Register-ScheduledTask -TaskName 'isoloom-{name}' -Action $a -Trigger (New-ScheduledTaskTrigger -AtStartup) -User 'SYSTEM' -RunLevel Highest -Force | Out-Null; Start-ScheduledTask -TaskName 'isoloom-{name}'"
    )
}

/// The working directory, on Windows.
fn windows_dir(mp: &MachinePlan) -> String {
    powershell::quote(mp.process.working_dir.as_deref().unwrap_or(WINDOWS_PROJECT_DIR))
}

/// The command on Windows: a startup task (as SYSTEM) running `command.ps1`, its secrets placed
/// first.
pub(super) fn windows_command(env: &Environment, machine: &str) -> Option<String> {
    let mp = env.machines.get(machine)?;
    let mut lines = windows_secrets_first(mp);
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
            "(Invoke-WebRequest -UseBasicParsing -TimeoutSec {} -Uri {}).StatusCode -lt 400",
            h.timeout,
            powershell::quote(&format!("http://127.0.0.1:{port}{path}"))
        ),
        HealthTest::Command(c) => format!("Set-Location {}; {} | Out-Null; $LASTEXITCODE -eq 0", windows_dir(mp), powershell(c)),
    };
    let failed = powershell::quote(&format!("{machine}: its healthcheck didn't pass after {} tries", h.retries));
    Some(format!(
        "$ok = {{ try {{ {test} }} catch {{ $false }} }}; Start-Sleep -Seconds {}; $n = 0; while (-not (& $ok)) {{ $n++; if ($n -ge {}) {{ throw {failed} }}; Start-Sleep -Seconds {} }}; ",
        h.start_period, h.retries, h.interval
    ))
}

#[cfg(test)]
mod tests {
    use isoloom_plan::{Environment, Lab, lifecycle::Step};
    use isoloom_spec::model::Target;

    const SPEC: &str = r"
version: 1
name: own-process
inputs: { FLAG: { sensitive: false } }
networks:
  lan: { cidr: 10.84.0.0/24 }
machines:
  web:
    networks: { lan: 10 }
    os: debian@12
    environment: { MODE: lab, FLAG: { input: FLAG } }
    ports: { web: 8000 }
    app: { steps: [{ run: setup.sh }] }
    command: { run: [python3, -m, http.server, '8000'], cwd: /srv/site }
    setup: [flag.sh]
    healthcheck: { http: { port: 8000, path: /health }, interval: 2s, retries: 30, start_period: 0s }
  win:
    networks: { lan: 11 }
    os: windows-server@2022
    app: { steps: [{ run: setup.ps1 }] }
    command: 'python -m http.server 80'
    setup: [flag.ps1]
    healthcheck: { test: [python, check.py] }
";

    fn env() -> Environment {
        let lab = Lab::parse(SPEC).expect("the test spec parses");
        assert_eq!(lab.problems(), [], "the test spec is valid");
        Environment::build(&lab.for_target(Target::Vagrant), Target::Vagrant, None)
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
    fn the_command_runs_as_a_service_and_the_setup_steps_once_after_it() {
        let env = env();
        let lifecycle = &env.machines["web"].lifecycle;
        let at = |s: &Step| lifecycle.iter().position(|x| x == s).expect("planned");
        let setup = step(&env, "web", |s| matches!(s, Step::Setup { .. }));
        assert!(at(&Step::Command) < at(&setup) && at(&setup) < at(&Step::Ready));
        assert!(
            lifecycle.contains(&Step::Inputs { persist: true }),
            "the service reads the inputs at every boot"
        );
        let sh = super::super::linux(&env, "web", &setup).expect("renders");
        assert!(
            sh.contains(
                "set -a\n# shellcheck source=/dev/null # the inputs file, written on the machine when it starts\n. /var/lib/isoloom/inputs.env\nset +a\nexport MODE='lab'\nmkdir -p '/srv/site' && cd '/srv/site'\nsh '/opt/isoloom/flag.sh'\n"
            ),
            "{sh}"
        );
        assert!(
            sh.starts_with("[ -f /var/lib/isoloom/setup.done ] || (\n") && sh.contains("touch /var/lib/isoloom/setup.done\n)"),
            "once: {sh}"
        );
        parses(&sh);
        let command = super::super::linux(&env, "web", &Step::Command).expect("renders");
        assert!(
            command.contains("exec 'python3' '-m' 'http.server' '8000'\n") && !command.contains("isoloom-before"),
            "{command}"
        );
        parses(&command);
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
    fn windows_runs_the_command_as_a_startup_task_and_the_setup_once() {
        let env = env();
        let setup = step(&env, "win", |s| matches!(s, Step::Setup { .. }));
        let once = super::super::windows(&env, "win", &setup).expect("renders");
        assert!(
            once.contains("flag.ps1") && once.contains("C:\\isoloom\\setup.done") && !once.contains("Register-ScheduledTask"),
            "run once, not a startup task: {once}"
        );
        let command = super::super::windows(&env, "win", &Step::Command).expect("renders");
        assert!(
            !command.contains("before.ps1") && command.contains("python -m http.server 80") && command.contains("-TaskName 'isoloom-command'"),
            "{command}"
        );
        let ready = super::super::windows(&env, "win", &Step::Ready).expect("renders");
        assert!(ready.contains("& 'python' 'check.py' | Out-Null; $LASTEXITCODE -eq 0"), "{ready}");
        assert!(ready.contains("Set-Content C:\\isoloom\\ready ready"), "{ready}");
    }

    /// `db` (Linux) and `iis` (Windows) get a secret, run a command of their own and say when
    /// they are healthy: `db` by a command, `iis` by a page.
    const KEPT: &str = r"
version: 1
name: kept
secrets: { pw: { env: PW } }
networks:
  lan: { cidr: 10.84.0.0/24 }
machines:
  db:
    networks: { lan: 10 }
    os: debian@12
    files: [{ path: /run/secrets/pw, secret: pw }]
    command: postgres
    healthcheck: { test: [pg_isready], timeout: 4s }
  iis:
    networks: { lan: 11 }
    os: windows-server@2022
    files: [{ path: 'C:\pw.txt', secret: pw }]
    command: { run: [iis.exe, -x], cwd: /site }
    healthcheck: { http: { port: 80, path: /it's }, retries: 3 }
";

    fn kept() -> Environment {
        let lab = Lab::parse(KEPT).expect("the test spec parses");
        assert_eq!(lab.problems(), [], "the test spec is valid");
        Environment::build(&lab.for_target(Target::Vagrant), Target::Vagrant, None)
    }

    #[test]
    fn a_command_starts_once_its_secrets_are_back_in_place() {
        let env = kept();
        let linux = super::super::linux(&env, "db", &Step::Command).expect("renders");
        assert!(
            linux.contains("After=network-online.target isoloom-apply-secrets.service\nWants=network-online.target\nRequires=isoloom-apply-secrets.service\n"),
            "{linux}"
        );
        assert!(
            linux.contains("mkdir -p '/opt/isoloom' && cd '/opt/isoloom'\nexec '/bin/sh' '-c' 'postgres'\n"),
            "{linux}"
        );
        let windows = super::super::windows(&env, "iis", &Step::Command).expect("renders");
        assert!(
            windows.starts_with("Set-Content -Path 'C:\\isoloom\\command.ps1' -Value '& powershell -NoProfile -ExecutionPolicy Bypass -File ''C:\\ProgramData\\isoloom\\apply-secrets.ps1'';"),
            "{windows}"
        );
        assert!(
            windows.contains("New-Item -ItemType Directory -Force ''/site'' | Out-Null; Set-Location ''/site''\n& ''iis.exe'' ''-x'''"),
            "{windows}"
        );
    }

    #[test]
    fn a_command_healthcheck_runs_within_its_timeout() {
        let env = kept();
        let sh = super::super::linux(&env, "db", &Step::Ready).expect("renders");
        assert!(sh.contains("timeout 4 sh -c ''\\''pg_isready'\\'''\n) >/dev/null 2>&1; }"), "{sh}");
        assert!(sh.contains("echo 'db: its healthcheck didn'\\''t pass after 60 tries' >&2"), "{sh}");
    }

    #[test]
    fn windows_tries_a_page_at_its_quoted_address() {
        let env = kept();
        let ps = super::super::windows(&env, "iis", &Step::Ready).expect("renders");
        assert!(
            ps.contains("(Invoke-WebRequest -UseBasicParsing -TimeoutSec 3 -Uri 'http://127.0.0.1:80/it''s').StatusCode -lt 400"),
            "{ps}"
        );
        assert!(ps.contains("if ($n -ge 3) { throw 'iis: its healthcheck didn''t pass after 3 tries' }"), "{ps}");
    }
}
