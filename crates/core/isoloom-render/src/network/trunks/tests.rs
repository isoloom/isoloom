use super::{Trunk, Vlan, interface, of_machines, vm_commands, vms};
use crate::test_support::VLAN_OFFICE;
use crate::testing;

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
    let spec = testing::spec(VLAN_OFFICE);
    let ts = of_machines(&spec, |m| m.docker.is_some());
    let machines: Vec<&str> = ts.iter().map(|t| t.machine.as_str()).collect();
    assert_eq!(machines, ["admin", "nas", "ops"], "windows has no container form; staff carries one VLAN");
    assert_eq!(ts[0], admin());
    assert_eq!(
        ts[1].vlans,
        [vlan("office-vlan20", 20, 60), vlan("office-vlan10", 10, 60)],
        "in the machine's order"
    );
}

#[test]
fn only_linux_vms_build_a_trunk_inside() {
    let spec = testing::spec(VLAN_OFFICE);
    let machines: Vec<String> = vms(&spec).into_iter().map(|t| t.machine).collect();
    assert_eq!(machines, ["admin", "nas", "ops"]);
}

#[test]
fn the_interface_is_named_after_the_lan_within_linux_s_limit() {
    assert_eq!(interface("a-very-long-lan-name"), "a-very-lon");
    assert_eq!(interface("abcdefghi-x"), "abcdefghi", "no trailing hyphen");
}

#[test]
fn a_vm_builds_its_trunk_once_per_boot() {
    let spec = testing::spec(VLAN_OFFICE);
    let cmds = vm_commands(&spec, &admin(), |n, o| isoloom_plan::addressing::address(&spec, n, o));
    assert_eq!(cmds.len(), 1);
    let sh = &cmds[0];
    assert!(
        sh.starts_with("ip link show office >/dev/null 2>&1 || { ip link add office type veth peer name office-s && "),
        "{sh}"
    );
    assert!(sh.contains("ip link set \"$IF\" address 02:1f:0a:46:0a:32"), "{sh}");
    assert!(
        sh.contains("ip link add link office-s name office-s.20 type vlan id 20 && ip link set office-s.20 master br20"),
        "{sh}"
    );
    assert!(sh.ends_with("ip addr add 10.70.20.50/24 dev office.20 && ip link set office.20 up; }"), "{sh}");
}
