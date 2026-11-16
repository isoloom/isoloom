//! Ports: a dormant one nothing waits for, and UDP ones published, opened, named apart and
//! validated.

use isoloom_spec::{Target, parse};
use isoloom_targets::Lab;

use isoloom_test_support::{hcl_body, hcl_literals, hcl_one, manifest, yaml_documents, yaml_values};

use crate::testing::{Outcome, define, generated, lab, problems, vagrantfile};

/// Whether a statement of any VM's `config.vm.define` block holds `text`.
fn any_define_holds(vf: &isoloom_test_support::RubyBlock, text: &str) -> bool {
    vf.find("config.vm.define \"")
        .iter()
        .any(|d| d.every_statement().iter().any(|s| s.contains(text)))
}

/// The readiness probes of every Deployment's containers, as YAML.
fn readiness_probes(docs: &[serde_yaml_ng::Value]) -> Vec<String> {
    docs.iter()
        .filter(|d| d.get("kind").and_then(serde_yaml_ng::Value::as_str) == Some("Deployment"))
        .flat_map(|d| {
            d.get("spec")
                .and_then(|s| s.get("template"))
                .and_then(|t| t.get("spec"))
                .and_then(|s| s.get("containers"))
                .and_then(serde_yaml_ng::Value::as_sequence)
                .cloned()
                .unwrap_or_default()
        })
        .map(|c| {
            c.get("readinessProbe")
                .map(|p| serde_yaml_ng::to_string(p).unwrap_or_default())
                .unwrap_or_default()
        })
        .collect()
}

/// Kubernetes: the dormant port is declared and in the Service, but not in the readiness probe
/// or the wait of the machines depending on it.
fn kubernetes_declares_the_dormant_port_and_waits_for_80(spec: &Lab) -> Outcome {
    let env = generated(spec, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    let docs = yaml_documents(&env);
    assert!(
        yaml_values(manifest(&docs, "Deployment", "web"), "containerPort").contains(&"4444".to_owned()),
        "{env}"
    );
    assert!(yaml_values(manifest(&docs, "Service", "web"), "name").contains(&"p4444".to_owned()), "{env}");
    let init = manifest(&docs, "Deployment", "app")
        .get("spec")
        .and_then(|s| s.get("template"))
        .and_then(|t| t.get("spec"))
        .and_then(|s| s.get("initContainers"))
        .ok_or("app has no initContainers")?;
    let waits = serde_yaml_ng::to_string(init)?;
    assert!(waits.contains("until nc -z web 80; do sleep 2; done"), "{waits}");
    isoloom_test_support::assert_every!(readiness_probes(&docs), |p: &String| !p.contains("4444"));
    Ok(())
}

/// VMs: app waits for web on 80 only.
fn vms_wait_for_80_only(spec: &Lab) -> Outcome {
    let vf = vagrantfile(spec)?;
    let waits = define(&vf, "app").statements("m.vm.provision \"shell\", name: \"wait for web\"");
    assert!(matches!(waits.as_slice(), [wait] if wait.contains("</dev/tcp/web/80")), "{waits:?}");
    assert!(!any_define_holds(&vf, "/dev/tcp/web/4444"), "{vf:?}");
    Ok(())
}

#[test]
fn a_dormant_service_is_declared_but_never_waited_for() -> Outcome {
    // 4444 answers only once something in the machine starts it: `shell` has nothing else, `web` serves 80 too.
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
    let probe = isoloom_test_support::yaml_strs(&compose, "services.web.healthcheck.test");
    assert_eq!(
        probe.last().map(String::as_str),
        Some("/.isoloom-probe/busybox nc -z -w 2 127.0.0.1 80"),
        "{probe:?}"
    );
    assert!(svc("shell").get("healthcheck").is_none());
    let published = isoloom_test_support::yaml_strs(&compose, "services.shell.ports");
    assert!(published.iter().any(|p| p.ends_with(":4444")), "still published: {published:?}");
    assert_eq!(svc("app")["depends_on"]["web"]["condition"].as_str(), Some("service_healthy"));
    let waits = isoloom_test_support::yaml_keys_at(&compose, "services.isoloom-check-app.depends_on");
    assert!(waits.contains(&"web".to_owned()) && !waits.contains(&"shell".to_owned()), "{waits:?}");

    kubernetes_declares_the_dormant_port_and_waits_for_80(&spec)?;
    vms_wait_for_80_only(&spec)?;

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

/// [`udp_source`] as an environment with no problem.
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
    let ports = |n: &str| {
        svc(n)["ports"]
            .as_sequence()
            .map(|p| p.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    assert!(ports("dns").iter().any(|p| p.ends_with("+10053}:53/udp")), "{:?}", ports("dns"));
    assert_eq!(svc("dns")["labels"]["isoloom.port.dns"].as_str(), Some("udp:53"));
    assert_eq!(svc("dns")["labels"]["isoloom.port.zone"].as_str(), Some("tcp:53"));
    let probe = isoloom_test_support::yaml_strs(&compose, "services.dns.healthcheck.test");
    assert_eq!(
        probe.last().map(String::as_str),
        Some("/.isoloom-probe/busybox nc -z -w 2 127.0.0.1 53"),
        "{probe:?}"
    );
    assert!(svc("syslog").get("healthcheck").is_none());
    assert!(ports("syslog").iter().any(|p| p.ends_with(":514/udp")), "{:?}", ports("syslog"));
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
    let mut keys = crate::testing::published_keys(&hcl_body(&aws));
    keys.sort();
    assert_eq!(keys, ["dns/dns", "dns/dns-udp"], "{aws}");
    // The same name twice is no map: the YAML says so.
    let twice = parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  \
         dns: { networks: { lab: 5 }, ports: { dns: { port: 53, protocol: udp }, dns: 53 }, app: { image: x } }\n",
    );
    let err = twice.err().ok_or("a port named twice parses")?.to_string();
    assert!(err.contains("duplicate") && err.contains("dns"), "{err}");
    Ok(())
}

#[test]
fn a_udp_service_is_published_and_opened_as_udp_on_kubernetes_and_the_vms() -> Outcome {
    let spec = udp_spec()?;
    // Kubernetes: UDP container ports and Service ports, named apart from the TCP one; the
    // NetworkPolicy for `reach` already opens both protocols.
    let env = generated(&spec, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    let docs = isoloom_test_support::yaml_documents(&env);
    let dns = isoloom_test_support::manifest(&docs, "Deployment", "dns");
    let container = &dns["spec"]["template"]["spec"]["containers"][0];
    let declared: Vec<(u64, String)> = container["ports"]
        .as_sequence()
        .into_iter()
        .flatten()
        .map(|p| {
            (
                p["containerPort"].as_u64().unwrap_or_default(),
                p["protocol"].as_str().unwrap_or("TCP").to_owned(),
            )
        })
        .collect();
    assert_eq!(declared, [(53, "UDP".to_owned()), (53, "TCP".to_owned())], "{env}");
    let service_ports: Vec<String> = docs
        .iter()
        .filter(|d| d["kind"].as_str() == Some("Service"))
        .flat_map(|d| d["spec"]["ports"].as_sequence().cloned().unwrap_or_default())
        .filter_map(|p| p["name"].as_str().map(str::to_owned))
        .collect();
    for name in ["p53-udp", "p53", "p10053-udp"] {
        assert!(service_ports.iter().any(|p| p == name), "{name}: {service_ports:?}");
    }
    isoloom_test_support::assert_every!(readiness_probes(&docs), |p: &String| !p.contains("514"));

    // Vagrant: forwarded as UDP; app waits for dns on TCP 53.
    let vf = vagrantfile(&spec)?;
    let forwards = define(&vf, "dns").statements("m.vm.network \"forwarded_port\", guest: 53,");
    let forward = "m.vm.network \"forwarded_port\", guest: 53, host: ENV.fetch(\"ISOTHRUM_HOST_PORT_DNS_53_UDP\", ISOLOOM_HOSTPORTS.fetch(\"dns/dns\", 10053)).to_i, host_ip: \"127.0.0.1\", protocol: \"udp\", auto_correct: true # not on libvirt";
    assert!(matches!(forwards.as_slice(), [one] if one.starts_with(forward)), "{forwards:?}");
    assert!(!any_define_holds(&vf, "/dev/tcp/syslog/514"), "{vf:?}");

    // Proxmox: the router forwards the published port as UDP.
    let tf = generated(&spec, Target::Proxmox, ".isoloom/proxmox/main.tf")?;
    let router = hcl_literals(hcl_one(&hcl_body(&tf), "proxmox_virtual_environment_file", "isoloom_router"));
    let ruleset = router
        .iter()
        .find(|l| l.starts_with("flush ruleset\n"))
        .ok_or("no ruleset in the router's set-up")?;
    let rules: Vec<&str> = ruleset.lines().map(str::trim).collect();
    assert!(rules.iter().any(|r| r.ends_with("udp dport 1514 dnat ip to 10.9.0.6:514")), "{ruleset}");
    assert!(
        rules.iter().any(|r| r.ends_with("meta l4proto { tcp, udp } th dport { 53 } accept")),
        "{ruleset}"
    );

    // The clouds: published as UDP.
    let aws = generated(&spec, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    let body = isoloom_test_support::hcl_body(&aws);
    let ingress: Vec<(String, String, String)> = isoloom_test_support::hcl_blocks(&body, "resource", &["aws_security_group"])
        .iter()
        .flat_map(|g| isoloom_test_support::hcl_nested(g.body(), "ingress"))
        .map(|r| {
            let field = |k: &str| isoloom_test_support::hcl_values(r.body(), k).join(",");
            (field("from_port"), field("to_port"), field("protocol"))
        })
        .collect();
    assert!(ingress.contains(&("1514".into(), "1514".into(), "udp".into())), "{ingress:?}");
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
