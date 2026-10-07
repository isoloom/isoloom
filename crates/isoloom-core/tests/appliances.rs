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
