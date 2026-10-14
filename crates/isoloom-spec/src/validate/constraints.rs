//! `constraints`: URLs where URLs go, registry mirrors as a registry and path, time servers.

use super::Problems;
use crate::model::{Constraints, Spec};

/// Whether `v` is an http(s) URL with something after the scheme and no spaces.
fn url(v: &str) -> bool {
    (v.starts_with("http://") || v.starts_with("https://")) && v.len() > "https://".len() && !v.contains(char::is_whitespace)
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
