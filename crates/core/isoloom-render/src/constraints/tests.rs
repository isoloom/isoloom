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
    assert!(sh.starts_with("set -e\n# The proxy, for every login shell"), "{sh}");
    assert!(sh.contains("Defaults env_keep += "), "{sh}");
    assert!(
        sh.contains("printf %s 'Acquire::http::Proxy \"http://p:3128\";\nAcquire::https::Proxy \"http://p:3128\";\n' > /etc/apt/apt.conf.d/95isoloom-proxy"),
        "{sh}"
    );
    assert!(sh.contains("printf 'proxy=%s\\n' 'http://p:3128' >> \"$f\""), "{sh}");
    assert!(
        sh.contains("Environment=\"HTTP_PROXY=http://p:3128\"\n") && !sh.contains("Environment=\"http_proxy"),
        "{sh}"
    );
    assert!(
        sh.contains("DefaultEnvironment=%s\\n' '\"HTTP_PROXY=http://p:3128\" \"HTTPS_PROXY=http://p:3128\" \"NO_PROXY="),
        "{sh}"
    );
    assert!(!sh.contains("chrony") && !sh.contains("mirror"), "{sh}");
}

#[test]
fn a_vm_takes_its_packages_from_the_mirrors_and_its_time_from_the_site() {
    let sh = linux_prelude(constraints(
        "constraints: { mirrors: { debian: \"http://m/debian/\", ubuntu: \"http://m/ubuntu\", pypi: \"https://pypi.corp\", npm: \"https://npm.corp\" }, ntp: [a.corp, b.corp] }\n",
    ).as_ref())
    .expect("mirrors and time servers");
    assert!(!sh.contains("proxy"), "{sh}");
    assert!(
        sh.contains("debian\\.org/debian/?([[:space:]]|$)#http://m/debian/\\2#g"),
        "a trailing slash is dropped: {sh}"
    );
    assert!(sh.contains("ubuntu\\.com/ubuntu/?([[:space:]]|$)#http://m/ubuntu/\\3#g"), "{sh}");
    assert!(
        sh.contains("printf '[global]\\nindex-url = %s\\n' 'https://pypi.corp' > /etc/pip.conf\n"),
        "{sh}"
    );
    assert!(sh.contains("printf 'registry=%s\\n' 'https://npm.corp' > /etc/npmrc\n"), "{sh}");
    assert!(
        sh.contains("printf '%s\\n' 'server a.corp iburst' 'server b.corp iburst' > /etc/chrony/sources.d/isoloom.sources"),
        "{sh}"
    );
    assert!(sh.ends_with("printf '[Time]\\nNTP=%s\\n' 'a.corp b.corp' > /etc/systemd/timesyncd.conf.d/isoloom.conf && (systemctl restart systemd-timesyncd 2>/dev/null || true); fi\n"), "{sh}");
}
