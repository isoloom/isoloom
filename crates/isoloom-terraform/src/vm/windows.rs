//! A Windows machine set up over WinRM by Terraform: renamed and restarted first, then one
//! PowerShell script made of its steps' Windows renderings, its firewall openings and published
//! ports. The target's image must start WinRM over HTTP for `isoloom` with the generated password
//! (see each target's user data).

use isoloom_plan::{Environment, Step};
use isoloom_render::hcl_builder::{Block, Expression, list, reference, string};
use isoloom_render::steps;
use isoloom_spec::model::Service;

use super::provisioner::{Connection, Provisioner, Upload};
use super::{linux, res};

/// Where the set-up script goes.
const SETUP: &str = "C:/ProgramData/isoloom/setup.ps1";
/// Where the project's archive goes (the `Project` step unpacks it from there).
const ARCHIVE: &str = "C:/Windows/Temp/isoloom-project.tgz";

/// A Windows machine, as Terraform reaches it.
#[derive(Debug, Clone)]
pub struct Machine<'a> {
    /// The plan.
    pub env: &'a Environment,
    /// The machine.
    pub name: &'a str,
    /// What replaces its set-up when it changes (the instance's id).
    pub replace_on: Expression,
    /// Its address, from where Terraform runs.
    pub host: Expression,
    /// The password of `isoloom`.
    pub password: Expression,
    /// The steps the platform does itself, left out.
    pub skip: fn(&Step) -> bool,
    /// What the platform does its own way.
    pub platform: Platform,
}

/// How a platform's Windows machines differ from the default (renamed by Terraform, WinRM with
/// Basic authentication, the auto-stop in the user data).
#[derive(Debug, Clone, Default)]
pub struct Platform {
    /// The platform names the machine when it creates it: no rename, no restart.
    pub named: bool,
    /// WinRM authenticates with NTLM.
    pub ntlm: bool,
    /// Commands run before the set-up script (what the platform's user data can't do).
    pub before: Vec<Expression>,
}

/// Its Windows name: NetBIOS keeps 15 characters.
fn computer_name(name: &str) -> String {
    name.chars().take(15).collect()
}

/// The machine's resources: its renaming (`terraform_data.<id>_name`) and the pause while it
/// restarts (`time_sleep.<id>_restart`) unless the platform names it, then its set-up
/// (`terraform_data.<id>`).
pub fn blocks(w: Machine) -> Vec<Block> {
    let id = res(w.name);
    let connection = Connection::WinRm {
        host: w.host.clone(),
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
        destination: SETUP.into(),
    });
    depends_on.extend(linux::depends_on(w.env, w.name));
    let mut commands = w.platform.before.clone();
    commands.push(string(format!("powershell -NoProfile -ExecutionPolicy Bypass -File {SETUP}")));
    let setup = Provisioner {
        name: id,
        replace_on: w.replace_on,
        connection,
        uploads,
        parts: vec![commands],
        depends_on,
    };
    out.push(setup.block());
    out
}

/// Its renaming (`terraform_data.<id>_name`) and the pause while it restarts
/// (`time_sleep.<id>_restart`).
fn rename(w: &Machine, connection: &Connection) -> [Block; 2] {
    let id = res(w.name);
    let host = computer_name(w.name);
    let rename = Provisioner {
        name: format!("{id}_name"),
        replace_on: w.replace_on.clone(),
        connection: connection.clone(),
        uploads: Vec::new(),
        parts: vec![vec![string(format!(
            "powershell -NoProfile -Command \"if ($env:COMPUTERNAME -ne '{up}') {{ Rename-Computer -NewName '{host}' -Force; shutdown /r /t 10 }}\"",
            up = host.to_uppercase()
        ))]],
        depends_on: Vec::new(),
    };
    let pause = Block::builder("resource")
        .add_label("time_sleep")
        .add_label(format!("{id}_restart"))
        .add_attribute(("create_duration", string("90s")))
        .add_attribute(("depends_on", list([reference(&format!("terraform_data.{id}_name"))])))
        .build();
    [rename.block(), pause]
}

/// Whether its steps read the project (it gets the project then).
fn has_runs(w: &Machine) -> bool {
    isoloom_plan::machine_files::reads_project(&w.env.machines[w.name].lifecycle)
}

/// The set-up script: its name checked, published ports forwarded, its services' ports opened,
/// then its steps.
fn setup_script(w: &Machine) -> String {
    let m = &w.env.spec().machines[w.name];
    let mut ps = vec![
        "$ErrorActionPreference = 'Stop'".to_string(),
        format!(
            "if ($env:COMPUTERNAME -ne '{}') {{ throw \"still named $env:COMPUTERNAME: the rename hasn't taken effect\" }}",
            computer_name(w.name).to_uppercase()
        ),
    ];
    ps.extend(m.services.iter().filter_map(port_proxy));
    ps.extend(m.services.iter().flat_map(firewall));
    let runs = has_runs(w);
    for step in w.env.machines[w.name].lifecycle.iter().filter(|s| !(w.skip)(s)) {
        if *step == Step::Project && !runs {
            continue;
        }
        ps.extend(steps::windows(w.env, w.name, step).map(|s| s.trim_end().to_string()));
    }
    ps.join("\n") + "\n"
}

/// A published TCP port that differs from the service's own, forwarded to it (Windows' port
/// proxy is TCP only: validation refuses a UDP one).
fn port_proxy(sv: &Service) -> Option<String> {
    let h = sv.publish.filter(|h| *h != sv.port && sv.protocol.is_tcp())?;
    Some(format!(
        "netsh interface portproxy add v4tov4 listenport={h} listenaddress=0.0.0.0 connectport={p} connectaddress=127.0.0.1 | Out-Null",
        p = sv.port
    ))
}

/// A service's firewall openings: its port, and the published port when that differs.
pub fn firewall(sv: &Service) -> Vec<String> {
    let (proto, suffix) = if sv.protocol.is_tcp() { ("TCP", "") } else { ("UDP", " udp") };
    let rule =
        |p: u16| format!("New-NetFirewallRule -DisplayName 'isoloom {p}{suffix}' -Direction Inbound -Protocol {proto} -LocalPort {p} -Action Allow | Out-Null");
    let mut out = vec![rule(sv.port)];
    out.extend(sv.publish.filter(|h| *h != sv.port).map(rule));
    out
}
