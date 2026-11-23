//! 802.1Q on Docker: a machine on several VLANs of one LAN gets one trunk to the LAN's switch.

use isoloom_spec::Target;
use isoloom_test_support::{yaml, yaml_at, yaml_keys_at, yaml_str, yaml_strs};
use serde_yaml_ng::Value;

use crate::testing::{Outcome, example, generated};

/// The vlan-office example's Compose file, parsed.
fn compose() -> Outcome<Value> {
    Ok(yaml(&generated(&example("vlan-office")?, Target::Docker, "compose.yml")?))
}

/// What service `name` runs: its entrypoint's items and its command's, one a line.
fn script(c: &Value, name: &str) -> String {
    [
        yaml_strs(c, &format!("services.{name}.entrypoint")),
        yaml_strs(c, &format!("services.{name}.command")),
    ]
    .concat()
    .join("\n")
}

#[test]
fn a_machine_on_two_vlans_gets_a_trunk() -> Outcome {
    let c = compose()?;
    // The trunk carries both VLANs: the machine's one network, at a MAC of its own.
    assert_eq!(yaml_keys_at(&c, "services.admin.networks"), ["isoloom-trunk-office-admin"]);
    assert!(yaml_at(&c, "services.admin.networks.isoloom-trunk-office-admin.mac_address").is_some());
    let sidecar = "isoloom-routes-admin";
    assert!(yaml_str(&c, &format!("services.{sidecar}.image")).starts_with("nicolaka/netshoot:"));
    let tags = script(&c, sidecar);
    assert!(
        tags.contains("name office.10 type vlan id 10") && tags.contains("10.70.10.50/24 dev office.10"),
        "{tags}"
    );
    let waits = yaml_keys_at(&c, &format!("services.{sidecar}.depends_on"));
    assert!(waits.contains(&"isoloom-switch-office".to_owned()), "waits for the switch: {waits:?}");
    // The switch: on each VLAN at the controller address, bridging the tagged trunk into each.
    let switch = "services.isoloom-switch-office.networks";
    assert_eq!(yaml_str(&c, &format!("{switch}.office-vlan10.ipv4_address")), "10.70.10.253");
    assert_eq!(yaml_str(&c, &format!("{switch}.office-vlan20.ipv4_address")), "10.70.20.253");
    let bridges = script(&c, "isoloom-switch-office");
    assert!(bridges.contains("type vlan id 20 && ip link set $$IF.20 master br20"), "{bridges}");
    assert_eq!(
        yaml_str(&c, "networks.isoloom-trunk-office-admin.internal"),
        "true",
        "the trunk link is internal"
    );
    Ok(())
}

#[test]
fn a_machine_on_one_vlan_keeps_its_network() -> Outcome {
    let c = compose()?;
    assert_eq!(yaml_keys_at(&c, "services.guest.networks"), ["office-vlan20"], "no trunk");
    Ok(())
}
