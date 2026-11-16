//! Switches: VLANs, Cisco switches and routers on them.

use isoloom_topology::{Compiled, Error, Lan, ProjectFile, Result, Topology, compile};

use crate::testing::{Outcome, accepted, at, cable, ep, files, join, lan, octet, parsed, spec, two_lans};

/// A switch running a Cisco OS, as the editor sends it: a node of its own with no networks,
/// named as the `device` of its VLANs; machines on its ports join the VLANs.
#[test]
fn a_cisco_switch_switches_its_vlans() -> Outcome {
    let topo: Topology = serde_json::from_str(
        r#"{
          "name": "cisco-sw",
          "nodes": {
            "pc1": { "kind": { "container": { "image": "alpine:3.20", "idle": true } }, "networks": { "vlan10": null } },
            "pc2": { "kind": { "container": { "image": "alpine:3.20", "idle": true } }, "networks": { "vlan20": null } },
            "sw1": { "kind": { "container": { "image": "vrnetlab/cisco_iol:L2-17.12.01", "appliance": "cisco-iol-l2", "config": "spanning-tree mode rapid-pvst" } } }
          },
          "networks": {
            "vlan10": { "cidr": "10.10.10.0/24", "switch": "sw1", "vlan": 10, "device": "sw1" },
            "vlan20": { "cidr": "10.10.20.0/24", "switch": "sw1", "vlan": 20, "device": "sw1" }
          }
        }"#,
    )?;
    let c = accepted(&topo)?;
    assert_eq!(spec(&c).networks["sw1"].switch.as_deref(), Some("sw1"));
    assert!(spec(&c).machines["sw1"].networks.is_empty());
    assert_eq!(spec(&c).machines["pc1"].networks.keys().collect::<Vec<_>>(), ["sw1.vlan10"]);
    assert_eq!(
        ["machines.sw1.app.appliance", "machines.sw1.app.config"].map(|path| at(&c, path)),
        ["cisco-iol-l2", "configs/sw1.cfg"]
    );
    assert_eq!(
        files(&c),
        [ProjectFile {
            path: "configs/sw1.cfg".into(),
            contents: "spanning-tree mode rapid-pvst\n".into(),
        }]
    );
    Ok(())
}

/// A Cisco router on a switch's VLAN: Isoloom's VLANs have no gateway, so the router joins it
/// at an ordinary address, and the project still builds.
#[test]
fn a_cisco_router_on_a_vlan_is_a_member_of_it() -> Outcome {
    let topo: Topology = serde_json::from_str(
        r#"{
          "name": "router-on-vlan",
          "nodes": {
            "pc": { "kind": { "container": { "image": "alpine:3.20", "idle": true } }, "networks": { "vlan10": null } },
            "r": { "kind": { "container": { "image": "vrnetlab/cisco_iol:17.12.01", "appliance": "cisco-iol" } }, "networks": { "vlan10": null, "office": null } }
          },
          "networks": {
            "office": { "cidr": "10.10.1.0/24" },
            "vlan10": { "cidr": "10.10.10.0/24", "switch": "sw1", "vlan": 10 }
          }
        }"#,
    )?;
    let c = accepted(&topo)?;
    assert_eq!(octet(&c, "r", "sw1.vlan10")?, 11);
    // On a plain LAN it is still the gateway.
    assert_eq!(spec(&c).networks["office"].gateway.as_deref(), Some("r"));
    assert_eq!(octet(&c, "r", "office")?, 1);
    Ok(())
}

/// Two Cisco routers on one LAN (a transit LAN between them): the first in the drawing's order is
/// its gateway, the other a member at an ordinary address.
#[test]
fn of_two_cisco_routers_on_a_lan_the_first_is_its_gateway() -> Outcome {
    let topo: Topology = serde_json::from_str(
        r#"{
          "name": "two-routers",
          "nodes": {
            "r1": { "kind": { "container": { "image": "vrnetlab/cisco_iol:17.12.01", "appliance": "cisco-iol" } }, "networks": { "transit": null } },
            "r2": { "kind": { "container": { "image": "vrnetlab/cisco_iol:17.12.01", "appliance": "cisco-iol" } }, "networks": { "transit": null } }
          },
          "networks": { "transit": { "cidr": "10.10.1.0/24" } }
        }"#,
    )?;
    let c = accepted(&topo)?;
    assert_eq!(spec(&c).networks["transit"].gateway.as_deref(), Some("r1"));
    assert_eq!(octet(&c, "r1", "transit")?, 1);
    assert_ne!(octet(&c, "r2", "transit")?, 1);
    Ok(())
}

/// A Cisco router drawn at an address of its own on a LAN it becomes the gateway of is refused,
/// naming the router and the LAN: it would sit at the gateway's address, not where it was drawn.
/// A router that stays a member keeps the address drawn for it.
#[test]
fn a_gateway_router_drawn_at_an_address_of_its_own_is_refused() -> Outcome {
    let drawn = |first: &str, second: &str| -> Result<Compiled> {
        let topo: Topology = serde_json::from_str(&format!(
            r#"{{
              "name": "two-routers",
              "nodes": {{
                "r1": {{ "kind": {{ "container": {{ "image": "vrnetlab/cisco_iol:17.12.01", "appliance": "cisco-iol" }} }}, "networks": {{ "transit": {first} }} }},
                "r2": {{ "kind": {{ "container": {{ "image": "vrnetlab/cisco_iol:17.12.01", "appliance": "cisco-iol" }} }}, "networks": {{ "transit": {second} }} }}
              }},
              "networks": {{ "transit": {{ "cidr": "10.10.1.0/24" }} }}
            }}"#
        ))
        .unwrap();
        compile(&topo)
    };
    let err = drawn("20", "null").unwrap_err();
    assert!(
        matches!(&err, Error::GatewayAddress { lan, node, octet: 20, gateway: 1 } if lan == "transit" && node == "r1"),
        "{err:?}"
    );
    assert_eq!(
        err.to_string(),
        "on LAN `transit`, `r1` is the gateway (the first Cisco router on it), at .1: leave its address there unset rather than .20"
    );
    let c = drawn("null", "20")?;
    assert_eq!(octet(&c, "r1", "transit")?, 1);
    assert_eq!(octet(&c, "r2", "transit")?, 20);
    Ok(())
}

/// A starter says the machine's addresses on every network the compiler made, a VLAN under its
/// spec name and a cable's network included: each block as the compiler read it, once.
#[test]
fn a_starter_names_its_machines_address_on_a_vlan_and_a_cable() -> Outcome {
    let topo: Topology = serde_json::from_str(
        r#"{
          "name": "vlan-starter",
          "nodes": {
            "pc": { "kind": { "vm": { "os": "debian@12", "provision": ["provision/pc.sh"] } }, "networks": { "vlan10": null } },
            "peer": { "kind": { "container": { "image": "alpine:3.20", "idle": true } } }
          },
          "networks": { "vlan10": { "cidr": "10.10.10.0/24", "switch": "sw1", "vlan": 10 } },
          "links": [{ "a": { "node": "pc", "port": "eth1" }, "b": { "node": "peer", "port": "eth0" } }]
        }"#,
    )?;
    let c = accepted(&topo)?;
    let starter = files(&c).into_iter().find(|f| f.path == "provision/pc.sh").ok_or("no starter")?;
    let addresses: Vec<&str> = starter.contents.lines().filter(|l| l.starts_with("# Address: ")).collect();
    assert_eq!(addresses, ["# Address: 10.10.10.10 (sw1.vlan10)", "# Address: 10.200.0.2 (link-0)"]);
    Ok(())
}

/// A Dynamips router boots the IOS image the user adds: no image of its own.
#[test]
fn a_dynamips_router_boots_its_firmware() -> Outcome {
    let topo: Topology = serde_json::from_str(
        r#"{
          "name": "classic",
          "nodes": {
            "r1": { "kind": { "container": { "image": "", "appliance": "cisco-dynamips" } }, "networks": { "office": null } },
            "pc": { "kind": { "container": { "image": "alpine:3.20", "idle": true } }, "networks": { "office": null } }
          },
          "networks": { "office": { "cidr": "10.10.1.0/24" } }
        }"#,
    )?;
    let c = accepted(&topo)?;
    assert_eq!(
        isoloom_test_support::yaml_keys_at(&parsed(&c), "machines.r1.app"),
        ["appliance", "firmware"],
        "no image of its own"
    );
    assert_eq!(
        ["machines.r1.app.appliance", "machines.r1.app.firmware"].map(|path| at(&c, path)),
        ["cisco-dynamips", "images/r1.bin"]
    );
    Ok(())
}

/// A switch named like a cable's network would have its VLANs nest under that network, which the
/// cable then takes over: refused, as a LAN of that name is.
#[test]
fn a_switch_can_not_take_a_cables_name() -> Outcome {
    let mut topo = two_lans()?;
    topo.networks.insert(
        "vlan10".into(),
        Lan {
            switch: Some("link-0".into()),
            vlan: Some(10),
            ..lan("10.10.10.0/24")
        },
    );
    join(&mut topo, "server", "vlan10", None)?;
    topo.links.push(cable(ep("client", "eth1"), ep("server", "eth1")));
    let e = compile(&topo).unwrap_err();
    assert!(matches!(&e, Error::ReservedName(n) if n == "link-0"), "{e}");
    Ok(())
}

/// A switch's VLANs nest under a LAN named after the switch: a LAN of the project can't have
/// that name too, in either order.
#[test]
fn a_switch_can_not_share_a_lans_name() -> Outcome {
    let mut topo = two_lans()?;
    let vlan = Lan {
        switch: Some("office".into()),
        vlan: Some(10),
        ..lan("10.10.10.0/24")
    };
    topo.networks.insert("vlan10".into(), vlan.clone());
    join(&mut topo, "server", "vlan10", None)?;
    let clash = |r: Result<Compiled>| matches!(r, Err(Error::SwitchNameTaken { switch }) if switch == "office");
    assert!(clash(compile(&topo)));
    topo.networks.shift_remove("vlan10");
    topo.networks.shift_insert(0, "vlan10".into(), vlan);
    assert!(clash(compile(&topo)));
    assert_eq!(
        compile(&topo).unwrap_err().to_string(),
        "switch `office` has the name of a LAN; rename one of them"
    );
    Ok(())
}

/// Regression: a VLAN's `tc` was dropped without a word (Isoloom's VLANs have none); it is
/// refused, naming the VLAN, while a plain LAN keeps its own.
#[test]
fn a_vlan_with_conditions_is_refused() -> Outcome {
    let mut topo = two_lans()?;
    topo.networks.insert(
        "vlan10".into(),
        Lan {
            switch: Some("sw1".into()),
            vlan: Some(10),
            tc: serde_json::from_str(r#"{ "delay": "20ms" }"#)?,
            ..lan("10.10.10.0/24")
        },
    );
    join(&mut topo, "server", "vlan10", None)?;
    let e = compile(&topo).unwrap_err();
    assert!(matches!(&e, Error::VlanTc { lan } if lan == "vlan10"), "{e}");
    assert_eq!(
        e.to_string(),
        "VLAN `vlan10` has a `tc`, which Isoloom gives only a whole LAN or a cable; remove it"
    );
    topo.networks["vlan10"].tc = None;
    topo.networks["office"].tc = serde_json::from_str(r#"{ "delay": "20ms" }"#)?;
    accepted(&topo)?;
    Ok(())
}

/// Regression: two LANs on the same VLAN of a switch compiled into one block, the first LAN's
/// machines moved silently into the second's; it is refused, naming both.
#[test]
fn two_lans_on_the_same_vlan_of_a_switch_are_refused() -> Outcome {
    let mut topo = two_lans()?;
    for (name, block) in [("a", "10.20.1.0/24"), ("b", "10.20.2.0/24")] {
        let vlan = Lan {
            switch: Some("sw1".into()),
            vlan: Some(10),
            ..lan(block)
        };
        topo.networks.insert(name.into(), vlan);
    }
    join(&mut topo, "server", "a", None)?;
    join(&mut topo, "client", "b", None)?;
    let e = compile(&topo).unwrap_err();
    assert!(
        matches!(&e, Error::VlanTaken { switch, vlan: 10, first, second } if switch == "sw1" && first == "a" && second == "b"),
        "{e}"
    );
    assert_eq!(e.to_string(), "LANs `a` and `b` are both VLAN 10 of switch `sw1`; give one another VLAN id");
    topo.networks["b"].vlan = Some(20);
    accepted(&topo)?;
    Ok(())
}

/// Regression: a LAN with a switch but no VLAN id (or the reverse), or a device on a plain LAN,
/// compiled as a plain LAN, the field dropped without a word.
#[test]
fn a_half_drawn_vlan_or_a_device_off_a_switch_is_refused() -> Outcome {
    let half = |switch: Option<&str>, vlan: Option<u16>| -> Outcome<Error> {
        let mut topo = two_lans()?;
        topo.networks["office"].switch = switch.map(str::to_owned);
        topo.networks["office"].vlan = vlan;
        Ok(compile(&topo).unwrap_err())
    };
    assert_eq!(
        half(Some("sw1"), None)?.to_string(),
        "LAN `office` has a switch but no vlan: a VLAN needs both, a plain LAN neither"
    );
    assert!(matches!(
        half(None, Some(10))?,
        Error::VlanIncomplete {
            given: "vlan",
            missing: "switch",
            ..
        }
    ));
    let mut topo = two_lans()?;
    topo.networks["office"].device = Some("sw1".into());
    assert!(matches!(compile(&topo).unwrap_err(), Error::DeviceOffSwitch { lan } if lan == "office"));
    Ok(())
}
