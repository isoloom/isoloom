//! Network appliances on Docker (`docker.appliance`): Cisco IOL wired as vrnetlab expects.

use std::path::Path;

use isoloom_core::{Target, generate, load, parse, refusal, validate};

fn example() -> isoloom_core::Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cisco-iol")).unwrap()
}

#[test]
fn iol_gets_its_files_its_interfaces_in_order_and_its_addresses() {
    let spec = example();
    assert_eq!(validate(&spec), vec![]);
    let files = generate(&spec, Target::Docker).unwrap();
    let file = |p: &str| files.iter().find(|f| f.path.ends_with(p)).unwrap_or_else(|| panic!("no {p}")).contents.clone();
    let cfg = file("appliances/r1/base.txt");
    assert!(cfg.contains("hostname r1"), "{cfg}");
    assert!(
        cfg.contains("interface Ethernet0/1\n description site-a\n ip address 10.91.1.1 255.255.255.0"),
        "{cfg}"
    );
    assert!(
        cfg.contains("interface Ethernet0/2\n description wan\n ip address 10.91.0.2 255.255.255.248"),
        "{cfg}"
    );
    assert!(cfg.contains("vrf forwarding isoloom-mgmt") && cfg.contains("ip address 10.255.255.10 255.255.255.0"));
    assert_eq!(file("appliances/r1/NETMAP"), "1:0/0 513:0/0\n1:0/1 513:0/1\n1:0/2 513:0/2\n");
    assert!(file("appliances/r2/NETMAP").starts_with("2:0/0"), "each IOL its own instance number");
    assert!(file("appliances/r1/iouyap.ini").contains("[513:0/2]\neth_dev = eth2"));
    let compose = file("compose.yml");
    // The management network first, then the machine's networks in order.
    let r1 = &compose[compose.find("\n  r1:\n").unwrap()..];
    let (mgmt, a, wan) = (r1.find("isoloom-mgmt:").unwrap(), r1.find("site-a:").unwrap(), r1.find("wan:").unwrap());
    assert!(mgmt < a && a < wan);
    assert!(r1.contains("priority: 1000") && r1.contains("priority: 999") && r1.contains("priority: 998"));
    assert!(r1.contains("../../configs/r1.cfg:/iol/own.txt:ro") && r1.contains("IOL_PID: '1'"));
    assert!(
        compose.contains("ip addr flush dev eth1 && ip addr flush dev eth2"),
        "IOS owns the data addresses"
    );
}

#[test]
fn appliances_are_docker_only_and_checked() {
    let spec = example();
    assert!(refusal(&spec, Target::Kubernetes).unwrap().contains("network appliance"));
    let problems = |yaml: &str| {
        validate(&parse(yaml).unwrap())
            .into_iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let base = "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n";
    let p = problems(&format!(
        "{base}  r: {{ networks: {{ lan: 1 }}, access: true, docker: {{ image: x, appliance: cisco-iol, idle: true }}, vm: {{ os: debian-12 }} }}\n"
    ));
    assert!(
        p.contains("Docker targets only") && p.contains("access machine") && p.contains("no `idle`"),
        "{p}"
    );
    let p = problems(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.255.255.0/24 }\nmachines:\n  r: { networks: { lan: 1 }, docker: { image: x, appliance: cisco-iol } }\n",
    );
    assert!(p.contains("management network"), "{p}");
}

fn compose_of(example: &str) -> (String, Vec<isoloom_core::GeneratedFile>) {
    let spec = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../examples/{example}"))).unwrap();
    assert_eq!(validate(&spec), vec![]);
    let files = generate(&spec, Target::Docker).unwrap();
    (files.iter().find(|f| f.path.ends_with("compose.yml")).unwrap().contents.clone(), files)
}

#[test]
fn interfaces_are_named_outright_in_the_order_the_image_expects() {
    // Docker's own attach order isn't reliable: each network names its interface.
    let (c, _) = compose_of("cisco-qemu");
    let csr = &c[c.find("\n  csr:\n").unwrap()..];
    let pos = |s: &str| csr.find(s).unwrap_or_else(|| panic!("no {s}"));
    assert!(pos("interface_name: eth0") < pos("interface_name: eth1") && pos("interface_name: eth1") < pos("interface_name: eth2"));
    assert!(
        csr.contains("isoloom-mgmt:\n        ipv4_address: 10.255.255.11\n        interface_name: eth0"),
        "{csr}"
    );
}

#[test]
fn qemu_images_get_launch_py_their_startup_config_and_kvm() {
    let (c, files) = compose_of("cisco-qemu");
    let file = |p: &str| files.iter().find(|f| f.path.ends_with(p)).unwrap().contents.clone();
    assert!(file("appliances/vios/base.cfg").contains("interface GigabitEthernet0/1\n description site-a\n ip address 10.92.1.1"));
    assert!(file("appliances/csr/base.cfg").contains("interface GigabitEthernet2\n description site-b\n ip address 10.92.2.1"));
    let csr = &c[c.find("\n  csr:\n").unwrap()..];
    assert!(
        csr.contains("privileged: true") && csr.contains("CLAB_INTFS: '2'") && csr.contains("CONNECTION_MODE: tc"),
        "{csr}"
    );
    assert!(csr.contains("exec uv run /launch.py --username admin --password admin --hostname csr --connection-mode tc"));
    assert!(csr.contains("../../configs/csr.cfg:/config/own.cfg:ro"));
    let vios = &c[c.find("\n  vios:\n").unwrap()..];
    assert!(vios.contains("CLAB_MGMT_PASSTHROUGH: 'true'"));
}

#[test]
fn dynamips_builds_its_emulator_and_boots_the_firmware() {
    let (c, files) = compose_of("cisco-dynamips");
    let r1 = &c[c.find("\n  r1:\n").unwrap()..];
    assert!(
        r1.contains("build:\n      context: ./appliances/r1/build") && !r1.split("\n  r1-routes:").next().unwrap().contains("image:"),
        "{r1}"
    );
    assert!(r1.contains("../../images/c7200.bin:/firmware/ios.bin:ro"));
    assert!(r1.contains("-p 1:PA-2FE-TX -s 0:0:linux_eth:eth1 -s 1:0:linux_eth:eth2"), "{r1}");
    assert!(
        r1.contains("exec dynamips $$DYNAMIPS_ARGS /firmware/ios.bin"),
        "Compose mustn't interpolate the script's variable"
    );
    let dockerfile = files.iter().find(|f| f.path.ends_with("appliances/r1/build/Dockerfile")).unwrap();
    assert!(dockerfile.contents.contains("dynamips"));
    assert!(
        files
            .iter()
            .any(|f| f.path.ends_with("appliances/r1/base.cfg") && f.contents.contains("interface FastEthernet1/0\n description wan"))
    );
    let problems = |yaml: &str| {
        validate(&parse(yaml).unwrap())
            .into_iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let p = problems(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  r: { networks: { lan: 1 }, docker: { appliance: cisco-dynamips, image: x } }\n",
    );
    assert!(p.contains("not `image` or `build`") && p.contains("firmware"), "{p}");
}

#[test]
fn a_lans_switch_takes_an_access_port_per_vlan_then_a_trunk_port_per_trunk() {
    let (c, files) = compose_of("cisco-switch");
    assert!(!c.contains("isoloom-switch-office"), "no Linux switch when the LAN names its own: {c}");
    let cfg = files.iter().find(|f| f.path.ends_with("appliances/sw1/base.txt")).unwrap().contents.clone();
    assert!(
        cfg.contains("vtp mode transparent\n!\nvlan 10\n name office-vlan10\n!\nvlan 20\n name office-vlan20\n"),
        "{cfg}"
    );
    assert!(
        cfg.contains("interface Ethernet0/1\n description office-vlan10\n switchport mode access\n switchport access vlan 10\n no shutdown"),
        "{cfg}"
    );
    assert!(cfg.contains("interface Ethernet0/2\n description office-vlan20\n switchport mode access\n switchport access vlan 20\n"));
    assert!(
        cfg.contains("interface Ethernet0/3\n description trunk to files\n switchport trunk encapsulation dot1q\n switchport mode trunk\n switchport trunk allowed vlan 10,20\n no shutdown"),
        "{cfg}"
    );
    assert!(!cfg.contains(" ip address 10.92."), "a switch's ports take no address: {cfg}");
    let netmap = files.iter().find(|f| f.path.ends_with("appliances/sw1/NETMAP")).unwrap();
    assert!(netmap.contents.ends_with("1:0/3 513:0/3\n"), "{}", netmap.contents);
    // Compose: the VLAN networks at the controller address, then the trunk link, in order.
    let sw = &c[c.find("\n  sw1:\n").unwrap()..c.find("\n  sw1-routes:\n").unwrap()];
    assert!(
        sw.contains("office-vlan10:\n        ipv4_address: 10.92.10.253\n        interface_name: eth1\n        priority: 999"),
        "{sw}"
    );
    assert!(
        sw.contains("office-vlan20:\n        ipv4_address: 10.92.20.253\n        interface_name: eth2"),
        "{sw}"
    );
    assert!(
        sw.contains("isoloom-trunk-office-files:\n        interface_name: eth3\n        priority: 997"),
        "{sw}"
    );
    assert!(sw.contains("../../configs/sw1.cfg:/iol/own.txt:ro"));
    assert!(
        c.contains("ip addr flush dev eth1 && ip addr flush dev eth2 && ip addr flush dev eth3"),
        "IOS owns every data port"
    );
    // The trunk machine's end is unchanged; its sidecar waits for the switch to start.
    let files_routes = &c[c.find("\n  files-routes:\n").unwrap()..c.find("\n  staff:\n").unwrap()];
    assert!(files_routes.contains("name office.10 type vlan id 10"), "{files_routes}");
    assert!(files_routes.contains("      sw1:\n        condition: service_started"), "{files_routes}");
    // The switch has no address: no other machine resolves it.
    let staff = &c[c.find("\n  staff:\n").unwrap()..];
    assert!(!staff.split("\n  guest:").next().unwrap().contains("sw1:"), "{staff}");
}

#[test]
fn a_lans_switch_is_checked() {
    let problems = |yaml: &str| match parse(yaml) {
        Ok(spec) => validate(&spec).into_iter().map(|p| p.to_string()).collect::<Vec<_>>().join("\n"),
        Err(e) => e.to_string(),
    };
    let spec = |lan: &str, sw: &str| {
        format!(
            "version: 1\nname: t\nnetworks:\n  office: {{ cidr: 10.9.0.0/16, {lan} }}\n  other: {{ cidr: 10.8.0.0/16, vlans: {{ 10: {{ cidr: 10.8.10.0/24 }}, 20: {{ cidr: 10.8.20.0/24 }} }} }}\nmachines:\n  sw: {sw}\n  a: {{ networks: {{ office.vlan10: 10, office.vlan20: 10 }}, docker: {{ image: x }} }}\n"
        )
    };
    let vlans = "vlans: { 10: { cidr: 10.9.10.0/24 }, 20: { cidr: 10.9.20.0/24 } }";
    let good = "{ docker: { image: x, appliance: cisco-iol-l2 } }";
    assert_eq!(problems(&spec(&format!("switch: sw, {vlans}"), good)), "");
    assert_eq!(
        problems(&spec(&format!("switch: sw, {vlans}"), "{ docker: { image: x, appliance: cisco-vios-l2 } }")),
        ""
    );
    // The image's own spelling, as the changelog had it.
    assert_eq!(
        problems(&spec(&format!("switch: sw, {vlans}"), "{ docker: { image: x, appliance: cisco-viosl2 } }")),
        ""
    );
    // Not a switch appliance.
    let p = problems(&spec(&format!("switch: sw, {vlans}"), "{ docker: { image: x, appliance: cisco-iol } }"));
    assert!(p.contains("networks.office.switch: `sw` isn't a switch"), "{p}");
    let p = problems(&spec(&format!("switch: sw, {vlans}"), "{ docker: { image: x } }"));
    assert!(p.contains("isn't a switch"), "{p}");
    // No such machine.
    let p = problems(&spec(&format!("switch: nope, {vlans}"), good));
    assert!(p.contains("networks.office.switch: no machine named `nope`"), "{p}");
    // Its ports are derived.
    let p = problems(&spec(
        &format!("switch: sw, {vlans}"),
        "{ networks: { other.vlan10: 5 }, docker: { image: x, appliance: cisco-iol-l2 } }",
    ));
    assert!(
        p.contains("machines.sw.networks: `sw` switches LAN `office`") && p.contains("leave `networks` out"),
        "{p}"
    );
    let p = problems(&spec(
        &format!("switch: sw, {vlans}"),
        "{ services: [{ port: 22 }], docker: { image: x, appliance: cisco-iol-l2 } }",
    ));
    assert!(p.contains("no `services` or `aliases`"), "{p}");
    // A LAN without VLANs has no switch; a machine that isn't a switch still needs a network.
    let p = problems(&spec("switch: sw", good));
    assert!(p.contains("networks.office.switch: only a LAN split into `vlans` has a switch"), "{p}");
    assert!(p.contains("machines.sw.networks: attach the machine"), "{p}");
    // One LAN per switch.
    let p = problems(&spec(&format!("switch: sw, {vlans}"), good).replace("  other: { cidr: 10.8.0.0/16, ", "  other: { cidr: 10.8.0.0/16, switch: sw, "));
    assert!(p.contains("networks.other.switch: `sw` already switches LAN `office`"), "{p}");
    // Machines joining the LAN directly bypass the switch.
    let direct = spec(&format!("switch: sw, {vlans}"), good)
        .replace("cidr: 10.9.0.0/16", "cidr: 10.9.0.0/24")
        .replace("10.9.10.0/24", "10.9.0.64/26")
        .replace("10.9.20.0/24", "10.9.0.128/26")
        .replace("office.vlan10: 10, office.vlan20: 10", "office.vlan10: 70, office.vlan20: 130")
        + "  b: { networks: { office: 10 }, docker: { image: x } }\n";
    let p = problems(&direct);
    assert!(p.contains("networks.office.switch: machines join `office` directly"), "{p}");
    // A switch is one machine.
    let p = problems(&spec(
        &format!("switch: sw, {vlans}"),
        "{ count: 2, docker: { image: x, appliance: cisco-iol-l2 } }",
    ));
    assert!(p.contains("networks.office.switch: `sw` has a count"), "{p}");
    // Docker targets only, like every appliance.
    let s = parse(&spec(&format!("switch: sw, {vlans}"), good)).unwrap();
    assert!(refusal(&s, Target::Kubernetes).unwrap().contains("network appliance"));
}
