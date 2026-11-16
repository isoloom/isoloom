//! Network appliances on Docker (`app.appliance`): Cisco IOL wired as vrnetlab expects.

use isoloom_spec::{Target, parse};
use isoloom_targets::{GeneratedFile, Lab, generate, refusal};

use isoloom_test_support::{yaml, yaml_at, yaml_keys_at, yaml_str, yaml_strs};

use crate::testing::{Outcome, example, file};

/// The stanza of an IOS configuration `cfg` that opens with the line `head`: the indented lines
/// under it, each trimmed.
fn stanza<'c>(cfg: &'c str, head: &str) -> Outcome<Vec<&'c str>> {
    let mut lines = cfg.lines().skip_while(|l| *l != head);
    lines.next().ok_or_else(|| format!("no `{head}` in:\n{cfg}"))?;
    Ok(lines.take_while(|l| l.starts_with(' ')).map(str::trim).collect())
}

/// What service `name` of the parsed Compose file `c` runs: its entrypoint's and command's
/// items, one a line.
fn script(c: &serde_yaml_ng::Value, name: &str) -> String {
    [
        yaml_strs(c, &format!("services.{name}.entrypoint")),
        yaml_strs(c, &format!("services.{name}.command")),
    ]
    .concat()
    .join("\n")
}

/// A spec's problems, found parsing it or after, one `path: reason` a line.
fn problems(yaml: &str) -> Outcome<String> {
    let found = match parse(yaml) {
        Ok(spec) => Lab::new(spec).problems(),
        Err(isoloom_spec::Error::Invalid { problems, .. }) => problems,
        Err(e) => return Err(e.into()),
    };
    Ok(found.iter().map(isoloom_spec::syntax::Problem::line).collect::<Vec<_>>().join("\n"))
}

#[test]
fn iol_gets_its_files_its_interfaces_in_order_and_its_addresses() -> Outcome {
    let (compose, files) = compose_of("cisco-iol")?;
    let cfg = file(&files, "appliances/r1/base.txt")?;
    assert!(cfg.lines().any(|l| l == "hostname r1"), "{cfg}");
    assert_eq!(
        stanza(cfg, "interface Ethernet0/1")?[..2],
        ["description site-a", "ip address 10.91.1.1 255.255.255.0"]
    );
    assert_eq!(
        stanza(cfg, "interface Ethernet0/2")?[..2],
        ["description wan", "ip address 10.91.0.2 255.255.255.248"]
    );
    let mgmt = stanza(cfg, "interface Ethernet0/0")?;
    assert!(
        mgmt.contains(&"vrf forwarding isoloom-mgmt") && mgmt.contains(&"ip address 10.255.255.10 255.255.255.0"),
        "{mgmt:?}"
    );
    assert_eq!(file(&files, "appliances/r1/NETMAP")?, "1:0/0 513:0/0\n1:0/1 513:0/1\n1:0/2 513:0/2\n");
    assert!(file(&files, "appliances/r2/NETMAP")?.starts_with("2:0/0"), "each IOL its own instance number");
    let iouyap = isoloom_test_support::ini_groups(file(&files, "appliances/r1/iouyap.ini")?);
    assert_eq!(iouyap.get("513:0/2").map(Vec::as_slice), Some(&["eth_dev = eth2".to_owned()][..]), "{iouyap:?}");
    // The project's files are mounted read-only, relabelled for SELinux (`ro,z`).
    // The management network first, then the machine's networks in order.
    let c = yaml(&compose);
    let networks = yaml_keys_at(&c, "services.r1.networks");
    let rank = |n: &str| networks.iter().position(|k| k == n).ok_or_else(|| format!("{n}: {networks:?}"));
    assert!(rank("isoloom-mgmt")? < rank("site-a")? && rank("site-a")? < rank("wan")?, "{networks:?}");
    let priority = |n: &str| yaml_str(&c, &format!("services.r1.networks.{n}.priority"));
    assert_eq!([priority("isoloom-mgmt"), priority("site-a"), priority("wan")], ["1000", "999", "998"]);
    assert!(yaml_strs(&c, "services.r1.volumes").contains(&"../../configs/r1.cfg:/iol/own.txt:ro,z".to_owned()));
    assert_eq!(yaml_str(&c, "services.r1.environment.IOL_PID"), "1");
    let runs = script(&c, "isoloom-routes-r1");
    assert!(
        runs.contains("ip addr flush dev eth1 && ip addr flush dev eth2"),
        "IOS owns the data addresses: {runs}"
    );
    Ok(())
}

#[test]
fn appliances_are_docker_only_and_checked() -> Outcome {
    let spec = example("cisco-iol")?;
    let why = refusal(&spec, Target::Kubernetes)?.ok_or("Kubernetes takes an appliance")?;
    assert!(why.contains("network appliance"), "{why}");
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
    let y = yaml(&c);
    let names: Vec<String> = yaml_keys_at(&y, "services.csr.networks")
        .iter()
        .map(|n| yaml_str(&y, &format!("services.csr.networks.{n}.interface_name")))
        .collect();
    assert_eq!(names, ["eth0", "eth1", "eth2"], "each network names its interface, in order");
    assert_eq!(yaml_str(&y, "services.csr.networks.isoloom-mgmt.ipv4_address"), "10.255.255.11");
    assert_eq!(yaml_str(&y, "services.csr.networks.isoloom-mgmt.interface_name"), "eth0");
    Ok(())
}

#[test]
fn qemu_images_get_launch_py_their_startup_config_and_kvm() -> Outcome {
    let (c, files) = compose_of("cisco-qemu")?;
    let vios = stanza(file(&files, "appliances/vios/base.cfg")?, "interface GigabitEthernet0/1")?;
    assert!(
        vios.first() == Some(&"description site-a") && vios[1].starts_with("ip address 10.92.1.1 "),
        "{vios:?}"
    );
    let csr_cfg = stanza(file(&files, "appliances/csr/base.cfg")?, "interface GigabitEthernet2")?;
    assert!(
        csr_cfg.first() == Some(&"description site-b") && csr_cfg[1].starts_with("ip address 10.92.2.1 "),
        "{csr_cfg:?}"
    );
    let y = yaml(&c);
    let csr = |key: &str| yaml_str(&y, &format!("services.csr.{key}"));
    assert_eq!(
        [csr("privileged"), csr("environment.CLAB_INTFS"), csr("environment.CONNECTION_MODE")],
        ["true", "2", "tc"]
    );
    let launch = "exec uv run /launch.py --username admin --password admin --hostname csr --connection-mode tc";
    let entrypoint = yaml_strs(&y, "services.csr.entrypoint");
    assert!(entrypoint.last().is_some_and(|script| script.contains(launch)), "{entrypoint:?}");
    assert!(yaml_strs(&y, "services.csr.volumes").contains(&"../../configs/csr.cfg:/config/own.cfg:ro,z".to_owned()));
    assert_eq!(yaml_str(&y, "services.vios.environment.CLAB_MGMT_PASSTHROUGH"), "true");
    Ok(())
}

#[test]
fn dynamips_builds_its_emulator_and_boots_the_firmware() -> Outcome {
    let (c, files) = compose_of("cisco-dynamips")?;
    let y = yaml(&c);
    let r1 = script(&y, "r1");
    assert_eq!(yaml_str(&y, "services.r1.build.context"), "./appliances/r1/build");
    assert!(!yaml_keys_at(&y, "services.r1").contains(&"image".to_owned()), "it builds its own: {r1}");
    assert!(yaml_strs(&y, "services.r1.volumes").contains(&"../../images/c7200.bin:/firmware/ios.bin:ro,z".to_owned()));
    let args = yaml_str(&y, "services.r1.environment.DYNAMIPS_ARGS");
    assert!(args.ends_with("-p 1:PA-2FE-TX -s 0:0:linux_eth:eth1 -s 1:0:linux_eth:eth2"), "{args}");
    assert!(
        r1.contains("exec dynamips $$DYNAMIPS_ARGS /firmware/ios.bin"),
        "Compose mustn't interpolate the script's variable"
    );
    let dockerfile = file(&files, "appliances/r1/build/Dockerfile")?;
    assert!(dockerfile.lines().any(|l| l.starts_with("RUN ") && l.contains("dynamips")), "{dockerfile}");
    let wan = stanza(file(&files, "appliances/r1/base.cfg")?, "interface FastEthernet1/0")?;
    assert_eq!(wan.first(), Some(&"description wan"), "{wan:?}");
    let p = problems(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  r: { networks: { lan: 1 }, app: { appliance: cisco-dynamips, image: x } }\n",
    )?;
    assert!(p.contains("boots its `firmware`; Isoloom builds its image, so no `image`"), "{p}");
    Ok(())
}

#[test]
fn a_lans_switch_takes_an_access_port_per_vlan_then_a_trunk_port_per_trunk() -> Outcome {
    let (c, files) = compose_of("cisco-switch")?;
    let services = yaml_keys_at(&yaml(&c), "services");
    assert!(
        !services.contains(&"isoloom-switch-office".to_owned()),
        "no Linux switch when the LAN names its own: {services:?}"
    );
    switch_ports(&files)?;
    switch_service(&c);
    Ok(())
}

/// cisco-switch's switch configuration: an access port per VLAN, then a trunk port per trunk.
fn switch_ports(files: &[GeneratedFile]) -> Outcome {
    let cfg = file(files, "appliances/sw1/base.txt")?;
    assert!(cfg.lines().any(|l| l == "vtp mode transparent"), "{cfg}");
    assert_eq!(stanza(cfg, "vlan 10")?, ["name office-vlan10"]);
    assert_eq!(stanza(cfg, "vlan 20")?, ["name office-vlan20"]);
    let access = |vlan: u8| {
        [
            format!("description office-vlan{vlan}"),
            "switchport mode access".to_owned(),
            format!("switchport access vlan {vlan}"),
            "no shutdown".to_owned(),
        ]
    };
    assert_eq!(stanza(cfg, "interface Ethernet0/1")?, access(10));
    assert_eq!(stanza(cfg, "interface Ethernet0/2")?, access(20));
    assert_eq!(
        stanza(cfg, "interface Ethernet0/3")?,
        [
            "description trunk to files",
            "switchport trunk encapsulation dot1q",
            "switchport mode trunk",
            "switchport trunk allowed vlan 10,20",
            "no shutdown"
        ]
    );
    for port in 1..=3 {
        let lines = stanza(cfg, &format!("interface Ethernet0/{port}"))?;
        assert!(
            !lines.iter().any(|l| l.starts_with("ip address")),
            "a switch's ports take no address: {lines:?}"
        );
    }
    let netmap = file(files, "appliances/sw1/NETMAP")?;
    assert!(netmap.ends_with("1:0/3 513:0/3\n"), "{netmap}");
    Ok(())
}

/// cisco-switch's Compose file: the switch's networks, and the machines around it.
fn switch_service(c: &str) {
    // Compose: the VLAN networks at the controller address, then the trunk link, in order.
    let y = yaml(c);
    let net = |n: &str, k: &str| yaml_at(&y, &format!("services.sw1.networks.{n}.{k}")).map(|_| yaml_str(&y, &format!("services.sw1.networks.{n}.{k}")));
    assert_eq!(
        [
            net("office-vlan10", "ipv4_address"),
            net("office-vlan10", "interface_name"),
            net("office-vlan10", "priority")
        ],
        [Some("10.92.10.253".into()), Some("eth1".into()), Some("999".into())]
    );
    assert_eq!(
        [net("office-vlan20", "ipv4_address"), net("office-vlan20", "interface_name")],
        [Some("10.92.20.253".into()), Some("eth2".into())]
    );
    assert_eq!(
        [
            net("isoloom-trunk-office-files", "interface_name"),
            net("isoloom-trunk-office-files", "priority")
        ],
        [Some("eth3".into()), Some("997".into())]
    );
    assert!(yaml_strs(&y, "services.sw1.volumes").contains(&"../../configs/sw1.cfg:/iol/own.txt:ro,z".to_owned()));
    let runs = script(&y, "isoloom-routes-sw1");
    assert!(
        runs.contains("ip addr flush dev eth1 && ip addr flush dev eth2 && ip addr flush dev eth3"),
        "IOS owns every data port: {runs}"
    );
    // The trunk machine's end is unchanged; its sidecar waits for the switch to start.
    let files_routes = script(&y, "isoloom-routes-files");
    assert!(files_routes.contains("name office.10 type vlan id 10"), "{files_routes}");
    assert_eq!(yaml_str(&y, "services.isoloom-routes-files.depends_on.sw1.condition"), "service_started");
    // The switch has no address: no other machine resolves it.
    let hosts = yaml_strs(&y, "services.staff.extra_hosts");
    assert!(!hosts.iter().any(|h| h.starts_with("sw1:")), "{hosts:?}");
    assert!(!yaml_keys_at(&y, "services.staff.depends_on").contains(&"sw1".to_owned()));
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
    assert!(refusal(&s, Target::Kubernetes).unwrap().unwrap().contains("network appliance"));
    Ok(())
}
