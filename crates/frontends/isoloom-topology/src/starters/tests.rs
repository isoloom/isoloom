use super::*;

fn spec(yaml: &str) -> Spec {
    isoloom_spec::parse(yaml).unwrap()
}

/// Each plain network's block of `spec`, by its name.
fn blocks_of(spec: &Spec) -> Blocks {
    spec.networks
        .iter()
        .filter_map(|(name, n)| Some((name.clone(), NetworkBlock::parse(&n.cidr)?)))
        .collect()
}

/// The starters of a spec's machines (no controller playbook).
fn starters_of(spec: &Spec) -> Vec<ProjectFile> {
    let mut files = Vec::new();
    add_starters(&mut files, spec, &blocks_of(spec), &[]);
    files
}

fn project(machine: &str) -> Spec {
    spec(&format!(
        "version: 1\nname: x\nnetworks:\n  lan: {{ cidr: 10.10.1.0/24 }}\nmachines:\n{machine}"
    ))
}

#[test]
fn a_database_gets_postgresql_listening_on_the_network() {
    let files = starters_of(&project(
        "  db:\n    os: debian@12\n    networks: { lan: 12 }\n    ports: { postgres: 5432 }\n    app: { steps: [{ run: provision/db.sh }] }\n",
    ));
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "provision/db.sh");
    let sh = &files[0].contents;
    assert!(sh.starts_with("#!/bin/sh\n"));
    assert!(sh.contains("# Starter set-up for `db` (debian@12)"));
    assert!(sh.contains("# Address: 10.10.1.12 (lan)"));
    assert!(sh.contains("apt-get install -y postgresql\n"));
    assert!(sh.contains("listen_addresses = '*'"));
    assert!(sh.contains("if [ -d /run/systemd/system ]; then\n  systemctl enable postgresql\n  systemctl restart postgresql\nfi\n"));
}

#[test]
fn a_script_builds_in_a_container_where_no_init_system_runs() {
    let sh = &starters_of(&project(
        "  web:\n    os: debian@12\n    networks: { lan: 10 }\n    ports: { http: 80 }\n    app: { steps: [{ run: provision/web.sh }] }\n",
    ))[0]
        .contents;
    let (before_start, _) = sh.split_once("if [ -d /run/systemd/system ]").unwrap();
    assert!(!before_start.contains("systemctl"), "{sh}");
    assert!(!sh.contains("pipefail"), "POSIX sh: {sh}");
}

#[test]
fn each_family_installs_with_its_own_tool() {
    for (os, install) in [
        ("rocky@9", "dnf install -y nginx"),
        ("centos@7", "yum install -y nginx"),
        ("alpine@3.19", "apk add --no-cache nginx"),
        ("debian@12", "apt-get install -y nginx"),
    ] {
        let files = starters_of(&project(&format!(
            "  web:\n    os: {os}\n    networks: {{ lan: 10 }}\n    ports: {{ http: 80, admin: 9000 }}\n    app: {{ steps: [{{ run: provision/web.sh }}] }}\n"
        )));
        assert!(files[0].contents.contains(install), "{os}: {}", files[0].contents);
        assert!(files[0].contents.contains("To do: install what answers on port(s) 9000"));
    }
    let apk = &starters_of(&project(
        "  web:\n    os: alpine\n    networks: { lan: 10 }\n    ports: { ssh: 22 }\n    app: { steps: [{ run: provision/web.sh }] }\n",
    ))[0]
        .contents;
    assert!(
        apk.contains("[ -d /run/openrc ]; then\n  rc-update add sshd default\n  rc-service sshd restart\nfi"),
        "{apk}"
    );
}

#[test]
fn windows_and_ansible_get_their_own_templates() {
    let files = starters_of(&project(
        "  win:\n    os: windows-server@2022\n    networks: { lan: 10 }\n    ports: { http: 80, admin: 8443 }\n    app: { steps: [{ run: provision/win.ps1 }] }\n  web:\n    os: debian@12\n    networks: { lan: 11 }\n    app: { steps: [{ playbook: provision/web.yml }] }\n",
    ));
    let win = &files[0].contents;
    assert!(win.contains("$ErrorActionPreference = 'Stop'\n"), "{win}");
    assert!(win.contains("Install-WindowsFeature -Name Web-Server"), "{win}");
    assert!(
        win.contains("New-NetFirewallRule -DisplayName 'project-8443-tcp' -Direction Inbound -Protocol TCP -LocalPort 8443 -Action Allow"),
        "{win}"
    );
    assert!(win.contains("# A web server (IIS) for port 80.\n"), "{win}");
    assert!(!win.contains("project-80-"), "IIS opens its own port: {win}");
    let web = &files[1].contents;
    assert!(
        web.starts_with("# Starter playbook for `web`") && web.contains("- hosts: all\n  become: true\n"),
        "{web}"
    );
    assert!(web.contains("ansible.builtin.ping"), "{web}");
}

/// A Windows starter opens each port on its own transport, once, and says which web ports IIS
/// is for.
#[test]
fn a_windows_starter_opens_each_port_on_its_transport() {
    let win = &starters_of(&project(
        "  win:\n    os: windows-server@2022\n    networks: { lan: 10 }\n    ports:\n      https: { port: 443, protocol: https }\n      dns: { port: 53, protocol: udp }\n      dns-tcp: 53\n      syslog: { port: 514, protocol: udp }\n    app: { steps: [{ run: provision/win.ps1 }] }\n",
    ))[0]
        .contents;
    assert!(win.contains("# A web server (IIS) for port 443.\n"), "only the web port drawn: {win}");
    assert!(
        win.contains("# Port 443: IIS answers HTTPS once a certificate is bound to its site.\n"),
        "{win}"
    );
    let rules: Vec<&str> = win.lines().filter(|l| l.starts_with("New-NetFirewallRule")).collect();
    assert_eq!(
        rules,
        [
            "New-NetFirewallRule -DisplayName 'project-53-udp' -Direction Inbound -Protocol UDP -LocalPort 53 -Action Allow",
            "New-NetFirewallRule -DisplayName 'project-53-tcp' -Direction Inbound -Protocol TCP -LocalPort 53 -Action Allow",
            "New-NetFirewallRule -DisplayName 'project-514-udp' -Direction Inbound -Protocol UDP -LocalPort 514 -Action Allow",
        ]
    );
}

#[test]
fn a_controller_playbook_sets_up_its_machine_over_winrm() {
    let starter = playbook("dc01", PlaybookOn::Controller);
    assert!(
        starter.starts_with("# Starter playbook for `dc01`, run from Isoloom's controller over WinRM, written by Isoloom"),
        "{starter}"
    );
    assert!(starter.contains("\n- hosts: dc01\n  tasks:\n"), "{starter}");
    assert!(starter.contains("ansible.windows.win_ping:"), "{starter}");
    assert!(!starter.contains("become"), "WinRM has no sudo: {starter}");
}

#[test]
fn a_step_said_twice_gets_one_starter() {
    // A path outside the project is Isoloom's to refuse; the controller's playbooks show it skipped.
    let files = starters_of(&project(
        "  a:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: provision/x.sh }] }\n  b:\n    os: debian@12\n    networks: { lan: 11 }\n    app: { steps: [{ run: provision/x.sh }] }\n",
    ));
    assert_eq!(files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["provision/x.sh"]);
}

#[test]
fn a_controller_playbook_gets_its_starter_after_the_machines_at_a_free_path_of_the_project() {
    let spec = project("  web:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: provision/web.sh }] }\n");
    let playbook = |path: &str, machine: &str| ControllerPlaybook {
        path: path.into(),
        machine: machine.into(),
    };
    let config = ProjectFile {
        path: "configs/r1.cfg".into(),
        contents: "hostname r1\n".into(),
    };
    let mut files = vec![config];
    let controller = [
        playbook("provision/dc01.yml", "dc01"),
        playbook("provision/web.sh", "dc01"),
        playbook("../dc01.yml", "dc01"),
        playbook("/etc/dc01.yml", "dc01"),
        playbook("configs/r1.cfg", "dc01"),
        playbook("isoloom.yml", "dc01"),
        playbook(".isoloom/docker/compose.yml", "dc01"),
    ];
    add_starters(&mut files, &spec, &blocks_of(&spec), &controller);
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["configs/r1.cfg", "provision/web.sh", "provision/dc01.yml"]);
    assert!(files[2].contents.contains("- hosts: dc01"));
}

#[test]
fn a_starter_names_the_services_it_answers_only_when_it_has_ports() {
    let with = &starters_of(&project(
        "  db:\n    os: debian@12\n    networks: { lan: 12 }\n    ports: { postgres: 5432 }\n    app: { steps: [{ run: provision/db.sh }] }\n",
    ))[0]
        .contents;
    assert!(with.contains("# Services the project expects to answer: 5432\n"), "{with}");
    let without = &starters_of(&project(
        "  box:\n    os: debian@12\n    networks: { lan: 12 }\n    app: { steps: [{ run: provision/box.sh }] }\n",
    ))[0]
        .contents;
    assert!(!without.contains("Services the project expects"), "{without}");
    assert!(without.contains("Nothing to install for this machine's ports yet"), "{without}");
}

#[test]
fn mariadb_and_redis_listen_on_the_network() {
    let sh = &starters_of(&project(
        "  data:\n    os: debian@12\n    networks: { lan: 12 }\n    ports: { mysql: 3306, redis: 6379 }\n    app: { steps: [{ run: provision/data.sh }] }\n",
    ))[0]
        .contents;
    assert!(sh.contains("apt-get install -y mariadb-server redis-server\n"), "{sh}");
    assert!(sh.contains("bind-address = 0.0.0.0"), "{sh}");
    assert!(sh.contains("protected-mode no"), "{sh}");
}

#[test]
fn a_package_two_ports_call_for_is_installed_once() {
    let sh = &starters_of(&project(
        "  web:\n    os: debian@12\n    networks: { lan: 12 }\n    ports: { http: 80, https: 443 }\n    app: { steps: [{ run: provision/web.sh }] }\n",
    ))[0]
        .contents;
    assert!(sh.contains("apt-get install -y nginx\n"), "{sh}");
    assert_eq!(sh.matches("systemctl enable nginx").count(), 1, "{sh}");
}

/// Regression: the guard spelled `isoloom.yml` alone, so a step at `isoloom.yaml` (a name Isoloom
/// also reads the spec from) got a starter over it; and it took `.isoloom/` alone for Isoloom's
/// outputs, so a step in an instance's `.isoloom-2/` got one, though Isoloom's rule for its
/// outputs (`/.isoloom*`) keeps every such name out of each copy of the project.
#[test]
fn no_starter_goes_where_isoloom_reads_or_writes() {
    for path in ["isoloom.yml", "isoloom.yaml", ".isoloom/docker/x.sh", ".isoloom-2/x.sh", ".isoloomignore.sh"] {
        let mut files = Vec::new();
        add(&mut files, path, String::new());
        assert!(files.is_empty(), "{path}");
    }
    let mut files = Vec::new();
    add(&mut files, "provision/.isoloom/x.sh", String::new());
    assert_eq!(files.len(), 1, "the rule is for the project's top only");
}

/// Regression: a path holding `..` anywhere was taken for one leaving the project, so
/// `provision/a..b.sh`, a file Isoloom reads in it, got no starter.
#[test]
fn only_a_dot_dot_segment_leaves_the_project() {
    for (path, kept) in [
        ("provision/a..b.sh", true),
        ("../x.sh", false),
        ("provision/../../x.sh", false),
        ("/x.sh", false),
        // Isoloom's rule (RelPath) also keeps a Windows path out: a drive, a backslash.
        ("C:/x.sh", false),
        ("provision\\..\\..\\x.sh", false),
    ] {
        let mut files = Vec::new();
        add(&mut files, path, String::new());
        assert_eq!(files.len() == 1, kept, "{path}");
    }
}
