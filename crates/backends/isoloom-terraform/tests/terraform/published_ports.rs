//! A published port opens on the machine that publishes it and on no other, on every cloud: no
//! shared firewall, security list or group carries it.

use crate::testing::{Outcome, plan};
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule};
use isoloom_test_support::{hcl_body, hcl_literals};

/// `web` publishes 18080 (UDP 18053 too); `db` and `cache`, beside it, publish nothing.
const SPEC: &str = "
version: 1
name: ports
networks:
  lan: { cidr: 10.30.0.0/24 }
machines:
  web:
    networks: { lan: 10 }
    ports: { web: { port: 80, publish: 18080 }, dns: { port: 53, protocol: udp, publish: 18053 } }
    os: debian@12
    app: { steps: [{ run: setup.sh }] }
  db:
    networks: { lan: 11 }
    ports: { db: 5432 }
    os: debian@12
    app: { steps: [{ run: setup.sh }] }
  cache:
    networks: { lan: 12 }
    os: debian@12
    app: { steps: [{ run: setup.sh }] }
";

/// Whether `literal` (a value of the module) is port `port`, alone or as a range's end
/// (`"18080"`, `18080`, `"18080-18080"`).
fn is_port(literal: &str, port: &str) -> bool {
    literal.split('-').any(|end| end == port)
}

#[test]
fn a_published_port_opens_on_its_machine_only() -> Outcome {
    let env = plan(SPEC, Target::CloudVm)?;
    let mut checked = 0;
    for cloud in Cloud::ALL {
        let backend = CloudVmModule::new(cloud);
        if backend.refusal(&env).is_some() {
            continue;
        }
        let module = hcl_body(&backend.render(&env)?.contents);
        let resources: Vec<(String, Vec<String>)> = module
            .blocks()
            .filter(|b| b.identifier() == "resource")
            .map(|b| (b.labels().get(1).map(|l| l.as_str().to_owned()).unwrap_or_default(), hcl_literals(b.body())))
            .collect();
        for port in ["18080", "18053"] {
            let opening: Vec<&str> = resources
                .iter()
                .filter(|(_, values)| values.iter().any(|v| is_port(v, port)))
                .map(|(name, _)| name.as_str())
                .collect();
            assert!(!opening.is_empty(), "{cloud}: {port} isn't published at all");
            for name in opening {
                assert!(
                    name == "web" || name.starts_with("web_"),
                    "{cloud}: `{name}` opens {port}, which only `web` publishes"
                );
            }
        }
        checked += 1;
    }
    assert!(checked >= 5, "every cloud taking several machines is checked ({checked})");
    Ok(())
}
