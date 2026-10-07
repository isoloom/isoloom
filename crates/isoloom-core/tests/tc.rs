//! `networks.*.tc`: link impairment on Isoloom's router and on the machines' own interfaces,
//! validated and generated.

use std::path::Path;

use isoloom_core::{Target, generate, load, parse, refusal, validate};

#[test]
fn the_router_applies_netem_on_its_interface_into_the_network() {
    let spec = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/slow-link")).unwrap();
    assert_eq!(validate(&spec), vec![]);
    assert_eq!(spec.networks["far"].tc.as_ref().unwrap().netem(), "delay 80ms 10ms loss 1% rate 10mbit");
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(compose.contains("iproute2"), "{compose}");
    assert!(
        compose.contains("10\\.75\\.2\\.254") && compose.contains("tc qdisc replace dev \"$$IF\" root netem delay 80ms 10ms loss 1% rate 10mbit"),
        "{compose}"
    );
    let vf = generate(&spec, Target::Vagrant)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with("Vagrantfile"))
        .unwrap()
        .contents;
    assert!(vf.contains("isoloom-tc.service") && vf.contains("netem delay 80ms 10ms loss 1%"), "{vf}");
    // No router in the path on Kubernetes or in the cloud.
    assert!(refusal(&spec, Target::Kubernetes).unwrap().contains("router"));
    assert!(refusal(&spec, Target::CloudVm).unwrap().contains("router"));
}

#[test]
fn impairment_is_checked() {
    let p = |nets: &str, reach: &str| -> Vec<String> {
        let yaml = format!(
            "version: 1\nname: t\nnetworks:\n{nets}{reach}machines:\n  a: {{ networks: {{ x: 5 }}, services: [{{ port: 80 }}], docker: {{ image: x }} }}\n  b: {{ networks: {{ y: 5 }}, docker: {{ image: x }} }}\n"
        );
        validate(&parse(&yaml).unwrap()).into_iter().map(|p| p.to_string()).collect()
    };
    let reach = "reach: [{ from: y, to: x, ports: [80] }]\n";
    assert_eq!(
        p("  x: { cidr: 10.9.0.0/24, tc: { delay: 50ms } }\n  y: { cidr: 10.9.1.0/24 }\n", reach),
        Vec::<String>::new()
    );
    assert!(p("  x: { cidr: 10.9.0.0/24, tc: { delay: fast } }\n  y: { cidr: 10.9.1.0/24 }\n", reach)[0].contains("isn't a time"));
    assert!(p("  x: { cidr: 10.9.0.0/24, tc: { jitter: 5ms } }\n  y: { cidr: 10.9.1.0/24 }\n", reach)[0].contains("set one"));
    assert!(p("  x: { cidr: 10.9.0.0/24, tc: { rate: 10mbps2 } }\n  y: { cidr: 10.9.1.0/24 }\n", reach)[0].contains("isn't a rate"));
    assert!(p("  x: { cidr: 10.9.0.0/24, tc: { loss: 120 } }\n  y: { cidr: 10.9.1.0/24 }\n", reach)[0].contains("0 to 100"));
    assert!(p("  x: { cidr: 10.9.0.0/24, tc: {} }\n  y: { cidr: 10.9.1.0/24 }\n", reach)[0].contains("say what to impair"));
    // Without a reach rule there is no router on the network: the machines' own interfaces
    // carry it (a direct link between two machines).
    assert_eq!(
        p("  x: { cidr: 10.9.0.0/24, tc: { delay: 50ms } }\n  y: { cidr: 10.9.1.0/24 }\n", ""),
        Vec::<String>::new()
    );
}

#[test]
fn machines_apply_netem_on_their_own_interface() {
    let spec = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/slow-link")).unwrap();
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    // `web` is on `far`: its network sidecar has `tc` (netshoot) and shapes its interface there.
    let sidecar = compose.split("\n  web-routes:\n").nth(1).expect("web has a network sidecar");
    assert!(sidecar.contains("netshoot"), "{sidecar}");
    assert!(
        sidecar.contains("10\\.75\\.2\\.10") && sidecar.contains("netem delay 80ms 10ms loss 1% rate 10mbit"),
        "{sidecar}"
    );
    assert!(sidecar.contains("tc qdisc show | grep -q netem"));
    let vf = generate(&spec, Target::Vagrant)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with("Vagrantfile"))
        .unwrap()
        .contents;
    assert!(vf.contains("name: \"tc\"") && vf.contains("this machine's interfaces"), "{vf}");
}
