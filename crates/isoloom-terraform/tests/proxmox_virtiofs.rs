//! Shared volumes as folders of the Proxmox server (`volumes.<name>.virtiofs`, not run on a real
//! host): the mapping, each VM's device, the mount by tag, and what can't take one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Write as _;
use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::{Target, Virtiofs, VirtiofsCache};
use isoloom_terraform::proxmox::ProxmoxBackend;

fn render(spec: &isoloom_spec::Spec) -> String {
    assert_eq!(isoloom_spec::validate(spec), vec![]);
    let e = Environment::build(spec, Target::Proxmox, None);
    match ProxmoxBackend.render(&e) {
        Ok(files) => files[0].contents.clone(),
        Err(e) => panic!("{e}"),
    }
}

fn refusal(yaml: &str) -> String {
    let spec = isoloom_spec::parse(yaml).unwrap();
    let e = Environment::build(&spec, Target::Proxmox, None);
    ProxmoxBackend.refusal(&e).expect("refused")
}

/// The `shared-files` example with its volume a folder of the server.
fn shared_files() -> isoloom_spec::Spec {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/shared-files");
    let mut spec = isoloom_spec::load(&dir).unwrap();
    spec.volumes["shared"].virtiofs = Some(Virtiofs::default());
    spec
}

#[test]
fn proxmox_shared_files_on_virtiofs() {
    insta::assert_snapshot!(render(&shared_files()));
}

#[test]
fn the_module_creates_the_mapping_and_each_vm_mounts_it() {
    let tf = render(&shared_files());
    assert!(tf.contains("resource \"proxmox_hardware_mapping_dir\" \"shared\""), "{tf}");
    assert!(tf.contains("\"iso${var.slot}-shared\""), "{tf}");
    assert!(tf.contains("path = \"${var.virtiofs_root}/shared-files/shared\""), "{tf}");
    assert!(tf.contains("variable \"virtiofs_root\""), "{tf}");
    // Both VMs, the owner too: no NFS any more.
    assert_eq!(tf.matches("mapping      = proxmox_hardware_mapping_dir.shared.name").count(), 2, "{tf}");
    assert!(tf.contains("cache        = \"auto\""), "{tf}");
    assert!(tf.contains("content     = proxmox_hardware_mapping_dir.shared.name"), "{tf}");
    assert!(!tf.contains("exportfs") && !tf.contains("nfs"), "{tf}");
    assert!(tf.contains("virtiofs defaults,nofail"), "{tf}");
}

#[test]
fn a_named_mapping_is_used_as_it_is() {
    let mut spec = shared_files();
    spec.volumes["shared"].virtiofs = Some(Virtiofs {
        mapping: Some("lab-shared".into()),
        cache: Some(VirtiofsCache::Never),
        direct_io: true,
        xattr: false,
        acl: true,
    });
    let tf = render(&spec);
    assert!(!tf.contains("proxmox_hardware_mapping_dir"), "{tf}");
    assert!(!tf.contains("virtiofs_root"), "{tf}");
    assert!(tf.contains("mapping      = \"lab-shared\""), "{tf}");
    assert!(tf.contains("cache        = \"never\""), "{tf}");
    assert!(tf.contains("direct_io    = true"), "{tf}");
    // ACLs imply extended attributes.
    assert!(tf.contains("expose_xattr = true") && tf.contains("expose_acl   = true"), "{tf}");
}

#[test]
fn a_folder_of_the_volume_is_bound_from_the_whole() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/shared-files/isoloom.yml");
    let text = std::fs::read_to_string(dir).unwrap();
    let text = text
        .replace("shared: { owner: writer }", "shared: { owner: writer, virtiofs: {} }")
        .replace("volumes: { shared: /data/shared }", "volumes: { shared: { path: /data/shared, subpath: out } }");
    let tf = render(&isoloom_spec::parse(&text).unwrap());
    assert!(tf.contains("/var/lib/isoloom/volumes/shared/out"), "{tf}");
    assert!(tf.contains("none bind,nofail"), "{tf}");
}

#[test]
fn a_container_cant_mount_a_folder_of_the_server() {
    let why = refusal(
        "version: 1\nname: f\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nvolumes: { shared: { virtiofs: {} } }\nmachines:\n  web: { networks: { lan: 10 }, volumes: { shared: /srv }, docker: { image: 'nginx:1.27', lxc: true } }\n",
    );
    assert!(why.contains("only a VM mounts") && why.contains("bind mount"), "{why}");
}

#[test]
fn a_vm_takes_at_most_eight_folders() {
    let mut yaml = String::from("version: 1\nname: f\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nvolumes:\n");
    for i in 0..9 {
        let _ = writeln!(yaml, "  v{i}: {{ virtiofs: {{}} }}");
    }
    let mounts: Vec<String> = (0..9).map(|i| format!("v{i}: /v{i}")).collect();
    let _ = writeln!(
        yaml,
        "machines:\n  web: {{ networks: {{ lan: 10 }}, volumes: {{ {} }}, vm: {{ os: debian@12, provision: [s.sh] }} }}",
        mounts.join(", ")
    );
    let why = refusal(&yaml);
    assert!(why.contains("at most 8"), "{why}");
}

#[test]
fn windows_is_refused_with_the_driver_it_lacks() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: f\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nvolumes: { shared: { virtiofs: {} } }\nmachines:\n  dc: { networks: { lan: 10 }, volumes: { shared: /srv }, vm: { os: windows-server@2022, provision: [s.ps1] } }\n",
    )
    .unwrap();
    let problems = format!("{:?}", isoloom_spec::validate(&spec));
    assert!(problems.contains("virtio-fs and WinFsp"), "{problems}");
}

#[test]
fn a_mapping_id_is_one_proxmox_takes() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: f\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nvolumes: { shared: { virtiofs: { mapping: '9 lives' } } }\nmachines:\n  web: { networks: { lan: 10 }, volumes: { shared: /srv }, vm: { os: debian@12, provision: [s.sh] } }\n",
    )
    .unwrap();
    let problems = format!("{:?}", isoloom_spec::validate(&spec));
    assert!(problems.contains("volumes.shared.virtiofs.mapping"), "{problems}");
}
