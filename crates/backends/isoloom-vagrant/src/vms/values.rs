//! The values a VM gets at launch, from the host's environment when Vagrant runs (Vagrant's own
//! way, decided for the Vagrant targets: the runner hands them to `vagrant up` and `vagrant
//! provision`, never to a generated file): its inputs, and its secret files (an input's file
//! included).
//!
//! Vagrant passes a provisioner's `env` by writing `NAME="value"` into the shell (or `$env:NAME
//! = "value"` into the PowerShell) that runs the script, escaping quotes only: a `$(...)` or a
//! backquote in a value would run in the guest, as root. So every value travels in base64
//! (`isoloom_b64`, defined with the inputs), which no shell reads anything into, and the script
//! decodes it.

use isoloom_plan::{Environment, SecretFile, ValueSource};
use isoloom_render::{secrets, steps};
use isoloom_spec::model::secret_variable;

use super::transport::Os;
use crate::ruby::{Ruby, Value};
use crate::vagrantfile;

/// The Ruby that reads input `name` from the host's environment, in base64.
fn input(name: &str) -> Value {
    Value::raw(format!("isoloom_b64(INPUTS[{}])", Value::str(name)))
}

/// The machine's inputs from the host's environment, in base64, when it reads some: the
/// environment its steps run with.
pub(crate) fn inputs_env(env: &Environment, machine: &str) -> Option<Value> {
    let names = &env.machines.get(machine)?.inputs;
    (!names.is_empty()).then(|| Value::hash(names.iter().map(|n| (Value::str(n), input(n)))))
}

/// PowerShell decoding the machine's inputs in its own environment before a step runs: what the
/// step's script reads is the value itself. Empty when it reads none.
pub(crate) fn windows_decode(env: &Environment, machine: &str) -> String {
    let Some(mp) = env.machines.get(machine).filter(|mp| !mp.inputs.is_empty()) else {
        return String::new();
    };
    format!(
        "foreach ($k in @({})) {{ $v = [Environment]::GetEnvironmentVariable($k); if ($v) {{ [Environment]::SetEnvironmentVariable($k, [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($v))) }} }}; ",
        list(&mp.inputs)
    )
}

/// `names` as a PowerShell list's items.
fn list(names: &[String]) -> String {
    names.iter().map(|n| isoloom_render::powershell::quote(n)).collect::<Vec<_>>().join(", ")
}

/// A Linux machine's inputs, decoded into the file its steps read (`KEY='value'` lines, readable
/// by root alone); then the step itself (kept on disk when a reboot comes).
pub(crate) fn linux_inputs(r: &mut Ruby, env: &Environment, machine: &str, step: &isoloom_plan::lifecycle::Step) {
    let Some(mp) = env.machines.get(machine) else { return };
    let file = steps::INPUTS_FILE;
    let mut script = format!(
        "umask 077\n: > {file}\nfor k in {}; do printf \"%s='%s'\\n\" \"$k\" \"$(printenv \"$k\" | base64 -d | sed \"s/'/'\\\\\\\\''/g\")\" >> {file}; done\n",
        mp.inputs.join(" ")
    );
    script.push_str(&steps::linux(env, machine, step).unwrap_or_default());
    let call = vagrantfile::shell("m", "inputs").kw_opt("env", inputs_env(env, machine));
    r.script(call, "inline", &script);
}

/// A Windows machine's inputs, kept as machine-wide environment variables: its `setup`, `before`
/// and command scripts (the last two startup tasks, run at every boot) read them as a Linux
/// machine's read its inputs file. Only inputs the spec lets show (`sensitive: false`) reach an
/// environment; a sensitive one is a file.
pub(crate) fn windows_inputs(r: &mut Ruby, env: &Environment, machine: &str) {
    let Some(mp) = env.machines.get(machine).filter(|mp| !mp.inputs.is_empty()) else {
        return;
    };
    let script = format!(
        "foreach ($k in @({})) {{ $v = [Environment]::GetEnvironmentVariable($k); $t = if ($v) {{ [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($v)) }} else {{ '' }}; [Environment]::SetEnvironmentVariable($k, $t, 'Machine') }}",
        list(&mp.inputs)
    );
    let call = vagrantfile::shell("m", "inputs").kw_opt("env", inputs_env(env, machine));
    r.script(call, "inline", &script);
}

/// The variable a secret file's value travels in, and the Ruby reading it from the host's
/// environment in base64: an input's from `INPUTS`, a secret's from `ISOLOOM_SECRET_<NAME>`
/// (where the runner delivers it).
fn secret_value(f: &SecretFile) -> (String, Value) {
    let read = match &f.source {
        ValueSource::Input(name) => input(name),
        ValueSource::Secret(name) => Value::raw(format!("isoloom_b64(ENV.fetch({}, \"\"))", Value::str(&secret_variable(name)))),
    };
    (secret_variable(&f.name), read)
}

/// The machine's secret files, their values from the host's environment: kept, then put in
/// place now and at every boot. Nothing when it has none.
pub(crate) fn secret_files(r: &mut Ruby, env: &Environment, machine: &str, os: Os) {
    let Some(mp) = env.machines.get(machine).filter(|mp| !mp.secrets.is_empty()) else {
        return;
    };
    let mut seen = std::collections::BTreeSet::new();
    let unique: Vec<&SecretFile> = mp.secrets.iter().filter(|f| seen.insert(f.name.as_str())).collect();
    let read: Vec<(String, Value)> = unique.iter().map(|f| secret_value(f)).collect();
    let values: Vec<(String, String)> = unique.iter().zip(&read).map(|(f, (var, _))| (f.name.clone(), var.clone())).collect();
    let script = match os {
        Os::Linux => secrets::from_environment(&values, &mp.secrets),
        Os::Windows => secrets::windows::from_environment(&values, &mp.secrets),
    };
    let hash = Value::hash(read.into_iter().map(|(var, value)| (Value::str(&var), value)));
    r.script(vagrantfile::shell("m", "secrets").kw("env", hash), "inline", &script);
}

/// Whether a provisioner of `env`'s VMs reads a value from the host's environment: the
/// Vagrantfile then defines `isoloom_b64`.
pub(crate) fn any(env: &Environment) -> bool {
    !env.spec().inputs.is_empty() || env.machines.values().any(|mp| !mp.secrets.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::environment;

    const SPEC: &str = r"
version: 1
name: t
inputs: { FLAG: {}, MODE: { sensitive: false } }
secrets: { db-password: { env: DB_PASSWORD } }
networks: { lab: { cidr: 10.84.0.0/24 } }
machines:
  web:
    networks: { lab: 10 }
    os: debian@12
    environment: { MODE: { input: MODE } }
    files:
      - { path: /srv/flag, input: FLAG }
      - { path: /srv/db, secret: db-password }
    app: { steps: [{ run: a.sh }] }
  dc:
    networks: { lab: 20 }
    os: windows-server@2019
    environment: { MODE: { input: MODE } }
    files: [{ path: C:/flag.txt, input: FLAG }]
    setup: [s.ps1]
    app: { steps: [{ run: dc.ps1 }] }
";

    fn text(write: impl FnOnce(&mut Ruby)) -> String {
        let mut r = Ruby::new();
        write(&mut r);
        r.finish()
    }

    #[test]
    fn secret_files_come_from_the_hosts_environment_without_waiting_for_a_delivery() {
        let env = environment(SPEC);
        let s = text(|r| secret_files(r, &env, "web", Os::Linux));
        let head = s.lines().next().unwrap_or_default();
        assert_eq!(
            head,
            "m.vm.provision \"shell\", name: \"secrets\", env: { \"ISOLOOM_SECRET_INPUT_FLAG\" => isoloom_b64(INPUTS[\"FLAG\"]), \"ISOLOOM_SECRET_DB_PASSWORD\" => isoloom_b64(ENV.fetch(\"ISOLOOM_SECRET_DB_PASSWORD\", \"\")) }, inline: <<~'SH'"
        );
        assert!(!s.contains("isoloom run"), "nothing waits for the old runtime: {s}");
        assert!(
            s.contains("install -D -o 0 -g 0 -m 0400 /var/lib/isoloom/secrets/input-flag '/srv/flag'"),
            "{s}"
        );
        let w = text(|r| secret_files(r, &env, "dc", Os::Windows));
        assert!(w.contains("'input-flag' = 'ISOLOOM_SECRET_INPUT_FLAG'") && !w.contains("isoloom run"), "{w}");
    }

    #[test]
    fn a_secret_value_reaches_its_file_as_given_whatever_it_holds() {
        // Vagrant writes `NAME="value"` into the shell that runs the script: only base64 keeps a
        // `$(...)` in a flag from running as root in the guest.
        let script = isoloom_render::secrets::from_environment(&[("input-flag".into(), "V".into())], &[]);
        let dir = std::env::temp_dir().join(format!("isoloom-values-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        // The kept folder as a relative path, the shell in the test's folder: a Windows path in a
        // shell script (Git's `sh` on Windows) loses its backslashes.
        let script = script
            .replace("/var/lib/isoloom/secrets", ".")
            .lines()
            .take_while(|l| !l.starts_with("touch"))
            .collect::<Vec<_>>()
            .join("\n");
        let tricky = "flag{$(touch pwned)`id`\"'}\n";
        let encoded = isoloom_spec::shell::base64(tricky.as_bytes());
        // As Vagrant runs it: the value in double quotes on the command line.
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("V=\"{encoded}\" sh -c '{}'", script.replace('\'', "'\\''")))
            .current_dir(&dir)
            .status()
            .expect("sh runs");
        let kept = std::fs::read_to_string(dir.join("input-flag")).unwrap_or_default();
        let pwned = dir.join("pwned").exists();
        std::fs::remove_dir_all(&dir).expect("the temporary folder goes");
        assert!(status.success() && !pwned);
        assert_eq!(kept, tricky);
    }

    #[test]
    fn a_reprovision_given_no_value_keeps_the_value_kept() {
        // The launcher re-provisions a VM lab without its launch values (it keeps none).
        let script = isoloom_render::secrets::from_environment(&[("input-flag".into(), "V".into())], &[]);
        let dir = std::env::temp_dir().join(format!("isoloom-values-keep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        let script: String = script
            .replace("/var/lib/isoloom/secrets", ".")
            .lines()
            .take_while(|l| !l.starts_with("touch"))
            .flat_map(|l| [l, "\n"])
            .collect();
        let run = |value: &str| {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(&script)
                .current_dir(&dir)
                .env("V", isoloom_spec::shell::base64(value.as_bytes()))
                .status()
                .expect("sh runs")
                .success()
        };
        let first = run("flag{1}");
        let again = run("");
        let kept = std::fs::read_to_string(dir.join("input-flag")).unwrap_or_default();
        std::fs::remove_dir_all(&dir).expect("the temporary folder goes");
        assert!(first && again);
        assert_eq!(kept, "flag{1}");
    }

    #[test]
    fn a_windows_machine_keeps_its_inputs_for_its_setup_and_startup_tasks() {
        let env = environment(SPEC);
        let s = text(|r| windows_inputs(r, &env, "dc"));
        assert!(s.contains("env: { \"MODE\" => isoloom_b64(INPUTS[\"MODE\"]) }"), "{s}");
        assert!(s.contains("SetEnvironmentVariable($k, $t, 'Machine')"), "{s}");
        assert!(windows_decode(&env, "dc").contains("FromBase64String"));
        assert!(windows_decode(&env, "web").contains("MODE"));
    }

    #[test]
    fn the_vagrantfile_defines_the_encoder_when_a_provisioner_reads_a_value() {
        assert!(any(&environment(SPEC)));
        let none = "version: 1\nname: t\nnetworks: { lab: { cidr: 10.84.0.0/24 } }\nmachines:\n  web: { networks: { lab: 10 }, os: debian@12, app: { steps: [{ run: a.sh }] } }\n";
        assert!(!any(&environment(none)));
    }
}
