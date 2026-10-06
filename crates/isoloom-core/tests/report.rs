//! `isoloom graph` and `isoloom report` read the resolved snapshot: the picture and the tables
//! show every network, machine, address and rule the generators use.

use std::path::Path;

use isoloom_core::report::{REPORTS, graph_d2, graph_dot, report};
use isoloom_core::{load, resolved::resolve};

fn snapshot(name: &str) -> serde_json::Value {
    resolve(&load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name)).unwrap())
}

#[test]
fn the_graph_shows_networks_machines_reach_and_the_router() {
    let d2 = graph_d2(&snapshot("segmented"));
    assert!(d2.contains("net_front: \"front\\n10.61.10.0/24\" { shape: hexagon }"), "{d2}");
    assert!(d2.contains("net_back: \"back\\n10.61.20.0/24\\nno internet\""), "{d2}");
    assert!(d2.contains("m_user: \"user\" { shape: person }"), "{d2}");
    assert!(d2.contains("m_web -- net_front: \".10\""), "{d2}");
    assert!(d2.contains("net_front -> net_back: \"6379\" { style.stroke-dash: 3 }"), "{d2}");
    assert!(d2.contains("router -- net_access"), "{d2}");
    // Graphviz says the same.
    let dot = graph_dot(&snapshot("edge-firewall"));
    assert!(dot.starts_with("// edge-firewall") && dot.trim_end().ends_with('}'));
    assert!(dot.contains("m_fw [label=\"fw\\n:8080\", shape=diamond]"), "{dot}");
    assert!(dot.contains("net_outside -> net_dmz [label=\"80\", style=dashed"), "{dot}");
    assert!(!dot.contains("router ["), "no router of Isoloom's: fw is the gateway\n{dot}");
}

#[test]
fn reports_cover_addresses_services_wiring_and_resources() {
    let r = snapshot("segmented");
    let addressing = report("addressing", &r, false).unwrap();
    let header = addressing.lines().next().unwrap();
    assert!(
        header.starts_with("machine") && header.contains("front") && header.contains("access"),
        "{header}"
    );
    assert!(addressing.lines().any(|l| l.starts_with("web") && l.contains("10.61.10.10")), "{addressing}");
    assert!(addressing.contains("(router)"), "{addressing}");
    let services = report("services", &r, true).unwrap();
    assert!(services.starts_with("| machine | port | name | http | published | reachable from |"));
    // The cache answers on 6379 from its own network and from front (reach rule), not from access.
    assert!(services.contains("| cache | 6379 | redis |  |  | front, back |"), "{services}");
    assert!(services.contains("| web | 80 |  | yes |  | front, access |"), "{services}");
    let wiring = report("wiring", &r, false).unwrap();
    assert!(
        wiring.contains("isoloom-segmented-front") && wiring.contains("net.isoloom.com/front=member") && wiring.contains("10.61.10.254"),
        "{wiring}"
    );
    let resources = report("resources", &r, false).unwrap();
    assert!(resources.lines().last().unwrap().starts_with("total"), "{resources}");
    assert!(report("nope", &r, false).unwrap_err().contains("addressing"));
    assert_eq!(REPORTS.len(), 4);
}
