//! Every VM a cloud module starts from user data that runs once (a lab machine's set-up, the
//! controller's, a Docker host's expiry) keeps it across a later apply: a `lifecycle` ignoring
//! that user data, wherever the cloud would replace or restart the VM for a change in it (the
//! launch's inputs, `var.expires_at` among them, differ on every run).

pub mod testing;

use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule, cloud_docker_modules};
use testing::{Outcome, example, file, planned};

/// A VM resource kind, the attribute its user data is in, and how `ignore_changes` names that
/// attribute.
struct Kind {
    resource: &'static str,
    attribute: &'static str,
    ignored: &'static str,
}

/// The VM kinds on every cloud, and where their user data is.
const KINDS: &[Kind] = &[
    Kind {
        resource: "aws_instance",
        attribute: "user_data_base64",
        ignored: "user_data_base64",
    },
    Kind {
        resource: "aws_instance",
        attribute: "user_data",
        ignored: "user_data",
    },
    Kind {
        resource: "azurerm_linux_virtual_machine",
        attribute: "custom_data",
        ignored: "custom_data",
    },
    Kind {
        resource: "azurerm_windows_virtual_machine",
        attribute: "custom_data",
        ignored: "custom_data",
    },
    Kind {
        resource: "google_compute_instance",
        attribute: "metadata_startup_script",
        ignored: "metadata_startup_script",
    },
    Kind {
        resource: "digitalocean_droplet",
        attribute: "user_data",
        ignored: "user_data",
    },
    Kind {
        resource: "oci_core_instance",
        attribute: "user_data",
        ignored: "metadata[\"user_data\"]",
    },
    Kind {
        resource: "linode_instance",
        attribute: "user_data",
        ignored: "metadata[0].user_data",
    },
];

/// Each top-level `resource` of `module`: its kind, its name and its body.
fn resources(module: &str) -> Vec<(&str, &str, &str)> {
    module
        .split("\nresource \"")
        .skip(1)
        .filter_map(|chunk| {
            let (kind, rest) = chunk.split_once('"')?;
            let name = rest.trim_start().strip_prefix('"')?.split('"').next()?;
            Some((kind, name, chunk))
        })
        .collect()
}

/// Whether `body` sets `attribute` (on a line of its own, as the modules write it).
fn sets(body: &str, attribute: &str) -> bool {
    body.lines()
        .any(|l| l.trim_start().strip_prefix(attribute).is_some_and(|rest| rest.trim_start().starts_with('=')))
}

/// The VMs of `module` with user data, each checked for its `lifecycle`; the names checked.
fn checked(module: &str, what: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (kind, name, body) in resources(module) {
        for k in KINDS.iter().filter(|k| k.resource == kind) {
            if !sets(body, k.attribute) {
                continue;
            }
            let ignore = format!("ignore_changes = [{}]", k.ignored);
            assert!(
                body.contains(&ignore),
                "{what}: {kind}.{name} has its user data in `{}` but no `{ignore}`",
                k.attribute
            );
            names.push(format!("{kind}.{name}"));
        }
    }
    names
}

/// The labs whose `cloud-vm` modules are checked, on every cloud that takes them.
const LABS: &[&str] = &["solo-web", "hello-stack", "ansible-pair", "windows-hello", "segmented", "configured-windows"];

#[test]
fn every_cloud_vm_keeps_its_user_data() -> Outcome {
    for cloud in Cloud::ALL {
        let mut all = Vec::new();
        for lab in LABS {
            let env = planned(&example(lab)?, Target::CloudVm);
            let Ok(module) = CloudVmModule::new(cloud).render(&env) else { continue };
            all.extend(checked(&module.contents, &format!("{} {lab}", cloud.id())));
        }
        assert!(!all.is_empty(), "{}: no VM with user data was checked", cloud.id());
        if matches!(cloud, Cloud::Aws | Cloud::Azure | Cloud::Gcp) {
            assert!(
                all.iter().any(|n| n.ends_with(".isoloom_controller")),
                "{}: the controller was checked ({all:?})",
                cloud.id()
            );
        }
    }
    Ok(())
}

#[test]
fn every_cloud_docker_host_keeps_its_expiry() -> Outcome {
    let files = cloud_docker_modules(&planned(&example("hello-stack")?, Target::CloudDocker))?;
    for cloud in Cloud::ALL {
        let module = file(&files, &format!("cloud-docker/{}/main.tf", cloud.id()))?;
        assert!(module.contains("var.expires_at"), "{}: the host's user data schedules its expiry", cloud.id());
        let hosts = checked(module, &format!("cloud-docker {}", cloud.id()));
        if cloud == Cloud::Gcp {
            // Its startup script is an entry of `metadata`, which the provider updates in place:
            // no replacement, no restart, and the next boot takes the new expiry.
            assert!(hosts.is_empty(), "gcp: the startup script is in `metadata`, not `metadata_startup_script`");
            assert!(sets(module, "startup-script"), "gcp: the startup script is a `metadata` entry");
        } else {
            assert_eq!(hosts.len(), 1, "{}: the host was checked", cloud.id());
        }
    }
    Ok(())
}
