//! Isoloom's own VMs (the router, the controller) take Isoloom's names (`isoloom-router`,
//! `isoloom-controller`) for their Terraform resources, snippets and VMs, so lab machines named
//! `router` or `controller` never collide with them.

pub mod testing;

use std::collections::BTreeMap;

use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, EsxiBackend};
use testing::{Outcome, cloud_vm, file, plan, proxmox};

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

/// What more than one resource of a type takes: a label, a top-level `name` or `guest_name`, or
/// a snippet's `file_name`.
fn twice(tf: &str) -> Vec<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut kind = "";
    for line in tf.lines() {
        if let Some(rest) = line.strip_prefix("resource \"") {
            let mut parts = rest.split('"');
            kind = parts.next().unwrap_or_default();
            *seen.entry(format!("{kind} label {}", parts.nth(1).unwrap_or_default())).or_default() += 1;
            continue;
        }
        let Some((key, value)) = line.trim().split_once('=') else { continue };
        let key = key.trim();
        let top = line.starts_with("  ") && !line.starts_with("   ");
        if (top && (key == "name" || key == "guest_name")) || key == "file_name" {
            *seen.entry(format!("{kind} {key} {}", value.trim())).or_default() += 1;
        }
    }
    seen.into_iter().filter(|(_, n)| *n > 1).map(|(what, _)| what).collect()
}

#[test]
fn machines_named_router_and_controller_never_collide_with_isoloom_s_own() -> Outcome {
    let px = proxmox(LAB)?;
    assert_eq!(twice(&px), Vec::<String>::new(), "Proxmox");
    assert!(px.contains("file_name = \"iso${var.slot}-isoloom-router.yaml\""), "{px}");
    let esxi = file(&EsxiBackend.render(&plan(LAB, Target::Esxi)?)?, "esxi/main.tf")?.to_owned();
    assert_eq!(twice(&esxi), Vec::<String>::new(), "ESXi");
    for cloud in [Cloud::Aws, Cloud::Azure, Cloud::Gcp] {
        let tf = cloud_vm(cloud, LAB)?;
        assert_eq!(twice(&tf), Vec::<String>::new(), "{}", cloud.id());
        assert!(tf.contains("-isoloom-controller"), "{}", cloud.id());
    }
    Ok(())
}
