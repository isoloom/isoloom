//! Network appliances on Docker (`app.appliance`): Cisco IOL wired as vrnetlab expects.

use isoloom_spec::{Target, parse};
use isoloom_targets::{GeneratedFile, Lab, generate, refusal};

use crate::testing::{Outcome, between, example, file, from};

/// A spec's problems, found parsing it or after, one `path: reason` a line.
fn problems(yaml: &str) -> Outcome<String> {
    let found = match parse(yaml) {
        Ok(spec) => Lab::new(spec).problems(),
        Err(isoloom_spec::LoadError::Invalid { problems, .. }) => problems,
        Err(e) => return Err(e.into()),
    };
    Ok(found.iter().map(isoloom_spec::syntax::Problem::line).collect::<Vec<_>>().join("\n"))
}

#[test]
fn iol_gets_its_files_its_interfaces_in_order_and_its_addresses() -> Outcome {
    let (compose, files) = compose_of("cisco-iol")?;
    let cfg = file(&files, "appliances/r1/base.txt")?;
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
    assert_eq!(file(&files, "appliances/r1/NETMAP")?, "1:0/0 513:0/0\n1:0/1 513:0/1\n1:0/2 513:0/2\n");
    assert!(file(&files, "appliances/r2/NETMAP")?.starts_with("2:0/0"), "each IOL its own instance number");
    assert!(file(&files, "appliances/r1/iouyap.ini")?.contains("[513:0/2]\neth_dev = eth2"));
    // The management network first, then the machine's networks in order.
    let r1 = from(&compose, "\n  r1:\n")?;
    let (mgmt, a, wan) = (r1.find("isoloom-mgmt:").unwrap(), r1.find("site-a:").unwrap(), r1.find("wan:").unwrap());
    assert!(mgmt < a && a < wan);
    assert!(r1.contains("priority: 1000") && r1.contains("priority: 999") && r1.contains("priority: 998"));
    assert!(r1.contains("../../configs/r1.cfg:/iol/own.txt:ro") && r1.contains("IOL_PID: '1'"));
    assert!(
        compose.contains("ip addr flush dev eth1 && ip addr flush dev eth2"),
        "IOS owns the data addresses"
    );
    Ok(())
}

#[test]
fn appliances_are_docker_only_and_checked() -> Outcome {
    let spec = example("cisco-iol")?;
    assert!(refusal(&spec, Target::Kubernetes).unwrap().contains("network appliance"));
    let base = "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n";
    // Steps would be its VM form, which an appliance hasn't.
    let steps = problems(&format!(
        "{base}  r: {{ networks: {{ lan: 1 }}, os: debian@12, app: {{ image: x, appliance: cisco-iol, steps: [{{ run: a.sh }}] }} }}\n"
    ))?;
    assert!(steps.contains("machines.r.app.steps: an appliance has no VM form"), "{steps}");
    let access = problems(&format!(
        "{base}  r: {{ networks: {{ lan: 1 }}, access: true, app: {{ image: x, appliance: cisco-iol }} }}\n"
    ))?;
    assert!(access.contains("access machine"), "{access}");
    let mgmt = problems(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.255.255.0/24 }\nmachines:\n  r: { networks: { lan: 1 }, app: { image: x, appliance: cisco-iol } }\n",
    )?;
    assert!(mgmt.contains("management network"), "{mgmt}");
    Ok(())
}

/// The Docker target's Compose file for the example `name`, and all its files.
fn compose_of(name: &str) -> Outcome<(String, Vec<GeneratedFile>)> {
    let lab = example(name)?;
    let problems = crate::testing::problems(&lab);
    if !problems.is_empty() {
        return Err(format!("{name} isn't valid: {problems:?}").into());
    }
    let files = generate(&lab, Target::Docker)?;
    Ok((file(&files, "compose.yml")?.to_owned(), files))
}

#[test]
fn interfaces_are_named_outright_in_the_order_the_image_expects() -> Outcome {
    // Docker's own attach order isn't reliable: each network names its interface.
    let (c, _) = compose_of("cisco-qemu")?;
    let csr = from(&c, "\n  csr:\n")?;
    let pos = |s: &str| csr.find(s).unwrap_or_else(|| panic!("no {s}"));
    assert!(pos("interface_name: eth0") < pos("interface_name: eth1") && pos("interface_name: eth1") < pos("interface_name: eth2"));
    assert!(
        csr.contains("isoloom-mgmt:\n        ipv4_address: 10.255.255.11\n        interface_name: eth0"),
        "{csr}"
    );
    Ok(())
}

#[test]
fn qemu_images_get_launch_py_their_startup_config_and_kvm() -> Outcome {
    let (c, files) = compose_of("cisco-qemu")?;
    assert!(file(&files, "appliances/vios/base.cfg")?.contains("interface GigabitEthernet0/1\n description site-a\n ip address 10.92.1.1"));
    assert!(file(&files, "appliances/csr/base.cfg")?.contains("interface GigabitEthernet2\n description site-b\n ip address 10.92.2.1"));
    let csr = from(&c, "\n  csr:\n")?;
    assert!(
        csr.contains("privileged: true") && csr.contains("CLAB_INTFS: '2'") && csr.contains("CONNECTION_MODE: tc"),
        "{csr}"
    );
    assert!(csr.contains("exec uv run /launch.py --username admin --password admin --hostname csr --connection-mode tc"));
    assert!(csr.contains("../../configs/csr.cfg:/config/own.cfg:ro"));
    let vios = from(&c, "\n  vios:\n")?;
    assert!(vios.contains("CLAB_MGMT_PASSTHROUGH: 'true'"));
    Ok(())
}

#[test]
fn dynamips_builds_its_emulator_and_boots_the_firmware() -> Outcome {
    let (c, files) = compose_of("cisco-dynamips")?;
    let r1 = from(&c, "\n  r1:\n")?;
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
    let p = problems(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  r: { networks: { lan: 1 }, app: { appliance: cisco-dynamips, image: x } }\n",
    )?;
    assert!(p.contains("boots its `firmware`; Isoloom builds its image, so no `image`"), "{p}");
    Ok(())
}

#[test]
fn a_lans_switch_takes_an_access_port_per_vlan_then_a_trunk_port_per_trunk() -> Outcome {
    let (c, files) = compose_of("cisco-switch")?;
    assert!(!c.contains("isoloom-switch-office"), "no Linux switch when the LAN names its own: {c}");
    switch_ports(&files)?;
    switch_service(&c)?;
    Ok(())
}

/// cisco-switch's switch configuration: an access port per VLAN, then a trunk port per trunk.
fn switch_ports(files: &[GeneratedFile]) -> Outcome {
    let cfg = file(files, "appliances/sw1/base.txt")?;
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
    let netmap = file(files, "appliances/sw1/NETMAP")?;
    assert!(netmap.ends_with("1:0/3 513:0/3\n"), "{netmap}");
    Ok(())
}

/// cisco-switch's Compose file: the switch's networks, and the machines around it.
fn switch_service(c: &str) -> Outcome {
    // Compose: the VLAN networks at the controller address, then the trunk link, in order.
    let sw = between(c, "\n  sw1:\n", "\n  sw1-routes:\n")?;
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
    let files_routes = between(c, "\n  files-routes:\n", "\n  staff:\n")?;
    assert!(files_routes.contains("name office.10 type vlan id 10"), "{files_routes}");
    assert!(files_routes.contains("      sw1:\n        condition: service_started"), "{files_routes}");
    // The switch has no address: no other machine resolves it.
    let staff = from(c, "\n  staff:\n")?;
    assert!(!staff.split("\n  guest:").next().unwrap_or_default().contains("sw1:"), "{staff}");
    Ok(())
}

#[test]
fn a_lans_switch_is_checked() -> Outcome {
    let spec = |lan: &str, sw: &str| {
        format!(
            "version: 1\nname: t\nnetworks:\n  office: {{ cidr: 10.9.0.0/16, {lan} }}\n  other: {{ cidr: 10.8.0.0/16, vlans: {{ 10: {{ cidr: 10.8.10.0/24 }}, 20: {{ cidr: 10.8.20.0/24 }} }} }}\nmachines:\n  sw: {sw}\n  a: {{ networks: {{ office.vlan10: 10, office.vlan20: 10 }}, app: {{ image: x }} }}\n"
        )
    };
    let vlans = "vlans: { 10: { cidr: 10.9.10.0/24 }, 20: { cidr: 10.9.20.0/24 } }";
    let good = "{ app: { image: x, appliance: cisco-iol-l2 } }";
    assert_eq!(problems(&spec(&format!("switch: sw, {vlans}"), good))?, "");
    assert_eq!(
        problems(&spec(&format!("switch: sw, {vlans}"), "{ app: { image: x, appliance: cisco-vios-l2 } }"))?,
        ""
    );
    // Not a switch appliance.
    let router = problems(&spec(&format!("switch: sw, {vlans}"), "{ app: { image: x, appliance: cisco-iol } }"))?;
    assert!(router.contains("networks.office.switch: `sw` isn't a switch"), "{router}");
    let plain = problems(&spec(&format!("switch: sw, {vlans}"), "{ app: { image: x } }"))?;
    assert!(plain.contains("isn't a switch"), "{plain}");
    // No such machine.
    let nope = problems(&spec(&format!("switch: nope, {vlans}"), good))?;
    assert!(nope.contains("networks.office.switch: no machine named `nope`"), "{nope}");
    // Its ports are derived.
    let networks = problems(&spec(
        &format!("switch: sw, {vlans}"),
        "{ networks: { other.vlan10: 5 }, app: { image: x, appliance: cisco-iol-l2 } }",
    ))?;
    assert!(
        networks.contains("machines.sw.networks: `sw` switches LAN `office`") && networks.contains("leave `networks` out"),
        "{networks}"
    );
    let ports = problems(&spec(
        &format!("switch: sw, {vlans}"),
        "{ ports: { ssh: 22 }, app: { image: x, appliance: cisco-iol-l2 } }",
    ))?;
    assert!(ports.contains("no `ports` or `aliases`"), "{ports}");
    // A LAN without VLANs has no switch; a machine that isn't a switch still needs a network.
    let unsplit = problems(&spec("switch: sw", good))?;
    assert!(
        unsplit.contains("networks.office.switch: only a LAN split into `vlans` has a switch"),
        "{unsplit}"
    );
    assert!(unsplit.contains("machines.sw.networks: attach the machine"), "{unsplit}");
    // One LAN per switch.
    let twice = problems(&spec(&format!("switch: sw, {vlans}"), good).replace("  other: { cidr: 10.8.0.0/16, ", "  other: { cidr: 10.8.0.0/16, switch: sw, "))?;
    assert!(twice.contains("networks.other.switch: `sw` already switches LAN `office`"), "{twice}");
    // Machines joining the LAN directly bypass the switch.
    let direct = spec(&format!("switch: sw, {vlans}"), good)
        .replace("cidr: 10.9.0.0/16", "cidr: 10.9.0.0/24")
        .replace("10.9.10.0/24", "10.9.0.64/26")
        .replace("10.9.20.0/24", "10.9.0.128/26")
        .replace("office.vlan10: 10, office.vlan20: 10", "office.vlan10: 70, office.vlan20: 130")
        + "  b: { networks: { office: 10 }, app: { image: x } }\n";
    let bypassed = problems(&direct)?;
    assert!(bypassed.contains("networks.office.switch: machines join `office` directly"), "{bypassed}");
    // A switch is one machine.
    let counted = problems(&spec(
        &format!("switch: sw, {vlans}"),
        "{ count: 2, app: { image: x, appliance: cisco-iol-l2 } }",
    ))?;
    assert!(counted.contains("networks.office.switch: `sw` has a count"), "{counted}");
    // Docker targets only, like every appliance.
    let s = Lab::new(parse(&spec(&format!("switch: sw, {vlans}"), good))?);
    assert!(refusal(&s, Target::Kubernetes).unwrap().contains("network appliance"));
    Ok(())
}
