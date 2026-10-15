use std::net::Ipv4Addr;

use super::{
    End, Trunk, Vlan, carried, containers, interface, link_network, mac, machine_commands, machine_ready, of, switch_address, switch_name, switch_ready,
    switch_script, vm_commands, vms,
};
use crate::testing;

/// An office LAN split into VLAN 10 (offline) and VLAN 20 (online); `admin` and `nas` are on
/// both, `staff` on one, `ops` on a VLAN and a network of its own, `win` is a Windows VM on both.
const SPEC: &str = r"
version: 1
name: v
networks:
  office:
    cidr: 10.70.0.0/16
    internet: false
    vlans:
      10: { cidr: 10.70.10.0/24 }
      20: { cidr: 10.70.20.0/24, internet: true }
  servers: { cidr: 10.70.99.0/24 }
machines:
  admin: { os: debian@12, networks: { office.vlan10: 50, office.vlan20: 50 } }
  nas: { os: debian@12, networks: { office.vlan20: 60, office.vlan10: 60 } }
  staff: { os: debian@12, networks: { office.vlan10: 10 } }
  ops: { os: debian@12, networks: { office.vlan10: 70, servers: 70 } }
  win: { os: windows-server@2022, networks: { office.vlan10: 80, office.vlan20: 80 } }
";

fn vlan(network: &str, id: u16, octet: u8) -> Vlan {
    Vlan {
        network: network.into(),
        id,
        octet,
    }
}

fn admin() -> Trunk {
    Trunk {
        lan: "office".into(),
        machine: "admin".into(),
        vlans: vec![vlan("office-vlan10", 10, 50), vlan("office-vlan20", 20, 50)],
    }
}

#[test]
fn a_machine_on_two_vlans_of_one_lan_has_a_trunk_and_one_on_a_single_vlan_none() {
    let spec = testing::spec(SPEC);
    let ts = containers(&spec);
    let machines: Vec<&str> = ts.iter().map(|t| t.machine.as_str()).collect();
    assert_eq!(machines, ["admin", "nas"], "windows has no container form; staff and ops carry one VLAN each");
    assert_eq!(ts[0], admin());
    assert_eq!(
        ts[1].vlans,
        [vlan("office-vlan20", 20, 60), vlan("office-vlan10", 10, 60)],
        "in the machine's order"
    );
}

#[test]
fn only_linux_vms_build_a_trunk_inside() {
    let spec = testing::spec(SPEC);
    let machines: Vec<String> = vms(&spec).into_iter().map(|t| t.machine).collect();
    assert_eq!(machines, ["admin", "nas"]);
}

#[test]
fn a_machine_reaches_its_trunk_s_vlans_through_it_and_nothing_else() {
    let ts = [admin()];
    assert_eq!(of(&ts, "admin").count(), 1);
    assert_eq!(of(&ts, "nas").count(), 0);
    assert!(carried(&ts, "admin", "office-vlan20"));
    assert!(!carried(&ts, "admin", "servers"));
    assert!(!carried(&ts, "nas", "office-vlan20"));
}

#[test]
fn each_end_has_a_stable_local_mac_of_its_own() {
    assert_eq!(mac(&admin(), End::Machine), "02:1e:1d:38:30:38");
    assert_eq!(mac(&admin(), End::Switch), "02:1e:1d:38:30:39");
    assert_eq!(link_network(&admin()), "isoloom-trunk-office-admin");
    assert_eq!(switch_name("office"), "isoloom-switch-office");
    assert_eq!(interface("a-very-long-lan-name"), "a-very-lon");
    assert_eq!(interface("abcdefghi-x"), "abcdefghi", "no trailing hyphen");
}

#[test]
fn the_switch_bridges_each_vlan_once_and_tags_every_trunk() {
    let spec = testing::spec(SPEC);
    let ts = containers(&spec);
    let refs: Vec<&Trunk> = ts.iter().collect();
    let script = switch_script(&spec, &refs);
    assert_eq!(script.matches("type bridge").count(), 2, "{script}");
    assert_eq!(script.matches("type vlan id").count(), 4, "{script}");
    let switch = switch_address(&spec, "office-vlan10");
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
    let spec = testing::spec(SPEC);
    let at = |n: &str, o: u8| isoloom_plan::addressing::address(&spec, n, o);
    let gateway = |_: &str| Ipv4Addr::new(10, 70, 20, 1);
    let cmds = machine_commands(&spec, &spec.machines["admin"], &admin(), at, gateway);
    assert_eq!(cmds.len(), 4, "{cmds:?}");
    assert!(cmds[0].contains("02:1e:1d:38:30:38") && cmds[0].ends_with("ip link set $IF name office && ip link set office up"));
    assert_eq!(
        cmds[1],
        "ip link add link office name office.10 type vlan id 10 && ip addr add 10.70.10.50/24 dev office.10 && ip link set office.10 up"
    );
    assert_eq!(cmds[3], "ip route replace default via 10.70.20.1");
    // `ops` has its own way out (`servers`, online): no default route through the trunk.
    let ops = Trunk {
        machine: "ops".into(),
        vlans: vec![vlan("office-vlan10", 10, 70), vlan("office-vlan20", 20, 70)],
        ..admin()
    };
    let other_way_out = machine_commands(&spec, &spec.machines["ops"], &ops, at, gateway);
    assert!(!other_way_out.iter().any(|c| c.contains("default")), "{other_way_out:?}");
    assert_eq!(
        machine_ready(&admin()),
        "ip link show office.10 | grep -q 'UP' && ip link show office.20 | grep -q 'UP'"
    );
}

#[test]
fn a_vm_builds_its_trunk_once_per_boot() {
    let spec = testing::spec(SPEC);
    let cmds = vm_commands(&spec, &admin(), |n, o| isoloom_plan::addressing::address(&spec, n, o));
    assert_eq!(cmds.len(), 1);
    let sh = &cmds[0];
    assert!(
        sh.starts_with("ip link show office >/dev/null 2>&1 || { ip link add office type veth peer name office-s && "),
        "{sh}"
    );
    assert!(sh.contains("ip link set $IF address 02:1f:0a:46:0a:32"), "{sh}");
    assert!(
        sh.contains("ip link add link office-s name office-s.20 type vlan id 20 && ip link set office-s.20 master br20"),
        "{sh}"
    );
    assert!(sh.ends_with("ip addr add 10.70.20.50/24 dev office.20 && ip link set office.20 up; }"), "{sh}");
}
