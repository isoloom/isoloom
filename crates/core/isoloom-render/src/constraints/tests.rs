use isoloom_spec::model::{Constraints, Target};

use super::{linux_prelude, machine_env, of};
use crate::testing;

/// The plan's constraints of an environment with `constraints` appended (YAML, empty for none).
fn constraints(constraints: &str) -> Option<Constraints> {
    testing::env(
        &format!(
            "version: 1\nname: c\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: 10 }}, aliases: [www], os: debian@12 }}\n{constraints}"
        ),
        Target::Vagrant,
    )
    .constraints
}

#[test]
fn constraints_that_change_nothing_are_none() {
    assert!(of(constraints("").as_ref()).is_none());
    assert!(of(constraints("constraints: {}\n").as_ref()).is_none());
    for one in [
        "constraints: { proxy: { http: \"http://p:3128\" } }\n",
        "constraints: { registries: { docker.io: r.corp } }\n",
        "constraints: { mirrors: { pypi: \"https://pypi.corp\" } }\n",
        "constraints: { ntp: [ntp.corp] }\n",
    ] {
        assert!(of(constraints(one).as_ref()).is_some(), "{one}");
    }
    assert_eq!(linux_prelude(constraints("").as_ref()), None);
    assert_eq!(machine_env(constraints("").as_ref()), []);
}

#[test]
fn the_proxy_s_variables_bypass_what_the_plan_reaches_without_it() {
    let env = machine_env(constraints("constraints: { proxy: { http: \"http://p:3128\", no_proxy: [.corp] } }\n").as_ref());
    let no_proxy = env.iter().find(|(k, _)| k == "NO_PROXY").map(|(_, v)| v.as_str());
    assert_eq!(no_proxy, Some("localhost,127.0.0.1,::1,web,www,10.9.0.0/24,.corp"));
}

#[test]
fn https_goes_through_the_http_proxy_unless_it_has_its_own() {
    let s = constraints("constraints: { proxy: { http: \"http://p:3128\" }, mirrors: { pypi: \"https://pypi.corp\", npm: \"https://npm.corp\" } }\n");
    let env = machine_env(s.as_ref());
    assert_eq!(
        env.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "NO_PROXY",
            "no_proxy",
            "PIP_INDEX_URL",
            "NPM_CONFIG_REGISTRY"
        ]
    );
    assert_eq!(env[2].1, "http://p:3128");
    assert_eq!(env[7].1, "https://npm.corp");
    let own = machine_env(constraints("constraints: { proxy: { http: \"http://p:3128\", https: \"http://s:3128\" } }\n").as_ref());
    assert_eq!((own[0].1.as_str(), own[2].1.as_str()), ("http://p:3128", "http://s:3128"));
}

#[test]
fn a_vm_gets_the_proxy_for_its_shell_services_and_package_managers() {
    let sh = linux_prelude(constraints("constraints: { proxy: { http: \"http://p:3128\" } }\n").as_ref()).expect("a proxy");
    let Some(run) = testing::on_etc(
        &sh,
        &["apt/apt.conf.d", "sudoers.d", "profile.d"],
        &[("environment", "PATH=/usr/bin\nhttp_proxy=http://old:3128\n")],
        &["docker"],
    ) else {
        return;
    };
    assert_eq!(
        run.etc("environment"),
        "PATH=/usr/bin\nHTTP_PROXY=http://p:3128\nhttp_proxy=http://p:3128\nHTTPS_PROXY=http://p:3128\nhttps_proxy=http://p:3128\nNO_PROXY=localhost,127.0.0.1,::1,web,www,10.9.0.0/24\nno_proxy=localhost,127.0.0.1,::1,web,www,10.9.0.0/24\n",
        "the old proxy replaced, the rest kept"
    );
    assert_eq!(
        run.etc("profile.d/isoloom-proxy.sh"),
        "export HTTP_PROXY='http://p:3128'\nexport http_proxy='http://p:3128'\nexport HTTPS_PROXY='http://p:3128'\nexport https_proxy='http://p:3128'\nexport NO_PROXY='localhost,127.0.0.1,::1,web,www,10.9.0.0/24'\nexport no_proxy='localhost,127.0.0.1,::1,web,www,10.9.0.0/24'\n"
    );
    assert_eq!(
        run.etc("systemd/system.conf.d/isoloom-proxy.conf"),
        "[Manager]\nDefaultEnvironment=\"HTTP_PROXY=http://p:3128\" \"HTTPS_PROXY=http://p:3128\" \"NO_PROXY=localhost,127.0.0.1,::1,web,www,10.9.0.0/24\"\n",
        "systemd's services, capitals alone"
    );
    assert_eq!(
        run.etc("sudoers.d/isoloom-proxy"),
        "Defaults env_keep += \"http_proxy https_proxy no_proxy HTTP_PROXY HTTPS_PROXY NO_PROXY\"\n"
    );
    assert_eq!(
        run.etc("apt/apt.conf.d/95isoloom-proxy"),
        "Acquire::http::Proxy \"http://p:3128\";\nAcquire::https::Proxy \"http://p:3128\";\n"
    );
    assert_eq!(
        run.etc("systemd/system/docker.service.d/isoloom-proxy.conf"),
        "[Service]\nEnvironment=\"HTTP_PROXY=http://p:3128\"\nEnvironment=\"HTTPS_PROXY=http://p:3128\"\nEnvironment=\"NO_PROXY=localhost,127.0.0.1,::1,web,www,10.9.0.0/24\"\n"
    );
    assert_eq!(
        run.calls(),
        ["systemctl daemon-reload", "systemctl is-active --quiet docker", "systemctl restart docker"],
        "a running Docker daemon restarted with it"
    );
    assert_eq!(run.etc("chrony/sources.d/isoloom.sources"), "", "no time server");
}

#[test]
fn a_vm_takes_its_packages_from_the_mirrors_and_its_time_from_the_site() {
    let sh = linux_prelude(constraints(
        "constraints: { mirrors: { debian: \"http://m/debian/\", ubuntu: \"http://m/ubuntu\", pypi: \"https://pypi.corp\", npm: \"https://npm.corp\" }, ntp: [a.corp, b.corp] }\n",
    ).as_ref())
    .expect("mirrors and time servers");
    // Each distribution's sources rewritten to its mirror, a trailing slash dropped.
    for line in [
        r#". /etc/os-release; if [ "$ID" = debian ]; then for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources; do if [ -f "$f" ]; then sed -i -E 's#https?://(deb|httpredir|ftp)\.debian\.org/debian/?([[:space:]]|$)#http://m/debian/\2#g' "$f"; fi; done; fi"#,
        r#". /etc/os-release; if [ "$ID" = ubuntu ]; then for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources; do if [ -f "$f" ]; then sed -i -E 's#https?://([a-z0-9-]+\.)?(archive|security)\.ubuntu\.com/ubuntu/?([[:space:]]|$)#http://m/ubuntu/\3#g' "$f"; fi; done; fi"#,
    ] {
        assert!(sh.lines().any(|l| l == line), "no `{line}` in {sh}");
    }
    // Neither Debian nor Ubuntu here: the sources are left alone; pip's and npm's files written,
    // and chrony's time servers where chrony is.
    let Some(run) = testing::on_etc(&sh, &["chrony"], &[("os-release", "ID=alpine\n")], &[]) else {
        return;
    };
    assert_eq!(run.etc("pip.conf"), "[global]\nindex-url = https://pypi.corp\n");
    assert_eq!(run.etc("npmrc"), "registry=https://npm.corp\n");
    assert_eq!(run.etc("chrony/sources.d/isoloom.sources"), "server a.corp iburst\nserver b.corp iburst\n");
    assert_eq!(run.calls(), ["systemctl restart chrony"]);
    assert_eq!(run.etc("environment"), "", "no proxy");
    // Without chrony: systemd-timesyncd's.
    let Some(timesyncd) = testing::on_etc(&sh, &[], &[("os-release", "ID=alpine\n")], &[]) else {
        return;
    };
    assert_eq!(timesyncd.etc("systemd/timesyncd.conf.d/isoloom.conf"), "[Time]\nNTP=a.corp b.corp\n");
    assert_eq!(timesyncd.calls(), ["systemctl restart systemd-timesyncd"]);
}
