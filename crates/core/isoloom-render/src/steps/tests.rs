use isoloom_plan::Environment;
use isoloom_plan::environment::Route;
use isoloom_plan::lifecycle::{RunKind, Step};
use isoloom_spec::model::Target;

use super::{linux, reboot_check, record_boot, windows};
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
fn the_owner_serves_a_shared_volume() {
    let env = env();
    let sh = rendered(&env, "writer", |s| matches!(s, Step::SharedVolume { .. }));
    assert!(sh.contains("exportfs -ra"), "{sh}");
    let mount = rendered(&env, "reader", |s| matches!(s, Step::SharedVolume { .. }));
    assert!(mount.contains("/srv/shared") && mount.contains("nfs defaults"), "{mount}");
}

#[test]
fn hosts_carry_aliases() {
    let env = env();
    let sh = rendered(&env, "reader", |s| matches!(s, Step::Hosts { .. }));
    assert!(sh.contains("writer store"), "{sh}");
    assert!(sh.contains("grep -qxF"), "each line once: {sh}");
    assert_eq!(linux(&env, "reader", &Step::Hosts { entries: Vec::new() }), None);
}

#[test]
fn offline_keeps_the_loopback() {
    let env = env();
    let sh = linux(&env, "reader", &Step::Egress).expect("the step renders");
    let lo = sh.find("oifname \"lo\" accept").expect("the loopback stays open");
    assert!(lo < sh.find("ct state new drop").expect("the drop"), "{sh}");
    assert!(sh.contains("'    ip daddr != { 10.84.0.0/24, 10.85.0.0/24 } ct state new drop'"), "{sh}");
    assert!(sh.contains("cat > /etc/isoloom/egress.sh <<'EGRESS'"), "loaded at every boot: {sh}");
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
        Some("mkdir -p /opt/isoloom && tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom --strip-components=1 && rm -f /tmp/isoloom-project.tgz\n")
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
    let run = |path: &str, kind| linux(&env, "web", &Step::Run { path: path.into(), kind });
    assert_eq!(
        run("steps/web.sh", RunKind::Shell).as_deref(),
        Some("cd /opt/isoloom && sh -c 'sh steps/web.sh'\n")
    );
    let play = run("site.yml", RunKind::Playbook).expect("a playbook runs locally");
    assert!(play.starts_with("pkg() {") && play.contains("|| pkg ansible-core\n"), "{play}");
    assert!(
        play.ends_with("cd /opt/isoloom && sh -c 'ansible-playbook -c local -i web, site.yml'\n"),
        "{play}"
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
    assert!(reboot_check().starts_with("if [ \"$(cat /proc/sys/kernel/random/boot_id)\" = \"$(cat /var/lib/isoloom/boot-id)\" ]"));
    assert!(reboot_check().ends_with("the machine did not restart' >&2; exit 1; fi\n"));
}

#[test]
fn a_machine_waits_for_the_ones_it_depends_on() {
    let env = testing::env(WINDOWS, Target::Vagrant);
    let wait = planned(&env, "win", |s| matches!(s, Step::WaitFor { .. }));
    let ps = windows(&env, "win", &wait).expect("db serves a port");
    assert!(ps.contains("foreach ($p in @(5432))"), "{ps}");
    let sh = linux(&env, "win", &wait).expect("the Linux form too");
    assert!(sh.contains("</dev/tcp/db/5432"), "{sh}");
}

#[test]
fn windows_has_hosts_but_no_trunks() {
    let env = env();
    let hosts = planned(&env, "reader", |s| matches!(s, Step::Hosts { .. }));
    assert!(windows(&env, "reader", &hosts).is_some_and(|ps| ps.contains("'10.85.0.10 writer store'")));
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
                cidr: "10.85.0.0/24".into(),
                via: "10.84.0.254".parse().unwrap(),
            },
            Route {
                cidr: "10.0.0.0/8".into(),
                via: "10.84.0.254".parse().unwrap(),
            },
        ],
        default: None,
    };
    assert_eq!(
        windows(&env, "web", &routes).as_deref(),
        Some("route -p add 10.85.0.0 MASK 255.255.255.0 10.84.0.254; route -p add 10.0.0.0 MASK 255.0.0.0 10.84.0.254")
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
    let step = |pick: fn(&Step) -> bool| windows(&env, "win", &planned(&env, "win", pick)).expect("a Windows step");
    assert!(step(|s| matches!(s, Step::Settings)).contains("SetEnvironmentVariable('MODE', 'lab', 'Machine')"));
    assert!(step(|s| matches!(s, Step::Ipv6 { .. })).contains("-IPAddress 'fd00:86::10' -PrefixLength 64"));
    assert!(step(|s| matches!(s, Step::MoreAddresses { .. })).contains("-IPAddress '10.86.0.11' -PrefixLength 24"));
    assert!(step(|s| matches!(s, Step::Project)).starts_with("$archive = 'C:\\Windows\\Temp\\isoloom-project.tgz'; $dest = 'C:\\isoloom'; $strip = 1\n"));
    assert!(step(|s| matches!(s, Step::Files { .. })).contains("[IO.File]::WriteAllText('C:\\app\\a.ini', 'x')"));
    assert!(step(|s| matches!(s, Step::DiskGrow)).contains("Resize-Partition -DriveLetter C"));
    assert!(step(|s| matches!(s, Step::Disks { .. })).contains("-DriveLetter E "));
    assert_eq!(
        step(|s| matches!(s, Step::Run { .. })),
        "Set-Location C:\\isoloom; & powershell -NoProfile -ExecutionPolicy Bypass -File 'setup.ps1'; if ($LASTEXITCODE) { exit $LASTEXITCODE }"
    );
    assert_eq!(
        step(|s| matches!(s, Step::Ready)),
        "New-Item -ItemType Directory -Force C:\\isoloom | Out-Null; Set-Content C:\\isoloom\\ready ready"
    );
}

#[test]
fn a_typographic_quote_in_a_windows_path_stays_inside_its_string() {
    let env = testing::env(WINDOWS, Target::Vagrant);
    let run = Step::Run {
        path: "it\u{2019}s.ps1".into(),
        kind: RunKind::PowerShell,
    };
    let ps = windows(&env, "win", &run).expect("a PowerShell step");
    assert!(ps.contains("-File 'it\u{2019}\u{2019}s.ps1';"), "{ps}");
}

#[test]
fn windows_secrets_wait_for_their_delivery() {
    let spec = "version: 1\nname: s\nsecrets: { pw: { env: PW } }\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  win: { networks: { lan: 10 }, os: windows-server@2022, files: [{ path: 'C:\\pw.txt', secret: pw }] }\n";
    let env = testing::env(spec, Target::Vagrant);
    let ps = windows(&env, "win", &Step::Secrets).expect("a secret to place");
    assert!(ps.contains("Set-IsoloomSecret") && ps.contains("isoloom-apply-secrets"), "{ps}");
    let sh = linux(&env, "win", &Step::Secrets).expect("the Linux form too");
    assert!(sh.starts_with("set -e\nn=0; until [ -f /var/lib/isoloom/secrets/.delivered ]"), "{sh}");
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
    let sh = linux(&env, "admin", &Step::Trunks).expect("admin carries a trunk");
    assert!(
        sh.starts_with("ip link show office >/dev/null 2>&1 || { ip link add office type veth peer name office-s"),
        "{sh}"
    );
    assert!(sh.ends_with("ip link set office.20 up; }\n"), "{sh}");
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
    let sh = linux(&env, "web", &Step::Tc).expect("web is on a slow network");
    assert!(sh.contains("/^10\\.75\\.2\\.10\\//") && sh.contains("root netem delay 80ms"), "{sh}");
    assert_eq!(linux(&env, "user", &Step::Tc), None);
    assert_eq!(linux(&env, "nobody", &Step::Tc), None);
}

/// The hosts step run by a real `sh` on a stand-in hosts file: every line lands once and as
/// written, whatever it holds (a backslash, runs of spaces, trailing blanks: the lines are read
/// with `IFS= read -r`), however often the step runs.
#[cfg(unix)]
mod hosts_in_a_real_shell {
    use std::net::Ipv4Addr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use proptest::prelude::*;

    use super::super::network::hosts;

    static RUN: AtomicUsize = AtomicUsize::new(0);

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn every_line_lands_once_and_unchanged(names in prop::collection::vec("[ a-z0-9.\\\\-]{1,8}", 1..4)) {
            let entries = vec![
                (Ipv4Addr::new(10, 0, 0, 1), names),
                (Ipv4Addr::new(10, 0, 0, 2), vec!["  lead".to_owned(), r"back\slash".to_owned(), "trail ".to_owned()]),
            ];
            let script = hosts(&entries).expect("a step").replace("/etc/hosts", "\"$HOSTS\"");
            let file = std::env::temp_dir().join(format!("isoloom-hosts-{}-{}", std::process::id(), RUN.fetch_add(1, Ordering::Relaxed)));
            std::fs::write(&file, "127.0.0.1 localhost\n").expect("a stand-in hosts file");
            for _ in 0..2 {
                let status = std::process::Command::new("sh").arg("-c").arg(&script).env("HOSTS", &file).status().expect("sh runs");
                prop_assert!(status.success());
            }
            let written = std::fs::read_to_string(&file).expect("the hosts file");
            std::fs::remove_file(&file).ok();
            let lines: Vec<String> = entries.iter().map(|(a, n)| format!("{a} {}", n.join(" "))).collect();
            prop_assert_eq!(written, format!("127.0.0.1 localhost\n{}\n", lines.join("\n")));
        }
    }
}
