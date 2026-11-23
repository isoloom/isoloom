//! The GCP driver for the `cloud-vm` target: it produces a valid-shaped `google` module for the
//! specs it supports, a VPC network per network, and declines Windows, which it doesn't set up.

use isoloom_spec::Target;
use isoloom_targets::generate;

use isoloom_test_support::{hcl, hcl_attribute, hcl_blocks, hcl_body, hcl_map, hcl_names, hcl_nested, hcl_one, hcl_text, hcl_values};

use crate::testing::{Outcome, example};

/// The GCP module of the example `name`, when it is generated.
fn gcp(name: &str) -> Outcome<Option<String>> {
    let files = generate(&example(name)?, Target::CloudVm)?;
    Ok(files.into_iter().find(|f| f.path.ends_with("cloud-vm/gcp/main.tf")).map(|f| f.contents))
}

/// The value of output `name`, as HCL writes it.
fn output(body: &hcl::Body, name: &str) -> String {
    hcl_blocks(body, "output", &[name])
        .first()
        .and_then(|o| hcl_attribute(o.body(), "value"))
        .map(ToString::to_string)
        .unwrap_or_default()
}

#[test]
fn gcp_uses_the_google_provider_and_static_internal_ips() -> Outcome {
    let tf = gcp("hello-stack")?.expect("hello-stack has a gcp file");
    let body = hcl_body(&tf);
    let terraform = hcl_blocks(&body, "terraform", &[]);
    let required = terraform.first().map(|t| hcl_nested(t.body(), "required_providers")).unwrap_or_default();
    let provider: Vec<(String, String)> = required.first().map(|r| hcl_map(r.body(), "google")).unwrap_or_default();
    assert_eq!(
        provider,
        [
            ("source".to_owned(), "hashicorp/google".to_owned()),
            ("version".to_owned(), "~> 8.5.0".to_owned())
        ]
    );
    assert_eq!(hcl_names(&body, "resource", "google_compute_network"), ["app"]);
    assert_ne!(hcl_names(&body, "resource", "google_compute_subnetwork"), Vec::<String>::new());
    // Every address of the spec is kept: each machine at its octet, pinned on its NIC.
    let pinned: Vec<String> = hcl_blocks(&body, "resource", &["google_compute_instance"])
        .iter()
        .flat_map(|i| hcl_nested(i.body(), "network_interface"))
        .flat_map(|n| hcl_values(n.body(), "network_ip"))
        .collect();
    assert!(
        pinned.contains(&"10.60.0.10".to_owned()) && pinned.contains(&"10.60.0.20".to_owned()),
        "{pinned:?}"
    );
    // The public address comes from the instance's access config.
    assert!(
        output(&body, "ip").contains("network_interface[0].access_config[0].nat_ip"),
        "{}",
        output(&body, "ip")
    );
    // The same outputs the AWS driver gives.
    let ready = hcl_blocks(&body, "output", &["ready_file"]);
    assert_eq!(ready.first().map(|o| hcl_text(o.body(), "value")).as_deref(), Some("/var/lib/isoloom/ready"));
    assert_ne!(output(&body, "machines"), "");
    // web publishes 8080: a firewall opening and a published output.
    assert!(hcl_names(&body, "resource", "google_compute_firewall").contains(&"web_published".to_owned()));
    assert_ne!(output(&body, "published"), "");
    Ok(())
}

#[test]
fn gcp_opens_reach_rules_across_networks() -> Outcome {
    // segmented has three networks and `reach` rules: a VPC network each, peered both ways.
    let tf = gcp("segmented")?.expect("segmented has a gcp file");
    let body = hcl_body(&tf);
    // front may reach back on 6379 only: a firewall on the cache, from front's range, in the
    // cache's network's VPC.
    let rule = hcl_one(&body, "google_compute_firewall", "cache_reach_front");
    let allowed: std::collections::BTreeSet<String> = hcl_nested(rule, "allow").iter().flat_map(|a| hcl_values(a.body(), "ports")).collect();
    assert_eq!(allowed.into_iter().collect::<Vec<_>>(), ["6379"], "TCP and UDP, that port alone");
    assert_eq!(hcl_text(rule, "network"), "google_compute_network.back.id");
    // The subnetworks carry each network's exact range.
    assert!(hcl_values(&body, "ip_cidr_range").contains(&"10.61.20.0/24".to_owned()));
    let peerings = hcl_names(&body, "resource", "google_compute_network_peering");
    assert!(
        peerings.contains(&"front_back".to_owned()) && peerings.contains(&"back_front".to_owned()),
        "{peerings:?}"
    );
    Ok(())
}

#[test]
fn gcp_puts_a_machine_on_several_networks_with_an_interface_in_each_vpc() -> Outcome {
    // vlan-office's admin is on several networks: an interface in each VPC, forwarding on.
    let tf = gcp("vlan-office")?.expect("vlan-office has a gcp file");
    let body = hcl_body(&tf);
    let admin = hcl_one(&body, "google_compute_instance", "admin");
    let interfaces = hcl_nested(admin, "network_interface");
    assert!(interfaces.len() > 1, "{tf}");
    assert_eq!(hcl_text(admin, "can_ip_forward"), "true");
    let public: Vec<usize> = interfaces.iter().map(|n| hcl_nested(n.body(), "access_config").len()).collect();
    assert_eq!(public.first(), Some(&1), "the public address on the first");
    assert_eq!(public.iter().sum::<usize>(), 1, "and only there: {public:?}");
    Ok(())
}

#[test]
fn gcp_declines_windows() -> Outcome {
    // windows-hello is Windows-only: AWS builds it, the GCP driver declines it.
    let files = generate(&example("windows-hello")?, Target::CloudVm)?;
    assert!(files.iter().any(|f| f.path.ends_with("cloud-vm/aws/main.tf")));
    assert!(
        !files.iter().any(|f| f.path.ends_with("cloud-vm/gcp/main.tf")),
        "Isoloom generates no Windows VM on Google Cloud, so no gcp file"
    );
    Ok(())
}
