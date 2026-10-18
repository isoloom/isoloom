use isoloom_spec::model::Protocol;

use super::*;

fn machine(yaml: &str) -> syntax::Machine {
    serde_yaml_ng::from_str(yaml).unwrap_or_else(|e| panic!("{yaml}: {e}"))
}

/// The recipe of `yaml`'s machine.
fn resolved(yaml: &str) -> Result<Recipe, Vec<Unportable>> {
    let m = machine(yaml);
    recipe(m.os.expect("an os"), &m, &DaemonTable::common())
}

fn key() -> String {
    format!("key: {{ url: \"https://a.test/k\", sha256: {} }}", "0".repeat(64))
}

#[test]
fn every_step_resolves_in_order_into_the_install_phase() {
    let r = resolved(
        "{ os: debian@12, app: { steps: [{ packages: [nginx] }, { run: a.sh }, { group: { name: ops } }, reboot, { firewall: { allow: [8443] } }] } }",
    )
    .unwrap();
    let kinds: Vec<&str> = r.resources().map(Resource::kind).collect();
    assert_eq!(kinds, ["packages", "run", "group", "reboot", "firewall"]);
    assert_eq!((r.family, r.configure.len(), r.start.len()), (Family::Debian, 0, 0));
}

#[test]
fn an_apt_repository_gets_the_releases_codename_and_main() {
    let r = resolved(&format!(
        "{{ os: debian@12, app: {{ steps: [{{ repository: {{ name: nginx, url: \"https://nginx.org/packages/debian\", {} }} }}] }} }}",
        key()
    ))
    .unwrap();
    let Resource::Repository(repo) = &r.install[0] else { panic!() };
    assert_eq!(repo.kind, RepositoryKind::Apt);
    assert_eq!(repo.suite.as_ref().map(ToString::to_string).as_deref(), Some("bookworm"));
    assert_eq!(repo.components.iter().map(ToString::to_string).collect::<Vec<_>>(), ["main"]);
    let rocky = resolved(&format!(
        "{{ os: rocky@9, app: {{ steps: [{{ repository: {{ name: nginx, url: \"https://nginx.org/packages/centos/9/\", {} }} }}] }} }}",
        key()
    ))
    .unwrap();
    let Resource::Repository(rpm) = &rocky.install[0] else { panic!() };
    assert_eq!((rpm.kind, rpm.suite.as_ref(), rpm.components.len()), (RepositoryKind::Rpm, None, 0));
}

#[test]
fn a_firewall_opens_its_named_ports_by_number_and_transport() {
    let r = resolved("{ os: rocky@9, ports: { dns: { port: 53, protocol: udp }, web: 80 }, app: { steps: [{ firewall: { allow: [dns, web, 8443/udp] } }] } }")
        .unwrap();
    let Resource::Firewall(ports) = &r.install[0] else { panic!() };
    let found: Vec<(u16, Protocol)> = ports.iter().map(|p| (p.port.get(), p.protocol)).collect();
    assert_eq!(found, [(53, Protocol::Udp), (80, Protocol::Tcp), (8443, Protocol::Udp)]);
}

#[test]
fn a_service_is_the_unit_its_family_names_with_its_foreground_form() {
    let r =
        resolved("{ os: rocky@9, app: { steps: [{ packages: [openssh-server] }, { service: ssh }, { service: { name: api, run: [/srv/api], user: api } }] } }")
            .unwrap();
    let Resource::Service(ssh) = &r.install[1] else { panic!() };
    assert_eq!((ssh.unit.as_str(), &ssh.source), ("sshd", &ServiceSource::Package));
    let units: Vec<&str> = r.daemons.iter().map(|d| d.unit.as_str()).collect();
    assert_eq!(units, ["sshd", "api"]);
    assert_eq!(r.daemons[1].user.as_ref().map(isoloom_spec::syntax::UserName::as_str), Some("api"));
}

#[test]
fn a_disabled_service_runs_nowhere() {
    let r = resolved("{ os: debian@12, app: { steps: [{ service: { name: nginx, enabled: false } }] } }").unwrap();
    assert_eq!(r.daemons, []);
    assert!(matches!(&r.install[0], Resource::Service(s) if !s.enabled));
}

#[test]
fn a_windows_script_runs_in_powershell() {
    let r = resolved("{ os: windows-server@2022, app: { steps: [{ run: iis.ps1 }, { packages: [Web-Server] }] } }").unwrap();
    assert!(matches!(&r.install[0], Resource::Run(run) if run.shell == Shell::PowerShell));
    assert_eq!(r.family, Family::Windows);
}

#[test]
fn what_windows_cant_carry_is_refused_at_its_field() {
    let found = resolved(&format!(
        "{{ os: windows-server@2022, app: {{ steps: [{{ repository: {{ name: r, url: \"https://a.test/r\", {} }} }}, {{ copy: {{ from: a, to: 'C:\\a', owner: admin }} }}, {{ template: {{ from: t, to: 'C:\\t', mode: \"0600\" }} }}, {{ file: {{ path: 'C:\\l', link: 'C:\\x' }} }}, {{ packages: [{{ name: Web-Server, version: \"1\" }}] }}] }} }}",
        key()
    ))
    .unwrap_err();
    let at: Vec<(usize, Option<&str>)> = found.iter().map(|u| (u.step, u.field)).collect();
    assert_eq!(at, [(0, None), (1, Some("owner")), (2, Some("mode")), (3, Some("link")), (4, None)]);
    assert!(found[0].reason.contains("no package repository") && found[1].reason.contains("ACL"));
}
