//! Cables: point-to-point networks and their mistakes.

use isoloom_topology::{Error, compile};

use crate::testing::{Outcome, accepted, cable, ep, octet, spec, three_node, yaml};

#[test]
fn each_cable_is_a_point_to_point_network() -> Outcome {
    let c = accepted(&three_node()?)?;
    let nets = &spec(&c).networks;
    assert_eq!(nets.len(), 2);
    assert_eq!(nets["link-0"].cidr, "10.200.0.0/29");
    assert_eq!(nets["link-1"].cidr, "10.200.0.8/29");
    assert!(!nets["link-0"].internet);
    Ok(())
}

#[test]
fn the_router_gets_one_address_per_cable() -> Outcome {
    let c = accepted(&three_node()?)?;
    assert_eq!(octet(&c, "router", "link-0")?, 3);
    assert_eq!(octet(&c, "router", "link-1")?, 10);
    assert_eq!(octet(&c, "client", "link-0")?, 2);
    assert_eq!(octet(&c, "server", "link-1")?, 11);
    Ok(())
}

#[test]
fn a_port_carries_one_cable() -> Outcome {
    let mut topo = three_node()?;
    topo.links.push(cable(ep("client", "eth0"), ep("server", "eth1")));
    let e = compile(&topo).unwrap_err();
    assert!(matches!(&e, Error::PortInUse { node, port } if node == "client" && port == "eth0"), "{e}");
    assert_eq!(e.to_string(), "port `eth0` of `client` already has a cable");
    Ok(())
}

#[test]
fn a_cable_needs_two_known_nodes() -> Outcome {
    let mut topo = three_node()?;
    topo.links.push(cable(ep("client", "eth1"), ep("ghost", "eth0")));
    let e = compile(&topo).unwrap_err();
    assert_eq!(e.to_string(), "link 2 refers to node `ghost`, which the project doesn't have");
    Ok(())
}

#[test]
fn a_node_can_not_be_cabled_to_itself() -> Outcome {
    let mut topo = three_node()?;
    topo.links = vec![cable(ep("router", "eth0"), ep("router", "eth1"))];
    let e = compile(&topo).unwrap_err();
    assert!(matches!(&e, Error::SelfLink { link: 0, node } if node == "router"), "{e}");
    assert_eq!(e.to_string(), "link 0 cables `router` to itself");
    topo.links = vec![cable(ep("router", "eth0"), ep("router", "eth0"))];
    let one_port = compile(&topo).unwrap_err();
    assert!(matches!(&one_port, Error::SelfLink { link: 0, .. }), "a port cabled to itself: {one_port}");
    Ok(())
}

#[test]
fn many_cables_get_distinct_blocks() -> Outcome {
    let mut topo = three_node()?;
    topo.nodes.shift_remove("router");
    topo.links.clear();
    for i in 0..100 {
        topo.links.push(cable(ep("client", &format!("p{i}")), ep("server", &format!("p{i}"))));
    }
    let c = accepted(&topo)?;
    assert_eq!(spec(&c).networks["link-31"].cidr, "10.200.0.248/29");
    assert_eq!(spec(&c).networks["link-32"].cidr, "10.200.1.0/29");
    assert_eq!(spec(&c).networks["link-99"].cidr, "10.200.3.24/29");
    Ok(())
}

#[test]
fn more_cables_than_the_block_holds_are_refused() -> Outcome {
    let mut topo = three_node()?;
    topo.links = (0..=256 * 32)
        .map(|i| cable(ep("client", &format!("p{i}")), ep("server", &format!("p{i}"))))
        .collect();
    let e = compile(&topo).unwrap_err();
    assert!(matches!(e, Error::TooManyLinks(8193)), "{e}");
    assert_eq!(e.to_string(), "8193 cables don't fit in 10.200.0.0/16");
    Ok(())
}

#[test]
fn a_cable_between_two_automatic_routers_carries_nothing() -> Outcome {
    let mut topo = three_node()?;
    topo.nodes["client"].kind = isoloom_topology::Kind::Router {};
    topo.nodes["router"].kind = isoloom_topology::Kind::Router {};
    let c = compile(&topo)?;
    assert!(!spec(&c).networks.contains_key("link-0"), "{}", yaml(&c));
    assert_eq!(spec(&c).networks["link-1"].cidr, "10.200.0.8/29");
    Ok(())
}

#[test]
fn a_cable_keeps_its_conditions() -> Outcome {
    let mut topo = three_node()?;
    topo.links[0].tc = serde_json::from_str(r#"{ "delay": "40ms", "loss": 0.5 }"#)?;
    let c = accepted(&topo)?;
    assert!(yaml(&c).contains("tc:\n      delay: 40ms\n      loss: 0.5"), "{}", yaml(&c));
    Ok(())
}
