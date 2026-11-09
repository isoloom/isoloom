//! Forwarding: a gateway's is the plan's step every VM backend renders, and a router's and a
//! gateway's IPv6 forwarding follow one rule, whether the lab routes IPv6.

use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, example, file, plan};

/// A gateway between two networks (IPv4 only: a gateway's network carries no IPv6 yet), and the
/// lab's router routing IPv6 between two others (`v6`) or not.
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
        let env = isoloom_plan::Environment::build(&lab.for_target(Target::Vagrant), Target::Vagrant, None);
        let sysctls = env.machines["fw"]
            .lifecycle
            .iter()
            .find_map(|s| match s {
                isoloom_plan::Step::Forwarding { sysctls } => Some(sysctls.clone()),
                _ => None,
            })
            .ok_or("no forwarding step")?;
        assert_eq!(sysctls.len(), if v6 { 2 } else { 1 }, "{sysctls:?}");
        assert_eq!(isoloom_plan::routing::routes_ipv6(env.spec()), v6);
        let line = if v6 { "'net.ipv6.conf.all.forwarding=1'" } else { "'net.ipv4.ip_forward=1'" };
        let vagrant = file(&generate(&lab, Target::Vagrant)?, "vagrant/Vagrantfile")?.to_owned();
        let fw = vagrant.split("config.vm.define \"fw\"").nth(1).ok_or("no fw")?;
        assert!(fw.contains("name: \"forwarding\"") && fw.contains(line), "{fw}");
        let azure = file(&generate(&lab, Target::CloudVm)?, "cloud-vm/azure/main.tf")?.to_owned();
        assert!(azure.contains("90-isoloom-forwarding.conf"), "the cloud's set-up runs it too");
    }
    Ok(())
}

#[test]
fn routers_forward_ipv6_when_the_lab_routes_it() -> Outcome {
    let routed = plan("dual-stack", Target::Docker)?;
    assert!(isoloom_plan::routing::routes_ipv6(routed.spec()));
    let compose = file(&generate(&example("dual-stack")?, Target::Docker)?, "docker/compose.yml")?.to_owned();
    let service = compose.split("\n  isoloom-router:\n").nth(1).ok_or("no router")?;
    assert!(service.contains("net.ipv6.conf.all.forwarding: '1'"), "{service}");
    let pve = file(&generate(&example("dual-stack")?, Target::Proxmox)?, "proxmox/main.tf")?.to_owned();
    assert!(
        pve.contains("net.ipv4.ip_forward=1\\nnet.ipv6.conf.all.forwarding=1\\n"),
        "the router's sysctl file"
    );
    let v4 = file(&generate(&example("segmented")?, Target::Proxmox)?, "proxmox/main.tf")?.to_owned();
    assert!(!v4.contains("net.ipv6.conf.all.forwarding"), "an IPv4 lab's router forwards IPv4 alone");
    Ok(())
}
