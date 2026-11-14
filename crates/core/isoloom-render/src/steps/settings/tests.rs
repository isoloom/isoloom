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
