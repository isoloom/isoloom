//! The seal on a cloud (`crate::seal`): each cloud closes the internet to the machines of every
//! `internet: false` network with resources of its own that wait for every machine's set-up
//! ([`after_set_up`]: the readiness waits, `crate::ready`), so the one `terraform apply` makes
//! the machines, waits for their set-up, then closes those networks. Each closing resource is
//! made after the waits, and none is a switch on a resource the machines need first: AWS a
//! network ACL on their subnets, Azure a security group on their subnets (a machine's own still
//! applies), Google Cloud deny rules above its allow rules, Oracle Cloud a route table with no
//! way to the internet attached to their subnets. Inbound is unchanged: SSH and the published
//! ports still answer from `allowed_cidr`.
//!
//! DigitalOcean's and Linode's firewalls only allow, and a machine has one firewall made with it,
//! so nothing made later can close it: those clouds refuse a network without internet
//! ([`refusal`]).
//!
//! Each cloud's resolver stays reachable (AWS's at the VPC's `.2`, Azure's `168.63.129.16`,
//! Google's metadata server, Oracle's VCN resolver): the clouds' firewalls don't filter them, so
//! a sealed machine can still resolve names, a channel out the seal leaves open there.

use crate::hcl_builder::{Expression, list, string};
use isoloom_plan::Environment;

/// What a closing resource waits for: every machine's readiness wait.
pub(crate) fn after_set_up(env: &Environment) -> Expression {
    list(crate::ready::all(super::outputs::waited(env)))
}

/// The environment's own blocks, every network's: where a sealed machine may still go.
pub(crate) fn environment_blocks(env: &Environment) -> Expression {
    list(env.networks.values().map(|p| string(p.cidr.to_string())))
}

/// Why a cloud whose firewalls only allow (DigitalOcean, Linode: `cloud`, its name) can't run
/// `env`: a network without internet, which it couldn't close once its machines are set up.
pub(crate) fn refusal(env: &Environment, cloud: &str) -> Option<String> {
    let (net, _) = env.networks.iter().find(|(n, _)| crate::seal::any(env) && crate::seal::closed(env, n))?;
    Some(format!(
        "network `{net}` has no internet: {cloud} can't seal an offline network (its firewalls only allow, and each machine's is made with it, so nothing made after the set-up can close its way out). Use `internet: true` (outbound only: nothing is reachable from outside but the published ports and SSH from allowed_cidr), or a provider that seals (AWS, Azure, GCP, OCI, Proxmox)"
    ))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;
    use crate::cloud::Cloud;
    use crate::cloud::vm::CloudVmModule;

    /// The resource that closes the way out on each sealing cloud: its type, and the part of its
    /// name every one has.
    const CLOSING: [(Cloud, &str); 4] = [
        (Cloud::Aws, "aws_network_acl"),
        (Cloud::Azure, "azurerm_subnet_network_security_group_association"),
        (Cloud::Gcp, "google_compute_firewall"),
        (Cloud::Oci, "oci_core_route_table_attachment"),
    ];

    /// The `depends_on` of `block`, each reference as its text.
    fn depends_on(block: &hcl::Body) -> Vec<String> {
        match isoloom_test_support::hcl_attribute(block, "depends_on") {
            Some(hcl::Expression::Array(refs)) => refs.iter().map(ToString::to_string).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn every_sealing_cloud_closes_a_routed_network_without_internet_only_after_every_machines_set_up() {
        let env = crate::testing::planned(
            "version: 1\nname: e\nnetworks:\n  outside: { cidr: 10.70.0.0/24 }\n  lan: { cidr: 10.70.20.0/24, gateway: fw, internet: false }\nmachines:\n  fw: { os: debian@12, networks: { outside: 5, lan: 254 } }\n  cache: { os: debian@12, networks: { lan: 20 } }\n  user: { os: debian@12, networks: { outside: 10 } }\n",
            Target::CloudVm,
        );
        let ready: Vec<String> = ["fw", "cache", "user"].iter().map(|m| format!("terraform_data.isoloom_ready_{m}")).collect();
        for (cloud, kind) in CLOSING {
            let tf = CloudVmModule::from(cloud).render(&env).unwrap_or_else(|e| panic!("{cloud:?}: {e}")).contents;
            let body = isoloom_test_support::hcl_body(&tf);
            let closing: Vec<&hcl::Block> = isoloom_test_support::hcl_blocks(&body, "resource", &[kind])
                .into_iter()
                .filter(|b| b.labels().get(1).is_some_and(|l| l.as_str().contains("sealed")))
                .collect();
            assert!(!closing.is_empty(), "{cloud:?} writes no {kind} that seals lan:\n{tf}");
            for b in closing {
                let waits = depends_on(b.body());
                for r in &ready {
                    assert!(waits.contains(r), "{cloud:?} {kind}.{:?} doesn't wait for {r}: {waits:?}", b.labels().get(1));
                }
            }
        }
    }

    #[test]
    fn a_cloud_whose_firewalls_only_allow_refuses_a_network_without_internet() {
        let env = crate::testing::planned(
            "version: 1\nname: s\nnetworks:\n  front: { cidr: 10.61.10.0/24 }\n  back: { cidr: 10.61.20.0/24, internet: false }\nmachines:\n  web: { os: debian@12, networks: { front: 10 } }\n  cache: { os: debian@12, networks: { back: 20 } }\n",
            Target::CloudVm,
        );
        assert!(refusal(&env, "Linode").is_some_and(|r| r.starts_with("network `back` has no internet")));
        let open = crate::testing::planned(
            "version: 1\nname: o\nnetworks:\n  front: { cidr: 10.61.10.0/24 }\nmachines:\n  web: { os: debian@12, networks: { front: 10 } }\n",
            Target::CloudVm,
        );
        assert!(refusal(&open, "Linode").is_none());
    }
}
