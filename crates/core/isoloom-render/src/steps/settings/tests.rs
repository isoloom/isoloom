use super::{linux, regex_escape, windows};
use crate::testing;

/// `web` sets all three kinds, `env` only its environment, `bare` nothing.
const SPEC: &str = r#"
version: 1
name: s
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web:
    networks: { lan: 10 }
    os: debian@12
    environment: { MODE: "it's \"on\"" }
    sysctls: { net.core.somaxconn: "4096" }
    ulimits: { nofile: { soft: 1024, hard: 4096 } }
  env: { networks: { lan: 11 }, os: debian@12, environment: { A: x } }
  bare: { networks: { lan: 12 }, os: debian@12 }
  win: { networks: { lan: 13 }, os: windows-server@2022, environment: { MODE: "it's" } }
"#;

#[test]
fn a_machine_with_no_settings_has_no_step() {
    let spec = testing::spec(SPEC);
    assert_eq!(linux(&spec.machines["bare"]), None);
    assert_eq!(windows(&spec.machines["bare"]), None);
}

#[test]
fn each_kind_of_setting_lands_where_logins_and_services_read_it() {
    let spec = testing::spec(SPEC);
    let sh = linux(&spec.machines["web"]).expect("settings");
    assert!(
        sh.starts_with("set -e\nmkdir -p /etc/systemd/system.conf.d\ngrep -v -E '^(MODE)=' /etc/environment"),
        "{sh}"
    );
    assert!(
        sh.contains("printf '[Manager]\\nDefaultEnvironment=%s\\n' '\"MODE=it'\\''s \\\"on\\\"\"'"),
        "{sh}"
    );
    assert!(
        sh.contains("printf %s 'net.core.somaxconn = 4096\n' > /etc/sysctl.d/90-isoloom.conf\nsysctl -q -p"),
        "{sh}"
    );
    assert!(
        sh.contains("'* soft nofile 1024\n* hard nofile 4096\nroot soft nofile 1024\nroot hard nofile 4096\n'"),
        "{sh}"
    );
    assert!(sh.contains("'[Manager]\nDefaultLimitNOFILE=1024:4096\n'"), "{sh}");
    assert!(sh.ends_with("systemctl daemon-reexec 2>/dev/null || true\n"), "{sh}");
}

#[test]
fn only_what_is_set_is_written() {
    let spec = testing::spec(SPEC);
    let sh = linux(&spec.machines["env"]).expect("an environment");
    assert!(sh.contains("/etc/environment") && !sh.contains("sysctl") && !sh.contains("limits"), "{sh}");
}

#[test]
fn windows_sets_the_environment_machine_wide() {
    let spec = testing::spec(SPEC);
    assert_eq!(
        windows(&spec.machines["win"]).as_deref(),
        Some("[Environment]::SetEnvironmentVariable('MODE', 'it''s', 'Machine')")
    );
}

#[test]
fn a_name_is_matched_literally() {
    assert_eq!(regex_escape("a.b*c|d"), "a\\.b\\*c\\|d");
    assert_eq!(regex_escape("PLAIN_1"), "PLAIN_1");
}

#[test]
fn a_value_of_several_lines_keeps_every_other_variable_and_reaches_logins_and_services() {
    let spec = testing::spec(
        "version: 1\nname: s\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web:\n    networks: { lan: 10 }\n    os: debian@12\n    environment: { A: x, NOTE: \"line1\\nB=injected\" }\n",
    );
    let sh = linux(&spec.machines["web"]).expect("settings");
    // `/etc/environment` is a variable a line: the second line would be a variable of its own, so
    // only the one-line value is written there.
    assert!(sh.contains("printf %s 'A=x\n' >> /etc/environment.isoloom"), "{sh}");
    assert!(
        sh.contains("printf %s 'export NOTE='\\''line1\nB=injected'\\''\n' > /etc/profile.d/90-isoloom-environment.sh\n"),
        "{sh}"
    );
    // systemd's assignment is one line: a raw line break would end it, an unclosed quote and all.
    let systemd = sh.lines().find(|l| l.contains("DefaultEnvironment=")).expect("the systemd assignment");
    assert!(
        systemd.ends_with("'\"A=x\" \"NOTE=line1\\nB=injected\"' > /etc/systemd/system.conf.d/90-isoloom-environment.conf"),
        "{sh}"
    );
}

#[test]
fn each_tmpfs_is_in_fstab_and_mounted_and_a_tmpfs_already_takes_its_size_and_mode() {
    let spec = testing::spec(
        r#"
version: 1
name: s
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web:
    networks: { lan: 10 }
    os: debian@12
    tmpfs: [/srv/cache, { path: /run, size: 64m, mode: "1777" }, { path: /srv/t, mode: "0700" }]
"#,
    );
    let sh = linux(&spec.machines["web"]).expect("the tmpfs mounts are settings");
    // Run the mounts against a copy of fstab and of the mount table, with `mount` recorded.
    let Some(mut posix) = crate::test_support::posix_sh() else { return };
    let dir = tempfile::tempdir().expect("a folder");
    let (fstab, mounts, calls, root) = (
        dir.path().join("fstab"),
        dir.path().join("mounts"),
        dir.path().join("calls"),
        dir.path().join("root"),
    );
    std::fs::write(&fstab, "UUID=1 / ext4 defaults 0 1\ntmpfs /srv/cache tmpfs size=1m 0 0\n").expect("fstab");
    std::fs::write(&mounts, "/dev/sda1 / ext4 rw 0 0\ntmpfs /run tmpfs rw 0 0\n").expect("the mount table");
    let script = sh
        .lines()
        .skip_while(|l| !l.starts_with("p="))
        .take_while(|l| !l.starts_with("systemctl"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("/etc/fstab", &fstab.display().to_string())
        .replace("/proc/mounts", &mounts.display().to_string())
        .replace("mkdir -p \"$p\"", &format!("mkdir -p \"{}$p\"", root.display()));
    let shim = format!("mount() {{ printf '%s\\n' \"$*\" >> {}; }}\n", calls.display());
    let out = posix.arg("-c").arg(format!("set -e\n{shim}{script}\n")).output().expect("sh runs");
    assert!(out.status.success(), "{}\n{script}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        std::fs::read_to_string(&fstab).expect("fstab"),
        "UUID=1 / ext4 defaults 0 1\ntmpfs /srv/cache tmpfs defaults 0 0\ntmpfs /run tmpfs size=64m,mode=1777 0 0\ntmpfs /srv/t tmpfs mode=0700 0 0\n"
    );
    assert_eq!(
        std::fs::read_to_string(&calls).expect("mount calls"),
        "/srv/cache\n-o remount,size=64m,mode=1777 /run\n/srv/t\n"
    );
    assert!(root.join("srv/t").is_dir() && root.join("srv/cache").is_dir());
}

/// `web`'s settings script run with `/etc` and `systemctl` stood in for: `resolved` says whether
/// systemd-resolved is active. Returns the stand-in `/etc`.
fn run_dns(sh: &str, resolved: bool, resolv_conf: &str) -> Option<tempfile::TempDir> {
    let mut posix = crate::test_support::posix_sh()?;
    let dir = tempfile::tempdir().expect("a folder");
    let etc = dir.path().join("etc");
    for sub in ["dhcp", "udhcpc", "systemd/system.conf.d"] {
        std::fs::create_dir_all(etc.join(sub)).expect("a folder");
    }
    std::fs::write(etc.join("resolv.conf"), resolv_conf).expect("resolv.conf");
    std::fs::write(etc.join("dhcp/dhclient.conf"), "request subnet-mask;\nsupersede domain-name-servers 1.1.1.1; # isoloom\n").expect("dhclient.conf");
    let calls = dir.path().join("systemctl");
    let shim = format!(
        "systemctl() {{ printf '%s\\n' \"$*\" >> {calls}; case \"$*\" in 'is-active -q systemd-resolved') {resolved} ;; is-active*) false ;; esac; }}\n",
        calls = calls.display(),
        resolved = if resolved { "true" } else { "false" }
    );
    let script = sh.replace("/etc/", &format!("{}/", etc.display()));
    let out = posix.arg("-c").arg(format!("{shim}{script}")).output().expect("sh runs");
    assert!(out.status.success(), "{}\n{script}", String::from_utf8_lossy(&out.stderr));
    Some(dir)
}

const DNS: &str = r#"
version: 1
name: s
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web:
    networks: { lan: 10 }
    os: debian@12
    dns: { servers: [10.9.0.53, 10.9.0.54], search: [corp.test], domain: lab.test, options: ["ndots:2"] }
  plain: { networks: { lan: 11 }, os: debian@12, dns: { search: [corp.test] } }
  win: { networks: { lan: 13 }, os: windows-server@2022, dns: { servers: [10.9.0.53], search: [corp.test], domain: lab.test } }
"#;

#[test]
fn a_vms_resolver_is_written_where_its_resolver_manager_keeps_it() {
    let spec = testing::spec(DNS);
    let sh = linux(&spec.machines["web"]).expect("the resolver is a setting");
    // A plain resolv.conf, which no lease rewrites.
    let Some(plain) = run_dns(&sh, false, "nameserver 10.0.2.3\n") else { return };
    let read = |f: &str| std::fs::read_to_string(plain.path().join("etc").join(f)).unwrap_or_default();
    assert_eq!(
        read("resolv.conf"),
        "nameserver 10.9.0.53\nnameserver 10.9.0.54\nsearch lab.test corp.test\noptions ndots:2\n"
    );
    assert_eq!(
        read("dhcp/dhclient.conf"),
        "request subnet-mask;\nsupersede domain-name-servers 10.9.0.53, 10.9.0.54; # isoloom\nsupersede domain-search \"lab.test\", \"corp.test\"; # isoloom\n"
    );
    assert_eq!(read("udhcpc/udhcpc.conf"), "RESOLV_CONF=\"no\"\n");
    // systemd-resolved's own configuration, and its stub's resolv.conf for the options.
    let Some(stub) = run_dns(&sh, true, "nameserver 127.0.0.53\n") else { return };
    let in_stub = |f: &str| std::fs::read_to_string(stub.path().join("etc").join(f)).unwrap_or_default();
    assert_eq!(
        in_stub("systemd/resolved.conf.d/90-isoloom.conf"),
        "[Resolve]\nDNS=10.9.0.53 10.9.0.54\nDomains=lab.test corp.test ~.\n"
    );
    assert_eq!(in_stub("resolv.conf"), "nameserver 127.0.0.53\nsearch lab.test corp.test\noptions ndots:2\n");
    assert!(std::fs::read_to_string(stub.path().join("systemctl")).unwrap_or_default().contains("restart systemd-resolved"));
}

#[test]
fn a_resolver_with_no_servers_keeps_the_ones_the_machine_has() {
    let spec = testing::spec(DNS);
    let sh = linux(&spec.machines["plain"]).expect("the resolver is a setting");
    let Some(dir) = run_dns(&sh, false, "nameserver 10.0.2.3\nsearch old.test\n") else { return };
    assert_eq!(
        std::fs::read_to_string(dir.path().join("etc/resolv.conf")).unwrap_or_default(),
        "nameserver 10.0.2.3\nsearch corp.test\n"
    );
}

#[test]
fn windows_asks_its_servers_on_every_interface_with_its_suffixes() {
    let spec = testing::spec(DNS);
    let ps = windows(&spec.machines["win"]).expect("the resolver is a setting");
    assert_eq!(
        ps,
        "Get-NetAdapter | Where-Object Status -eq 'Up' | Set-DnsClientServerAddress -ServerAddresses @('10.9.0.53'); Set-DnsClientGlobalSetting -SuffixSearchList @('lab.test', 'corp.test')"
    );
}
