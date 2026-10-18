//! `constraints`: URLs where URLs go, registry mirrors as a registry and path, time servers.

use super::Problems;
use crate::model::{Constraints, Spec};

/// Whether `v` is an http(s) URL with something after the scheme and no spaces.
fn url(v: &str) -> bool {
    v.strip_prefix("http://")
        .or_else(|| v.strip_prefix("https://"))
        .is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace))
}

/// The environment's constraints: proxy, registry mirrors, package mirrors and NTP.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    let Some(c) = &spec.constraints else { return };
    proxy(c, p);
    registries(c, p);
    if let Some(m) = &c.mirrors {
        for (k, v) in [("debian", &m.debian), ("ubuntu", &m.ubuntu), ("pypi", &m.pypi), ("npm", &m.npm)] {
            if let Some(v) = v.as_deref().filter(|v| !url(v)) {
                p.add(format!("constraints.mirrors.{k}"), format!("`{v}`: a URL, e.g. http://mirror.corp/{k}"));
            }
        }
    }
    for (i, v) in c.ntp.iter().enumerate() {
        if v.trim().is_empty() || v.contains(char::is_whitespace) {
            p.add(format!("constraints.ntp[{i}]"), format!("`{v}`: a time server's name or address"));
        }
    }
}

/// `constraints.proxy`: an http(s) proxy URL, and `no_proxy` entries one per item.
fn proxy(c: &Constraints, p: &mut Problems) {
    let Some(px) = &c.proxy else { return };
    if px.http.is_none() && px.https.is_none() {
        p.add("constraints.proxy", "give `http` (and `https` when it differs), e.g. http://proxy.corp:3128");
    }
    for (k, v) in [("http", &px.http), ("https", &px.https)] {
        if let Some(v) = v.as_deref().filter(|v| !url(v)) {
            p.add(format!("constraints.proxy.{k}"), format!("`{v}`: a URL, e.g. http://proxy.corp:3128"));
        }
    }
    for (i, v) in px.no_proxy.iter().enumerate() {
        if v.trim().is_empty() || v.contains(char::is_whitespace) || v.contains(',') {
            p.add(
                format!("constraints.proxy.no_proxy[{i}]"),
                format!("`{v}`: one name, domain (.corp) or block per entry"),
            );
        }
    }
}

/// `constraints.registries`: a registry host, mapped to a mirror written as an image prefix.
fn registries(c: &Constraints, p: &mut Problems) {
    for (registry, mirror) in &c.registries {
        if registry.is_empty() || registry.contains('/') || registry.contains("://") {
            p.add(
                format!("constraints.registries.{registry}"),
                "the key is a registry's host, e.g. docker.io or ghcr.io",
            );
        }
        if mirror.contains("://") || mirror.trim().is_empty() || mirror.contains(char::is_whitespace) || mirror.starts_with('/') {
            p.add(
                format!("constraints.registries.{registry}"),
                format!("`{mirror}`: the mirror as an image prefix, host and path without a scheme, e.g. registry.corp/dockerhub"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// The problems of `constraints` written as given.
    fn of(constraints: &str) -> Vec<String> {
        problems(&lab(&format!(
            "machines: {{ web: {{ networks: {{ lan: 10 }}, docker: {{ image: nginx }} }} }}\nconstraints: {constraints}\n"
        )))
    }

    #[test]
    fn sound_constraints_pass() {
        let ok = of(
            "{ proxy: { http: 'http://proxy.corp:3128', https: 'https://p', no_proxy: [.corp, 10.0.0.0/8] }, registries: { docker.io: registry.corp/dockerhub }, mirrors: { debian: 'http://m/debian', npm: 'https://m/npm' }, ntp: [pool.ntp.org] }",
        );
        assert_eq!(ok, Vec::<String>::new());
        assert_eq!(of("{ proxy: { https: 'https://proxy.corp' } }"), Vec::<String>::new());
    }

    #[test]
    fn a_proxy_is_an_http_url() {
        assert_eq!(
            of("{ proxy: { no_proxy: [.corp] } }"),
            ["constraints.proxy: give `http` (and `https` when it differs), e.g. http://proxy.corp:3128"]
        );
        for bad in ["ftp://proxy", "https://", "http://", "http://pro xy"] {
            let why = format!("constraints.proxy.http: `{bad}`: a URL, e.g. http://proxy.corp:3128");
            assert_eq!(of(&format!("{{ proxy: {{ http: '{bad}' }} }}")), [why], "{bad}");
        }
        assert_eq!(
            of("{ proxy: { http: 'http://p' } }"),
            Vec::<String>::new(),
            "one character after the scheme is a host"
        );
    }

    #[test]
    fn no_proxy_holds_one_entry_per_item() {
        for bad in ["' '", "'a b'", "'a,b'"] {
            let entry = bad.trim_matches('\'');
            let why = format!("constraints.proxy.no_proxy[0]: `{entry}`: one name, domain (.corp) or block per entry");
            assert_eq!(of(&format!("{{ proxy: {{ http: 'http://p', no_proxy: [{bad}] }} }}")), [why], "{bad}");
        }
    }

    #[test]
    fn a_registry_maps_a_host_to_an_image_prefix() {
        for bad in ["''", "docker.io/library", "'https://docker.io'"] {
            let host = bad.trim_matches('\'');
            let why = format!("constraints.registries.{host}: the key is a registry's host, e.g. docker.io or ghcr.io");
            assert_eq!(of(&format!("{{ registries: {{ {bad}: registry.corp/x }} }}")), [why], "{bad}");
        }
        for bad in ["'reg://corp/x'", "''", "'reg corp/x'", "/corp/x"] {
            let mirror = bad.trim_matches('\'');
            let why = format!(
                "constraints.registries.docker.io: `{mirror}`: the mirror as an image prefix, host and path without a scheme, e.g. registry.corp/dockerhub"
            );
            assert_eq!(of(&format!("{{ registries: {{ docker.io: {bad} }} }}")), [why], "{bad}");
        }
    }

    #[test]
    fn mirrors_and_time_servers_are_named() {
        assert_eq!(
            of("{ mirrors: { pypi: pypi.corp } }"),
            ["constraints.mirrors.pypi: `pypi.corp`: a URL, e.g. http://mirror.corp/pypi"]
        );
        assert_eq!(
            of("{ mirrors: { ubuntu: 'http://m/u b' } }"),
            ["constraints.mirrors.ubuntu: `http://m/u b`: a URL, e.g. http://mirror.corp/ubuntu"]
        );
        for bad in ["' '", "'ntp a'"] {
            let server = bad.trim_matches('\'');
            assert_eq!(
                of(&format!("{{ ntp: [{bad}] }}")),
                [format!("constraints.ntp[0]: `{server}`: a time server's name or address")],
                "{bad}"
            );
        }
    }
}
