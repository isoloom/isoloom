use super::{build_args, linux_prelude, machine_env, no_proxy, of};
use crate::testing;

/// A lab with `constraints` appended (YAML, empty for none).
fn spec(constraints: &str) -> isoloom_spec::model::Spec {
    testing::spec(&format!(
        "version: 1\nname: c\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: 10 }}, aliases: [www], os: debian@12 }}\n{constraints}"
    ))
}

#[test]
fn constraints_that_change_nothing_are_none() {
    assert!(of(&spec("")).is_none());
    assert!(of(&spec("constraints: {}\n")).is_none());
    for one in [
        "constraints: { proxy: { http: \"http://p:3128\" } }\n",
        "constraints: { registries: { docker.io: r.corp } }\n",
        "constraints: { mirrors: { pypi: \"https://pypi.corp\" } }\n",
        "constraints: { ntp: [ntp.corp] }\n",
    ] {
        assert!(of(&spec(one)).is_some(), "{one}");
    }
    assert_eq!(linux_prelude(&spec("")), None);
    assert_eq!(machine_env(&spec("")), []);
}

#[test]
fn what_bypasses_the_proxy_is_local_then_the_lab_then_the_spec_s_own_each_once() {
    let s = spec("constraints: { proxy: { http: \"http://p:3128\", no_proxy: [.corp, web] } }\n");
    assert_eq!(no_proxy(&s), ["localhost", "127.0.0.1", "::1", "web", "www", "10.9.0.0/24", ".corp"]);
}

#[test]
fn https_goes_through_the_http_proxy_unless_it_has_its_own() {
    let s = spec("constraints: { proxy: { http: \"http://p:3128\" }, mirrors: { pypi: \"https://pypi.corp\", npm: \"https://npm.corp\" } }\n");
    let env = machine_env(&s);
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
    assert_eq!(build_args(&s), env);
    let own = machine_env(&spec("constraints: { proxy: { http: \"http://p:3128\", https: \"http://s:3128\" } }\n"));
    assert_eq!((own[0].1.as_str(), own[2].1.as_str()), ("http://p:3128", "http://s:3128"));
}

#[test]
fn a_vm_gets_the_proxy_for_its_shell_services_and_package_managers() {
    let sh = linux_prelude(&spec("constraints: { proxy: { http: \"http://p:3128\" } }\n")).expect("a proxy");
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
    let sh = linux_prelude(&spec(
        "constraints: { mirrors: { debian: \"http://m/debian/\", ubuntu: \"http://m/ubuntu\", pypi: \"https://pypi.corp\", npm: \"https://npm.corp\" }, ntp: [a.corp, b.corp] }\n",
    ))
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
