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

/// The first starter of the project of `machine`.
fn starter(machine: &str) -> String {
    starters_of(&project(machine)).into_iter().next().map(|f| f.contents).unwrap_or_default()
}

/// A starter's parts, as a blank line separates them: what it is, the install, each port's
/// configuration, the start, what is left to do.
fn parts(text: &str) -> Vec<&str> {
    text.trim_end().split("\n\n").collect()
}

/// What the starters run for `port` to make it listen beyond localhost.
fn configure(port: u16) -> &'static str {
    PACKAGES.iter().find(|p| p.port == port).map_or("", |p| p.configure)
}

/// The start part on systemd, for `units`.
fn systemd_start(units: &[&str]) -> String {
    let mut part =
        "# Start the services where an init system runs them (a VM; a container's build has none).\nif [ -d /run/systemd/system ]; then\n".to_owned();
    for unit in units {
        for verb in ["enable", "restart"] {
            part.push_str("  systemctl ");
            part.push_str(verb);
            part.push(' ');
            part.push_str(unit);
            part.push('\n');
        }
    }
    part + "fi"
}

#[test]
fn a_database_gets_postgresql_listening_on_the_network() {
    let files = starters_of(&project(
        "  db:\n    os: debian@12\n    networks: { lan: 12 }\n    ports: { postgres: 5432 }\n    app: { steps: [{ run: provision/db.sh }] }\n",
    ));
    assert_eq!(files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["provision/db.sh"]);
    let listen = format!("# Port 5432: listen on the project's networks, not only on localhost.\n{}", configure(5432));
    assert_eq!(
        parts(&files[0].contents),
        [
            "#!/bin/sh\n# Starter set-up for `db` (debian@12), written by Isoloom: edit it to build your project.\n# Address: 10.10.1.12 (lan)\n# Services the project expects to answer: 5432\nset -eu",
            "export DEBIAN_FRONTEND=noninteractive\napt-get update\napt-get install -y postgresql",
            listen.as_str(),
            systemd_start(&["postgresql"]).as_str(),
        ]
    );
    assert!(
        configure(5432)
            .lines()
            .any(|l| l == r#"if [ -n "$conf" ]; then sed -i "s/^#\?listen_addresses.*/listen_addresses = '*'/" "$conf"; fi"#),
        "PostgreSQL listens on every address"
    );
}

#[test]
fn a_script_builds_in_a_container_where_no_init_system_runs() {
    let sh = starter("  web:\n    os: debian@12\n    networks: { lan: 10 }\n    ports: { http: 80 }\n    app: { steps: [{ run: provision/web.sh }] }\n");
    // systemctl only inside the start's guard; POSIX sh's options alone (no `pipefail`).
    assert_eq!(parts(&sh).last().copied(), Some(systemd_start(&["nginx"]).as_str()));
    let systemctl: Vec<&str> = sh.lines().filter(|l| l.contains("systemctl")).collect();
    assert_eq!(systemctl, ["  systemctl enable nginx", "  systemctl restart nginx"]);
    assert_eq!(sh.lines().filter(|l| l.starts_with("set ")).collect::<Vec<_>>(), ["set -eu"]);
}

#[test]
fn each_family_installs_with_its_own_tool() {
    for (os, install) in [
        ("rocky@9", "dnf install -y nginx"),
        ("centos@7", "yum install -y nginx"),
        ("alpine@3.19", "apk add --no-cache nginx"),
        ("debian@12", "export DEBIAN_FRONTEND=noninteractive\napt-get update\napt-get install -y nginx"),
    ] {
        let sh = starter(&format!(
            "  web:\n    os: {os}\n    networks: {{ lan: 10 }}\n    ports: {{ http: 80, admin: 9000 }}\n    app: {{ steps: [{{ run: provision/web.sh }}] }}\n"
        ));
        let parts = parts(&sh);
        assert_eq!(parts.get(1).copied(), Some(install), "{os}");
        assert_eq!(parts.last().copied(), Some("# To do: install what answers on port(s) 9000."), "{os}");
    }
    let apk = starter("  web:\n    os: alpine\n    networks: { lan: 10 }\n    ports: { ssh: 22 }\n    app: { steps: [{ run: provision/web.sh }] }\n");
    assert_eq!(
        parts(&apk).last().copied(),
        Some(
            "# Start the services where an init system runs them (a VM; a container's build has none).\nif command -v rc-service >/dev/null 2>&1 && [ -d /run/openrc ]; then\n  rc-update add sshd default\n  rc-service sshd restart\nfi"
        )
    );
}

#[test]
fn windows_and_ansible_get_their_own_templates() {
    let files = starters_of(&project(
        "  win:\n    os: windows-server@2022\n    networks: { lan: 10 }\n    ports: { http: 80, admin: 8443 }\n    app: { steps: [{ run: provision/win.ps1 }] }\n  web:\n    os: debian@12\n    networks: { lan: 11 }\n    app: { steps: [{ playbook: provision/web.yml }] }\n",
    ));
    // IIS opens its own port: no rule for 80.
    assert_eq!(
        files[0].contents.lines().collect::<Vec<_>>(),
        [
            "# Starter set-up for `win` (windows-server@2022), written by Isoloom: edit it to build your project.",
            "# Address: 10.10.1.10 (lan)",
            "$ErrorActionPreference = 'Stop'",
            "",
            "# A web server (IIS) for port 80.",
            "Install-WindowsFeature -Name Web-Server -IncludeManagementTools",
            "# To do: install what answers on port 8443/tcp, then open it:",
            "New-NetFirewallRule -DisplayName 'project-8443-tcp' -Direction Inbound -Protocol TCP -LocalPort 8443 -Action Allow",
        ]
    );
    assert_eq!(
        files[1].contents,
        "# Starter playbook for `web`, written by Isoloom: edit it to build your project.\n- hosts: all\n  become: true\n  tasks:\n    - name: The machine answers\n      ansible.builtin.ping:\n"
    );
}

/// A Windows starter opens each port on its own transport, once, and says which web ports IIS
/// is for.
#[test]
fn a_windows_starter_opens_each_port_on_its_transport() {
    let win = starter(
        "  win:\n    os: windows-server@2022\n    networks: { lan: 10 }\n    ports:\n      https: { port: 443, protocol: https }\n      dns: { port: 53, protocol: udp }\n      dns-tcp: 53\n      syslog: { port: 514, protocol: udp }\n    app: { steps: [{ run: provision/win.ps1 }] }\n",
    );
    let said: Vec<&str> = win.lines().filter(|l| l.starts_with("# A web") || l.starts_with("# Port")).collect();
    assert_eq!(
        said,
        [
            "# A web server (IIS) for port 443.",
            "# Port 443: IIS answers HTTPS once a certificate is bound to its site."
        ],
        "only the web port drawn"
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
    // WinRM has no sudo: no `become`.
    assert_eq!(
        playbook("dc01", PlaybookOn::Controller),
        "# Starter playbook for `dc01`, run from Isoloom's controller over WinRM, written by Isoloom: edit it to build your project.\n- hosts: dc01\n  tasks:\n    - name: The machine answers\n      ansible.windows.win_ping:\n"
    );
}

#[test]
fn a_step_said_twice_gets_one_starter() {
    let files = starters_of(&project(
        "  a:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: provision/x.sh }] }\n  b:\n    os: debian@12\n    networks: { lan: 11 }\n    app: { steps: [{ run: provision/x.sh }] }\n",
    ));
    assert_eq!(files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["provision/x.sh"]);
}

#[test]
fn a_controller_playbook_gets_its_starter_after_the_machines_at_a_free_path_of_the_project() {
    // A path outside the project is Isoloom's to refuse; the controller's playbooks show it skipped.
    let spec = project("  web:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: provision/web.sh }] }\n");
    let playbook_at = |path: &str| ControllerPlaybook {
        path: path.into(),
        machine: "dc01".into(),
    };
    let config = ProjectFile {
        path: "configs/r1.cfg".into(),
        contents: "hostname r1\n".into(),
    };
    let mut files = vec![config];
    let controller = [
        "provision/dc01.yml",
        "provision/web.sh",
        "../dc01.yml",
        "/etc/dc01.yml",
        "configs/r1.cfg",
        "isoloom.yml",
        ".isoloom/docker/compose.yml",
    ]
    .map(playbook_at);
    add_starters(&mut files, &spec, &blocks_of(&spec), &controller);
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["configs/r1.cfg", "provision/web.sh", "provision/dc01.yml"]);
    assert_eq!(files[2].contents, playbook("dc01", PlaybookOn::Controller));
}

#[test]
fn a_starter_names_the_services_it_answers_only_when_it_has_ports() {
    let with = starter("  db:\n    os: debian@12\n    networks: { lan: 12 }\n    ports: { postgres: 5432 }\n    app: { steps: [{ run: provision/db.sh }] }\n");
    assert_eq!(
        parts(&with).first().copied(),
        Some(
            "#!/bin/sh\n# Starter set-up for `db` (debian@12), written by Isoloom: edit it to build your project.\n# Address: 10.10.1.12 (lan)\n# Services the project expects to answer: 5432\nset -eu"
        )
    );
    let without = starter("  box:\n    os: debian@12\n    networks: { lan: 12 }\n    app: { steps: [{ run: provision/box.sh }] }\n");
    assert_eq!(
        parts(&without),
        [
            "#!/bin/sh\n# Starter set-up for `box` (debian@12), written by Isoloom: edit it to build your project.\n# Address: 10.10.1.12 (lan)\nset -eu",
            "# Nothing to install for this machine's ports: add your own steps here."
        ]
    );
}

#[test]
fn mariadb_and_redis_listen_on_the_network() {
    let sh = starter(
        "  data:\n    os: debian@12\n    networks: { lan: 12 }\n    ports: { mysql: 3306, redis: 6379 }\n    app: { steps: [{ run: provision/data.sh }] }\n",
    );
    let mariadb = format!("# Port 3306: listen on the project's networks, not only on localhost.\n{}", configure(3306));
    let redis = format!("# Port 6379: listen on the project's networks, not only on localhost.\n{}", configure(6379));
    assert_eq!(
        parts(&sh)[1..],
        [
            "export DEBIAN_FRONTEND=noninteractive\napt-get update\napt-get install -y mariadb-server redis-server",
            mariadb.as_str(),
            redis.as_str(),
            systemd_start(&["mariadb", "redis-server"]).as_str(),
        ]
    );
    for (port, listens) in [
        (3306, r#"  if [ -f "$f" ]; then sed -i "s/^bind-address.*/bind-address = 0.0.0.0/" "$f"; fi"#),
        (
            6379,
            r#"  if [ -f "$f" ]; then sed -i -e "s/^bind .*/bind 0.0.0.0/" -e "s/^protected-mode yes/protected-mode no/" "$f"; fi"#,
        ),
    ] {
        assert!(configure(port).lines().any(|l| l == listens), "{port} listens on every address");
    }
}

#[test]
fn a_package_two_ports_call_for_is_installed_once() {
    let sh =
        starter("  web:\n    os: debian@12\n    networks: { lan: 12 }\n    ports: { http: 80, https: 443 }\n    app: { steps: [{ run: provision/web.sh }] }\n");
    assert_eq!(
        parts(&sh)[1..],
        [
            "export DEBIAN_FRONTEND=noninteractive\napt-get update\napt-get install -y nginx",
            systemd_start(&["nginx"]).as_str()
        ]
    );
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
