//! A VM's own process: its command as a service kept across restarts
//! ([`boot::service`]), and its healthcheck tried in the `Ready` step, on Linux and Windows.
//! Each runs where the machine's process runs (its working directory, by default the project
//! folder) with its environment and its inputs, as a container's process would.

use isoloom_plan::{Environment, Health, HealthTest, MachinePlan};
use isoloom_spec::model::CommandLine;
use isoloom_spec::shell::quote;
use isoloom_spec::windows::{ISOLOOM_DIR, STATE_DIR as WINDOWS_STATE_DIR};

use super::{MAKE_STATE_DIR, PROJECT_DIR, inputs_file};
use crate::boot;
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
    Some(boot::service("command", "The machine's own command", &script, &secrets_first(mp)))
}

/// Marks a machine's `setup` scripts as run: they run once, not at every provisioning.
const SETUP_DONE: &str = isoloom_spec::linux_state_path!("/setup.done");

/// The `setup` scripts, run once in order with the machine's service up (a failure fails the
/// set-up), as root, with its inputs and environment.
pub(super) fn setup(env: &Environment, machine: &str, paths: &[String]) -> Option<String> {
    let mp = env.machines.get(machine)?;
    Some(format!(
        "[ -f {SETUP_DONE} ] || (\nset -e\n{}{}{MAKE_STATE_DIR} && touch {SETUP_DONE}\n)\n",
        prelude(env, machine, mp),
        scripts(paths)
    ))
}

/// The `setup` scripts on Windows: each run once in order, from the project folder.
pub(super) fn windows_setup(paths: &[String]) -> String {
    let runs: Vec<String> = paths.iter().map(|p| powershell::run_file(p)).collect();
    format!(
        // The mark lives in the state folder, which the project's unpack never touches (it wipes
        // the project folder), so a reprovision doesn't run the setup again.
        "if (-not (Test-Path '{WINDOWS_STATE_DIR}\\setup.done')) {{ Set-Location {ISOLOOM_DIR}; {}; New-Item -ItemType Directory -Force '{WINDOWS_STATE_DIR}' | Out-Null; Set-Content '{WINDOWS_STATE_DIR}\\setup.done' done }}",
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

/// A script in the state folder (the project's unpack wipes the project folder), written from
/// `lines`.
fn windows_script(name: &str, lines: &[String]) -> String {
    let body = lines.join("\n");
    format!(
        "New-Item -ItemType Directory -Force '{WINDOWS_STATE_DIR}' | Out-Null; Set-Content -Path '{WINDOWS_STATE_DIR}\\{name}.ps1' -Value {}",
        powershell::quote(&body)
    )
}

/// A startup task running `name.ps1` as SYSTEM, started now.
fn startup_task(name: &str) -> String {
    format!(
        "$a = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File {WINDOWS_STATE_DIR}\\{name}.ps1'; Register-ScheduledTask -TaskName 'isoloom-{name}' -Action $a -Trigger (New-ScheduledTaskTrigger -AtStartup) -User 'SYSTEM' -RunLevel Highest -Force | Out-Null; Start-ScheduledTask -TaskName 'isoloom-{name}'"
    )
}

/// The working directory, on Windows.
fn windows_dir(mp: &MachinePlan) -> String {
    powershell::quote(mp.process.working_dir.as_deref().unwrap_or(ISOLOOM_DIR))
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
        HealthTest::Command(c) => windows_test(mp, c, h.timeout),
    };
    let failed = powershell::quote(&format!("{machine}: its healthcheck didn't pass after {} tries", h.retries));
    Some(format!(
        "$ok = {{ try {{ {test} }} catch {{ $false }} }}; Start-Sleep -Seconds {}; $n = 0; while (-not (& $ok)) {{ $n++; if ($n -ge {}) {{ throw {failed} }}; Start-Sleep -Seconds {} }}; ",
        h.start_period, h.retries, h.interval
    ))
}

/// A healthcheck command on Windows, run in a PowerShell of its own as Linux runs it in
/// `sh -c`: an `exit` in it ends that PowerShell, not the step. It passes when that PowerShell
/// exits 0 and the command's last output isn't `$false` (a cmdlet's test: `Test-Path`); a
/// cmdlet's error fails it, and it is stopped after `timeout` seconds.
fn windows_test(mp: &MachinePlan, c: &CommandLine, timeout: u32) -> String {
    let child = format!(
        "$ErrorActionPreference = 'Stop'; Set-Location {}; $r = & {{ {} }}; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}; $last = @($r)[-1]; if ($last -is [bool] -and -not $last) {{ exit 1 }}; exit 0",
        windows_dir(mp),
        powershell(c)
    );
    // Encoded (UTF-16LE in base64), the command reaches the child as written, whatever quotes it holds.
    format!(
        "$c = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes({})); $p = Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', $c -NoNewWindow -PassThru; $null = $p.Handle; if ($p.WaitForExit({})) {{ $p.ExitCode -eq 0 }} else {{ $p.Kill(); $false }}",
        powershell::quote(&child),
        u64::from(timeout) * 1000
    )
}

#[cfg(test)]
mod tests {
    use isoloom_plan::{Environment, Lab, lifecycle::Step};
    use isoloom_spec::model::{CommandLine, Target};

    const SPEC: &str = r"
version: 1
name: own-process
inputs: { DEMO_SECRET: { sensitive: false } }
networks:
  lan: { cidr: 10.84.0.0/24 }
machines:
  web:
    networks: { lan: 10 }
    os: debian@12
    environment: { MODE: lab, DEMO_SECRET: { input: DEMO_SECRET } }
    ports: { web: 8000 }
    app: { steps: [{ run: setup.sh }] }
    command: { run: [python3, -m, http.server, '8000'], cwd: /srv/site }
    setup: [secret.sh]
    healthcheck: { http: { port: 8000, path: /health }, interval: 2s, retries: 30, start_period: 0s }
  win:
    networks: { lan: 11 }
    os: windows-server@2022
    app: { steps: [{ run: setup.ps1 }] }
    command: 'python -m http.server 80'
    setup: [secret.ps1]
    healthcheck: { test: [python, check.py] }
";

    /// The valid spec `yaml`, planned for `vagrant`.
    fn planned(yaml: &str) -> Environment {
        let lab = Lab::parse(yaml).expect("the test spec parses");
        assert_eq!(lab.problems(), [], "the test spec is valid");
        lab.plan(&lab.for_target(Target::Vagrant), Target::Vagrant, None).expect("the spec plans")
    }

    fn env() -> Environment {
        planned(SPEC)
    }

    fn step(env: &Environment, machine: &str, pick: impl Fn(&Step) -> bool) -> Step {
        env.machines[machine].lifecycle.iter().find(|s| pick(s)).expect("the step is planned").clone()
    }

    /// `sh -n`: the script parses.
    fn parses(sh: &str) {
        let Some(mut posix) = crate::test_support::posix_sh() else { return };
        let out = posix.arg("-n").arg("-c").arg(sh).output().expect("sh runs");
        assert!(out.status.success(), "{}\n{sh}", String::from_utf8_lossy(&out.stderr));
    }

    /// What `web`'s command, setup and healthcheck run in: the inputs, the environment, the folder.
    const CONTEXT: &str = "set -a\n# shellcheck source=/dev/null # the inputs file, written on the machine when it starts\n. /var/lib/isoloom/inputs.env\nset +a\nexport MODE='lab'\nmkdir -p '/srv/site' && cd '/srv/site'\n";

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
        assert_eq!(
            sh,
            format!(
                "[ -f /var/lib/isoloom/setup.done ] || (\nset -e\n{CONTEXT}sh '/opt/isoloom/secret.sh'\nmkdir -p /var/lib/isoloom && touch /var/lib/isoloom/setup.done\n)\n"
            ),
            "once, in the command's environment and folder"
        );
        parses(&sh);
        let command = super::super::linux(&env, "web", &Step::Command).expect("renders");
        assert_eq!(
            command,
            crate::boot::service(
                "command",
                "The machine's own command",
                &format!("{CONTEXT}exec 'python3' '-m' 'http.server' '8000'\n"),
                &[]
            ),
            "a service of its own, after nothing"
        );
        parses(&command);
    }

    #[test]
    fn the_healthcheck_is_tried_before_the_machine_is_ready() {
        let env = env();
        let sh = super::super::linux(&env, "web", &Step::Ready).expect("renders");
        assert_eq!(
            sh,
            format!(
                "isoloom_healthy() {{ (\n{CONTEXT}curl -fsS -o /dev/null --max-time 3 http://127.0.0.1:8000/health || wget -q -O /dev/null -T 3 http://127.0.0.1:8000/health\n) >/dev/null 2>&1; }}\nsleep 0; n=0; until isoloom_healthy; do n=$((n+1)); if [ \"$n\" -ge 30 ]; then echo 'web: its healthcheck didn'\\''t pass after 30 tries' >&2; exit 1; fi; sleep 2; done\nmkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready\n"
            ),
            "its start period, then a try every interval up to its retries, then the ready mark"
        );
        parses(&sh);
    }

    #[test]
    fn windows_runs_the_command_as_a_startup_task_and_the_setup_once() {
        let env = env();
        let setup = step(&env, "win", |s| matches!(s, Step::Setup { .. }));
        let once = super::super::windows(&env, "win", &setup);
        assert_eq!(
            once.as_deref(),
            Some(
                "if (-not (Test-Path 'C:\\ProgramData\\isoloom\\setup.done')) { Set-Location C:\\isoloom; & powershell -NoProfile -ExecutionPolicy Bypass -File 'secret.ps1'; if ($LASTEXITCODE) { exit $LASTEXITCODE }; New-Item -ItemType Directory -Force 'C:\\ProgramData\\isoloom' | Out-Null; Set-Content 'C:\\ProgramData\\isoloom\\setup.done' done }"
            ),
            "run once, its mark outside the project folder, not a startup task"
        );
        let command = super::super::windows(&env, "win", &Step::Command);
        assert_eq!(
            command.as_deref(),
            Some(
                "New-Item -ItemType Directory -Force 'C:\\ProgramData\\isoloom' | Out-Null; Set-Content -Path 'C:\\ProgramData\\isoloom\\command.ps1' -Value 'New-Item -ItemType Directory -Force ''C:\\isoloom'' | Out-Null; Set-Location ''C:\\isoloom''\npython -m http.server 80'; $a = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File C:\\ProgramData\\isoloom\\command.ps1'; Register-ScheduledTask -TaskName 'isoloom-command' -Action $a -Trigger (New-ScheduledTaskTrigger -AtStartup) -User 'SYSTEM' -RunLevel Highest -Force | Out-Null; Start-ScheduledTask -TaskName 'isoloom-command'"
            ),
            "a startup task of its own"
        );
        let ready = super::super::windows(&env, "win", &Step::Ready);
        assert_eq!(
            ready.as_deref(),
            Some(
                "$ok = { try { $c = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('$ErrorActionPreference = ''Stop''; Set-Location ''C:\\isoloom''; $r = & { & ''python'' ''check.py'' }; if ($LASTEXITCODE) { exit $LASTEXITCODE }; $last = @($r)[-1]; if ($last -is [bool] -and -not $last) { exit 1 }; exit 0')); $p = Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', $c -NoNewWindow -PassThru; $null = $p.Handle; if ($p.WaitForExit(3000)) { $p.ExitCode -eq 0 } else { $p.Kill(); $false } } catch { $false } }; Start-Sleep -Seconds 10; $n = 0; while (-not (& $ok)) { $n++; if ($n -ge 60) { throw 'win: its healthcheck didn''t pass after 60 tries' }; Start-Sleep -Seconds 5 }; New-Item -ItemType Directory -Force C:\\isoloom | Out-Null; Set-Content C:\\isoloom\\ready ready"
            ),
            "the test runs in a PowerShell of its own, stopped after its timeout"
        );
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
        planned(KEPT)
    }

    #[test]
    fn a_command_starts_once_its_secrets_are_back_in_place() {
        let env = kept();
        assert_eq!(
            super::super::linux(&env, "db", &Step::Command),
            Some(crate::boot::service(
                "command",
                "The machine's own command",
                "mkdir -p '/opt/isoloom' && cd '/opt/isoloom'\nexec '/bin/sh' '-c' 'postgres'\n",
                &["apply-secrets"]
            )),
            "after, and requiring, the secrets' service"
        );
        assert_eq!(
            super::super::windows(&env, "iis", &Step::Command).as_deref(),
            Some(
                "New-Item -ItemType Directory -Force 'C:\\ProgramData\\isoloom' | Out-Null; Set-Content -Path 'C:\\ProgramData\\isoloom\\command.ps1' -Value '& powershell -NoProfile -ExecutionPolicy Bypass -File ''C:\\ProgramData\\isoloom\\apply-secrets.ps1''; if ($LASTEXITCODE) { exit $LASTEXITCODE }\nNew-Item -ItemType Directory -Force ''/site'' | Out-Null; Set-Location ''/site''\n& ''iis.exe'' ''-x'''; $a = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File C:\\ProgramData\\isoloom\\command.ps1'; Register-ScheduledTask -TaskName 'isoloom-command' -Action $a -Trigger (New-ScheduledTaskTrigger -AtStartup) -User 'SYSTEM' -RunLevel Highest -Force | Out-Null; Start-ScheduledTask -TaskName 'isoloom-command'"
            ),
            "the secrets placed, then the command, in its folder"
        );
    }

    #[test]
    fn a_command_healthcheck_runs_within_its_timeout() {
        let env = kept();
        assert_eq!(
            super::super::linux(&env, "db", &Step::Ready).as_deref(),
            Some(
                "isoloom_healthy() { (\nmkdir -p '/opt/isoloom' && cd '/opt/isoloom'\ntimeout 4 sh -c ''\\''pg_isready'\\'''\n) >/dev/null 2>&1; }\nsleep 10; n=0; until isoloom_healthy; do n=$((n+1)); if [ \"$n\" -ge 60 ]; then echo 'db: its healthcheck didn'\\''t pass after 60 tries' >&2; exit 1; fi; sleep 5; done\nmkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready\n"
            ),
            "within its timeout, the defaults' start period, interval and retries"
        );
    }

    #[test]
    #[ignore = "needs pwsh: runs the healthcheck's PowerShell, one waiting out its timeout; `cargo xtask test` runs it"]
    fn a_windows_command_healthcheck_runs_in_a_powershell_of_its_own() {
        // pwsh stands in for Windows PowerShell.
        let env = env();
        let mp = &env.machines["win"];
        let passes = |text: &str, timeout: u32| {
            let test = super::windows_test(mp, &CommandLine::Shell(text.to_owned()), timeout)
                .replace("'C:\\isoloom'", "'.'")
                .replace("-FilePath powershell.exe", "-FilePath pwsh");
            let out = std::process::Command::new("pwsh")
                .args(["-NoProfile", "-NonInteractive", "-Command", &test])
                .output()
                .expect("pwsh runs");
            match String::from_utf8_lossy(&out.stdout).trim() {
                "True" => true,
                "False" => false,
                other => panic!("{text}: {other}\n{}", String::from_utf8_lossy(&out.stderr)),
            }
        };
        assert!(passes("exit 0", 30), "an exit 0 passes");
        assert!(!passes("exit 1", 30), "an exit 1 fails, and ends the test only");
        assert!(passes("Test-Path .", 30), "a cmdlet's true passes");
        assert!(!passes("Test-Path ./nowhere", 30), "a cmdlet's false fails");
        assert!(!passes("Get-Item ./nowhere", 30), "a cmdlet's error fails");
        assert!(passes("if (\"a\" -eq 'a') { exit 0 } else { exit 1 }", 30), "quotes reach it as written");
        assert!(!passes("Start-Sleep -Seconds 20", 1), "it is stopped after its timeout");
    }

    #[test]
    fn windows_tries_a_page_at_its_quoted_address() {
        let env = kept();
        assert_eq!(
            super::super::windows(&env, "iis", &Step::Ready).as_deref(),
            Some(
                "$ok = { try { (Invoke-WebRequest -UseBasicParsing -TimeoutSec 3 -Uri 'http://127.0.0.1:80/it''s').StatusCode -lt 400 } catch { $false } }; Start-Sleep -Seconds 10; $n = 0; while (-not (& $ok)) { $n++; if ($n -ge 3) { throw 'iis: its healthcheck didn''t pass after 3 tries' }; Start-Sleep -Seconds 5 }; New-Item -ItemType Directory -Force C:\\isoloom | Out-Null; Set-Content C:\\isoloom\\ready ready"
            )
        );
    }
}
