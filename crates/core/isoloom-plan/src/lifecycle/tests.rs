use isoloom_spec::model::Target;

use super::*;
use crate::testing;

/// A Linux web server, a Windows file server, a database web reaches, a machine on no
/// network, and the steps each VM runs.
const LAB: &str = "version: 1
name: t
inputs: { TOKEN: { sensitive: false } }
networks: { lan: { cidr: 10.9.0.0/24 }, db: { cidr: 10.8.0.0/24 } }
machines:
  web:
    networks: { lan: 10, db: 10 }
    aliases: [www]
    depends_on: [sql]
    environment: { TOKEN: { input: TOKEN } }
    os: debian@12
    app: { steps: [{ run: a.sh }, reboot, { playbook: site.yml }, { run: b.SH }] }
  files: { networks: { lan: 20 }, os: windows-server@2022, app: { steps: [{ run: share.ps1 }] } }
  sql: { networks: { db: 30 }, ports: { sql: 5432 }, os: debian@12, command: [postgres] }
  lone: { os: debian@12 }
";

fn steps(machine: &str) -> MachineLifecycle {
    testing::environment(LAB, Target::Vagrant)
        .machines
        .get(machine)
        .map(|m| m.lifecycle.clone())
        .unwrap_or_default()
}

fn hosts_of(machine: &str) -> Vec<(Ipv4Addr, Vec<String>)> {
    steps(machine)
        .into_iter()
        .find_map(|s| match s {
            Step::Hosts { entries } => Some(entries),
            _ => None,
        })
        .expect("a hosts step")
}

#[test]
fn a_linux_machine_names_the_others_but_windows_ones_and_those_on_no_network() {
    let web = hosts_of("web");
    assert_eq!(web, [(Ipv4Addr::new(10, 8, 0, 30), vec!["sql".to_owned()])]);
    let files = hosts_of("files");
    assert_eq!(
        files,
        [
            (Ipv4Addr::new(10, 9, 0, 10), vec!["web".to_owned(), "www".to_owned()]),
            (Ipv4Addr::new(10, 8, 0, 30), vec!["sql".to_owned()])
        ]
    );
}

#[test]
fn each_app_step_runs_its_way_and_a_reboot_keeps_the_inputs() {
    let web = steps("web");
    let runs: Vec<&Step> = web.iter().filter(|s| matches!(s, Step::Run { .. } | Step::Reboot)).collect();
    assert_eq!(
        runs,
        [
            &Step::Run {
                path: "a.sh".into(),
                kind: RunKind::Shell
            },
            &Step::Reboot,
            &Step::Run {
                path: "site.yml".into(),
                kind: RunKind::Playbook
            },
            &Step::Run {
                path: "b.SH".into(),
                kind: RunKind::Shell
            },
        ]
    );
    assert!(web.contains(&Step::Inputs { persist: true }));
    assert!(web.contains(&Step::WaitFor {
        machine: "sql".into(),
        ports: vec![5432]
    }));
    assert!(steps("files").contains(&Step::Run {
        path: "share.ps1".into(),
        kind: RunKind::PowerShell
    }));
    let sql = steps("sql");
    assert!(sql.contains(&Step::Command) && !sql.iter().any(|s| matches!(s, Step::Inputs { .. })));
}

#[test]
fn a_step_runs_first_from_the_prelude_and_ends_ready() {
    let web = steps("web");
    assert_eq!((web.first(), web.last()), (Some(&Step::Prelude), Some(&Step::Ready)));
    assert!(web.contains(&Step::Project) && web.contains(&Step::Settings) && web.contains(&Step::Trunks));
}

#[test]
fn an_extension_matches_in_any_case_and_only_whole() {
    assert!(has_extension("setup.sh", "sh") && has_extension("setup.SH", "sh") && has_extension("a/b.Ps1", "ps1"));
    assert!(!has_extension("setup.shx", "sh") && !has_extension("sh", "sh") && !has_extension("setup.ps1", "sh"));
}
