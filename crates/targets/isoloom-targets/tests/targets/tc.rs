//! `networks.*.tc`: link impairment on Isoloom's router and on the machines' own interfaces,
//! validated and generated.

use isoloom_spec::{Target, parse};
use isoloom_targets::{Lab, refusal};
use isoloom_test_support::{yaml, yaml_str, yaml_strs};

use crate::testing::{Outcome, define, example, generated, vagrantfile};

/// The `slow-link` example.
fn slow_link() -> Outcome<Lab> {
    example("slow-link")
}

#[test]
fn the_router_applies_netem_on_its_interface_into_the_network() -> Outcome {
    let spec = slow_link()?;
    assert_eq!(spec.problems(), vec![]);
    assert_eq!(
        spec.source().networks["far"].tc.as_ref().unwrap().netem(),
        "delay 80ms 10ms loss 1% rate 10mbit"
    );
    let compose = yaml(&generated(&spec, Target::Docker, "docker/compose.yml")?);
    let router = [
        yaml_strs(&compose, "services.isoloom-router.entrypoint"),
        yaml_strs(&compose, "services.isoloom-router.command"),
    ]
    .concat()
    .join("\n");
    assert!(router.contains("apk add --no-cache nftables dnsmasq iproute2"), "{router}");
    // Its interface into `far` (the router's address there), shaped.
    let tc = yaml_str(&compose, "services.isoloom-router.environment.TC");
    assert!(
        tc.contains("10\\.75\\.2\\.254") && tc.contains("tc qdisc replace dev \"$$IF\" root netem delay 80ms 10ms loss 1% rate 10mbit"),
        "{tc}"
    );
    let vf = vagrantfile(&spec)?;
    let vm = define(&vf, "isoloom-router").every_statement().join("\n");
    assert!(vm.contains("isoloom-tc.service") && vm.contains("netem delay 80ms 10ms loss 1%"), "{vm}");
    // No router in the path on Kubernetes or in the cloud (asked without its toolbox, which
    // those targets refuse first).
    let mut source = spec.source().clone();
    source.tools.clear();
    let without_tools = Lab::new(source);
    for target in [Target::Kubernetes, Target::CloudVm] {
        let why = refusal(&without_tools, target)?.ok_or("not refused")?;
        assert!(why.contains("router"), "{target:?}: {why}");
    }
    Ok(())
}

#[test]
fn impairment_is_checked() {
    let p = |nets: &str, reach: &str| -> Vec<String> {
        let yaml = format!(
            "version: 1\nname: t\nnetworks:\n{nets}{reach}machines:\n  a: {{ networks: {{ x: 5 }}, ports: {{ web: 80 }}, app: {{ image: x }} }}\n  b: {{ networks: {{ y: 5 }}, app: {{ image: x }} }}\n"
        );
        Lab::new(parse(&yaml).unwrap())
            .problems()
            .iter()
            .map(isoloom_spec::syntax::Problem::line)
            .collect()
    };
    let reach = "reach: [{ from: y, to: x, ports: [80] }]\n";
    assert_eq!(
        p("  x: { cidr: 10.9.0.0/24, tc: { delay: 50ms } }\n  y: { cidr: 10.9.1.0/24 }\n", reach),
        Vec::<String>::new()
    );
    for (tc, says) in [
        ("{ delay: fast }", "isn't a time"),
        ("{ jitter: 5ms }", "set one"),
        ("{ rate: 10mbps2 }", "isn't a rate"),
        ("{ loss: 120 }", "0 to 100"),
        ("{}", "say what to impair"),
    ] {
        let found = p(&format!("  x: {{ cidr: 10.9.0.0/24, tc: {tc} }}\n  y: {{ cidr: 10.9.1.0/24 }}\n"), reach);
        assert!(
            matches!(found.as_slice(), [one] if one.starts_with("networks.x.tc") && one.contains(says)),
            "{tc}: {found:?}"
        );
    }
    // Without a reach rule there is no router on the network: the machines' own interfaces
    // carry it (a direct link between two machines).
    assert_eq!(
        p("  x: { cidr: 10.9.0.0/24, tc: { delay: 50ms } }\n  y: { cidr: 10.9.1.0/24 }\n", ""),
        Vec::<String>::new()
    );
}

#[test]
fn machines_apply_netem_on_their_own_interface() -> Outcome {
    let spec = slow_link()?;
    let compose = yaml(&generated(&spec, Target::Docker, "docker/compose.yml")?);
    // `web` is on `far`: its network sidecar has `tc` (netshoot) and shapes its interface there.
    let sidecar = "services.isoloom-routes-web";
    assert!(yaml_str(&compose, &format!("{sidecar}.image")).starts_with("nicolaka/netshoot:"));
    let command = yaml_strs(&compose, &format!("{sidecar}.entrypoint")).join("\n");
    assert!(
        command.contains("10\\.75\\.2\\.10") && command.contains("netem delay 80ms 10ms loss 1% rate 10mbit"),
        "{command}"
    );
    let health = yaml_strs(&compose, &format!("{sidecar}.healthcheck.test")).join("\n");
    assert!(health.contains("tc qdisc show | grep -q netem"), "{health}");
    let vf = vagrantfile(&spec)?;
    let steps = define(&vf, "web").statements("m.vm.provision \"shell\", name: \"tc\"");
    assert!(matches!(steps.as_slice(), [one] if one.contains("this machine's interfaces")), "{steps:?}");
    Ok(())
}
