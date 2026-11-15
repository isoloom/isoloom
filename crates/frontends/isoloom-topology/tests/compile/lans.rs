//! LANs: addresses, mistakes, automatic routers.

use isoloom_topology::{Error, Kind, compile};

use crate::testing::{Outcome, accepted, cable, container, ep, join, lan, node, octet, spec, two_lans, yaml};

#[test]
fn machines_on_a_lan_take_its_free_addresses() -> Outcome {
    let mut topo = two_lans()?;
    join(&mut topo, "router", "office", Some(10))?;
    let c = accepted(&topo)?;
    assert_eq!(spec(&c).networks["office"].cidr, "10.10.1.0/24");
    // The router asked for .10 on office, so the client takes the next one.
    assert_eq!(octet(&c, "router", "office")?, 10);
    assert_eq!(octet(&c, "client", "office")?, 11);
    assert_eq!(octet(&c, "router", "servers")?, 10);
    assert_eq!(octet(&c, "server", "servers")?, 11);
    Ok(())
}

#[test]
fn a_small_lan_starts_low_and_fills_up() -> Outcome {
    let mut topo = two_lans()?;
    topo.nodes["router"].networks.shift_remove("office");
    topo.networks["office"].cidr = "10.10.1.0/29".into();
    join(&mut topo, "server", "office", None)?;
    join(&mut topo, "router", "office", None)?;
    let c = accepted(&topo)?;
    // A /29 has .2 to .4 for machines.
    assert_eq!(octet(&c, "client", "office")?, 2);
    assert_eq!(octet(&c, "router", "office")?, 3);
    assert_eq!(octet(&c, "server", "office")?, 4);

    topo.nodes.insert("extra".into(), node(container("alpine:3.20", true)));
    join(&mut topo, "extra", "office", None)?;
    let full = compile(&topo).unwrap_err();
    assert!(
        matches!(&full, Error::LanFull { lan, cidr } if lan == "office" && cidr == "10.10.1.0/29"),
        "{full}"
    );
    assert_eq!(full.to_string(), "LAN `office` (10.10.1.0/29) has no free address left; use a larger block");
    Ok(())
}

#[test]
fn a_block_not_at_the_start_of_its_octet_gives_its_own_addresses() -> Outcome {
    let mut topo = two_lans()?;
    topo.nodes["router"].networks.shift_remove("office");
    topo.networks["office"].cidr = "10.10.1.8/29".into();
    topo.networks["servers"].cidr = "10.10.2.128/25".into();
    join(&mut topo, "router", "office", None)?;
    let c = accepted(&topo)?;
    assert_eq!((octet(&c, "client", "office")?, octet(&c, "router", "office")?), (10, 11));
    assert_eq!((octet(&c, "router", "servers")?, octet(&c, "server", "servers")?), (130, 131));
    Ok(())
}

#[test]
fn a_block_isoloom_gives_no_address_on_is_refused() -> Outcome {
    for cidr in ["10.10.0.0/16", "10.10.1.0/30", "10.10.1.5/24"] {
        let mut topo = two_lans()?;
        topo.networks["office"].cidr = cidr.into();
        let e = compile(&topo).unwrap_err();
        assert!(matches!(&e, Error::BadCidr { lan, .. } if lan == "office"), "{cidr}: {e}");
    }
    Ok(())
}

#[test]
fn a_node_on_a_lan_the_project_has_not_is_refused() -> Outcome {
    let mut topo = two_lans()?;
    join(&mut topo, "client", "dmz", None)?;
    assert_eq!(
        compile(&topo).unwrap_err().to_string(),
        "`client` is on LAN `dmz`, which the project doesn't have"
    );
    Ok(())
}

#[test]
fn two_machines_asking_for_one_address_are_refused() -> Outcome {
    let mut topo = two_lans()?;
    join(&mut topo, "client", "office", Some(20))?;
    let taken = compile(&topo).unwrap_err();
    assert!(
        matches!(&taken, Error::AddressTaken { octet: 20, first, second, .. } if first == "client" && second == "router"),
        "{taken}"
    );
    assert_eq!(taken.to_string(), "on LAN `office`, `client` and `router` both ask for .20");
    Ok(())
}

#[test]
fn an_address_isoloom_gives_no_machine_is_refused() -> Outcome {
    // Regression: an asked-for address was taken as it was, and Isoloom refused the spec.
    let mut topo = two_lans()?;
    topo.nodes["router"].networks.shift_remove("office");
    topo.networks["office"].cidr = "10.10.1.8/29".into();
    // Outside the block, its network, its gateway, its controller, its router, its broadcast.
    for asked in [0, 1, 200, 255, 8, 9, 13, 14, 15] {
        join(&mut topo, "client", "office", Some(asked))?;
        let e = compile(&topo).unwrap_err();
        assert!(
            matches!(&e, Error::AddressOutside { lan, node, octet, cidr } if lan == "office" && node == "client" && *octet == asked && cidr == "10.10.1.8/29"),
            ".{asked}: {e}"
        );
    }
    assert_eq!(
        compile(&topo).unwrap_err().to_string(),
        "on LAN `office` (10.10.1.8/29), `client` asks for .15, which no machine can take there"
    );
    for asked in [10, 12] {
        join(&mut topo, "client", "office", Some(asked))?;
        assert_eq!(octet(&accepted(&topo)?, "client", "office")?, asked);
    }
    Ok(())
}

#[test]
fn a_machine_may_ask_for_an_address_below_the_first_a_24_gives() -> Outcome {
    let mut topo = two_lans()?;
    join(&mut topo, "client", "office", Some(2))?;
    assert_eq!(octet(&accepted(&topo)?, "client", "office")?, 2);
    Ok(())
}

#[test]
fn a_lan_named_like_a_cable_is_refused() -> Outcome {
    let mut topo = two_lans()?;
    topo.networks.insert("link-0".into(), lan("10.10.9.0/24"));
    let e = compile(&topo).unwrap_err();
    assert!(matches!(&e, Error::ReservedName(n) if n == "link-0"), "{e}");
    assert_eq!(e.to_string(), "`link-0` starts with `link-`, which the cables' networks take; rename it");
    Ok(())
}

#[test]
fn a_lan_whose_block_is_not_one_is_refused() -> Outcome {
    let mut topo = two_lans()?;
    topo.networks["office"].cidr = "office".into();
    let e = compile(&topo).unwrap_err();
    assert!(matches!(&e, Error::BadCidr { lan, cidr } if lan == "office" && cidr == "office"), "{e}");
    assert_eq!(
        e.to_string(),
        "LAN `office`: `office` isn't a block like 10.10.1.0/24 (the network's own address, a /24 to a /29)"
    );
    Ok(())
}

#[test]
fn lans_and_cables_mix() -> Outcome {
    let mut topo = two_lans()?;
    join(&mut topo, "router", "office", None)?;
    topo.links.push(cable(ep("client", "eth1"), ep("server", "eth1")));
    let c = accepted(&topo)?;
    assert_eq!(spec(&c).networks.len(), 3);
    assert_eq!(octet(&c, "client", "link-0")?, 2);
    Ok(())
}

#[test]
fn an_automatic_router_lets_its_networks_reach_each_other() -> Outcome {
    let mut topo = two_lans()?;
    topo.nodes["router"].kind = Kind::Router {};
    join(&mut topo, "router", "office", None)?;
    let c = accepted(&topo)?;
    // Not a machine, and no address taken on its LANs.
    assert!(!spec(&c).machines.contains_key("router"));
    assert_eq!(octet(&c, "client", "office")?, 10);
    let s = spec(&c);
    let pairs: Vec<(&str, &str)> = s.reach.iter().map(|r| (r.from.as_str(), r.to.as_str())).collect();
    assert_eq!(pairs, vec![("office", "servers"), ("servers", "office")]);
    Ok(())
}

#[test]
fn an_automatic_router_routes_its_cables_too() -> Outcome {
    let mut topo = two_lans()?;
    topo.nodes["router"].kind = Kind::Router {};
    topo.nodes["router"].networks.shift_remove("office");
    topo.links.push(cable(ep("router", "eth1"), ep("client", "eth1")));
    let c = accepted(&topo)?;
    assert_eq!(spec(&c).networks["link-0"].cidr, "10.200.0.0/29");
    assert_eq!(octet(&c, "client", "link-0")?, 3);
    assert!(spec(&c).reach.iter().any(|r| r.from == "link-0" && r.to == "servers"));
    Ok(())
}

#[test]
fn a_lan_keeps_its_conditions_and_its_internet() -> Outcome {
    let mut topo = two_lans()?;
    topo.networks["servers"].internet = false;
    topo.networks["servers"].tc = serde_json::from_str(r#"{ "delay": "20ms", "rate": "10mbit" }"#)?;
    let c = accepted(&topo)?;
    assert!(
        yaml(&c).contains("servers:\n    cidr: 10.10.2.0/24\n    internet: false\n    tc:\n      delay: 20ms\n      rate: 10mbit\n"),
        "{}",
        yaml(&c)
    );
    assert!(
        yaml(&c).contains("office:\n    cidr: 10.10.1.0/24\n  servers"),
        "internet: true is the default: {}",
        yaml(&c)
    );
    Ok(())
}
