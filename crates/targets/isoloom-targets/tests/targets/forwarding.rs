//! Forwarding: a gateway's is the plan's step every VM backend renders, and a router's and a
//! gateway's IPv6 forwarding follow one rule, whether the environment routes IPv6.

use isoloom_spec::Target;
use isoloom_targets::generate;

use isoloom_test_support::{yaml, yaml_str};

use crate::testing::{Outcome, cloud_config, define, example, file, set_up, vagrantfile, written};

/// A gateway between two networks (IPv4 only: a gateway's network carries no IPv6), and the
/// environment's router routing IPv6 between two others (`v6`) or not.
fn gateway(v6: bool) -> String {
    let ipv6 = |n: u8| {
        if v6 {
            format!(", ipv6: {{ cidr: \"fd00:{n}::/64\" }}")
        } else {
            String::new()
        }
    };
    format!(
        "version: 1\nname: gw\nnetworks:\n  out: {{ cidr: 10.1.0.0/24{} }}\n  dmz: {{ cidr: 10.2.0.0/24, gateway: fw }}\n  far: {{ cidr: 10.3.0.0/24{} }}\nreach:\n  - {{ from: out, to: far, ports: [80] }}\nmachines:\n  fw: {{ os: debian@12, networks: {{ out: 5, dmz: 1 }}, app: {{ steps: [{{ run: fw.sh }}] }} }}\n  web: {{ os: debian@12, networks: {{ dmz: 10 }}, app: {{ steps: [{{ run: web.sh }}] }} }}\n  db: {{ os: debian@12, networks: {{ far: 10 }}, app: {{ steps: [{{ run: db.sh }}] }} }}\n",
        ipv6(1),
        ipv6(3)
    )
}

#[test]
fn a_gateway_forwards_by_the_plans_step_on_every_vm_target() -> Outcome {
    for v6 in [false, true] {
        let lab = crate::testing::valid(&gateway(v6))?;
        let env = lab.plan(&lab.for_target(Target::Vagrant), Target::Vagrant, None).expect("the spec plans");
        let sysctls = env.machines["fw"]
            .lifecycle
            .iter()
            .find_map(|s| match s {
                isoloom_plan::Step::Forwarding { sysctls } => Some(sysctls.clone()),
                _ => None,
            })
            .ok_or("no forwarding step")?;
        assert_eq!(sysctls.len(), if v6 { 2 } else { 1 }, "{sysctls:?}");
        let line = if v6 { "'net.ipv6.conf.all.forwarding=1'" } else { "'net.ipv4.ip_forward=1'" };
        let vf = vagrantfile(&lab)?;
        let steps = define(&vf, "fw").statements("m.vm.provision \"shell\", name: \"forwarding\"");
        assert!(matches!(steps.as_slice(), [one] if one.contains(line)), "{steps:?}");
        let azure = file(&generate(&lab, Target::CloudVm)?, "cloud-vm/azure/main.tf")?.to_owned();
        assert!(set_up(&azure, "fw")?.contains("90-isoloom-forwarding.conf"), "the cloud's set-up runs it too");
    }
    Ok(())
}

/// The sysctl file the router's cloud-config writes on Proxmox.
fn router_sysctls(lab: &str) -> Outcome<String> {
    let tf = file(&generate(&example(lab)?, Target::Proxmox)?, "proxmox/main.tf")?.to_owned();
    written(&cloud_config(&tf, "isoloom_router")?)
        .into_iter()
        .find(|(path, _)| path == "/etc/sysctl.d/90-isoloom.conf")
        .map(|(_, content)| content)
        .ok_or_else(|| "no sysctl file for the router".into())
}

#[test]
fn routers_forward_ipv6_when_the_lab_routes_it() -> Outcome {
    let compose = yaml(file(&generate(&example("dual-stack")?, Target::Docker)?, "docker/compose.yml")?);
    assert_eq!(yaml_str(&compose, "services.isoloom-router.sysctls.`net.ipv6.conf.all.forwarding`"), "1");
    assert_eq!(router_sysctls("dual-stack")?, "net.ipv4.ip_forward=1\nnet.ipv6.conf.all.forwarding=1\n");
    assert_eq!(
        router_sysctls("segmented")?,
        "net.ipv4.ip_forward=1\n",
        "an IPv4 lab's router forwards IPv4 alone"
    );
    Ok(())
}
