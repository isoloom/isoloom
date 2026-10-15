use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::num::{NonZeroU16, NonZeroU32};

use isoloom_manifest::{
    Action, Arch, Argv, Check, CheckKind, Failure, GeneratorName, HostArgv, Machine, Name, OutputRole, OutputSource, Part, Placeholder, Port, Protocol,
    PublishMode, Role, Run, Template, ToolName, When,
};
use isoloom_plan::PublishedPort;
use isoloom_plan::checks::{Host, HttpExpect, HttpRequest, Position, Probe, Resolved, TcpExchange, TcpExpect, Url};
use isoloom_spec::model::{self, Target};

use super::{act, addresses, arch, argv, check, cleanup, generator, lab, networks, noted, outputs, outputs_at, port, program, run, timeout_s, tolerated, tool};
use crate::testing;

fn resolved(name: &str, wait: u32) -> Resolved {
    Resolved {
        name: name.into(),
        wait,
        derived: false,
        position: isoloom_plan::checks::Position::Networks,
        probe: Probe::Script { path: "c.sh".into() },
    }
}

#[test]
fn a_runner_gets_its_waits_plus_15_s_a_check_plus_60_s() {
    let (a, b) = (resolved("a", 300), resolved("b", 600));
    assert_eq!(timeout_s(&[&a, &b]), 300 + 600 + 2 * 15 + 60);
    assert_eq!(timeout_s(&[]), 60);
}

#[test]
fn a_clean_up_runs_always_and_is_tolerated() {
    let s = cleanup(run("halt", argv("vagrant", &["halt"]).unwrap(), 300).unwrap());
    assert_eq!((s.when, s.failure), (When::Always, isoloom_manifest::Failure::Tolerate));
    assert_eq!(s.timeout_s.map(NonZeroU32::get), Some(300));
    assert!(program("sh").is_err(), "a shell is never a program");
}

#[test]
fn isoloom_writes_the_manifest_at_its_own_version() {
    let g = generator().unwrap();
    assert_eq!(g.name, GeneratorName::Isoloom);
    assert_eq!(g.version.to_string(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn the_lab_is_named_after_the_environment_and_its_instance() {
    const LAB: &str = "version: 1\nname: demo\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n";
    let one = lab(&testing::env(LAB, Target::Docker)).unwrap();
    assert_eq!((one.name.to_string(), one.instance, one.message), ("demo".to_owned(), None, None));
    let spec = testing::lab(LAB).for_target(Target::Docker);
    let third = lab(&isoloom_plan::Environment::build(&spec, Target::Docker, Some(3))).unwrap();
    assert_eq!(third.instance, isoloom_manifest::Instance::new(3).ok());
}

#[test]
fn a_step_runs_a_program_or_a_driver_action_its_way() {
    let HostArgv::Every(a) = argv("docker", &["compose", "up"]).unwrap() else {
        panic!("the same on every host")
    };
    assert_eq!(a, Argv::new(program("docker").unwrap(), vec![Template::text("compose"), Template::text("up")]));
    let s = run("up", argv("docker", &[]).unwrap(), 0).unwrap();
    assert_eq!(
        (s.id.to_string(), s.timeout_s, s.when, s.failure),
        ("up".to_owned(), None, When::IfOk, Failure::Fail)
    );
    let pin = act("pin", Action::ComposePinPorts).unwrap();
    assert!(
        matches!(
            pin.run,
            Run::Action {
                action: Action::ComposePinPorts,
                ..
            }
        ),
        "{pin:?}"
    );
    assert!(run("Not An Id", argv("docker", &[]).unwrap(), 1).is_err());
    let t = tolerated(pin.clone());
    assert_eq!((t.when, t.failure), (When::IfOk, Failure::Tolerate));
    assert_eq!(noted(pin, "pinning the ports").note.as_deref(), Some("pinning the ports"));
}

#[test]
fn a_tool_is_probed_its_way_everywhere() {
    let probe = argv("docker", &["--version"]).unwrap();
    let t = tool("docker", probe.clone()).unwrap();
    assert_eq!((t.name, t.probe), (ToolName::parse("docker").unwrap(), probe));
    assert!(t.min_version.is_none() && t.container.is_none() && t.hosts.is_empty() && t.providers.is_empty());
}

#[test]
fn each_network_keeps_its_block_its_internet_and_its_native_name() {
    let env = testing::env(
        "version: 1\nname: n\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\n  dark: { cidr: 10.8.0.0/24, internet: false }\nmachines:\n  web: { networks: { lan: 10, dark: 10 }, os: debian@12 }\n",
        Target::Vagrant,
    );
    let nets = networks(&env, |n| Template::text(&format!("lab-{n}"))).unwrap();
    let seen: Vec<(String, String, bool, Option<String>)> = nets
        .iter()
        .map(|n| (n.name.to_string(), n.cidr.to_string(), n.internet, n.native.as_ref().map(ToString::to_string)))
        .collect();
    assert_eq!(
        seen,
        [
            ("lan".to_owned(), "10.9.0.0/24".to_owned(), true, Some("lab-lan".to_owned())),
            ("dark".to_owned(), "10.8.0.0/24".to_owned(), false, Some("lab-dark".to_owned())),
        ]
    );
}

#[test]
fn a_machine_is_at_its_ipv4_address_else_at_its_ipv6_one() {
    let env = testing::env(
        "version: 1\nname: a\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\n  six: { cidr: 10.7.0.0/24, internet: false, ipv6: { cidr: \"fd00:7::/64\", only: true } }\nmachines:\n  web: { networks: { lan: 10, six: 10 }, os: debian@12 }\n",
        Target::Vagrant,
    );
    let mut web = env.machines["web"].clone();
    let at = addresses(&web).unwrap();
    assert_eq!(at[&Name::parse("lan").unwrap()], IpAddr::V4(Ipv4Addr::new(10, 9, 0, 10)));
    assert_eq!(
        at[&Name::parse("six").unwrap()],
        IpAddr::V4(Ipv4Addr::new(10, 7, 0, 10)),
        "the plan gives an IPv4 address there too"
    );
    // A network the plan gives no IPv4 address on: its IPv6 one.
    web.addresses.shift_remove("six");
    let fallback = addresses(&web).unwrap();
    assert_eq!(fallback[&Name::parse("six").unwrap()], IpAddr::V6("fd00:7::10".parse::<Ipv6Addr>().unwrap()));
    assert_eq!(fallback.len(), 2);
    assert_eq!((arch(model::Arch::Amd64), arch(model::Arch::Arm64)), (Arch::Amd64, Arch::Arm64));
}

fn model_port(port: u16, protocol: model::Protocol, http: bool) -> model::Port {
    model::Port {
        port,
        name: Some("site".into()),
        http,
        tls: false,
        publish: None,
        fixed: false,
        dormant: false,
        protocol,
    }
}

fn published(fixed: bool) -> PublishedPort {
    PublishedPort {
        name: "site".into(),
        port: 80,
        publish: 8080,
        protocol: model::Protocol::Tcp,
        fixed,
    }
}

#[test]
fn a_published_port_is_fixed_or_ephemeral_at_the_host_address() {
    let host = Template::text("127.0.0.1");
    let p = port("site", &model_port(80, model::Protocol::Tcp, true), &[published(true)], &host).unwrap();
    let publish = p.publish.expect("published");
    assert_eq!(
        (publish.mode, publish.preferred, publish.address),
        (PublishMode::Fixed, NonZeroU16::new(8080), Some(host.clone()))
    );
    let ephemeral = port("site", &model_port(80, model::Protocol::Tcp, true), &[published(false)], &host).unwrap();
    assert_eq!(ephemeral.publish.map(|x| x.mode), Some(PublishMode::Ephemeral));
    let dns = port("dns", &model_port(53, model::Protocol::Udp, false), &[published(true)], &host).unwrap();
    assert_eq!(
        (dns.protocol, dns.http, dns.publish),
        (Protocol::Udp, false, None),
        "another port's publish isn't its"
    );
    assert!(port("site", &model_port(0, model::Protocol::Tcp, false), &[], &host).is_err());
}

fn machine(name: &str, role: Role, ports: Vec<Port>) -> Machine {
    Machine {
        name: Name::parse(name).unwrap(),
        role,
        of: None,
        os: None,
        windows: false,
        arch: None,
        resources: None,
        addresses: std::collections::BTreeMap::new(),
        keep_running: false,
        native: None,
        volumes: Vec::new(),
        ports,
    }
}

fn lab_port(name: &str, number: u16, protocol: Protocol, (http, tls): (bool, bool), published: bool) -> Port {
    Port {
        name: Name::parse(name).unwrap(),
        port: NonZeroU16::new(number).unwrap(),
        protocol,
        http,
        tls,
        publish: published.then_some(isoloom_manifest::Publish {
            mode: PublishMode::Ephemeral,
            preferred: None,
            address: None,
        }),
    }
}

#[test]
fn each_published_port_of_a_shown_machine_is_an_output() {
    let web = machine(
        "web",
        Role::Target,
        vec![
            lab_port("site", 443, Protocol::Tcp, (true, true), true),
            lab_port("plain", 80, Protocol::Tcp, (true, false), true),
            lab_port("dns", 53, Protocol::Udp, (false, false), true),
            lab_port("inner", 9000, Protocol::Tcp, (false, false), false),
        ],
    );
    let job = machine("web-init-1", Role::Job, vec![lab_port("x", 1, Protocol::Tcp, (false, false), true)]);
    let shown = [
        web,
        job,
        machine("kali", Role::Access, vec![lab_port("vnc", 5900, Protocol::Tcp, (false, false), true)]),
        machine("shell", Role::Tool, vec![lab_port("ttyd", 7681, Protocol::Tcp, (true, false), true)]),
    ];
    let out = outputs(&shown).unwrap();
    let names: Vec<String> = out.iter().map(|o| o.name.to_string()).collect();
    assert_eq!(names, ["web_site", "web_plain", "web_dns", "kali_vnc", "shell_ttyd"]);
    let formats: Vec<String> = out.iter().map(|o| o.format.as_ref().map(ToString::to_string).unwrap_or_default()).collect();
    assert_eq!(formats[0], "https://127.0.0.1:${value}/");
    assert_eq!(formats[1], "http://127.0.0.1:${value}/");
    assert_eq!(formats[2], "127.0.0.1:${value}");
    assert_eq!((out[0].role, out[2].role), (OutputRole::Entry, OutputRole::Info));
    assert_eq!(out[2].description.as_deref(), Some("web's dns (port 53/udp)."));
    assert_eq!(out[0].description.as_deref(), Some("web's site (port 443)."));
    assert!(matches!(&out[3].from, OutputSource::PublishedPort { machine, port } if machine.to_string() == "kali" && port.to_string() == "vnc"));
    let odd = machine("my-box", Role::Target, vec![lab_port("a-b", 1, Protocol::Tcp, (false, false), true)]);
    assert_eq!(outputs(&[odd]).unwrap()[0].name.to_string(), "my_box_a_b");
    assert_eq!(Template::from_parts([Part::Placeholder(Placeholder::Value)]).to_string(), "${value}");
}

#[test]
fn a_server_lab_shows_its_published_ports_at_the_host_it_names() {
    let web = machine(
        "web",
        Role::Target,
        vec![
            lab_port("site", 80, Protocol::Tcp, (true, false), true),
            lab_port("dns", 53, Protocol::Udp, (false, false), true),
        ],
    );
    let router: Template = "${outputs.ip}".parse().unwrap();
    let out = outputs_at(&[web], &router).unwrap();
    let formats: Vec<String> = out.iter().map(|o| o.format.as_ref().map(ToString::to_string).unwrap_or_default()).collect();
    assert_eq!(formats, ["http://${outputs.ip}:${value}/", "${outputs.ip}:${value}"]);
}

fn resolved_probe(probe: Probe) -> Resolved {
    Resolved {
        name: "c".into(),
        position: Position::Networks,
        probe,
        wait: 7,
        derived: true,
    }
}

#[test]
fn a_check_reports_its_kind_its_wait_and_whether_isoloom_derived_it() {
    let url = Url {
        https: false,
        host: Host::Literal("web".into()),
        port: None,
        path: "/".into(),
    };
    let kinds: Vec<CheckKind> = [
        Probe::Http {
            url,
            expect: HttpExpect::Any,
            request: HttpRequest::default(),
        },
        Probe::Tcp {
            host: Host::Literal("web".into()),
            port: 22,
            expect: TcpExpect::Open,
            exchange: TcpExchange::default(),
        },
        Probe::Exec {
            command: "true".into(),
            expect: None,
        },
        Probe::Script { path: "c.sh".into() },
        Probe::Playbook { path: "c.yml".into() },
    ]
    .into_iter()
    .map(|p| check(&resolved_probe(p)).kind)
    .collect();
    assert_eq!(
        kinds,
        [CheckKind::Http, CheckKind::Tcp, CheckKind::Exec, CheckKind::Script, CheckKind::Playbook]
    );
    let c: Check = check(&resolved_probe(Probe::Script { path: "c.sh".into() }));
    assert_eq!((c.name.as_str(), c.wait_s, c.derived), ("c", 7, true));
    assert_eq!(NonZeroU32::new(timeout_s(&[])), NonZeroU32::new(60));
}
