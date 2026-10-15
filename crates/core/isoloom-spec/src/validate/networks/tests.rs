use crate::testing::{lab, problems};

/// The problems of the lab with `networks` beside `lan`, and a web server on `lan`.
fn with(networks: &str) -> Vec<String> {
    problems(&lab(&format!(
        "{networks}machines:\n  web: {{ networks: {{ lan: 10 }}, docker: {{ image: nginx }} }}\n"
    )))
}

/// The problems of LAN `office` split into two VLANs (its other settings `lan`), a PC on both,
/// and the machines `more`.
fn office(lan: &str, more: &str) -> Vec<String> {
    problems(&format!(
        "version: 1\nname: t\nnetworks:\n  office: {{ cidr: 10.10.0.0/16, {lan}vlans: {{ 10: {{ cidr: 10.10.10.0/24 }}, 20: {{ cidr: 10.10.20.0/24 }} }} }}\nmachines:\n  pc: {{ networks: {{ office.vlan10: 10, office.vlan20: 10 }}, docker: {{ image: alpine }} }}\n{more}"
    ))
}

const SWITCH: &str = "  sw: { docker: { image: x, appliance: cisco-iol-l2 } }\n";

#[test]
fn a_spec_declares_a_network() {
    assert_eq!(
        problems("version: 1\nname: t\nmachines:\n  web: { docker: { image: nginx } }\n"),
        [
            "networks: declare at least one network",
            "machines.web.networks: attach the machine to at least one network"
        ]
    );
}

#[test]
fn a_network_is_a_24_to_a_29() {
    for fits in ["10.1.0.0/24", "10.1.0.0/25", "10.1.0.8/29"] {
        assert_eq!(with(&format!("  b: {{ cidr: {fits} }}\n")), Vec::<String>::new(), "{fits}");
    }
    for wrong in ["10.2.0.0/23", "10.1.0.0/30"] {
        assert_eq!(
            with(&format!("  b: {{ cidr: {wrong} }}\n")),
            ["networks.b.cidr: use a /24 to /29 block"],
            "{wrong}"
        );
    }
}

#[test]
fn a_networks_docker_block_is_its_size_inside_10() {
    let docker = |d: &str| with(&format!("  b: {{ cidr: 192.168.1.0/24, docker: {{ cidr: {d} }} }}\n"));
    assert_eq!(docker("10.1.0.0/24"), Vec::<String>::new());
    let wrong = "networks.b.docker.cidr: use a /24 block inside 10.0.0.0/8 (the same size as `cidr`, so addresses keep their last octet)";
    assert_eq!(docker("10.1.0.0/25"), [wrong]);
    assert_eq!(docker("172.16.1.0/24"), [wrong]);
    assert_eq!(
        docker("nonsense"),
        ["networks.b.docker.cidr: `nonsense` isn't a network address like 10.20.0.0/24"]
    );
}

#[test]
fn a_gateway_is_a_machine_on_its_network() {
    let gateway = |gw: &str| {
        problems(&lab(&format!(
            "  dmz: {{ cidr: 10.1.0.8/29, gateway: {gw} }}\nmachines:\n  gw: {{ networks: {{ lan: 10 }}, docker: {{ image: router }} }}\n"
        )))
    };
    assert_eq!(gateway("nobody"), ["networks.dmz.gateway: no machine named `nobody`"]);
    assert_eq!(
        gateway("gw"),
        ["networks.dmz.gateway: attach `gw` to `dmz` at the gateway address: `networks: { dmz: 9 }`"]
    );
}

#[test]
fn a_lan_split_into_vlans_has_a_switch_appliance() {
    assert_eq!(office("switch: sw, ", SWITCH), Vec::<String>::new());
    assert_eq!(
        office("switch: sw, ", "  sw: { docker: { image: x, appliance: cisco-iol } }\n"),
        ["networks.office.switch: `sw` isn't a switch: give it `app.appliance: cisco-iol-l2` or `cisco-vios-l2`"]
    );
    assert_eq!(
        with("  b: { cidr: 10.1.0.0/24, switch: web }\n"),
        ["networks.b.switch: only a LAN split into `vlans` has a switch"]
    );
}

#[test]
fn a_missing_switch_is_reported_for_each_vlan() {
    let missing = "networks.office.switch: no machine named `ghost`";
    assert_eq!(office("switch: ghost, ", ""), [missing, missing]);
}

#[test]
fn a_switch_is_reported_once_for_its_lan() {
    assert_eq!(
        office(
            "switch: sw, ",
            "  sw: { networks: { office.vlan10: 11 }, docker: { image: x, appliance: cisco-iol-l2 } }\n"
        ),
        [
            "machines.sw.networks: `sw` switches LAN `office`: its ports come from the LAN (an access port per VLAN, a trunk per machine on several of its VLANs); leave `networks` out"
        ]
    );
    let no_address = "machines.sw: `sw` switches LAN `office` and has no address on the environment's networks: no `ports` or `aliases`";
    assert_eq!(
        office(
            "switch: sw, ",
            "  sw: { docker: { image: x, appliance: cisco-iol-l2 }, ports: [{ port: 22 }] }\n"
        ),
        [no_address]
    );
    assert_eq!(
        office("switch: sw, ", "  sw: { docker: { image: x, appliance: cisco-iol-l2 }, aliases: [core] }\n"),
        [no_address]
    );
}

#[test]
fn a_switch_serves_one_lan() {
    let two = |second: &str, more: &str| {
        problems(&format!(
            "version: 1\nname: t\nnetworks:\n  a: {{ cidr: 10.10.0.0/16, switch: sw, vlans: {{ 10: {{ cidr: 10.10.10.0/24 }} }} }}\n  b: {{ cidr: 10.20.0.0/16, switch: {second}, vlans: {{ 10: {{ cidr: 10.20.10.0/24 }} }} }}\nmachines:\n  pc: {{ networks: {{ a.vlan10: 10, b.vlan10: 10 }}, docker: {{ image: alpine }} }}\n{SWITCH}{more}"
        ))
    };
    assert_eq!(two("sw2", "  sw2: { docker: { image: x, appliance: cisco-iol-l2 } }\n"), Vec::<String>::new());
    assert_eq!(two("sw", ""), ["networks.b.switch: `sw` already switches LAN `a`: a switch serves one LAN"]);
}

#[test]
fn only_appliances_keep_their_management_network_free() {
    let mgmt = "  mgmt: { cidr: 10.255.255.0/24 }\n";
    assert_eq!(with(mgmt), Vec::<String>::new());
    let found = problems(&lab(&format!(
        "{mgmt}machines:\n  r1: {{ networks: {{ lan: 10 }}, docker: {{ image: vrnetlab/cisco_iol:17, appliance: cisco-iol }} }}\n"
    )));
    assert_eq!(
        found,
        ["networks.mgmt.cidr: overlaps 10.255.255.0/24, the network appliances' management network"]
    );
}

#[test]
fn an_impairment_is_times_a_percentage_and_a_rate() {
    let tc = |t: &str| with(&format!("  slow: {{ cidr: 10.1.0.0/24, tc: {t} }}\n"));
    assert_eq!(tc("{ delay: 50ms, jitter: 5ms, loss: 1, rate: 10mbit }"), Vec::<String>::new());
    assert_eq!(tc("{ delay: 1.5s, rate: 512kbit }"), Vec::<String>::new());
    for (bad, why) in [("ms", "has no number"), ("50xs", "has no time unit"), ("1.2.3ms", "isn't a number")] {
        assert_eq!(
            tc(&format!("{{ delay: {bad} }}")),
            [format!("networks.slow.tc.delay: `{bad}` isn't a time like 50ms or 1s")],
            "{why}"
        );
    }
    assert_eq!(tc("{ delay: 50ms, jitter: 5m }"), ["networks.slow.tc.jitter: `5m` isn't a time like 5ms"]);
    assert_eq!(tc("{ rate: 10mb }"), ["networks.slow.tc.rate: `10mb` isn't a rate like 10mbit or 512kbit"]);
}

#[test]
fn on_docker_a_networks_own_block_overlaps_no_other() {
    assert_eq!(
        with("  b: { cidr: 192.168.1.0/24, docker: { cidr: 10.0.0.0/24 } }\n"),
        ["networks.b.docker.cidr: on Docker, overlaps network `lan`; choose another block"]
    );
}
