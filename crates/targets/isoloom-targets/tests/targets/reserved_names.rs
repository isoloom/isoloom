//! What Isoloom names beside a lab's machines (a check runner's script, a machine's published
//! Service) is under its own `isoloom-` prefix, which no machine's name takes, so a machine named
//! like one of them (`exec-web`, `networks`, `web-published`) never meets it.

use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, valid};

/// `web`, publishing a port and running an `exec` check, beside machines named as Isoloom's files
/// and Services were (`exec-web`, `networks`, `web-published`), and checks from the networks.
const NAMED_LIKE_ISOLOOMS: &str = r#"
version: 1
name: col
networks:
  lan: { cidr: 10.70.0.0/24 }
machines:
  web:
    networks: { lan: 10 }
    app: { image: "nginx:1.27-alpine" }
    ports: { http: { port: 80, protocol: http, publish: 8080 } }
  web-published:
    networks: { lan: 11 }
    app: { image: "nginx:1.27-alpine" }
    ports: { http: { port: 80, protocol: http, publish: 8081 } }
  exec-web:
    networks: { lan: 12 }
    app: { image: "nginx:1.27-alpine" }
  networks:
    networks: { lan: 13 }
    app: { image: "nginx:1.27-alpine" }
checks:
  - { from: web, tcp: "exec-web:80" }
  - { from: networks, tcp: "web:80" }
  - { from: web, exec: "true" }
  - { http: "http://web/" }
"#;

#[test]
fn a_check_runners_script_never_takes_a_machines_name() -> Outcome {
    let files = generate(&valid(NAMED_LIKE_ISOLOOMS)?, Target::Docker)?;
    let checks: Vec<&str> = files.iter().filter_map(|f| f.path.strip_prefix(".isoloom/docker/checks/")).collect();
    for runner in ["exec-web.sh", "networks.sh", "isoloom-exec-web.sh", "isoloom-networks.sh", "web.sh"] {
        assert!(checks.contains(&runner), "{runner}: {checks:?}");
    }
    Ok(())
}

#[test]
fn a_machines_published_service_never_takes_another_machines_name() -> Outcome {
    let files = generate(&valid(NAMED_LIKE_ISOLOOMS)?, Target::Kubernetes)?;
    let services: Vec<String> = files
        .iter()
        .filter(|f| f.path == ".isoloom/kubernetes/environment.yaml")
        .flat_map(|f| f.contents.split("\n---").map(str::to_owned).collect::<Vec<_>>())
        .filter(|doc| doc.contains("kind: Service\n"))
        .filter_map(|doc| doc.split("\n  name: ").nth(1).and_then(|rest| rest.lines().next()).map(str::to_owned))
        .collect();
    let mut unique = services.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), services.len(), "a Service named twice: {services:?}");
    for name in ["web", "web-published", "isoloom-web", "isoloom-web-published"] {
        assert!(services.iter().any(|s| s == name), "{name}: {services:?}");
    }
    Ok(())
}
