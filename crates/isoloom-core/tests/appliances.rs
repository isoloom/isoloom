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
