//! Docker Compose for the Docker targets: the project and file an environment runs under, and
//! the ports it really published.

use std::path::Path;
use std::process::{Command, Stdio};

use isoloom_plan::Environment;
use isoloom_runtime::registry;
use isoloom_spec::Target;
use isoloom_spec::model::Protocol;

use crate::tools::process;

/// A published port: (machine, container port, protocol, host port).
pub type Published = (String, u16, Protocol, u16);

/// The Compose project name the registry recorded for an environment on Docker: set when a
/// tool embedding Isoloom ran its Compose file under its own name.
pub fn compose_project(dir: &Path, instance: Option<u8>) -> Option<String> {
    let reg = registry::load().ok()?;
    reg.for_dir(dir)
        .into_iter()
        .find(|e| e.instance == instance && matches!(e.target, Target::Docker | Target::Hosted))
        .and_then(|e| e.project.clone())
}

/// `[-p <project>] -f <compose file>` for an environment's Compose file, under the project
/// name it runs as (see [`compose_project`]).
pub fn compose_files(dir: &Path, instance: Option<u8>) -> Vec<String> {
    let f = dir
        .join(isoloom_spec::instance::output_dir(instance))
        .join("docker/compose.yml")
        .display()
        .to_string();
    match compose_project(dir, instance) {
        Some(p) => vec!["-p".into(), p, "-f".into(), f],
        None => vec!["-f".into(), f],
    }
}

/// `docker compose --progress quiet <files>`, run from the project folder.
pub fn quiet(dir: &Path, instance: Option<u8>) -> Command {
    let mut c = process::command("docker");
    c.args(["compose", "--progress", "quiet"]).args(compose_files(dir, instance));
    c.current_dir(dir);
    c
}

/// The items of `docker compose ps --format json`: Compose prints one JSON object per line
/// (older versions: one array).
pub fn ps_items(text: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<Vec<serde_json::Value>>(text).unwrap_or_else(|_| text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
}

/// The host ports an environment really got, for its message: local Docker publishes on free
/// ports unless `ISOLOOM_PUBLISH_FIXED` pins them to the spec's; the other targets keep them.
/// Compose is asked only when the environment publishes a port (`publishes`).
pub fn real_ports(dir: &Path, t: Target, instance: Option<u8>, publishes: bool) -> Vec<Published> {
    if t != Target::Docker || !publishes || std::env::var_os("ISOLOOM_PUBLISH_FIXED").is_some_and(|v| !v.is_empty()) {
        return Vec::new();
    }
    docker_published(dir, instance)
}

/// Whether the plan publishes a port of any machine on the host.
pub fn publishes(plan: &Environment) -> bool {
    plan.machines.values().any(|m| !m.ports.is_empty())
}

/// The host ports a local Docker environment really got, as (machine, port, protocol, host
/// port): its
/// Compose file publishes on free loopback ports unless `ISOLOOM_PUBLISH_FIXED` is set, so the
/// spec's `publish:` values aren't where it answers. Empty when Compose can't say.
pub fn docker_published(dir: &Path, instance: Option<u8>) -> Vec<Published> {
    let Ok(out) = process::command("docker")
        .arg("compose")
        .args(compose_files(dir, instance))
        .args(["ps", "--format", "json"])
        .current_dir(dir)
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    published_of(&ps_items(&String::from_utf8_lossy(&out.stdout)))
}

/// (service, container port, protocol, host port) for each port `docker compose ps` lists as
/// published.
fn published_of(items: &[serde_json::Value]) -> Vec<Published> {
    let mut found = Vec::new();
    for i in items {
        let Some(service) = i["Service"].as_str() else { continue };
        for p in i["Publishers"].as_array().into_iter().flatten() {
            let port = p["TargetPort"].as_u64().and_then(|n| u16::try_from(n).ok());
            let host = p["PublishedPort"].as_u64().and_then(|n| u16::try_from(n).ok()).filter(|h| *h != 0);
            let proto = if p["Protocol"].as_str() == Some("udp") {
                Protocol::Udp
            } else {
                Protocol::Tcp
            };
            if let (Some(port), Some(host)) = (port, host)
                && !found.iter().any(|(s, c, pr, _): &Published| s == service && *c == port && *pr == proto)
            {
                found.push((service.to_string(), port, proto, host));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_ports_come_from_compose_ps() {
        use isoloom_spec::model::Protocol;
        let items: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"Service":"web","Publishers":[{"URL":"127.0.0.1","TargetPort":80,"PublishedPort":32772,"Protocol":"tcp"},{"URL":"::1","TargetPort":80,"PublishedPort":32772,"Protocol":"tcp"}]},
                {"Service":"dns","Publishers":[{"URL":"127.0.0.1","TargetPort":53,"PublishedPort":32780,"Protocol":"udp"},{"URL":"127.0.0.1","TargetPort":53,"PublishedPort":32781,"Protocol":"tcp"}]},
                {"Service":"cache","Publishers":[{"URL":"","TargetPort":6379,"PublishedPort":0,"Protocol":"tcp"}]},
                {"Service":"isoloom-check"}]"#,
        )
        .unwrap();
        assert_eq!(
            published_of(&items),
            [
                ("web".to_string(), 80, Protocol::Tcp, 32772),
                ("dns".to_string(), 53, Protocol::Udp, 32780),
                ("dns".to_string(), 53, Protocol::Tcp, 32781)
            ]
        );
    }
}
