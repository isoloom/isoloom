//! What the network an environment runs in imposes (see [`crate::model::Constraints`]), as
//! each target needs it:
//!
//! - images: every `image:` of the Compose file and the Kubernetes manifests, and the `FROM`
//!   of the Dockerfiles Isoloom writes, go through the registry mirrors ([`rewrite_image`], as each
//!   backend writes them);
//! - containers: the machines (never Isoloom's own checks or router) get the proxy and the
//!   package mirrors as environment variables ([`machine_env`]), their builds the proxy as
//!   build arguments ([`build_args`]);
//! - Linux VMs: a shell preamble run as root before anything else ([`linux_prelude`]) sets the
//!   proxy for the shell, apt, dnf and Docker, rewrites the apt sources to the mirrors, writes
//!   pip's and npm's configuration and the time servers.

use std::fmt::Write as _;

use crate::model::{Constraints, Spec};
use crate::shell::quote;

/// The spec's constraints, when it has any that change something.
pub fn of(spec: &Spec) -> Option<&Constraints> {
    spec.constraints
        .as_ref()
        .filter(|c| c.proxy.is_some() || !c.registries.is_empty() || c.mirrors.is_some() || !c.ntp.is_empty())
}

/// The proxy URLs, `https` defaulting to `http`.
fn proxies(c: &Constraints) -> Option<(Option<&str>, Option<&str>)> {
    let p = c.proxy.as_ref()?;
    let http = p.http.as_deref();
    let https = p.https.as_deref().or(http);
    (http.is_some() || https.is_some()).then_some((http, https))
}

/// What is reached without the proxy: localhost, the environment's machine names and network
/// blocks (as written, and Docker's), then the spec's own list.
pub fn no_proxy(spec: &Spec) -> Vec<String> {
    let mut out: Vec<String> = ["localhost", "127.0.0.1", "::1"].iter().map(std::string::ToString::to_string).collect();
    for (name, m) in &spec.machines {
        out.push(name.clone());
        out.extend(m.aliases.iter().cloned());
    }
    out.extend(spec.networks.values().map(|n| n.cidr.clone()));
    out.extend(
        crate::validate::docker_cidrs(spec)
            .into_iter()
            .map(|(_, c)| format!("{}/{}", std::net::Ipv4Addr::from(c.base), c.len)),
    );
    if let Some(p) = spec.constraints.as_ref().and_then(|c| c.proxy.as_ref()) {
        out.extend(p.no_proxy.iter().cloned());
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|x| seen.insert(x.clone()));
    out
}

/// The proxy as `NAME=value` pairs, both cases (tools read one or the other).
fn proxy_env(spec: &Spec, c: &Constraints) -> Vec<(String, String)> {
    let Some((http, https)) = proxies(c) else { return vec![] };
    let mut out = Vec::new();
    if let Some(h) = http {
        out.push(("HTTP_PROXY".into(), h.to_string()));
        out.push(("http_proxy".into(), h.to_string()));
    }
    if let Some(h) = https {
        out.push(("HTTPS_PROXY".into(), h.to_string()));
        out.push(("https_proxy".into(), h.to_string()));
    }
    let np = no_proxy(spec).join(",");
    out.push(("NO_PROXY".into(), np.clone()));
    out.push(("no_proxy".into(), np));
    out
}

/// The environment a machine's container gets: the proxy, and the package mirrors tools read
/// from the environment.
pub fn machine_env(spec: &Spec) -> Vec<(String, String)> {
    let Some(c) = of(spec) else { return vec![] };
    let mut out = proxy_env(spec, c);
    if let Some(m) = &c.mirrors {
        if let Some(u) = &m.pypi {
            out.push(("PIP_INDEX_URL".into(), u.clone()));
        }
        if let Some(u) = &m.npm {
            out.push(("NPM_CONFIG_REGISTRY".into(), u.clone()));
        }
    }
    out
}

/// The build arguments a machine's image build gets: the proxy (Docker's predefined `ARG`s,
/// so a Dockerfile needs no change), and the mirrors as the machine's environment has them.
pub fn build_args(spec: &Spec) -> Vec<(String, String)> {
    machine_env(spec)
}

pub use isoloom_plan::images::rewrite_image;

/// The shell (run as root, idempotent) that applies the constraints on a Linux VM, before its
/// own set-up and steps. `None` when the spec has none.
pub fn linux_prelude(spec: &Spec) -> Option<String> {
    let c = of(spec)?;
    let mut sh = String::from("set -e\n");
    let env = proxy_env(spec, c);
    if !env.is_empty() {
        let lines: String = env.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        let exports: String = env.iter().map(|(k, v)| format!("export {k}={}\n", quote(v))).collect();
        let _ = write!(
            sh,
            "# The proxy, for every login shell, systemd's services and sudo.\n\
             grep -v -i -E '^(http|https|no)_proxy=' /etc/environment > /etc/environment.isoloom 2>/dev/null || true\n\
             printf %s {} >> /etc/environment.isoloom && mv /etc/environment.isoloom /etc/environment\n\
             printf %s {} > /etc/profile.d/isoloom-proxy.sh\n\
             mkdir -p /etc/systemd/system.conf.d && printf '[Manager]\\nDefaultEnvironment=%s\\n' {} > /etc/systemd/system.conf.d/isoloom-proxy.conf\n\
             if [ -d /etc/sudoers.d ]; then printf 'Defaults env_keep += \"http_proxy https_proxy no_proxy HTTP_PROXY HTTPS_PROXY NO_PROXY\"\\n' > /etc/sudoers.d/isoloom-proxy && chmod 0440 /etc/sudoers.d/isoloom-proxy; fi\n",
            quote(&lines),
            quote(&exports),
            quote(
                &env.iter()
                    .filter(|(k, _)| k.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_'))
                    .map(|(k, v)| format!("\"{k}={v}\""))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        );
        let (http, https) = proxies(c).unwrap_or((None, None));
        let mut apt = String::new();
        if let Some(h) = http {
            let _ = writeln!(apt, "Acquire::http::Proxy \"{h}\";");
        }
        if let Some(h) = https {
            let _ = writeln!(apt, "Acquire::https::Proxy \"{h}\";");
        }
        let _ = writeln!(sh, "# apt, dnf/yum and Docker's daemon through it.");
        let _ = writeln!(
            sh,
            "if [ -d /etc/apt/apt.conf.d ]; then printf %s {} > /etc/apt/apt.conf.d/95isoloom-proxy; fi",
            quote(&apt)
        );
        if let Some(h) = http.or(https) {
            let _ = writeln!(
                sh,
                "for f in /etc/dnf/dnf.conf /etc/yum.conf; do if [ -f \"$f\" ]; then sed -i '/^proxy=/d' \"$f\" && printf 'proxy=%s\\n' {} >> \"$f\"; fi; done",
                quote(h)
            );
        }
        let docker: String = env
            .iter()
            .filter(|(k, _)| k.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_'))
            .map(|(k, v)| format!("Environment=\"{k}={v}\"\n"))
            .collect();
        let _ = writeln!(
            sh,
            "mkdir -p /etc/systemd/system/docker.service.d && printf '[Service]\\n%s' {} > /etc/systemd/system/docker.service.d/isoloom-proxy.conf",
            quote(&docker)
        );
        let _ = writeln!(
            sh,
            "systemctl daemon-reload 2>/dev/null || true; if systemctl is-active --quiet docker 2>/dev/null; then systemctl restart docker; fi"
        );
    }
    if let Some(m) = &c.mirrors {
        let sources = "/etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources";
        if let Some(u) = &m.debian {
            let u = u.trim_end_matches('/');
            let _ = writeln!(sh, "# Debian's packages from the mirror.");
            let _ = writeln!(
                sh,
                ". /etc/os-release; if [ \"$ID\" = debian ]; then for f in {sources}; do if [ -f \"$f\" ]; then sed -i -E 's#https?://(deb|httpredir|ftp)\\.debian\\.org/debian/?([[:space:]]|$)#{u}/\\2#g' \"$f\"; fi; done; fi"
            );
        }
        if let Some(u) = &m.ubuntu {
            let u = u.trim_end_matches('/');
            let _ = writeln!(sh, "# Ubuntu's packages from the mirror.");
            let _ = writeln!(
                sh,
                ". /etc/os-release; if [ \"$ID\" = ubuntu ]; then for f in {sources}; do if [ -f \"$f\" ]; then sed -i -E 's#https?://([a-z0-9-]+\\.)?(archive|security)\\.ubuntu\\.com/ubuntu/?([[:space:]]|$)#{u}/\\3#g' \"$f\"; fi; done; fi"
            );
        }
        if let Some(u) = &m.pypi {
            let _ = writeln!(sh, "printf '[global]\\nindex-url = %s\\n' {} > /etc/pip.conf", quote(u));
        }
        if let Some(u) = &m.npm {
            let _ = writeln!(sh, "printf 'registry=%s\\n' {} > /etc/npmrc", quote(u));
        }
    }
    if !c.ntp.is_empty() {
        let servers = c.ntp.join(" ");
        let _ = writeln!(sh, "# Time from the site's servers: chrony when it's there, else systemd-timesyncd.");
        let _ = writeln!(
            sh,
            "if [ -d /etc/chrony ]; then mkdir -p /etc/chrony/conf.d /etc/chrony/sources.d; printf '%s\\n' {} > /etc/chrony/sources.d/isoloom.sources; systemctl restart chrony 2>/dev/null || systemctl restart chronyd 2>/dev/null || true; \
             else mkdir -p /etc/systemd/timesyncd.conf.d && printf '[Time]\\nNTP=%s\\n' {} > /etc/systemd/timesyncd.conf.d/isoloom.conf && (systemctl restart systemd-timesyncd 2>/dev/null || true); fi",
            c.ntp.iter().map(|s| quote(&format!("server {s} iburst"))).collect::<Vec<_>>().join(" "),
            quote(&servers),
        );
    }
    Some(sh)
}
