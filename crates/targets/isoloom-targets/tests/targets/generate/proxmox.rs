//! The `proxmox` target: published ports through the router, and a machine's resolver.

use isoloom_spec::Target;
use isoloom_test_support::{hcl_blocks, hcl_body, hcl_literals, hcl_nested, hcl_one, hcl_text, hcl_values};

use crate::testing::{Outcome, example, generated, lab, published_keys};

#[test]
fn proxmox_forwards_published_ports_from_the_router() -> Outcome {
    let spec = example("hello-stack")?;
    let body = hcl_body(&generated(&spec, Target::Proxmox, ".isoloom/proxmox/main.tf")?);
    // From anything outside the lab to the machine, and the forward allowed through: in every
    // ruleset the router's set-up writes.
    let router = hcl_literals(hcl_one(&body, "proxmox_virtual_environment_file", "isoloom_router"));
    let rulesets: Vec<Vec<&str>> = router
        .iter()
        .filter(|l| l.starts_with("flush ruleset\n"))
        .map(|r| r.lines().map(str::trim).collect())
        .collect();
    isoloom_test_support::assert_every!(&rulesets, |rules: &&Vec<&str>| {
        rules.contains(&"ip saddr != { 10.60.0.0/24 } tcp dport 8080 dnat ip to 10.60.0.10:80")
            && rules.contains(&"ip daddr 10.60.0.10 tcp dport 80 ct status dnat accept")
    });
    // Where to connect: the router's uplink address, from the guest agent.
    assert_eq!(published_keys(&body), ["web/web"]);
    let published = hcl_blocks(&body, "output", &["published"]);
    let value = hcl_text(published.first().ok_or("no published output")?.body(), "value");
    assert!(value.contains("local.router_address") && value.contains(":8080"), "{value}");
    Ok(())
}

/// The resolver servers and domain VM `vm`'s cloud-init is given (`initialization.dns`).
fn resolver(body: &isoloom_test_support::hcl::Body, vm: &str) -> Outcome<(Vec<String>, Option<String>)> {
    let vm = hcl_one(body, "proxmox_virtual_environment_vm", vm);
    let init = hcl_nested(vm, "initialization");
    let dns = init
        .first()
        .and_then(|i| hcl_nested(i.body(), "dns").into_iter().next())
        .ok_or("no initialization.dns")?;
    let domain = isoloom_test_support::hcl_attribute(dns.body(), "domain").map(|_| hcl_text(dns.body(), "domain"));
    Ok((hcl_values(dns.body(), "servers"), domain))
}

#[test]
fn proxmox_cloud_init_uses_the_machines_declared_resolver() -> Outcome {
    // A machine that points at the lab's own resolver (e.g. an AD member at the domain
    // controller) must get that in cloud-init, not a forced public resolver.
    let spec = lab(
        "version: 1\nname: dns-lab\nnetworks:\n  lab: { cidr: 10.60.0.0/24 }\nmachines:\n  member:\n    os: debian@12\n    networks: { lab: 10 }\n    dns: { servers: [10.60.0.2], domain: arm.lab }\n    app: { steps: [{ run: p.sh }] }\n  plain:\n    os: debian@12\n    networks: { lab: 11 }\n    app: { steps: [{ run: p.sh }] }\n",
    )?;
    let body = hcl_body(&generated(&spec, Target::Proxmox, ".isoloom/proxmox/main.tf")?);
    assert_eq!(resolver(&body, "member")?, (vec!["10.60.0.2".to_owned()], Some("arm.lab".to_owned())));
    // The machine with no resolver of its own still gets a public one so it can install software.
    assert_eq!(resolver(&body, "plain")?, (vec!["1.1.1.1".to_owned()], None));
    Ok(())
}
