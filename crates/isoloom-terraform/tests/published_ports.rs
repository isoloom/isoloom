//! A published port opens on the machine that publishes it and on no other, on every cloud: no
//! shared firewall, security list or group carries it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use isoloom_plan::Environment;
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_vm::CLOUDS;

/// `web` publishes 18080 (UDP 18053 too); `db` and `cache`, beside it, publish nothing.
const SPEC: &str = "
version: 1
name: ports
networks:
  lan: { cidr: 10.30.0.0/24 }
machines:
  web:
    networks: { lan: 10 }
    services: [{ port: 80, publish: 18080 }, { port: 53, protocol: udp, publish: 18053 }]
    vm: { os: debian-12, provision: [setup.sh] }
  db:
    networks: { lan: 11 }
    services: [{ port: 5432 }]
    vm: { os: debian-12, provision: [setup.sh] }
  cache:
    networks: { lan: 12 }
    vm: { os: debian-12, provision: [setup.sh] }
";

/// Each `resource "<type>" "<name>"` block of a module: its name, and its text.
fn resources(module: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in module.lines() {
        if let Some(rest) = line.strip_prefix("resource \"") {
            let name = rest.split('"').nth(2).unwrap_or_default().to_string();
            out.push((name, String::new()));
        } else if !line.starts_with(|c: char| c.is_ascii_alphabetic()) {
            if let Some((_, body)) = out.last_mut() {
                body.push_str(line);
                body.push('\n');
            }
        } else {
            // Another top-level block (a variable, an output, locals): not a resource's.
            out.push((String::new(), String::new()));
        }
    }
    out
}

#[test]
fn a_published_port_opens_on_its_machine_only() {
    let spec = isoloom_spec::parse(SPEC).unwrap();
    assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
    let env = Environment::build(&spec, Target::CloudVm, None);
    let mut checked = 0;
    for (cloud, backend) in CLOUDS {
        if backend.refusal(&env).is_some() {
            continue;
        }
        let module = &backend.render(&env).unwrap()[0].contents;
        for port in ["18080", "18053"] {
            assert!(module.contains(port), "{cloud}: {port} isn't published at all");
            for (name, body) in resources(module).into_iter().filter(|(n, _)| !n.is_empty()) {
                let own = name == "web" || name.starts_with("web_");
                assert!(own || !body.contains(port), "{cloud}: `{name}` opens {port}, which only `web` publishes");
            }
        }
        checked += 1;
    }
    assert!(checked >= 5, "every cloud taking several machines is checked ({checked})");
}
