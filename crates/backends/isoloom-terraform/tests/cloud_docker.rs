//! The cloud-docker modules of hello-stack on every cloud, and the cloud-services scripts of
//! cloud-bucket, rendered from their plan, as snapshots; and their determinism.

pub mod testing;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{CloudServicesBackend, cloud_docker_modules as modules};
use testing::{Outcome, example, file};

/// Example `name`'s plan for `target`.
fn env(name: &str, target: Target) -> Outcome<Environment> {
    Ok(testing::planned(&example(name)?, target))
}

/// hello-stack's module on `cloud`.
fn cloud(cloud: &str) -> Outcome<String> {
    let files = modules(&env("hello-stack", Target::CloudDocker)?)?;
    Ok(file(&files, &format!("cloud-docker/{cloud}/main.tf"))?.to_owned())
}

#[test]
fn cloud_docker_aws_hello_stack() -> Outcome {
    insta::assert_snapshot!(cloud("aws")?);
    Ok(())
}

#[test]
fn cloud_docker_azure_hello_stack() -> Outcome {
    insta::assert_snapshot!(cloud("azure")?);
    Ok(())
}

#[test]
fn cloud_docker_gcp_hello_stack() -> Outcome {
    insta::assert_snapshot!(cloud("gcp")?);
    Ok(())
}

#[test]
fn cloud_docker_digitalocean_hello_stack() -> Outcome {
    insta::assert_snapshot!(cloud("digitalocean")?);
    Ok(())
}

#[test]
fn cloud_docker_linode_hello_stack() -> Outcome {
    insta::assert_snapshot!(cloud("linode")?);
    Ok(())
}

#[test]
fn cloud_docker_oci_hello_stack() -> Outcome {
    insta::assert_snapshot!(cloud("oci")?);
    Ok(())
}

#[test]
fn cloud_services_cloud_bucket() -> Outcome {
    let files = CloudServicesBackend.render(&env("cloud-bucket", Target::CloudServices)?)?;
    let all: String = files.iter().map(|f| format!("==> {}\n{}", f.path, f.contents)).collect::<Vec<_>>().concat();
    insta::assert_snapshot!(all);
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for name in ["hello-stack", "udp-dns", "segmented", "workbench"] {
        let e = env(name, Target::CloudDocker)?;
        assert_eq!(modules(&e)?, modules(&e)?, "{name}");
    }
    let e = env("cloud-bucket", Target::CloudServices)?;
    assert_eq!(CloudServicesBackend.render(&e), CloudServicesBackend.render(&e));
    Ok(())
}
