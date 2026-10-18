//! 802.1Q on Docker: a machine on several VLANs of one LAN gets one trunk to the LAN's switch.

use isoloom_spec::Target;

use crate::testing::{Outcome, example, from, generated};

/// The vlan-office example's Compose file.
fn compose() -> Outcome<String> {
    generated(&example("vlan-office")?, Target::Docker, "compose.yml")
}

/// The block of one service in the Compose file: up to the next line indented by two spaces.
fn service<'a>(compose: &'a str, name: &str) -> Outcome<&'a str> {
    let block = from(compose, &format!("\n  {name}:\n"))?.get(1..).unwrap_or_default();
    let sibling = block
        .match_indices("\n  ")
        .map(|(i, _)| i)
        .find(|&i| !block.get(i + 3..).is_some_and(|r| r.starts_with(' ')));
    Ok(sibling.and_then(|end| block.get(..end)).unwrap_or(block))
}

#[test]
fn a_machine_on_two_vlans_gets_a_trunk() -> Outcome {
    let c = compose()?;
    let admin = service(&c, "admin")?;
    assert!(admin.contains("isoloom-trunk-office-admin:") && admin.contains("mac_address"), "{admin}");
    assert!(
        !admin.contains("office-vlan10:") && !admin.contains("office-vlan20:"),
        "the trunk carries them: {admin}"
    );
    let sidecar = service(&c, "admin-routes")?;
    assert!(sidecar.contains("netshoot"), "{sidecar}");
    assert!(
        sidecar.contains("name office.10 type vlan id 10") && sidecar.contains("10.70.10.50/24 dev office.10"),
        "{sidecar}"
    );
    assert!(sidecar.contains("isoloom-switch-office"), "waits for the switch: {sidecar}");
    let switch = service(&c, "isoloom-switch-office")?;
    assert!(
        switch.contains("10.70.10.253") && switch.contains("10.70.20.253"),
        "on each VLAN at the controller address: {switch}"
    );
    assert!(switch.contains("type vlan id 20 && ip link set $$IF.20 master br20"), "{switch}");
    assert!(c.contains("  isoloom-trunk-office-admin:\n    internal: true"), "the trunk link is internal");
    Ok(())
}

#[test]
fn a_machine_on_one_vlan_keeps_its_network() -> Outcome {
    let c = compose()?;
    let guest = service(&c, "guest")?;
    assert!(guest.contains("office-vlan20:") && !guest.contains("isoloom-trunk"), "{guest}");
    Ok(())
}
