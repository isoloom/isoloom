//! What the network an environment runs in imposes (see [`Constraints`]), as each target needs
//! it:
//!
//! - images: every `image:` of the Compose file and the Kubernetes manifests, and the `FROM` of
//!   the Dockerfiles Isoloom writes, go through the registry mirrors ([`rewrite_image`], as each
//!   backend writes them);
//! - containers: the machines (never Isoloom's own checks or router) get the proxy and the
//!   package mirrors as environment variables ([`machine_env`]), their builds the proxy as build
//!   arguments ([`build_args`]);
//! - Linux VMs: a shell preamble run as root before anything else ([`linux_prelude`]) sets the
//!   proxy for the shell, apt, dnf and Docker, rewrites the apt sources to the mirrors, writes
//!   pip's and npm's configuration and the time servers.

use std::collections::HashSet;

pub use isoloom_plan::images::rewrite_image;
use isoloom_spec::model::{Constraints, Mirrors, Spec};
use isoloom_spec::shell::quote;
use isoloom_spec::validate::docker_cidrs;

/// The spec's constraints, when it has any that change something.
#[must_use]
pub fn of(spec: &Spec) -> Option<&Constraints> {
    spec.constraints
        .as_ref()
        .filter(|c| c.proxy.is_some() || !c.registries.is_empty() || c.mirrors.is_some() || !c.ntp.is_empty())
}

/// The proxy's URLs, `https` defaulting to `http`; `None` without a proxy (validation gives
/// every proxy its `http`).
fn proxies(c: &Constraints) -> Option<(&str, &str)> {
    let p = c.proxy.as_ref()?;
    let http = p.http.as_deref()?;
    Some((http, p.https.as_deref().unwrap_or(http)))
}

/// What is reached without the proxy: localhost, the environment's machine names and network
/// blocks (as written, and Docker's), then the spec's own list, each once.
#[must_use]
pub fn no_proxy(spec: &Spec) -> Vec<String> {
    let local = ["localhost", "127.0.0.1", "::1"].map(str::to_owned);
    let names = spec.machines.iter().flat_map(|(name, m)| std::iter::once(name).chain(&m.aliases).cloned());
    let blocks = spec.networks.values().map(|n| n.cidr.clone());
    let docker = docker_cidrs(spec).into_iter().map(|(_, c)| c.to_string());
    let own = spec.constraints.iter().flat_map(|c| &c.proxy).flat_map(|p| p.no_proxy.iter().cloned());
    let mut seen = HashSet::new();
    local
        .into_iter()
        .chain(names)
        .chain(blocks)
        .chain(docker)
        .chain(own)
        .filter(|x| seen.insert(x.clone()))
        .collect()
}

/// The proxy's `(http, https)` URLs as `NAME=value` pairs, both cases (tools read one or the
/// other), and what bypasses it.
fn proxy_env(spec: &Spec, (http, https): (&str, &str)) -> Vec<(String, String)> {
    let np = no_proxy(spec).join(",");
    [
        ("HTTP_PROXY", "http_proxy", http),
        ("HTTPS_PROXY", "https_proxy", https),
        ("NO_PROXY", "no_proxy", np.as_str()),
    ]
    .into_iter()
    .flat_map(|(upper, lower, value)| [(upper.to_owned(), value.to_owned()), (lower.to_owned(), value.to_owned())])
    .collect()
}

/// The environment a machine's container gets: the proxy, and the package mirrors tools read
/// from the environment.
#[must_use]
pub fn machine_env(spec: &Spec) -> Vec<(String, String)> {
    let Some(c) = of(spec) else { return Vec::new() };
    let proxy = proxies(c).map(|urls| proxy_env(spec, urls)).unwrap_or_default();
    let mirrors = c.mirrors.iter().flat_map(|m| {
        [("PIP_INDEX_URL", &m.pypi), ("NPM_CONFIG_REGISTRY", &m.npm)]
            .into_iter()
            .filter_map(|(k, url)| Some((k.to_owned(), url.clone()?)))
    });
    proxy.into_iter().chain(mirrors).collect()
}

/// The build arguments a machine's image build gets: the proxy (Docker's predefined `ARG`s,
/// so a Dockerfile needs no change), and the mirrors as the machine's environment has them.
#[must_use]
pub fn build_args(spec: &Spec) -> Vec<(String, String)> {
    machine_env(spec)
}

/// The shell (run as root, idempotent) that applies the constraints on a Linux VM, before its
/// own set-up and steps. `None` when the spec has none.
#[must_use]
pub fn linux_prelude(spec: &Spec) -> Option<String> {
    let c = of(spec)?;
    let mirrors = c.mirrors.as_ref().map(mirror_lines).unwrap_or_default();
    Some(format!("set -e\n{}{mirrors}{}", proxy(spec, c), ntp(&c.ntp)))
}

/// The mirrors: Debian's and Ubuntu's apt sources rewritten to them, pip's and npm's
/// configuration.
fn mirror_lines(m: &Mirrors) -> String {
    let sources = "/etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources";
    let debian = m.debian.as_deref().map(|u| {
        format!(
            "# Debian's packages from the mirror.\n. /etc/os-release; if [ \"$ID\" = debian ]; then for f in {sources}; do if [ -f \"$f\" ]; then sed -i -E 's#https?://(deb|httpredir|ftp)\\.debian\\.org/debian/?([[:space:]]|$)#{}/\\2#g' \"$f\"; fi; done; fi\n",
            u.trim_end_matches('/')
        )
    });
    let ubuntu = m.ubuntu.as_deref().map(|u| {
        format!(
            "# Ubuntu's packages from the mirror.\n. /etc/os-release; if [ \"$ID\" = ubuntu ]; then for f in {sources}; do if [ -f \"$f\" ]; then sed -i -E 's#https?://([a-z0-9-]+\\.)?(archive|security)\\.ubuntu\\.com/ubuntu/?([[:space:]]|$)#{}/\\3#g' \"$f\"; fi; done; fi\n",
            u.trim_end_matches('/')
        )
    });
    let pypi = m
        .pypi
        .as_deref()
        .map(|u| format!("printf '[global]\\nindex-url = %s\\n' {} > /etc/pip.conf\n", quote(u)));
    let npm = m.npm.as_deref().map(|u| format!("printf 'registry=%s\\n' {} > /etc/npmrc\n", quote(u)));
    [debian, ubuntu, pypi, npm].into_iter().flatten().collect()
}

/// The time servers: chrony's when it's there, else systemd-timesyncd's. Nothing without any.
fn ntp(servers: &[String]) -> String {
    if servers.is_empty() {
        return String::new();
    }
    format!(
        "# Time from the site's servers: chrony when it's there, else systemd-timesyncd.\n\
         if [ -d /etc/chrony ]; then mkdir -p /etc/chrony/conf.d /etc/chrony/sources.d; printf '%s\\n' {} > /etc/chrony/sources.d/isoloom.sources; systemctl restart chrony 2>/dev/null || systemctl restart chronyd 2>/dev/null || true; \
         else mkdir -p /etc/systemd/timesyncd.conf.d && printf '[Time]\\nNTP=%s\\n' {} > /etc/systemd/timesyncd.conf.d/isoloom.conf && (systemctl restart systemd-timesyncd 2>/dev/null || true); fi\n",
        servers.iter().map(|s| quote(&format!("server {s} iburst"))).collect::<Vec<_>>().join(" "),
        quote(&servers.join(" ")),
    )
}

/// Whether a variable is spelled in capitals (`HTTP_PROXY`): the case systemd and Docker read.
fn upper(k: &str) -> bool {
    k.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_')
}

/// The proxy, when the constraints set one: for every login shell, systemd's services, sudo,
/// apt, dnf/yum and Docker's daemon. Nothing without one.
fn proxy(spec: &Spec, c: &Constraints) -> String {
    let Some(urls) = proxies(c) else { return String::new() };
    let env = proxy_env(spec, urls);
    let lines: String = env.iter().map(|(k, v)| format!("{k}={v}\n")).collect::<Vec<_>>().concat();
    let exports: String = env.iter().map(|(k, v)| format!("export {k}={}\n", quote(v))).collect::<Vec<_>>().concat();
    let systemd = env
        .iter()
        .filter(|(k, _)| upper(k))
        .map(|(k, v)| format!("\"{k}={v}\""))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "# The proxy, for every login shell, systemd's services and sudo.\n\
         grep -v -i -E '^(http|https|no)_proxy=' /etc/environment > /etc/environment.isoloom 2>/dev/null || true\n\
         printf %s {} >> /etc/environment.isoloom && mv /etc/environment.isoloom /etc/environment\n\
         printf %s {} > /etc/profile.d/isoloom-proxy.sh\n\
         mkdir -p /etc/systemd/system.conf.d && printf '[Manager]\\nDefaultEnvironment=%s\\n' {} > /etc/systemd/system.conf.d/isoloom-proxy.conf\n\
         if [ -d /etc/sudoers.d ]; then printf 'Defaults env_keep += \"http_proxy https_proxy no_proxy HTTP_PROXY HTTPS_PROXY NO_PROXY\"\\n' > /etc/sudoers.d/isoloom-proxy && chmod 0440 /etc/sudoers.d/isoloom-proxy; fi\n{}",
        quote(&lines),
        quote(&exports),
        quote(&systemd),
        package_proxies(urls, &env),
    )
}

/// The proxy's `(http, https)` URLs for apt, dnf/yum and Docker's daemon (`env`: the proxy's
/// variables).
fn package_proxies((http, https): (&str, &str), env: &[(String, String)]) -> String {
    let apt = format!("Acquire::http::Proxy \"{http}\";\nAcquire::https::Proxy \"{https}\";\n");
    let docker: String = env
        .iter()
        .filter(|(k, _)| upper(k))
        .map(|(k, v)| format!("Environment=\"{k}={v}\"\n"))
        .collect::<Vec<_>>()
        .concat();
    format!(
        "# apt, dnf/yum and Docker's daemon through it.\n\
         if [ -d /etc/apt/apt.conf.d ]; then printf %s {} > /etc/apt/apt.conf.d/95isoloom-proxy; fi\n\
         for f in /etc/dnf/dnf.conf /etc/yum.conf; do if [ -f \"$f\" ]; then sed -i '/^proxy=/d' \"$f\" && printf 'proxy=%s\\n' {} >> \"$f\"; fi; done\n\
         mkdir -p /etc/systemd/system/docker.service.d && printf '[Service]\\n%s' {} > /etc/systemd/system/docker.service.d/isoloom-proxy.conf\n\
         systemctl daemon-reload 2>/dev/null || true; if systemctl is-active --quiet docker 2>/dev/null; then systemctl restart docker; fi\n",
        quote(&apt),
        quote(http),
        quote(&docker),
    )
}

#[cfg(test)]
mod tests;
