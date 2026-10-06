//! The DigitalOcean `cloud-vm` driver: it takes a single-network, single-Linux-VM lab (one
//! droplet, a firewall, the ready marker) and refuses anything else. `refusal` itself is
//! internal, so these tests read its decision from the presence of the generated file.

use isoloom_core::{Target, generate, parse};

const DO: &str = ".isoloom/cloud-vm/digitalocean/main.tf";

fn cloud_vm(yaml: &str) -> Vec<isoloom_core::GeneratedFile> {
    let spec = parse(yaml).expect("spec parses");
    generate(&spec, Target::CloudVm).expect("cloud-vm generates")
}

#[test]
fn single_network_single_vm_builds_a_droplet() {
    let files = cloud_vm(
        r#"
version: 1
name: do-lab
networks:
  lab: { cidr: 10.60.0.0/24 }
machines:
  web:
    networks: { lab: 10 }
    services: [{ port: 80, http: true, publish: 8080 }]
    vm: { os: debian-12, provision: [p.sh] }
"#,
    );
    let tf = &files.iter().find(|f| f.path == DO).expect("a digitalocean file (refusal is None)").contents;
    assert!(tf.contains("resource \"digitalocean_droplet\" \"web\""), "the droplet");
    assert!(tf.contains("resource \"digitalocean_firewall\" \"web\""), "the firewall");
    assert!(tf.contains("image     = \"debian-12-x64\""), "the Debian 12 droplet image");
    assert!(tf.contains("output \"ip\""), "the ip output");
    assert!(tf.contains("value = \"/var/lib/isoloom/ready\""), "the ready_file output");
    // The published port opens for allowed_cidr and is listed in the published output.
    assert!(tf.contains("port_range       = \"8080\""), "the published port");
    assert!(
        tf.contains("\"web/80\" = \"${digitalocean_droplet.web.ipv4_address}:8080\""),
        "the published map"
    );
}

#[test]
fn multi_network_is_refused() {
    // Two networks: DigitalOcean can't hold the fixed addressing, so it builds no file.
    let files = cloud_vm(
        r#"
version: 1
name: two-net
networks:
  a: { cidr: 10.60.0.0/24 }
  b: { cidr: 10.61.0.0/24 }
machines:
  web:
    networks: { a: 10 }
    services: [{ port: 80, http: true }]
    vm: { os: debian-12 }
  box:
    networks: { b: 10 }
    vm: { os: debian-12 }
"#,
    );
    assert!(files.iter().any(|f| f.path == ".isoloom/cloud-vm/aws/main.tf"), "AWS still generates");
    assert!(!files.iter().any(|f| f.path == DO), "no digitalocean file (refusal is Some)");
}
