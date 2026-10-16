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
                kind: RunKind::Shell,
                creates: None
            },
            &Step::Reboot,
            &Step::Run {
                path: "site.yml".into(),
                kind: RunKind::Playbook,
                creates: None
            },
            &Step::Run {
                path: "b.SH".into(),
                kind: RunKind::Shell,
                creates: None
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
        kind: RunKind::PowerShell,
        creates: None
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

/// The steps of machine `m` of a lab whose machines are `machines`.
fn steps_of(machines: &str, m: &str) -> MachineLifecycle {
    let yaml = format!(
        "version: 1\nname: t\nsecrets: {{ db: {{ env: DB }} }}\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24, ipv6: {{ cidr: \"fd00:9::/64\" }} }}, b: {{ cidr: 10.8.0.0/24 }} }}\nvolumes: {{ shared: {{ owner: a }} }}\nmachines:\n{machines}"
    );
    testing::environment(&yaml, Target::Vagrant)
        .machines
        .get(m)
        .map(|p| p.lifecycle.clone())
        .unwrap_or_default()
}

#[test]
fn a_vm_grows_its_disk_formats_its_disks_and_makes_its_own_volumes_only_when_it_has_them() {
    let plain = steps_of("  a: { networks: { b: 10 }, os: debian@12 }\n", "a");
    assert!(!plain.iter().any(|s| matches!(
        s,
        Step::DiskGrow | Step::Disks { .. } | Step::Volumes { .. } | Step::Secrets | Step::Ipv6 { .. }
    )));
    let full = steps_of(
        "  a: { networks: { lan: 10 }, os: debian@12, resources: { disk_gb: 20 }, disks: { data: { size_gb: 2, mount: /data } }, volumes: { shared: /s, own: /o }, files: [{ path: /run/db, secret: db }] }\n",
        "a",
    );
    assert!(full.contains(&Step::DiskGrow));
    assert!(full.iter().any(|s| matches!(s, Step::Disks { disks } if disks.len() == 1)));
    assert!(full.contains(&Step::Volumes { paths: vec!["/o".into()] }), "its own volume only: {full:?}");
    assert!(full.contains(&Step::Secrets));
    assert!(full.iter().any(|s| matches!(s, Step::Ipv6 { addresses, .. } if addresses.len() == 1)));
}

#[test]
fn a_machines_further_addresses_are_named_after_it_in_the_others_hosts() {
    let machines = "  a: { networks: { b: [10, 11, 12] }, os: debian@12 }\n  c: { networks: { b: 20 }, os: debian@12 }\n  w: { networks: { b: [30, 31] }, os: windows-server@2022 }\n";
    let entries = |m: &str| {
        steps_of(machines, m)
            .into_iter()
            .find_map(|s| match s {
                Step::Hosts { entries } => Some(entries),
                _ => None,
            })
            .unwrap_or_default()
    };
    let a = |o: u8| (Ipv4Addr::new(10, 8, 0, o), vec!["a".to_owned()]);
    assert_eq!(entries("c"), [a(10), a(11), a(12)], "a Linux machine leaves the Windows one out");
    let w = entries("w");
    assert!(w.contains(&a(11)) && w.contains(&(Ipv4Addr::new(10, 8, 0, 20), vec!["c".to_owned()])), "{w:?}");
    assert!(!entries("a").iter().any(|(_, names)| names == &["a".to_owned()]), "never itself");
}
