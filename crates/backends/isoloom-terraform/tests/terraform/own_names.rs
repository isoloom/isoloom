//! Isoloom's own VMs (the router, the controller) take Isoloom's names (`isoloom-router`,
//! `isoloom-controller`) for their Terraform resources, snippets and VMs, so environment machines named
//! `router` or `controller` never collide with them.

use std::collections::BTreeMap;

use crate::testing::{Outcome, cloud_vm, file, plan, proxmox};
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, EsxiBackend};
use isoloom_test_support::{hcl_attribute, hcl_blocks, hcl_body, hcl_literals, hcl_resource, hcl_values};

/// Machines named as Isoloom's own VMs would be, with a reach rule (a router) and a `provision:`
/// step (a controller).
const LAB: &str = "version: 1
name: own
networks: { a: { cidr: 10.9.1.0/24 }, b: { cidr: 10.9.2.0/24 } }
reach: [ { from: a, to: b, ports: [80] } ]
machines:
  router: { networks: { a: 5 }, os: debian@12 }
  controller: { networks: { b: 5 }, os: debian@12 }
provision: [ { ansible: p.yml } ]
";

/// What more than one resource of a type takes: a label, a `name` or `guest_name` of its own,
/// or a snippet's `file_name`.
fn twice(tf: &str) -> Vec<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for resource in hcl_blocks(&hcl_body(tf), "resource", &[]) {
        let labels: Vec<&str> = resource.labels().iter().map(hcl::BlockLabel::as_str).collect();
        let [kind, label] = labels[..] else { continue };
        *seen.entry(format!("{kind} label {label}")).or_default() += 1;
        for key in ["name", "guest_name"] {
            if let Some(value) = hcl_attribute(resource.body(), key) {
                *seen.entry(format!("{kind} {key} {value}")).or_default() += 1;
            }
        }
        for value in hcl_values(resource.body(), "file_name") {
            *seen.entry(format!("{kind} file_name {value}")).or_default() += 1;
        }
    }
    seen.into_iter().filter(|(_, n)| *n > 1).map(|(what, _)| what).collect()
}

#[test]
fn machines_named_router_and_controller_never_collide_with_isoloom_s_own() -> Outcome {
    let px = proxmox(LAB)?;
    assert_eq!(twice(&px), Vec::<String>::new(), "Proxmox");
    let snippet = hcl_resource(&px, "proxmox_virtual_environment_file", "isoloom_router");
    assert_eq!(hcl_values(&snippet, "file_name"), ["iso${var.slot}-isoloom-router.yaml"]);
    let esxi = file(&EsxiBackend.render(&plan(LAB, Target::Esxi)?)?, "esxi/main.tf")?.to_owned();
    assert_eq!(twice(&esxi), Vec::<String>::new(), "ESXi");
    for (cloud, vm) in [
        (Cloud::Aws, "aws_instance"),
        (Cloud::Azure, "azurerm_linux_virtual_machine"),
        (Cloud::Gcp, "google_compute_instance"),
    ] {
        let tf = cloud_vm(cloud, LAB)?;
        assert_eq!(twice(&tf), Vec::<String>::new(), "{}", cloud.id());
        let named: Vec<String> = hcl_literals(&hcl_resource(&tf, vm, "isoloom_controller"))
            .into_iter()
            .filter(|l| l.ends_with("-isoloom-controller"))
            .collect();
        assert!(!named.is_empty(), "{}: the controller's VM takes Isoloom's name", cloud.id());
    }
    Ok(())
}
