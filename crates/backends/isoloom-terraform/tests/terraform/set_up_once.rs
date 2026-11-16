//! Every VM a cloud module starts from user data that runs once (an environment machine's set-up, the
//! controller's, a Docker host's expiry) keeps it across a later apply: a `lifecycle` ignoring
//! that user data, wherever the cloud would replace or restart the VM for a change in it (the
//! launch's inputs, `var.expires_at` among them, differ on every run).

use crate::testing::{Outcome, example, file, planned};
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule, cloud_docker_modules};
use isoloom_test_support::{hcl_attribute, hcl_blocks, hcl_body, hcl_map, hcl_nested, hcl_resource};

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

/// Attribute `key` of `body` as HCL writes it, without its whitespace (`[user_data]`).
fn expression(body: &hcl::Body, key: &str) -> Option<String> {
    Some(hcl::format::to_string(hcl_attribute(body, key)?).ok()?.split_whitespace().collect())
}

/// Whether `body` sets `key`: an attribute of its own or of a nested block (Linode's
/// `metadata { user_data }`), or a key of an object it sets (Oracle Cloud's
/// `metadata = { user_data }`).
fn sets(body: &hcl::Body, key: &str) -> bool {
    body.iter().any(|s| match s {
        hcl::Structure::Attribute(a) => a.key() == key || hcl_map(body, a.key()).iter().any(|(k, _)| k == key),
        hcl::Structure::Block(b) => sets(b.body(), key),
    })
}

/// The VMs of `module` with user data, each checked for its `lifecycle`; the names checked.
fn checked(module: &str, what: &str) -> Vec<String> {
    let body = hcl_body(module);
    let mut names = Vec::new();
    for k in KINDS {
        for vm in hcl_blocks(&body, "resource", &[k.resource]) {
            let name = vm.labels().get(1).map(|l| l.as_str().to_owned()).unwrap_or_default();
            if !sets(vm.body(), k.attribute) {
                continue;
            }
            let ignored: Vec<String> = hcl_nested(vm.body(), "lifecycle")
                .iter()
                .filter_map(|l| expression(l.body(), "ignore_changes"))
                .collect();
            assert_eq!(
                ignored,
                [format!("[{}]", k.ignored)],
                "{what}: {}.{name} has its user data in `{}`",
                k.resource,
                k.attribute
            );
            names.push(format!("{}.{name}", k.resource));
        }
    }
    names
}

/// The environments whose `cloud-vm` modules are checked, on every cloud that takes them.
const LABS: &[&str] = &["solo-web", "hello-stack", "ansible-pair", "windows-hello", "segmented", "configured-windows"];

#[test]
fn every_cloud_vm_keeps_its_user_data() -> Outcome {
    for cloud in Cloud::ALL {
        let mut all = Vec::new();
        for lab in LABS {
            let env = planned(&example(lab)?, Target::CloudVm)?;
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
    let files = cloud_docker_modules(&planned(&example("hello-stack")?, Target::CloudDocker)?, &crate::testing::docker_host(&[]))?;
    for cloud in Cloud::ALL {
        let module = file(&files, &format!("cloud-docker/{}/main.tf", cloud.id()))?;
        let hosts = checked(module, &format!("cloud-docker {}", cloud.id()));
        if cloud == Cloud::Gcp {
            // Its startup script is an entry of `metadata`, which the provider updates in place:
            // no replacement, no restart, and the next boot takes the new expiry.
            assert!(hosts.is_empty(), "gcp: the startup script is in `metadata`, not `metadata_startup_script`");
            let host = hcl_resource(module, "google_compute_instance", "env");
            let script = hcl_map(&host, "metadata").into_iter().find(|(k, _)| k == "startup-script");
            assert!(
                script.is_some_and(|(_, s)| s.contains("${var.expires_at}")),
                "gcp: the startup script schedules the expiry"
            );
        } else {
            assert_eq!(hosts.len(), 1, "{}: the host was checked", cloud.id());
            let (kind, _) = hosts[0].split_once('.').ok_or("a host")?;
            let texts = crate::testing::texts(module, kind, "env");
            assert!(
                texts.iter().any(|t| t.contains("${var.expires_at}")),
                "{}: the host's user data schedules its expiry",
                cloud.id()
            );
        }
    }
    Ok(())
}
