//! 802.1Q on Docker: a machine on several VLANs of one LAN gets one trunk to the LAN's switch.

use std::path::Path;

use isoloom_core::{Target, generate, load};

fn compose() -> String {
    let spec = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/vlan-office")).unwrap();
    generate(&spec, Target::Docker)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with("compose.yml"))
        .unwrap()
        .contents
}

/// The block of one service in the Compose file: up to the next line indented by two spaces.
fn service<'a>(compose: &'a str, name: &str) -> &'a str {
    let start = compose.find(&format!("\n  {name}:\n")).unwrap_or_else(|| panic!("no service {name}")) + 1;
    let rest = &compose[start..];
    let mut i = 0;
    while let Some(j) = rest[i + 1..].find("\n  ") {
        let k = i + 1 + j;
        if !rest[k + 3..].starts_with(' ') {
            return &rest[..k];
        }
        i = k;
    }
    rest
}

#[test]
fn a_machine_on_two_vlans_gets_a_trunk() {
    let c = compose();
    let admin = service(&c, "admin");
    assert!(admin.contains("isoloom-trunk-office-admin:") && admin.contains("mac_address"), "{admin}");
    assert!(
        !admin.contains("office-vlan10:") && !admin.contains("office-vlan20:"),
        "the trunk carries them: {admin}"
    );
    let sidecar = service(&c, "admin-routes");
    assert!(sidecar.contains("netshoot"), "{sidecar}");
    assert!(
        sidecar.contains("name office.10 type vlan id 10") && sidecar.contains("10.70.10.50/24 dev office.10"),
        "{sidecar}"
    );
    assert!(sidecar.contains("isoloom-switch-office"), "waits for the switch: {sidecar}");
    let switch = service(&c, "isoloom-switch-office");
    assert!(
        switch.contains("10.70.10.253") && switch.contains("10.70.20.253"),
        "on each VLAN at the controller address: {switch}"
    );
    assert!(switch.contains("type vlan id 20 && ip link set $$IF.20 master br20"), "{switch}");
    assert!(c.contains("  isoloom-trunk-office-admin:\n    internal: true"), "the trunk link is internal");
}

#[test]
fn a_machine_on_one_vlan_keeps_its_network() {
    let c = compose();
    let guest = service(&c, "guest");
    assert!(guest.contains("office-vlan20:") && !guest.contains("isoloom-trunk"), "{guest}");
}
