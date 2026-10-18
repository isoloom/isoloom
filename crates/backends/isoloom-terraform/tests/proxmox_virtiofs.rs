//! Shared volumes as folders of the Proxmox server (`volumes.<name>.virtiofs`, not run on a real
//! host): the mapping, each VM's device, the mount by tag, and what can't take one.

pub mod testing;

use std::path::Path;

use isoloom_plan::{Environment, Lab};
use isoloom_render::backend::Backend;
use isoloom_spec::model::{Spec, Target, Virtiofs, VirtiofsCache};
use isoloom_terraform::ProxmoxBackend;
use testing::{Outcome, example, file};

/// The module of `spec`, a lowered spec, checked valid.
fn render(spec: &Spec) -> Outcome<String> {
    let problems = isoloom_spec::validate(spec);
    if !problems.is_empty() {
        return Err(format!("the spec isn't valid: {problems:?}").into());
    }
    Ok(file(&ProxmoxBackend.render(&Environment::build(spec, Target::Proxmox, None))?, "proxmox/main.tf")?.to_owned())
}

/// The problems an author sees in `yaml`, one line each.
fn problems(yaml: &str) -> String {
    let found: Vec<String> = isoloom_plan::problems_in(yaml).iter().map(isoloom_spec::syntax::Problem::line).collect();
    found.join("\n")
}

/// Why Proxmox refuses `yaml`, if it does.
fn refusal(yaml: &str) -> Outcome<Option<String>> {
    testing::refusal(&ProxmoxBackend, yaml)
}

/// The `shared-files` example, lowered for Proxmox, with its volume a folder of the server.
fn shared_files() -> Outcome<Spec> {
    let mut spec = example("shared-files")?.for_target(Target::Proxmox);
    if let Some(volume) = spec.volumes.get_mut("shared") {
        volume.virtiofs = Some(Virtiofs::default());
    }
    Ok(spec)
}

#[test]
fn proxmox_shared_files_on_virtiofs() -> Outcome {
    insta::assert_snapshot!(render(&shared_files()?)?);
    Ok(())
}

#[test]
fn the_module_creates_the_mapping_and_each_vm_mounts_it() -> Outcome {
    let tf = render(&shared_files()?)?;
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
    Ok(())
}

#[test]
fn a_named_mapping_is_used_as_it_is() -> Outcome {
    let mut spec = shared_files()?;
    spec.volumes["shared"].virtiofs = Some(Virtiofs {
        mapping: Some("lab-shared".into()),
        cache: Some(VirtiofsCache::Never),
        direct_io: true,
        xattr: false,
        acl: true,
    });
    let tf = render(&spec)?;
    assert!(!tf.contains("proxmox_hardware_mapping_dir"), "{tf}");
    assert!(!tf.contains("virtiofs_root"), "{tf}");
    assert!(tf.contains("mapping      = \"lab-shared\""), "{tf}");
    assert!(tf.contains("cache        = \"never\""), "{tf}");
    assert!(tf.contains("direct_io    = true"), "{tf}");
    // ACLs imply extended attributes.
    assert!(tf.contains("expose_xattr = true") && tf.contains("expose_acl   = true"), "{tf}");
    Ok(())
}

#[test]
fn a_folder_of_the_volume_is_bound_from_the_whole() -> Outcome {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/shared-files/isoloom.yml");
    let text = std::fs::read_to_string(dir)?;
    let text = text.replace("volumes: { shared: /data/shared }", "volumes: { shared: { path: /data/shared, subpath: out } }")
        + "\noverrides:\n  proxmox:\n    volumes: { shared: { virtiofs: {} } }\n";
    let lab = Lab::parse(&text).map_err(|problems| format!("{problems:?}"))?;
    let tf = render(&lab.for_target(Target::Proxmox))?;
    assert!(tf.contains("/var/lib/isoloom/volumes/shared/out"), "{tf}");
    assert!(tf.contains("none bind,nofail"), "{tf}");
    Ok(())
}

#[test]
fn a_container_cant_mount_a_folder_of_the_server() -> Outcome {
    let why = refusal(
        "version: 1\nname: f\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nvolumes: { shared: {} }\nmachines:\n  web: { networks: { lan: 10 }, volumes: { shared: /srv }, app: { image: 'nginx:1.27' } }\noverrides: { proxmox: { machines: { web: { lxc: true } }, volumes: { shared: { virtiofs: {} } } } }\n",
    )?.expect("refused");
    assert!(why.contains("only a VM mounts") && why.contains("bind mount"), "{why}");
    Ok(())
}

#[test]
fn a_vm_takes_at_most_eight_folders() -> Outcome {
    let volumes = (0..9).map(|i| format!("  v{i}: {{}}\n")).collect::<Vec<_>>().concat();
    let folders = (0..9).map(|i| format!("      v{i}: {{ virtiofs: {{}} }}\n")).collect::<Vec<_>>().concat();
    let mounts: Vec<String> = (0..9).map(|i| format!("v{i}: /v{i}")).collect();
    let yaml = format!(
        "version: 1\nname: f\nnetworks: {{ lan: {{ cidr: 10.30.0.0/24 }} }}\nvolumes:\n{volumes}machines:\n  web: {{ networks: {{ lan: 10 }}, volumes: {{ {} }}, os: debian@12, app: {{ steps: [{{ run: s.sh }}] }} }}\noverrides:\n  proxmox:\n    volumes:\n{folders}",
        mounts.join(", ")
    );
    let why = refusal(&yaml)?.expect("refused");
    assert!(why.contains("at most 8"), "{why}");
    Ok(())
}

#[test]
fn windows_is_refused_with_the_driver_it_lacks() {
    let problems = problems(
        "version: 1\nname: f\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nvolumes: { shared: {} }\nmachines:\n  dc: { networks: { lan: 10 }, volumes: { shared: /srv }, os: windows-server@2022, app: { steps: [{ run: s.ps1 }] } }\noverrides: { proxmox: { volumes: { shared: { virtiofs: {} } } } }\n",
    );
    assert!(problems.contains("virtio-fs and WinFsp"), "{problems}");
}

#[test]
fn a_mapping_id_is_one_proxmox_takes() {
    let problems = problems(
        "version: 1\nname: f\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nvolumes: { shared: {} }\nmachines:\n  web: { networks: { lan: 10 }, volumes: { shared: /srv }, os: debian@12, app: { steps: [{ run: s.sh }] } }\noverrides: { proxmox: { volumes: { shared: { virtiofs: { mapping: '9 lives' } } } } }\n",
    );
    assert!(problems.contains("overrides.proxmox.volumes.shared.virtiofs.mapping"), "{problems}");
}
