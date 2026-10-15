//! The `proxmox` target: published ports through the router, and a machine's resolver.

use isoloom_spec::Target;

use crate::testing::{Outcome, example, generated, lab};

#[test]
fn proxmox_forwards_published_ports_from_the_router() -> Outcome {
    let spec = example("hello-stack")?;
    let tf = generated(&spec, Target::Proxmox, ".isoloom/proxmox/main.tf")?;
    // From anything outside the lab to the machine, and the forward allowed through.
    assert!(tf.contains("tcp dport 8080 dnat ip to 10.60.0.10:"), "{tf}");
    assert!(tf.contains("ct status dnat accept"));
    // Where to connect: the router's uplink address, from the guest agent.
    assert!(tf.contains("output \"published\""));
    assert!(tf.contains("\"web/") && tf.contains(":8080\""));
    Ok(())
}

#[test]
fn proxmox_cloud_init_uses_the_machines_declared_resolver() -> Outcome {
    // A machine that points at the lab's own resolver (e.g. an AD member at the domain
    // controller) must get that in cloud-init, not a forced public resolver.
    let spec = lab(
        "version: 1\nname: dns-lab\nnetworks:\n  lab: { cidr: 10.60.0.0/24 }\nmachines:\n  member:\n    os: debian@12\n    networks: { lab: 10 }\n    dns: { servers: [10.60.0.2], domain: arm.lab }\n    app: { steps: [{ run: p.sh }] }\n  plain:\n    os: debian@12\n    networks: { lab: 11 }\n    app: { steps: [{ run: p.sh }] }\n",
    )?;
    let tf = generated(&spec, Target::Proxmox, ".isoloom/proxmox/main.tf")?;
    assert!(tf.contains("servers = [\"10.60.0.2\"]"), "{tf}");
    assert!(tf.contains("domain  = \"arm.lab\""), "{tf}");
    // The machine with no resolver of its own still gets a public one so it can install software.
    assert!(tf.contains("servers = [\"1.1.1.1\"]"), "{tf}");
    Ok(())
}
