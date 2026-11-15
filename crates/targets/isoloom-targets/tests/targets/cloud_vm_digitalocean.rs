//! The DigitalOcean `cloud-vm` driver: it takes a single-network, single-Linux-VM environment (one
//! droplet, a firewall, the ready marker) and refuses anything else. `refusal` itself is
//! internal, so these tests read its decision from the presence of the generated file.

use isoloom_spec::Target;
use isoloom_targets::generate;
use isoloom_test_support::{hcl_blocks, hcl_body, hcl_literals, hcl_one, hcl_resource, hcl_text, hcl_values};

use crate::testing::{Outcome, lab};

const DO: &str = ".isoloom/cloud-vm/digitalocean/main.tf";

/// The `cloud-vm` files of `yaml`.
fn cloud_vm(yaml: &str) -> Outcome<Vec<isoloom_targets::GeneratedFile>> {
    Ok(generate(&lab(yaml)?, Target::CloudVm)?)
}

#[test]
fn single_network_single_vm_builds_a_droplet() -> Outcome {
    let files = cloud_vm(
        r"
version: 1
name: do-lab
networks:
  lab: { cidr: 10.60.0.0/24 }
machines:
  web:
    networks: { lab: 10 }
    os: debian@12
    ports: { web: { port: 80, protocol: http, publish: 8080 } }
    app: { steps: [{ run: p.sh }] }
",
    )?;
    let tf = &files.iter().find(|f| f.path == DO).expect("a digitalocean file (refusal is None)").contents;
    let body = hcl_body(tf);
    let droplet = hcl_one(&body, "digitalocean_droplet", "web");
    assert_eq!(hcl_text(droplet, "image"), "debian-12-x64", "the Debian 12 droplet image");
    let firewall = hcl_one(&body, "digitalocean_firewall", "web");
    crate::testing::assert_cloud_outputs(&body);
    // The published port opens for allowed_cidr and is listed in the published output.
    assert!(hcl_values(firewall, "port_range").contains(&"8080".to_owned()), "the published port: {tf}");
    let published = hcl_blocks(&body, "output", &["published"]);
    let map: Vec<String> = published.iter().flat_map(|o| hcl_literals(o.body())).collect();
    assert_eq!(map, ["${digitalocean_droplet.web.ipv4_address}:8080"], "the published map: {tf}");
    assert_eq!(crate::testing::published_keys(&body), ["web/web"], "the published map: {tf}");
    Ok(())
}

#[test]
fn multi_network_is_refused() -> Outcome {
    // Two networks: DigitalOcean can't hold the fixed addressing, so it builds no file.
    let files = cloud_vm(
        r"
version: 1
name: two-net
networks:
  a: { cidr: 10.60.0.0/24 }
  b: { cidr: 10.61.0.0/24 }
machines:
  web:
    networks: { a: 10 }
    os: debian@12
    ports: { web: { port: 80, protocol: http } }
  box:
    os: debian@12
    networks: { b: 10 }
",
    )?;
    assert!(files.iter().any(|f| f.path == ".isoloom/cloud-vm/aws/main.tf"), "AWS still generates");
    assert!(!files.iter().any(|f| f.path == DO), "no digitalocean file (refusal is Some)");
    Ok(())
}

/// A one-machine environment whose machine has `resources`, on DigitalOcean: its module, or why it is
/// refused there.
fn sized(resources: &str) -> Outcome<Result<String, String>> {
    let lab = isoloom_targets::Lab::parse(&format!(
        "version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  web:\n    networks: {{ lab: 10 }}\n    os: debian@12\n    resources: {resources}\n    app: {{ steps: [{{ run: p.sh }}] }}\n"
    ))
    .map_err(|p| format!("{p:?}"))?;
    let files = generate(&lab, Target::CloudVm)?;
    if let Some(f) = files.iter().find(|f| f.path == DO) {
        return Ok(Ok(f.contents.clone()));
    }
    let snapshot = isoloom_targets::resolved::resolve(&lab);
    let refused = snapshot.pointer("/targets/clouds/cloud-vm/refused/digitalocean");
    Ok(Err(refused.and_then(serde_json::Value::as_str).unwrap_or_default().to_owned()))
}

#[test]
fn a_droplet_size_fits_the_disk_too_and_a_disk_beyond_the_largest_is_refused() -> Outcome {
    // 1 GB of memory fits the smallest droplet; a 50 GB disk takes the next one (60 GB).
    let tf = sized("{ memory_mb: 1024, disk_gb: 50 }")?.map_err(|e| format!("refused: {e}"))?;
    assert_eq!(hcl_text(&hcl_resource(&tf, "digitalocean_droplet", "web"), "size"), "s-2vcpu-2gb", "{tf}");
    let small = sized("{ memory_mb: 1024 }")?.map_err(|e| format!("refused: {e}"))?;
    assert_eq!(hcl_text(&hcl_resource(&small, "digitalocean_droplet", "web"), "size"), "s-1vcpu-1gb", "{small}");
    let refused = sized("{ memory_mb: 1024, disk_gb: 400 }")?.err().ok_or("a 400 GB disk isn't refused")?;
    assert_eq!(
        refused,
        "machine `web` asks for a 400 GB disk: the largest DigitalOcean size here, `s-8vcpu-16gb`, has 320 GB"
    );
    Ok(())
}
