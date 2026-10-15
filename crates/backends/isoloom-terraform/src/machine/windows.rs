//! A Windows machine set up over WinRM by Terraform: renamed and restarted first, then one
//! PowerShell script made of its steps' Windows renderings, its firewall openings and published
//! ports. The target's image must start WinRM over HTTP for `isoloom` with the generated
//! password ([`write_password`]; see each target's user data).

use isoloom_plan::{Environment, Step};
use isoloom_render::hcl_builder::{Block, Expression, Module, list, reference, string};
use isoloom_render::steps;
use isoloom_spec::model::Port;

use super::provisioner::{Connection, Provisioner, Upload};
use super::{Remote, linux, vms};
use crate::module::{self, ident, resource};

/// The Terraform expression of the Windows machines' password.
pub(crate) const PASSWORD: &str = "random_password.windows.result";

/// Whether the environment has a Windows VM: its module then generates their password.
pub(crate) fn any(env: &Environment) -> bool {
    vms(env).any(|(_, mp)| mp.is_windows())
}

/// The Windows machines' administrator password (user `isoloom`), for WinRM, when a VM runs
/// Windows.
pub(crate) fn write_password(env: &Environment, m: &mut Module) {
    if any(env) {
        m.commented(
            "The Windows machines' administrator password (user isoloom), for WinRM.",
            module::password("windows"),
        );
    }
}

/// Where the set-up script goes.
const SETUP: &str = "C:/ProgramData/isoloom/setup.ps1";
/// Where the project's archive goes (the `Project` step unpacks it from there).
const ARCHIVE: &str = "C:/Windows/Temp/isoloom-project.tgz";

/// A Windows machine, as Terraform reaches it over WinRM.
#[derive(Debug, Clone)]
pub(crate) struct Machine<'a> {
    /// The machine.
    pub(crate) remote: Remote<'a>,
    /// The password of `isoloom`.
    pub(crate) password: Expression,
    /// The steps the platform does itself, left out.
    pub(crate) skip: fn(&Step) -> bool,
    /// What the platform does its own way.
    pub(crate) platform: Platform,
}

/// How a platform's Windows machines differ from the default (renamed by Terraform, WinRM with
/// Basic authentication, the auto-stop in the user data).
#[derive(Debug, Clone, Default)]
pub(crate) struct Platform {
    /// The platform names the machine when it creates it: no rename, no restart.
    pub(crate) named: bool,
    /// WinRM authenticates with NTLM.
    pub(crate) ntlm: bool,
    /// Commands run before the set-up script (what the platform's user data can't do).
    pub(crate) before: Vec<Expression>,
}

/// Its Windows name: NetBIOS keeps 15 characters.
pub(crate) fn computer_name(name: &str) -> String {
    name.chars().take(15).collect()
}

/// The machine's resources: its renaming (`terraform_data.<id>_name`) and the pause while it
/// restarts (`time_sleep.<id>_restart`) unless the platform names it, then its set-up
/// (`terraform_data.<id>`).
pub(crate) fn blocks(w: Machine) -> Vec<Block> {
    let Remote { env, name, plan, .. } = w.remote;
    let id = ident(name);
    let connection = Connection::WinRm {
        host: w.remote.host.clone(),
        password: w.password.clone(),
        ntlm: w.platform.ntlm,
    };
    let mut out = Vec::new();
    let mut depends_on = Vec::new();
    if !w.platform.named {
        out.extend(rename(&w, &connection));
        depends_on.push(reference(&format!("time_sleep.{id}_restart")));
    }
    let mut uploads = Vec::new();
    if has_runs(&w) {
        uploads.push(Upload::Project {
            id: id.clone(),
            destination: ARCHIVE,
        });
    }
    uploads.push(Upload::Content {
        content: string(setup_script(&w)),
        destination: SETUP.to_owned(),
    });
    depends_on.extend(linux::depends_on(env, plan));
    let mut commands = w.platform.before.clone();
    commands.push(string(format!("powershell -NoProfile -ExecutionPolicy Bypass -File {SETUP}")));
    let setup = Provisioner {
        name: id,
        replace_on: w.remote.replace_on,
        connection,
        uploads,
        parts: vec![commands],
        depends_on,
        // `isoloom run` delivers its secrets over WinRM while the set-up waits for them.
        announce: (!plan.secrets.is_empty()).then(|| name.to_owned()),
    };
    out.push(setup.block());
    out
}

/// Its renaming (`terraform_data.<id>_name`) and the pause while it restarts
/// (`time_sleep.<id>_restart`).
fn rename(w: &Machine, connection: &Connection) -> [Block; 2] {
    let id = ident(w.remote.name);
    let host = computer_name(w.remote.name);
    let rename = Provisioner {
        name: format!("{id}_name"),
        replace_on: w.remote.replace_on.clone(),
        connection: connection.clone(),
        uploads: Vec::new(),
        parts: vec![vec![string(format!(
            "powershell -NoProfile -Command \"if ($env:COMPUTERNAME -ne '{up}') {{ Rename-Computer -NewName '{host}' -Force; shutdown /r /t 10 }}\"",
            up = host.to_uppercase()
        ))]],
        depends_on: Vec::new(),
        announce: None,
    };
    let pause = resource("time_sleep", &format!("{id}_restart"))
        .add_attribute(("create_duration", string("90s")))
        .add_attribute(("depends_on", list([reference(&format!("terraform_data.{id}_name"))])))
        .build();
    [rename.block(), pause]
}

/// Whether its steps read the project (it gets the project then).
fn has_runs(w: &Machine) -> bool {
    isoloom_plan::machine_files::reads_project(&w.remote.plan.lifecycle)
}

/// The set-up script: its name checked, published ports forwarded, its services' ports opened,
/// then its steps.
fn setup_script(w: &Machine) -> String {
    let Remote { env, name, plan, .. } = w.remote;
    let ports = env.spec().machines.get(name).map(|m| m.ports.as_slice()).unwrap_or_default();
    let mut ps = vec![
        "$ErrorActionPreference = 'Stop'".to_owned(),
        format!(
            "if ($env:COMPUTERNAME -ne '{}') {{ throw \"still named $env:COMPUTERNAME: the rename hasn't taken effect\" }}",
            computer_name(name).to_uppercase()
        ),
    ];
    ps.extend(ports.iter().filter_map(port_proxy));
    ps.extend(ports.iter().flat_map(firewall));
    let runs = has_runs(w);
    for step in plan.lifecycle.iter().filter(|s| !(w.skip)(s)) {
        if *step == Step::Project && !runs {
            continue;
        }
        ps.extend(steps::windows(env, name, step).map(|s| s.trim_end().to_owned()));
    }
    ps.join("\n") + "\n"
}

/// A published TCP port that differs from the service's own, forwarded to it (Windows' port
/// proxy is TCP only: validation refuses a UDP one).
fn port_proxy(sv: &Port) -> Option<String> {
    let h = sv.publish.filter(|h| *h != sv.port && sv.protocol.is_tcp())?;
    Some(format!(
        "netsh interface portproxy add v4tov4 listenport={h} listenaddress=0.0.0.0 connectport={p} connectaddress=127.0.0.1 | Out-Null",
        p = sv.port
    ))
}

/// A service's firewall openings: its port, and the published port when that differs.
pub(crate) fn firewall(sv: &Port) -> Vec<String> {
    let (proto, suffix) = if sv.protocol.is_tcp() { ("TCP", "") } else { ("UDP", " udp") };
    let rule =
        |p: u16| format!("New-NetFirewallRule -DisplayName 'isoloom {p}{suffix}' -Direction Inbound -Protocol {proto} -LocalPort {p} -Action Allow | Out-Null");
    let mut out = vec![rule(sv.port)];
    out.extend(sv.publish.filter(|h| *h != sv.port).map(rule));
    out
}
