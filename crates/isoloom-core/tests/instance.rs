//! Instances: a spec several times on one host, each with its own names, Docker blocks,
//! published ports and output folder.

use std::path::Path;

use isoloom_core::{Target, generate, generate_instance, instance, load};

fn example(name: &str) -> isoloom_core::Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name)).expect("example parses")
}

#[test]
fn an_instance_gets_its_own_name_blocks_and_ports() {
    let spec = example("hello-stack");
    let two = instance::apply(&spec, 2).unwrap();
    assert_eq!(two.name, "hello-stack-2");
    // The Docker block moves by the instance number in the second octet; the spec's own
    // addresses (what VMs use) stay as written.
    assert_eq!(two.networks["app"].docker.as_ref().unwrap().cidr, "10.62.0.0/24");
    assert_eq!(two.networks["app"].cidr, "10.60.0.0/24");
    assert_eq!(two.machines["web"].services[0].publish, Some(8280));
    assert!(instance::apply(&spec, 0).is_err());
    assert!(instance::apply(&spec, 100).is_err());
    let mut high = spec.clone();
    high.machines["web"].services[0].publish = Some(65500);
    assert!(instance::apply(&high, 1).unwrap_err().contains("65500"));
}

#[test]
fn instance_files_live_next_to_the_committed_ones() {
    let spec = example("segmented");
    let files = generate_instance(&spec, Target::Docker, 3).unwrap();
    let compose = files
        .iter()
        .find(|f| f.path == ".isoloom-3/docker/compose.yml")
        .expect("compose under .isoloom-3/");
    assert!(compose.contents.contains("name: segmented-3"), "{}", compose.contents);
    // Every network moved: 10.61.x -> 10.64.x, machines keeping their last octets.
    assert!(
        compose.contents.contains("subnet: 10.64.10.0/24") && compose.contents.contains("ipv4_address: 10.64.10.10"),
        "{}",
        compose.contents
    );
    assert!(!compose.contents.contains("ipv4_address: 10.61.") && !compose.contents.contains("subnet: 10.61."), "{}", compose.contents);
    // Paths inside the files follow the folder.
    assert!(compose.contents.contains("isoloom test docker"));
    assert!(
        files.iter().all(|f| f.path.starts_with(".isoloom-3/")),
        "{:?}",
        files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
    let snap = files.iter().find(|f| f.path == ".isoloom-3/resolved.json").unwrap();
    assert!(snap.contents.contains("\"instance\": 3"));
    assert!(snap.contents.contains("\"name\": \"segmented-3\""));
    // The committed files are untouched by instances.
    let plain = generate(&spec, Target::Docker).unwrap();
    assert!(plain.iter().any(|f| f.path == ".isoloom/docker/compose.yml"));
}

#[test]
fn vagrant_instances_differ_by_name_only() {
    let spec = example("hello-stack");
    let files = generate_instance(&spec, Target::Vagrant, 1).unwrap();
    let vf = &files.iter().find(|f| f.path == ".isoloom-1/vagrant/Vagrantfile").unwrap().contents;
    // VM labels and internal network names carry the instance; addresses don't move.
    assert!(vf.contains("isoloom-hello-stack-1-app") && vf.contains("hello-stack-1 · web"), "{vf}");
    assert!(vf.contains("10.60.0.10"));
    // Published ports shift so two instances can both forward to the host.
    assert!(vf.contains("guest: 80, host: 8180"), "{vf}");
    // The project copy leaves every instance folder behind.
    assert!(vf.contains("e.start_with?(\".isoloom\")"));
}
