//! A Windows machine set up over WinRM by Terraform: renamed and restarted first, then one
//! PowerShell script made of its steps' Windows renderings, its firewall openings and published
//! ports. The target's image must start WinRM over HTTP for `isoloom` with the generated
//! password ([`write_password`]; see each target's user data).

use crate::hcl_builder::{Block, Expression, Module, Template, list, reference, string};
use isoloom_plan::{Environment, Step};
use isoloom_render::steps;
use isoloom_spec::model::Port;

use super::controller::WinrmAuth;
use super::provisioner::{Connection, Provisioner, Upload};
use super::{Remote, linux};
use crate::module::{self, ident, resource};

/// The Terraform expression of the Windows machines' password.
pub(crate) const PASSWORD: &str = "random_password.windows.result";

/// Whether the environment has a Windows VM: its module then generates their password.
pub(crate) fn any(env: &Environment) -> bool {
    env.vms().any(|vm| vm.plan.is_windows())
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
const SETUP: &str = isoloom_spec::windows_state_path!("\\setup.ps1");
/// Where the project's archive goes: where the `Project` step unpacks it from.
const ARCHIVE: &str = isoloom_render::steps::WINDOWS_PROJECT_ARCHIVE;

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
        // The runner delivers its secrets over WinRM while the set-up waits for them.
        announce: (!plan.secrets.is_empty()).then(|| name.to_owned()),
    };
    out.push(setup.block());
    out
}

/// Its renaming (`terraform_data.<id>_name`) and the pause while it restarts
/// (`time_sleep.<id>_restart`).
fn rename(w: &Machine, connection: &Connection) -> [Block; 2] {
    let id = ident(w.remote.name);
    let host = w.remote.name;
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
            name.to_uppercase()
        ),
    ];
    ps.extend(ports.iter().filter_map(port_proxy));
    ps.extend(ports.iter().flat_map(firewall));
    if ports.iter().any(|p| p.port == REMOTE_DESKTOP && p.protocol.is_tcp()) {
        ps.push(remote_desktop());
    }
    let runs = has_runs(w);
    for step in plan.lifecycle.iter().filter(|s| !(w.skip)(s)) {
        if *step == Step::Project && !runs {
            continue;
        }
        ps.extend(steps::windows(env, name, step).map(|s| s.trim_end().to_owned()));
    }
    ps.join("\n") + "\n"
}

/// Remote Desktop's port: a spec opening it turns Remote Desktop on.
const REMOTE_DESKTOP: u16 = 3389;

/// Remote Desktop turned on (a server image may have it off), with Network Level Authentication:
/// the port the spec opens is its firewall rule.
fn remote_desktop() -> String {
    "Set-ItemProperty -Path 'HKLM:\\System\\CurrentControlSet\\Control\\Terminal Server' -Name fDenyTSConnections -Value 0\nSet-ItemProperty -Path 'HKLM:\\System\\CurrentControlSet\\Control\\Terminal Server\\WinStations\\RDP-Tcp' -Name UserAuthentication -Value 1".to_owned()
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
    let rule = |p: u16| open_port(&format!("{p}{suffix}"), proto, p);
    let mut out = vec![rule(sv.port)];
    out.extend(sv.publish.filter(|h| *h != sv.port).map(rule));
    out
}

/// An inbound firewall rule `isoloom <name>` opening `port` over `proto` (`TCP`, `UDP`), made
/// once: a set-up run again finds it.
fn open_port(name: &str, proto: &str, port: u16) -> String {
    format!(
        "if (-not (Get-NetFirewallRule -DisplayName 'isoloom {name}' -ErrorAction SilentlyContinue)) {{ New-NetFirewallRule -DisplayName 'isoloom {name}' -Direction Inbound -Protocol {proto} -LocalPort {port} -Action Allow | Out-Null }}"
    )
}

/// The administrator Terraform and the controller reach a Windows VM as.
pub(crate) const ADMIN: &str = "isoloom";

/// The Windows Administrators group's well-known SID: its name differs with the image's
/// language, its SID doesn't.
const ADMINISTRATORS_SID: &str = "S-1-5-32-544";

/// First boot, as PowerShell in the platform's user data (cloudbase-init's script, EC2's
/// `<powershell>`): [`ADMIN`] with the generated password ([`PASSWORD`]), made, or given that
/// password when the image has the user already, then put in the Administrators group (by its
/// SID) unless it is in it. Each platform adds only what differs: its WinRM authentication
/// ([`winrm`]), its own steps.
pub(crate) fn first_boot_admin() -> Template {
    Template::text("$password = ConvertTo-SecureString '")
        .with_expr(reference(PASSWORD))
        .with_text(format!(
            "' -AsPlainText -Force\nif (Get-LocalUser -Name {ADMIN} -ErrorAction SilentlyContinue) {{ Set-LocalUser -Name {ADMIN} -Password $password -PasswordNeverExpires $true }} else {{ New-LocalUser -Name {ADMIN} -Password $password -PasswordNeverExpires -AccountNeverExpires | Out-Null }}\nif (-not (Get-LocalGroupMember -SID {ADMINISTRATORS_SID} | Where-Object {{ $_.Name -like '*\\{ADMIN}' }})) {{ Add-LocalGroupMember -SID {ADMINISTRATORS_SID} -Member {ADMIN} }}\n"
        ))
}

/// WinRM over plain HTTP for [`ADMIN`], its port (5985) open: the remote token of a local
/// administrator kept whole (`LocalAccountTokenFilterPolicy`), the networks private, unencrypted
/// messages taken, and Basic authentication on only for [`WinrmAuth::Basic`] (with NTLM, Basic,
/// which sends the password with every request, stays off). Terraform's WinRM client
/// (masterzen/winrm) doesn't seal NTLM messages, so a service refusing unencrypted ones would
/// refuse Terraform's set-up.
pub(crate) fn winrm(auth: WinrmAuth) -> String {
    let basic = match auth {
        WinrmAuth::Basic => "Set-Item WSMan:\\localhost\\Service\\Auth\\Basic $true\n",
        WinrmAuth::Ntlm => "",
    };
    format!(
        "New-ItemProperty -Path HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System -Name LocalAccountTokenFilterPolicy -Value 1 -PropertyType DWord -Force | Out-Null\nGet-NetConnectionProfile | Set-NetConnectionProfile -NetworkCategory Private -ErrorAction SilentlyContinue\nEnable-PSRemoting -Force -SkipNetworkProfileCheck | Out-Null\n{basic}Set-Item WSMan:\\localhost\\Service\\AllowUnencrypted $true\n{}",
        open_port("winrm", "TCP", 5985)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hcl_builder::string;

    fn script(ports: &str) -> String {
        let env = crate::testing::environment(
            &format!(
                "version: 1\nname: w\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  dc: {{ networks: {{ lab: 10 }}, os: windows-server@2022, ports: {{ {ports} }} }}\n"
            ),
            isoloom_spec::model::Target::CloudVm,
        );
        setup_script(&Machine {
            remote: Remote {
                env: &env,
                name: "dc",
                plan: &env.machines["dc"],
                replace_on: string("id"),
                host: string("10.9.0.10"),
            },
            password: string("pw"),
            skip: |_| false,
            platform: Platform::default(),
        })
    }

    #[test]
    fn a_spec_opening_remote_desktops_port_turns_it_on() {
        let with = script("rdp: { port: 3389 }");
        assert!(with.contains("-Name fDenyTSConnections -Value 0"), "{with}");
        assert!(with.contains("'isoloom 3389'"), "its firewall rule: {with}");
        let without = script("web: { port: 80 }");
        assert!(!without.contains("fDenyTSConnections"), "{without}");
    }
}
