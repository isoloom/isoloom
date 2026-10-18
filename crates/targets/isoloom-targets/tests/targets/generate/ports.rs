//! Ports: a dormant one nothing waits for, and UDP ones published, opened, named apart and
//! validated.

use isoloom_spec::{Target, parse};
use isoloom_targets::Lab;

use crate::testing::{Outcome, generated, lab, problems};

#[test]
fn a_dormant_service_is_declared_but_never_waited_for() -> Outcome {
    // 4444 is the shell an exploit opens: `shell` has nothing else, `web` serves 80 too.
    let source = parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  \
         shell: { os: debian@12, networks: { lab: 5 }, ports: { shell: { port: 4444, dormant: true, publish: 4444 } }, app: { image: x, steps: [{ run: p.sh }] } }\n  \
         web: { os: debian@12, networks: { lab: 10 }, ports: { web: 80, shell: { port: 4444, dormant: true } }, app: { image: x, steps: [{ run: p.sh }] } }\n  \
         app: { os: debian@12, networks: { lab: 20 }, depends_on: [web], app: { image: x, steps: [{ run: p.sh }] } }\n",
    )
    .unwrap();
    let spec = Lab::new(source.clone());
    assert_eq!(spec.problems(), vec![]);

    // Docker: the healthcheck probes 80 only; a machine with only dormant services has none,
    // so whatever waits for it waits for it to start.
    let compose: serde_yaml_ng::Value = serde_yaml_ng::from_str(&generated(&spec, Target::Docker, ".isoloom/docker/compose.yml")?).unwrap();
    let svc = |n: &str| compose["services"][n].clone();
    let probe = serde_yaml_ng::to_string(&svc("web")["healthcheck"]).unwrap();
    assert!(probe.contains("127.0.0.1 80") && !probe.contains("4444"), "{probe}");
    assert!(svc("shell").get("healthcheck").is_none());
    assert!(
        serde_yaml_ng::to_string(&svc("shell")).unwrap().contains(":4444"),
        "still published: {:?}",
        svc("shell")
    );
    assert_eq!(svc("app")["depends_on"]["web"]["condition"].as_str(), Some("service_healthy"));
    let check = serde_yaml_ng::to_string(&svc("isoloom-check-app")["depends_on"]).unwrap();
    assert!(check.contains("web:") && !check.contains("shell:"), "{check}");

    // Kubernetes: the port is declared and in the Service, but not in the readiness probe or
    // the wait of the machines depending on it.
    let env = generated(&spec, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    assert!(env.contains("containerPort: 4444") && env.contains("name: p4444"), "{env}");
    assert!(env.contains("until nc -z web 80; do sleep 2; done"), "{env}");
    assert!(!env.contains("127.0.0.1 4444"), "{env}");

    // VMs: app waits for web on 80 only.
    let vf = generated(&spec, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(vf.contains("</dev/tcp/web/80") && !vf.contains("/dev/tcp/web/4444"), "{vf}");

    // No derived check expects it to answer.
    let plan = isoloom_plan::checks::plan(&spec.spec());
    assert!(plan.iter().any(|c| c.name == "web:80 from app"));
    assert!(
        !plan.iter().any(|c| c.name.contains(":4444")),
        "{:?}",
        plan.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    // Depending on a machine whose services are all dormant waits for nothing: refused.
    let mut bad = source;
    bad.machines.get_mut("app").unwrap().depends_on = vec!["shell".into()];
    assert_eq!(
        problems(&Lab::new(bad)),
        ["machines.app.depends_on[0]: `shell`'s ports are all dormant (they answer later), so there's nothing to wait for"]
    );
    Ok(())
}

/// dns serves 53 over UDP (published, `dns`) and TCP (`zone`); syslog serves UDP only; app
/// waits for dns.
fn udp_source() -> Outcome<isoloom_spec::Spec> {
    Ok(parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n  far: { cidr: 10.9.1.0/24 }\nreach: [{ from: far, to: lab, ports: [53] }]\nmachines:\n  \
         dns: { os: debian@12, networks: { lab: 5 }, ports: { dns: { port: 53, protocol: udp, publish: 10053 }, zone: 53 }, app: { image: x, steps: [{ run: p.sh }] } }\n  \
         syslog: { os: debian@12, networks: { lab: 6 }, ports: { syslog: { port: 514, protocol: udp, publish: 1514 } }, app: { image: x, steps: [{ run: p.sh }] } }\n  \
         app: { os: debian@12, networks: { far: 20 }, depends_on: [dns], app: { image: x, steps: [{ run: p.sh }] } }\n",
    )?)
}

/// [`udp_source`] as a lab with no problem.
fn udp_spec() -> Outcome<Lab> {
    let spec = Lab::new(udp_source()?);
    assert_eq!(spec.problems(), vec![]);
    Ok(spec)
}

#[test]
fn a_udp_service_is_published_as_udp_on_docker() -> Outcome {
    let spec = udp_spec()?;
    // Docker: published as `<port>/udp`, labelled udp, and the healthcheck probes TCP 53 only;
    // a machine with only UDP services has none.
    let compose: serde_yaml_ng::Value = serde_yaml_ng::from_str(&generated(&spec, Target::Docker, ".isoloom/docker/compose.yml")?).unwrap();
    let svc = |n: &str| compose["services"][n].clone();
    let ports = serde_yaml_ng::to_string(&svc("dns")["ports"]).unwrap();
    assert!(ports.contains("+10053}:53/udp"), "{ports}");
    assert_eq!(svc("dns")["labels"]["isoloom.port.dns"].as_str(), Some("udp:53"));
    assert_eq!(svc("dns")["labels"]["isoloom.port.zone"].as_str(), Some("tcp:53"));
    let probe = serde_yaml_ng::to_string(&svc("dns")["healthcheck"]).unwrap();
    assert!(probe.contains("127.0.0.1 53"), "{probe}");
    assert!(svc("syslog").get("healthcheck").is_none());
    assert!(serde_yaml_ng::to_string(&svc("syslog")["ports"]).unwrap().contains(":514/udp"));
    Ok(())
}

#[test]
fn a_udp_and_a_tcp_port_on_one_number_stay_apart_by_name() -> Outcome {
    // A name is one port's (a TCP and a UDP port share a number, never a name): `dns-udp` and
    // `dns`, on Docker and on the clouds.
    let named = lab("version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  \
         dns: { os: debian@12, networks: { lab: 5 }, ports: { dns-udp: { port: 53, protocol: udp, publish: 10053 }, dns: { port: 53, publish: 1053 } }, app: { image: x, steps: [{ run: p.sh }] } }\n")?;
    assert_eq!(named.problems(), vec![]);
    let c: serde_yaml_ng::Value = serde_yaml_ng::from_str(&generated(&named, Target::Docker, ".isoloom/docker/compose.yml")?).unwrap();
    assert_eq!(c["services"]["dns"]["labels"]["isoloom.port.dns-udp"].as_str(), Some("udp:53"));
    assert_eq!(c["services"]["dns"]["labels"]["isoloom.port.dns"].as_str(), Some("tcp:53"));
    let aws = generated(&named, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    assert!(aws.contains("\"dns/dns-udp\"") && aws.contains("\"dns/dns\""), "{aws}");
    // The same name twice is no map: the YAML says so.
    let twice = parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  \
         dns: { networks: { lab: 5 }, ports: { dns: { port: 53, protocol: udp }, dns: 53 }, app: { image: x } }\n",
    );
    assert!(twice.unwrap_err().to_string().contains("dns"));
    Ok(())
}

#[test]
fn a_udp_service_is_published_and_opened_as_udp_on_kubernetes_and_the_vms() -> Outcome {
    let spec = udp_spec()?;
    // Kubernetes: UDP container ports and Service ports, named apart from the TCP one; the
    // NetworkPolicy for `reach` already opens both protocols.
    let env = generated(&spec, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    assert!(
        env.contains("- containerPort: 53\n          protocol: UDP\n        - containerPort: 53\n"),
        "{env}"
    );
    assert!(env.contains("name: p53-udp") && env.contains("name: p53\n"), "{env}");
    assert!(env.contains("name: p10053-udp"), "{env}");
    assert!(!env.contains("127.0.0.1 514"), "{env}");

    // Vagrant: forwarded as UDP; app waits for dns on TCP 53.
    let vf = generated(&spec, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(
        vf.contains("guest: 53, host: 10053, host_ip: \"127.0.0.1\", protocol: \"udp\" # not on libvirt"),
        "{vf}"
    );
    assert!(!vf.contains("/dev/tcp/syslog/514"), "{vf}");

    // Proxmox: the router forwards the published port as UDP.
    let tf = generated(&spec, Target::Proxmox, ".isoloom/proxmox/main.tf")?;
    assert!(tf.contains("udp dport 1514 dnat ip to 10.9.0.6:514"), "{tf}");
    assert!(tf.contains("meta l4proto { tcp, udp } th dport { 53 } accept"), "{tf}");

    // The clouds: published as UDP.
    let aws = generated(&spec, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    assert!(aws.contains("from_port   = 1514\n    to_port     = 1514\n    protocol    = \"udp\""), "{aws}");
    Ok(())
}

#[test]
fn a_udp_service_is_never_waited_for_nor_checked() -> Outcome {
    let spec = udp_spec()?;
    // No derived check probes UDP (nothing tells an answer from a drop); TCP 53 is checked.
    let plan = isoloom_plan::checks::plan(&spec.spec());
    let names: Vec<&String> = plan.iter().map(|c| &c.name).collect();
    assert!(names.iter().any(|n| *n == "dns:53 from app"), "{names:?}");
    assert!(!names.iter().any(|n| n.contains(":514")), "{names:?}");

    // The snapshot says so: each port with its protocol.
    let snap = isoloom_targets::resolved::resolve(&spec);
    assert_eq!(snap["machines"]["dns"]["ports"]["dns"]["protocol"], "udp");
    assert_eq!(snap["machines"]["dns"]["ports"]["zone"]["protocol"], "tcp");

    // Depending on a machine with only UDP services waits for nothing: refused.
    let mut bad = udp_source()?;
    bad.machines.get_mut("app").unwrap().depends_on = vec!["syslog".into()];
    assert_eq!(
        problems(&Lab::new(bad)),
        ["machines.app.depends_on[0]: `syslog`'s ports are all UDP (no handshake to wait for), so there's nothing to wait for"]
    );
    Ok(())
}

#[test]
fn a_udp_service_is_validated() -> Outcome {
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 5 }, app: { image: x }, ports: ";
    let problems = |svcs: &str| -> Outcome<Vec<String>> { Ok(problems(&lab(&format!("{base}{svcs} }}\n"))?)) };
    // One port over both protocols is two services; the same one twice isn't.
    assert_eq!(
        problems("{ dns: { port: 53, protocol: udp }, zone: { port: 53, protocol: tcp } }")?,
        Vec::<String>::new()
    );
    assert_eq!(
        problems("{ dns: { port: 53, protocol: udp }, again: { port: 53, protocol: udp } }")?,
        ["machines.a.ports.again.port: port 53 (UDP) is listed twice"]
    );
    // A host port may be published once per protocol.
    assert_eq!(
        problems("{ dns: { port: 53, protocol: udp, publish: 53 }, zone: { port: 53, publish: 53 } }")?,
        Vec::<String>::new()
    );
    // HTTP over UDP can't be written: one `protocol` says either.
    let err = parse(&format!("{base}{{ dns: {{ port: 53, protocol: sctp }} }} }}\n")).unwrap_err().to_string();
    assert!(err.contains("sctp"), "{err}");
    Ok(())
}
