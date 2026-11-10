use std::net::Ipv4Addr;

use isoloom_plan::{Environment, Lab};
use isoloom_render::test_support::VLAN_OFFICE;
use isoloom_spec::model::{Spec, Target};

use super::{End, carried, containers, link_network, mac, machine_commands, machine_ready, of, switch_address, switch_name, switch_ready, switch_script};

fn spec() -> Spec {
    Lab::parse(VLAN_OFFICE).expect("the test's spec parses").spec()
}

/// The plan of the example on the `docker` target, whose blocks the trunks take.
fn env() -> Environment {
    let lab = Lab::parse(VLAN_OFFICE).expect("the test's spec parses");
    Environment::build(&lab.for_target(Target::Docker), Target::Docker, None).expect("the spec plans")
}

#[test]
fn a_container_on_two_vlans_of_one_lan_has_a_trunk_and_one_on_a_single_vlan_none() {
    let ts = containers(&spec());
    let machines: Vec<&str> = ts.iter().map(|t| t.machine.as_str()).collect();
    assert_eq!(machines, ["admin", "nas", "ops"], "windows has no container form; staff carries one VLAN");
    let vlans: Vec<(&str, u16, u8)> = ts[1].vlans.iter().map(|v| (v.network.as_str(), v.id, v.octet)).collect();
    assert_eq!(vlans, [("office-vlan20", 20, 60), ("office-vlan10", 10, 60)], "in the machine's order");
}

#[test]
fn a_machine_reaches_its_trunk_s_vlans_through_it_and_nothing_else() {
    let ts = containers(&spec());
    assert_eq!(of(&ts, "admin").count(), 1);
    assert_eq!(of(&ts, "staff").count(), 0);
    assert!(carried(&ts, "admin", "office-vlan20"));
    assert!(!carried(&ts, "ops", "servers"));
    assert!(!carried(&ts, "staff", "office-vlan10"));
}

#[test]
fn each_end_has_a_stable_local_mac_of_its_own() {
    let admin = &containers(&spec())[0];
    assert_eq!(mac(admin, End::Machine), "02:1e:1d:38:30:38");
    assert_eq!(mac(admin, End::Switch), "02:1e:1d:38:30:39");
    assert_eq!(link_network(admin), "isoloom-trunk-office-admin");
    assert_eq!(switch_name("office"), "isoloom-switch-office");
}

#[test]
fn the_switch_bridges_each_vlan_once_and_tags_every_trunk() {
    let env = env();
    let ts = containers(env.spec());
    let refs: Vec<&super::Trunk> = ts.iter().take(2).collect();
    let script = switch_script(&env, &refs);
    assert_eq!(script.matches("ip link add br10 type bridge").count(), 1, "{script}");
    assert_eq!(script.matches("ip link add br20 type bridge").count(), 1, "{script}");
    assert_eq!(script.matches("type vlan id").count(), 4, "{script}");
    let switch = switch_address(&env, "office-vlan10");
    assert!(
        script.starts_with(&format!("IF=$(ip -o -4 addr show | awk '$4 ~ /^{}\\//", switch.to_string().replace('.', "\\."))),
        "{script}"
    );
    assert!(script.ends_with("&& exec sleep infinity"), "{script}");
    assert!(script.contains(&mac(&ts[1], End::Switch)), "{script}");
    assert_eq!(
        switch_ready(&refs),
        "ip link show br10 | grep -q 'state UP' && ip link show br20 | grep -q 'state UP'"
    );
}

#[test]
fn the_machine_end_routes_out_through_a_vlan_with_internet_when_nothing_else_does() {
    let env = env();
    let spec = env.spec();
    let ts = containers(spec);
    let gateway = |_: &str| Ipv4Addr::new(10, 70, 20, 1);
    let cmds = machine_commands(&env, &spec.machines["admin"], &ts[0], gateway);
    assert_eq!(cmds.len(), 4, "{cmds:?}");
    assert!(cmds[0].contains("02:1e:1d:38:30:38") && cmds[0].ends_with("ip link set $IF name office && ip link set office up"));
    assert_eq!(
        cmds[1],
        "ip link add link office name office.10 type vlan id 10 && ip addr add 10.70.10.50/24 dev office.10 && ip link set office.10 up"
    );
    assert_eq!(cmds[3], "ip route replace default via 10.70.20.1");
    // `ops` has its own way out (`servers`, online): no default route through the trunk.
    let other_way_out = machine_commands(&env, &spec.machines["ops"], &ts[2], gateway);
    assert!(!other_way_out.iter().any(|c| c.contains("default")), "{other_way_out:?}");
    assert_eq!(
        machine_ready(&ts[0]),
        "ip link show office.10 | grep -q 'UP' && ip link show office.20 | grep -q 'UP'"
    );
}
