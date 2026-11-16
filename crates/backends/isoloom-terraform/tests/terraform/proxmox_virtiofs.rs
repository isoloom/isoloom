//! Shared volumes as folders of the Proxmox server (`volumes.<name>.virtiofs`, not run on a real
//! host): the mapping, each VM's device, the mount by tag, and what can't take one.

use isoloom_test_support::{hcl_blocks, hcl_body, hcl_literals, hcl_names, hcl_nested, hcl_one, hcl_text, hcl_values};
use std::path::Path;

use crate::testing::{Outcome, example, file};
use isoloom_plan::Lab;
use isoloom_render::backend::Backend;
use isoloom_spec::model::{Spec, Target, Virtiofs, VirtiofsCache};
use isoloom_terraform::ProxmoxBackend;

/// The module of `spec`, a lowered spec of `lab`, checked valid.
/// Every VM's `virtiofs` block of the parsed module.
fn virtiofs(body: &hcl::Body) -> Vec<&hcl::Body> {
    hcl_blocks(body, "resource", &["proxmox_virtual_environment_vm"])
        .into_iter()
        .flat_map(|vm| hcl_nested(vm.body(), "virtiofs"))
        .map(hcl::Block::body)
        .collect()
}

fn render(lab: &Lab, spec: &Spec) -> Outcome<String> {
    let problems = isoloom_spec::validate(spec);
    if !problems.is_empty() {
        return Err(format!("the spec isn't valid: {problems:?}").into());
    }
    Ok(file(&ProxmoxBackend.render(&lab.plan(spec, Target::Proxmox, None)?)?, "proxmox/main.tf")?.to_owned())
}

/// The problems an author sees in `yaml`, one line each.
fn problems(yaml: &str) -> String {
    let found = isoloom_plan::Lab::parse(yaml).map_or_else(|refused| refused, |lab| lab.problems());
    let found: Vec<String> = found.iter().map(isoloom_spec::syntax::Problem::line).collect();
    found.join("\n")
}

/// Why Proxmox refuses `yaml`, if it does.
fn refusal(yaml: &str) -> Outcome<Option<String>> {
    crate::testing::refusal(&ProxmoxBackend, yaml)
}

/// The `shared-files` example, and its spec lowered for Proxmox with its volume a folder of the
/// server.
fn shared_files() -> Outcome<(Lab, Spec)> {
    let lab = example("shared-files")?;
    let mut spec = lab.for_target(Target::Proxmox);
    if let Some(volume) = spec.volumes.get_mut("shared") {
        volume.virtiofs = Some(Virtiofs::default());
    }
    Ok((lab, spec))
}

#[test]
fn proxmox_shared_files_on_virtiofs() -> Outcome {
    insta::assert_snapshot!({
        let (lab, spec) = shared_files()?;
        render(&lab, &spec)?
    });
    Ok(())
}

#[test]
fn the_module_creates_the_mapping_and_each_vm_mounts_it() -> Outcome {
    let tf = {
        let (lab, spec) = shared_files()?;
        render(&lab, &spec)?
    };
    let body = hcl_body(&tf);
    let mapping = hcl_one(&body, "proxmox_hardware_mapping_dir", "shared");
    assert_eq!(hcl_values(mapping, "name"), ["iso${var.slot}-shared"]);
    // Its one path, on the node: the volume's folder under the server's virtiofs root.
    assert!(hcl_literals(mapping).iter().any(|l| l == "${var.virtiofs_root}/shared-files/shared"), "{tf}");
    assert_eq!(hcl_blocks(&body, "variable", &["virtiofs_root"]).len(), 1);
    // Both VMs, the owner too: no NFS any more.
    let mounts = virtiofs(&body);
    assert_eq!(mounts.len(), 2, "{mounts:?}");
    for m in &mounts {
        assert_eq!(hcl_text(m, "mapping"), "proxmox_hardware_mapping_dir.shared.name");
        assert_eq!(hcl_text(m, "cache"), "auto");
    }
    let scripts: Vec<String> = hcl_literals(&body).into_iter().filter(|l| l.contains('\n')).collect();
    assert!(scripts.iter().all(|s| !s.contains("exportfs") && !s.contains("nfs")), "{scripts:?}");
    assert!(
        scripts.iter().any(|s| s.lines().any(|l| l.contains(" virtiofs defaults,nofail"))),
        "{scripts:?}"
    );
    Ok(())
}

#[test]
fn a_named_mapping_is_used_as_it_is() -> Outcome {
    let (lab, mut spec) = shared_files()?;
    spec.volumes["shared"].virtiofs = Some(Virtiofs {
        mapping: Some("lab-shared".into()),
        cache: Some(VirtiofsCache::Never),
        direct_io: true,
        xattr: false,
        acl: true,
    });
    let tf = render(&lab, &spec)?;
    let body = hcl_body(&tf);
    assert!(hcl_names(&body, "resource", "proxmox_hardware_mapping_dir").is_empty(), "{tf}");
    assert!(hcl_blocks(&body, "variable", &["virtiofs_root"]).is_empty(), "{tf}");
    let mounts = virtiofs(&body);
    assert_eq!(mounts.len(), 2, "both VMs mount it");
    for m in &mounts {
        let field = |k: &str| hcl_text(m, k);
        assert_eq!(
            (field("mapping"), field("cache"), field("direct_io")),
            ("lab-shared".into(), "never".into(), "true".into())
        );
        // ACLs imply extended attributes.
        assert_eq!((field("expose_xattr"), field("expose_acl")), ("true".into(), "true".into()));
    }
    Ok(())
}

#[test]
fn a_folder_of_the_volume_is_bound_from_the_whole() -> Outcome {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/shared-files/isoloom.yml");
    let text = std::fs::read_to_string(dir)?;
    let text = text.replace("volumes: { shared: /data/shared }", "volumes: { shared: { path: /data/shared, subpath: out } }")
        + "\noverrides:\n  proxmox:\n    volumes: { shared: { virtiofs: {} } }\n";
    let lab = Lab::parse(&text).map_err(|problems| format!("{problems:?}"))?;
    let tf = render(&lab, &lab.for_target(Target::Proxmox))?;
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
