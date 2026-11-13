//! `isoloom graph` and `isoloom report` read the resolved snapshot: the picture and the tables
//! show every network, machine, address and rule the generators use.

use isoloom_targets::graph;
use isoloom_targets::report::{REPORTS, hyperv_ports_note, report};
use isoloom_targets::resolved::resolve;

use crate::testing::{Outcome, example};

/// The resolved snapshot of the example `name`.
fn snapshot(name: &str) -> Outcome<serde_json::Value> {
    Ok(resolve(&example(name)?))
}

#[test]
fn the_graph_shows_networks_machines_reach_and_the_router() -> Outcome {
    let d2 = graph::d2(&snapshot("segmented")?);
    assert!(d2.contains("net_front: \"front\\n10.61.10.0/24\" { shape: hexagon }"), "{d2}");
    assert!(d2.contains("net_back: \"back\\n10.61.20.0/24\\nno internet\""), "{d2}");
    assert!(d2.contains("m_user: \"user\" { shape: person }"), "{d2}");
    assert!(d2.contains("m_web -- net_front: \".10\""), "{d2}");
    assert!(d2.contains("net_front -> net_back: \"6379\" { style.stroke-dash: 3 }"), "{d2}");
    assert!(d2.contains("router -- net_access"), "{d2}");
    // Graphviz says the same.
    let dot = graph::dot(&snapshot("edge-firewall")?);
    assert!(dot.starts_with("// edge-firewall") && dot.trim_end().ends_with('}'));
    assert!(dot.contains("m_fw [label=\"fw\\n:8080\", shape=diamond]"), "{dot}");
    assert!(dot.contains("net_outside -> net_dmz [label=\"80\", style=dashed"), "{dot}");
    assert!(!dot.contains("router ["), "no router of Isoloom's: fw is the gateway\n{dot}");
    Ok(())
}

#[test]
fn the_addressing_report_shows_each_machines_address_on_each_network() -> Outcome {
    let addressing = report("addressing", &snapshot("segmented")?, false)?;
    let header = addressing.lines().next().ok_or("an empty report")?;
    assert!(
        header.starts_with("machine") && header.contains("front") && header.contains("access"),
        "{header}"
    );
    assert!(addressing.lines().any(|l| l.starts_with("web") && l.contains("10.61.10.10")), "{addressing}");
    assert!(addressing.contains("(router)"), "{addressing}");
    Ok(())
}

#[test]
fn the_ports_report_says_from_where_each_service_answers() -> Outcome {
    let services = report("ports", &snapshot("segmented")?, true)?;
    assert!(services.starts_with("| machine | port | name | http | published | reachable from |"));
    // The cache answers on 6379 from its own network and from front (reach rule), not from access.
    assert!(services.contains("| cache | 6379 | redis |  |  | front, back |"), "{services}");
    assert!(services.contains("| web | 80 | web | yes |  | front, access |"), "{services}");
    Ok(())
}

#[test]
fn the_wiring_report_names_the_bridges_their_labels_and_gateways() -> Outcome {
    let wiring = report("wiring", &snapshot("segmented")?, false)?;
    assert!(wiring.contains("isoloom-segmented-front"), "{wiring}");
    assert!(wiring.contains("net.isoloom.com/front=member"), "{wiring}");
    assert!(wiring.contains("10.61.10.254"), "{wiring}");
    Ok(())
}

/// The totals read the snapshot as its writer writes it: an all-zero total is the reader missing
/// the sizes, not a lab of empty machines.
#[test]
fn the_resources_report_totals_every_machines_size() -> Outcome {
    let resources = report("resources", &snapshot("segmented")?, false)?;
    let total: Vec<&str> = resources.lines().last().ok_or("an empty report")?.split_whitespace().collect();
    assert_eq!(total.first(), Some(&"total"), "{resources}");
    let sizes = total.get(1..4).ok_or("cpus, memory and disk")?;
    assert!(sizes.iter().all(|n| n.parse::<u64>().is_ok_and(|n| n > 0)), "{resources}");
    Ok(())
}

#[test]
fn an_unknown_report_names_the_known_ones() -> Outcome {
    let e = report("nope", &snapshot("segmented")?, false).expect_err("no report is named nope");
    assert!(e.to_string().contains("addressing"), "{e}");
    assert_eq!(REPORTS.len(), 4);
    Ok(())
}

/// Hyper-V relays a published TCP port on the host port the run reports: only a UDP one isn't
/// there.
#[test]
fn on_hyperv_only_published_udp_ports_are_said_not_to_be_relayed() -> Outcome {
    let note = hyperv_ports_note(&snapshot("udp-dns")?).expect("udp-dns publishes a UDP port");
    assert!(note.contains("127.0.0.1:10053/udp (dns:53) is not there"), "{note}");
    assert!(note.contains("relays TCP only"), "{note}");
    // A TCP port answers: nothing to say; nor with nothing published.
    assert_eq!(hyperv_ports_note(&snapshot("windows-hello")?), None);
    assert_eq!(hyperv_ports_note(&snapshot("existing-hosts")?), None);
    Ok(())
}
