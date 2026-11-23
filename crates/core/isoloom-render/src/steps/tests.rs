use isoloom_plan::Environment;
use isoloom_plan::environment::Route;
use isoloom_plan::lifecycle::{RunKind, Step};
use isoloom_spec::model::Target;
use isoloom_spec::net::NetworkBlock;

use super::{linux, reboot_check, record_boot, wait_for_mark, windows};
use crate::testing;

const SPEC: &str = r"
version: 1
name: steps
networks:
  front: { cidr: 10.84.0.0/24 }
  back: { cidr: 10.85.0.0/24, internet: false }
volumes:
  shared: { owner: writer }
reach:
  - { from: front, to: back }
machines:
  writer:
    networks: { front: 10, back: 10 }
    aliases: [store]
    volumes: { shared: /srv/shared }
    os: debian@12
    app: { steps: [{ run: steps/write.sh }] }
  reader:
    networks: { back: 11 }
    volumes: { shared: /data/shared }
    os: debian@12
    app: { steps: [{ run: steps/look.sh }] }
  web:
    networks: { front: 12 }
    os: debian@12
    app: { steps: [{ run: steps/web.sh }] }
";

/// A Windows machine and a Linux one it waits for, with files, disks, settings and IPv6.
const WINDOWS: &str = r#"
version: 1
name: win
networks:
  lan: { cidr: 10.86.0.0/24, ipv6: { cidr: "fd00:86::/64" } }
  far: { cidr: 10.87.0.0/24 }
reach: [{ from: lan, to: far }]
machines:
  db: { networks: { far: 10 }, os: debian@12, ports: { pg: 5432 }, app: { steps: [{ run: db.sh }] } }
  win:
    networks: { lan: [10, 11] }
    os: windows-server@2022
    environment: { MODE: lab }
    depends_on: [db]
    resources: { disk_gb: 80 }
    disks: { logs: { size_gb: 2, mount: "E:" } }
    files: [{ path: 'C:\app\a.ini', content: x }]
    app: { steps: [{ run: setup.ps1 }] }
"#;

fn env() -> Environment {
    testing::env(SPEC, Target::Vagrant)
}

fn planned(env: &Environment, machine: &str, pick: impl Fn(&Step) -> bool) -> Step {
    env.machines[machine].lifecycle.iter().find(|s| pick(s)).expect("the step is planned").clone()
}

fn rendered(env: &Environment, machine: &str, pick: impl Fn(&Step) -> bool) -> String {
    linux(env, machine, &planned(env, machine, pick)).expect("the step renders")
}

#[test]
fn routes_go_through_the_router() {
    let env = env();
    let sh = rendered(&env, "web", |s| matches!(s, Step::Routes { .. }));
    assert_eq!(sh, "ip route replace 10.85.0.0/24 via 10.84.0.254\n");
    let with_default = Step::Routes {
        routes: Vec::new(),
        default: Some("10.84.0.1".parse().unwrap()),
    };
    assert_eq!(linux(&env, "web", &with_default).as_deref(), Some("ip route replace default via 10.84.0.1\n"));
    let none = Step::Routes {
        routes: Vec::new(),
        default: None,
    };
    assert_eq!(linux(&env, "web", &none), None);
}

#[test]
fn hosts_carry_aliases() {
    let env = env();
    let sh = rendered(&env, "reader", |s| matches!(s, Step::Hosts { .. }));
    assert_eq!(
        sh,
        "printf '%s\\n' '10.85.0.10 writer store' '10.85.0.11 reader' '10.84.0.12 web' | while IFS= read -r l; do grep -qxF \"$l\" /etc/hosts || printf '%s\\n' \"$l\" >> /etc/hosts; done\n",
        "each line once, a machine with its aliases"
    );
    assert_eq!(linux(&env, "reader", &Step::Hosts { entries: Vec::new() }), None);
}

#[test]
fn offline_keeps_the_loopback() {
    let env = env();
    let sh = linux(&env, "reader", &Step::Egress).expect("the step renders");
    // nft installed, the rules written, then loaded now and at every boot.
    let load = crate::boot::at_boot(
        "egress",
        "No new connection leaves the isoloom networks",
        "nft -f /etc/isoloom-egress.nft\n",
        crate::boot::Trigger::Boot,
        &[],
    );
    let write = sh
        .strip_prefix(&super::nftables())
        .and_then(|rest| rest.strip_suffix(&load))
        .unwrap_or_else(|| panic!("nft, the rules, then their loading: {sh}"));
    let Some(mut posix) = crate::test_support::posix_sh() else { return };
    let dir = tempfile::Builder::new().prefix("isoloom-egress-").tempdir().expect("a temporary folder");
    let rules = dir.path().join("egress.nft");
    let write = write.replace("/etc/isoloom-egress.nft", &crate::test_support::sh_path(&rules));
    assert!(posix.args(["-c", &write]).status().expect("sh runs").success(), "{write}");
    assert_eq!(
        std::fs::read_to_string(&rules).expect("the rules"),
        "table inet isoloom-egress\ndelete table inet isoloom-egress\ntable inet isoloom-egress {\n  chain output {\n    type filter hook output priority 0; policy accept;\n    oifname \"lo\" accept\n    ip daddr != { 10.84.0.0/24, 10.85.0.0/24 } ct state new drop\n  }\n}\n",
        "the loopback stays open, new connections out of the networks are dropped"
    );
}

#[test]
fn ready_leaves_its_marker() {
    let env = env();
    let sh = rendered(&env, "writer", |s| matches!(s, Step::Ready));
    assert_eq!(sh, "mkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready\n");
}

#[test]
fn the_project_and_volumes_are_unpacked_and_made() {
    let env = env();
    assert_eq!(
        linux(&env, "web", &Step::Project).as_deref(),
        Some(
            "rm -rf /opt/isoloom && mkdir -p /opt/isoloom && tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom --strip-components=1 && rm -f /tmp/isoloom-project.tgz\n"
        )
    );
    let volumes = Step::Volumes {
        paths: vec!["/a b".into(), "/c".into()],
    };
    assert_eq!(linux(&env, "web", &volumes).as_deref(), Some("mkdir -p '/a b' '/c'\n"));
    assert_eq!(linux(&env, "web", &Step::Volumes { paths: Vec::new() }), None);
}

#[test]
fn inputs_are_kept_only_when_a_reboot_or_a_service_needs_them() {
    let env = env();
    assert_eq!(
        linux(&env, "web", &Step::Inputs { persist: true }).as_deref(),
        Some("mkdir -p /var/lib/isoloom && install -m 600 /tmp/isoloom-inputs.env /var/lib/isoloom/inputs.env\n")
    );
    assert_eq!(linux(&env, "web", &Step::Inputs { persist: false }), None);
}

#[test]
fn a_provision_step_runs_from_the_project_folder_its_way() {
    let env = env();
    let run = |path: &str, kind| {
        linux(
            &env,
            "web",
            &Step::Run {
                path: path.into(),
                kind,
                creates: None,
            },
        )
    };
    assert_eq!(
        run("steps/web.sh", RunKind::Shell).as_deref(),
        Some(concat!(r"cd /opt/isoloom && sh -c 'sh '\''steps/web.sh'\'''", "\n"))
    );
    assert_eq!(
        run("site.yml", RunKind::Playbook),
        Some(format!(
            "set -e\n{}\ncommand -v ansible-playbook >/dev/null || pkg ansible-core\n{}\n",
            super::PKG,
            r"cd /opt/isoloom && sh -c 'ansible-playbook -c local -i web, '\''site.yml'\'''"
        )),
        "Ansible installed unless it is there, then the playbook run locally on the machine by its name"
    );
    assert_eq!(run("setup.ps1", RunKind::PowerShell), None, "a Windows script is PowerShell's");
}

#[test]
fn a_reboot_is_recorded_then_checked() {
    let env = env();
    let sh = linux(&env, "web", &Step::Reboot).expect("a reboot");
    assert_eq!(sh, format!("{}(sleep 2 && reboot) >/dev/null 2>&1 &\n", record_boot()));
    assert_eq!(
        record_boot(),
        "mkdir -p /var/lib/isoloom && cp /proc/sys/kernel/random/boot_id /var/lib/isoloom/boot-id\n"
    );
    assert_eq!(
        reboot_check(),
        "if [ \"$(cat /proc/sys/kernel/random/boot_id)\" = \"$(cat /var/lib/isoloom/boot-id)\" ]; then echo 'the reboot step: the machine did not restart' >&2; exit 1; fi\n"
    );
}

#[test]
fn a_machine_waits_for_the_ones_it_depends_on() {
    let env = testing::env(WINDOWS, Target::Vagrant);
    let wait = planned(&env, "win", |s| matches!(s, Step::WaitFor { .. }));
    assert_eq!(
        wait,
        Step::WaitFor {
            machine: "db".into(),
            ports: vec![5432]
        }
    );
    assert_eq!(windows(&env, "win", &wait), super::wait::windows("db", &[5432]));
    assert_eq!(linux(&env, "win", &wait), Some(super::wait::linux("db", &[5432])));
}

#[test]
fn windows_has_hosts_but_no_trunks() {
    let env = env();
    let hosts = planned(&env, "reader", |s| matches!(s, Step::Hosts { .. }));
    assert_eq!(
        windows(&env, "reader", &hosts).as_deref(),
        Some(
            "$h = \"$env:SystemRoot\\System32\\drivers\\etc\\hosts\"; foreach ($l in @('10.85.0.10 writer store', '10.85.0.11 reader', '10.84.0.12 web')) { if (-not (Select-String -Path $h -SimpleMatch $l -Quiet)) { Add-Content -Path $h -Value $l } }"
        )
    );
    assert_eq!(windows(&env, "reader", &Step::Hosts { entries: Vec::new() }), None);
    for linux_only in [
        Step::Trunks,
        Step::Prelude,
        Step::Egress,
        Step::Tc,
        Step::Reboot,
        Step::Inputs { persist: true },
    ] {
        assert_eq!(windows(&env, "reader", &linux_only), None, "{linux_only:?}");
    }
}

#[test]
fn windows_routes_keep_across_restarts() {
    let env = env();
    let routes = Step::Routes {
        routes: vec![
            Route {
                cidr: NetworkBlock::parse("10.85.0.0/24").unwrap(),
                via: "10.84.0.254".parse().unwrap(),
            },
            Route {
                cidr: NetworkBlock::parse("10.86.0.0/29").unwrap(),
                via: "10.84.0.254".parse().unwrap(),
            },
        ],
        default: None,
    };
    assert_eq!(
        windows(&env, "web", &routes).as_deref(),
        Some("route -p add 10.85.0.0 MASK 255.255.255.0 10.84.0.254; route -p add 10.86.0.0 MASK 255.255.255.248 10.84.0.254")
    );
    let none = Step::Routes {
        routes: Vec::new(),
        default: None,
    };
    assert_eq!(windows(&env, "web", &none), None);
}

#[test]
fn every_windows_step_is_powershell() {
    let env = testing::env(WINDOWS, Target::Vagrant);
    let lifecycle = &env.machines["win"].lifecycle;
    let spec = env.spec();
    let win = &spec.machines["win"];
    // Each step of the plan, as its own renderer writes it in PowerShell.
    for step in lifecycle {
        let expected = match step {
            Step::Settings => super::settings::windows(win),
            Step::Ipv6 { addresses, routes } => Some(crate::network::ipv6::windows(addresses, routes)),
            Step::MoreAddresses { addresses } => Some(crate::network::more_addresses::windows(addresses)),
            Step::Files { files } => Some(super::files::windows(files)),
            Step::DiskGrow => Some(super::disks::GROW_ROOT_PS1.to_owned()),
            Step::Disks { disks } => super::disks::windows(disks),
            Step::WaitFor { machine, ports } => super::wait::windows(machine, ports),
            Step::Project => Some(super::windows_project_unpack(super::TopFolder::Stripped)),
            Step::Run { .. } => Some(
                "Set-Location C:\\isoloom; & powershell -NoProfile -ExecutionPolicy Bypass -File 'setup.ps1'; if ($LASTEXITCODE) { exit $LASTEXITCODE }"
                    .to_owned(),
            ),
            Step::Hosts { entries } => super::network::windows_hosts(entries),
            Step::Routes { routes, .. } => super::network::windows_routes(routes),
            Step::Ready => Some("New-Item -ItemType Directory -Force C:\\isoloom | Out-Null; Set-Content C:\\isoloom\\ready ready".to_owned()),
            // Linux's own steps.
            _ => None,
        };
        assert_eq!(windows(&env, "win", step), expected, "{step:?}");
    }
    let rendered: Vec<String> = lifecycle
        .iter()
        .filter(|s| windows(&env, "win", s).is_some())
        .map(|s| format!("{s:?}").split([' ', '{', '(']).next().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(
        rendered,
        [
            "DiskGrow",
            "Disks",
            "Project",
            "Hosts",
            "Settings",
            "MoreAddresses",
            "Routes",
            "Ipv6",
            "Files",
            "WaitFor",
            "Run",
            "Ready"
        ],
        "every step a Windows machine has"
    );
}

#[test]
fn a_typographic_quote_in_a_windows_path_stays_inside_its_string() {
    let env = testing::env(WINDOWS, Target::Vagrant);
    let run = Step::Run {
        path: "it\u{2019}s.ps1".into(),
        kind: RunKind::PowerShell,
        creates: None,
    };
    assert_eq!(
        windows(&env, "win", &run).as_deref(),
        Some(
            "Set-Location C:\\isoloom; & powershell -NoProfile -ExecutionPolicy Bypass -File 'it\u{2019}\u{2019}s.ps1'; if ($LASTEXITCODE) { exit $LASTEXITCODE }"
        )
    );
    // Behind its `creates`: skipped once that path is there.
    let guarded = Step::Run {
        path: "it\u{2019}s.ps1".into(),
        kind: RunKind::PowerShell,
        creates: Some("C:\\done".into()),
    };
    assert_eq!(
        windows(&env, "win", &guarded).as_deref(),
        Some(
            "if (-not (Test-Path -LiteralPath 'C:\\done')) { Set-Location C:\\isoloom; & powershell -NoProfile -ExecutionPolicy Bypass -File 'it\u{2019}\u{2019}s.ps1'; if ($LASTEXITCODE) { exit $LASTEXITCODE } }"
        )
    );
}

#[test]
fn windows_secrets_wait_for_their_delivery() {
    let spec = "version: 1\nname: s\nsecrets: { pw: { env: PW } }\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  win: { networks: { lan: 10 }, os: windows-server@2022, files: [{ path: 'C:\\pw.txt', secret: pw }] }\n";
    let env = testing::env(spec, Target::Vagrant);
    let files = &env.machines["win"].secrets;
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(windows(&env, "win", &Step::Secrets), Some(crate::secrets::windows::step("win", files)));
    assert_eq!(linux(&env, "win", &Step::Secrets), Some(crate::secrets::step("win", files)));
}

#[test]
fn a_vm_on_two_vlans_of_a_lan_builds_its_trunk_inside() {
    const VLANS: &str = r"
version: 1
name: v
networks:
  office:
    cidr: 10.70.0.0/16
    vlans:
      10: { cidr: 10.70.10.0/24 }
      20: { cidr: 10.70.20.0/24 }
machines:
  admin: { os: debian@12, networks: { office.vlan10: 50, office.vlan20: 50 } }
  staff: { os: debian@12, networks: { office.vlan10: 10 } }
";
    let env = testing::env(VLANS, Target::Vagrant);
    let trunks = crate::network::trunks::vms(&env);
    let admin = trunks.iter().find(|t| t.machine == "admin").expect("admin carries a trunk");
    let built: String = crate::network::trunks::vm_commands(&env, admin).into_iter().map(|l| l + "\n").collect();
    assert_eq!(linux(&env, "admin", &Step::Trunks), Some(built));
    assert_eq!(linux(&env, "staff", &Step::Trunks), None, "one VLAN is an interface, not a trunk");
}

#[test]
fn a_vm_impairs_its_own_links_into_a_slow_network() {
    const SLOW: &str = r"
version: 1
name: s
networks:
  near: { cidr: 10.75.1.0/24 }
  far: { cidr: 10.75.2.0/24, tc: { delay: 80ms } }
machines:
  web: { os: debian@12, networks: { far: 10 } }
  user: { os: debian@12, networks: { near: 10 } }
";
    let env = testing::env(SLOW, Target::Vagrant);
    let spec = env.spec();
    let own = crate::network::router::machine_tc_script(spec, &spec.machines["web"], |n, o| env.address_on(n, o));
    assert!(own.is_some(), "web is on a slow network");
    assert_eq!(linux(&env, "web", &Step::Tc), own);
    assert_eq!(linux(&env, "user", &Step::Tc), None);
    assert_eq!(linux(&env, "nobody", &Step::Tc), None);
}

/// The hosts step run by a real `sh` on a stand-in hosts file: every line lands once and as
/// written, whatever it holds (a backslash, runs of spaces, trailing blanks: the lines are read
/// with `IFS= read -r`), however often the step runs.
#[cfg(unix)]
mod hosts_in_a_real_shell {
    use std::net::Ipv4Addr;

    use proptest::prelude::*;

    use super::super::network::hosts;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn every_line_lands_once_and_unchanged(names in prop::collection::vec("[ a-z0-9.\\\\-]{1,8}", 1..4)) {
            let entries = vec![
                (Ipv4Addr::new(10, 0, 0, 1), names),
                (Ipv4Addr::new(10, 0, 0, 2), vec!["  lead".to_owned(), r"back\slash".to_owned(), "trail ".to_owned()]),
            ];
            let script = hosts(&entries).expect("a step").replace("/etc/hosts", "\"$HOSTS\"");
            let hosts_file = tempfile::Builder::new().prefix("isoloom-hosts-").tempfile().expect("a stand-in hosts file");
            let file = hosts_file.path();
            std::fs::write(file, "127.0.0.1 localhost\n").expect("a stand-in hosts file");
            for _ in 0..2 {
                let status = std::process::Command::new("sh").arg("-c").arg(&script).env("HOSTS", file).status().expect("sh runs");
                prop_assert!(status.success());
            }
            let written = std::fs::read_to_string(file).expect("the hosts file");
            let lines: Vec<String> = entries.iter().map(|(a, n)| format!("{a} {}", n.join(" "))).collect();
            prop_assert_eq!(written, format!("127.0.0.1 localhost\n{}\n", lines.join("\n")));
        }
    }
}

#[test]
fn a_docker_host_gets_the_engine_and_compose_at_their_pinned_versions() {
    use super::{COMPOSE_VERSION, DOCKER_APT_KEY_SHA256, DOCKER_VERSION, install_docker};
    let script = install_docker();
    let (apt, rpm) = script
        .strip_prefix("command -v docker >/dev/null || if command -v apt-get >/dev/null; then (\n")
        .and_then(|rest| rest.split_once("\n)\nelse\n"))
        .unwrap_or_else(|| panic!("an installed Docker kept, else an apt branch and an rpm fallback: {script}"));
    let apt: Vec<&str> = apt.lines().map(str::trim).collect();
    // The apt family adds Docker's repository with the key pinned by SHA-256, no `curl | sh`, and
    // resolves the engine's exact package from the repo at the pinned upstream version.
    let escaped = DOCKER_VERSION.replace('.', "[.]");
    for line in [
        format!("echo \"{DOCKER_APT_KEY_SHA256}  $k\" | sha256sum -c -"),
        "echo \"deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/$_id $_cn stable\" > /etc/apt/sources.list.d/docker.list".to_owned(),
        format!("v=$(apt-cache madison docker-ce | awk '$3 ~ /(^|:){escaped}/ {{ print $3; exit }}')"),
        format!("[ -n \"$v\" ] || {{ echo \"docker-ce {DOCKER_VERSION} is not in Docker's apt repository\" >&2; exit 1; }}"),
        format!("apt-get install -y -qq --allow-downgrades \"docker-ce=$v\" \"docker-ce-cli=$v\" containerd.io docker-buildx-plugin \"docker-compose-plugin={COMPOSE_VERSION}-*\""),
    ] {
        assert!(apt.contains(&line.as_str()), "the apt branch has no `{line}`: {apt:#?}");
    }
    assert!(!apt.iter().any(|l| l.contains("get.docker.com")), "the apt branch never pipes a script into sh");
    // The RPM family installs with Docker's pinned install script.
    assert_eq!(rpm, format!("  curl -fsSL https://get.docker.com | VERSION={DOCKER_VERSION} sh\nfi"));
}

#[test]
fn a_mark_wait_ends_when_the_mark_is_there_and_gives_up_with_its_message_after_the_timeout() {
    let dir = tempfile::Builder::new().prefix("isoloom-mark-").tempdir().unwrap();
    let mark = dir.path().join("delivered");
    let at = crate::test_support::sh_path(&mark);
    let run = |timeout_s| {
        let line = wait_for_mark(&at, timeout_s, "waiting for it", "it never came");
        crate::test_support::posix_sh().map(|mut sh| sh.args(["-c", &line]).output().unwrap())
    };
    let Some(missing) = run(0) else { return };
    // No mark, no time: it says it waits, then gives up at once with its message.
    assert!(!missing.status.success());
    assert_eq!(String::from_utf8_lossy(&missing.stdout), "waiting for it\n");
    assert_eq!(String::from_utf8_lossy(&missing.stderr), "it never came\n");
    // The mark there: it ends at once, saying nothing.
    std::fs::write(&mark, "").unwrap();
    let there = run(0).unwrap();
    assert!(there.status.success() && there.stdout.is_empty(), "{there:?}");
}

#[cfg(unix)]
#[test]
fn a_project_script_with_a_space_in_its_path_runs() {
    let env = testing::env(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  m: { networks: { lan: 10 }, os: debian@12, app: { steps: [{ run: \"provision/my setup.sh\" }] } }\n",
        Target::Vagrant,
    );
    let line = rendered(&env, "m", |s| matches!(s, Step::Run { .. }));
    let dir = tempfile::Builder::new().prefix("isoloom-spaced-").tempdir().unwrap();
    let dir = dir.path();
    std::fs::create_dir_all(dir.join("provision")).unwrap();
    std::fs::write(dir.join("provision/my setup.sh"), "touch ran\n").unwrap();
    let script = line.replace(super::PROJECT_DIR, &dir.to_string_lossy());
    let status = std::process::Command::new("sh").arg("-c").arg(&script).status().expect("sh runs");
    assert!(status.success() && dir.join("ran").exists(), "{script}");
}

#[test]
fn root_runs_a_command_as_it_is_and_another_user_through_sudo() {
    use super::AsRoot;
    assert_eq!(AsRoot::of("root"), AsRoot::Root);
    assert_eq!((AsRoot::Root.prefix(), AsRoot::Root.keeping_env()), ("", ""));
    assert_eq!(AsRoot::of("isoloom"), AsRoot::Sudo);
    assert_eq!((AsRoot::Sudo.prefix(), AsRoot::Sudo.keeping_env()), ("sudo ", "sudo -E "));
}

#[test]
fn every_project_unpack_replaces_the_old_copy() {
    use super::{Archive, TopFolder, project_unpack};
    assert_eq!(
        project_unpack(Archive::Stdin, TopFolder::Kept),
        "rm -rf /opt/isoloom && mkdir -p /opt/isoloom && tar -xzf - -C /opt/isoloom"
    );
    assert_eq!(
        project_unpack(Archive::Uploaded, TopFolder::Stripped),
        "rm -rf /opt/isoloom && mkdir -p /opt/isoloom && tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom --strip-components=1 && rm -f /tmp/isoloom-project.tgz"
    );
    assert!(!project_unpack(Archive::Uploaded, TopFolder::Kept).contains('\''), "it goes in sh -c '…'");
}

#[test]
fn the_windows_project_unpack_reads_the_uploaded_archive_into_the_project_folder() {
    for (top, strip) in [(super::TopFolder::Kept, 0), (super::TopFolder::Stripped, 1)] {
        assert_eq!(
            super::windows_project_unpack(top),
            format!(
                "$archive = 'C:\\Windows\\Temp\\isoloom-project.tgz'; $dest = 'C:\\isoloom'; $strip = {strip}\n{}",
                crate::powershell::UNTAR
            ),
            "{top:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_tracked_step_writes_lines_of_the_ready_grammar() {
    use isoloom_spec::ready_line::ReadyLine;
    let dir = tempfile::Builder::new().prefix("isoloom-tracked-").tempdir().unwrap();
    let ready = dir.path().join("ready");
    let run = |command: &str| {
        let script = super::tracked("a step's 'name'", command, &|mark| format!("{{ {mark}; }}")).replace(super::READY_FILE, &ready.display().to_string());
        let ok = std::process::Command::new("sh").args(["-c", &script]).status().unwrap().success();
        (ok, ReadyLine::parse(&std::fs::read_to_string(&ready).unwrap()))
    };
    assert_eq!(run("true"), (true, Some(ReadyLine::running("a step's 'name'"))));
    assert_eq!(run("false"), (false, Some(ReadyLine::failed("a step's 'name'"))));
    let done = super::ready_mark(&ReadyLine::Ready).replace(super::READY_FILE, &ready.display().to_string());
    assert!(std::process::Command::new("sh").args(["-c", &done]).status().unwrap().success());
    assert_eq!(ReadyLine::parse(&std::fs::read_to_string(&ready).unwrap()), Some(ReadyLine::Ready));
}
