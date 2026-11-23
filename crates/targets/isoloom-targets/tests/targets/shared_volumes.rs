//! `volumes:` shared between machines: one Docker volume, one Kubernetes claim (its users on
//! the owner's node), and on VMs the owner's directory served over NFS to the others.

use isoloom_spec::{Spec, Target, parse};
use isoloom_targets::Lab;

use isoloom_test_support::{manifest, manifest_names, yaml, yaml_documents, yaml_keys_at, yaml_str, yaml_strs, yaml_values};

use crate::testing::{Outcome, define, defined, generated, set_up, source, vagrantfile};

/// The shared-files example, as written.
fn shared_source() -> Outcome<Spec> {
    source("shared-files")
}

/// The shared-files example.
fn shared() -> Outcome<Lab> {
    Ok(Lab::new(shared_source()?))
}

#[test]
fn compose_mounts_one_volume_in_both() -> Outcome {
    let doc = yaml(&generated(&shared()?, Target::Docker, "docker/compose.yml")?);
    assert!(yaml_strs(&doc, "services.writer.volumes").contains(&"shared:/srv/shared".to_owned()));
    assert!(yaml_strs(&doc, "services.reader.volumes").contains(&"shared:/data/shared".to_owned()));
    let volumes = yaml_keys_at(&doc, "volumes");
    assert_eq!(
        volumes,
        ["shared", "writer-data"],
        "one volume, not one per machine (and the writer's data disk)"
    );
    Ok(())
}

#[test]
fn kubernetes_has_one_claim_and_the_reader_beside_the_owner() -> Outcome {
    let docs = yaml_documents(&generated(&shared()?, Target::Kubernetes, "kubernetes/environment.yaml")?);
    let mut claims = manifest_names(&docs, "PersistentVolumeClaim");
    claims.sort();
    assert_eq!(claims, ["shared", "writer-data"], "one claim, not one per machine (and the writer's data disk)");
    for machine in ["writer", "reader"] {
        let pod = manifest(&docs, "Deployment", machine);
        assert!(yaml_values(pod, "claimName").contains(&"shared".to_owned()), "{machine} mounts the claim");
    }
    // The reader runs on the writer's node: the claim is the writer's disk.
    let affinity = "spec.template.spec.affinity.podAffinity.requiredDuringSchedulingIgnoredDuringExecution.0";
    let reader = manifest(&docs, "Deployment", "reader");
    assert_eq!(
        yaml_str(reader, &format!("{affinity}.labelSelector.matchLabels.`isoloom.com/machine`")),
        "writer"
    );
    assert_eq!(yaml_str(reader, &format!("{affinity}.topologyKey")), "kubernetes.io/hostname");
    Ok(())
}

#[test]
fn on_vms_the_owner_serves_it_and_the_reader_mounts_it_after() -> Outcome {
    let vf = vagrantfile(&shared()?)?;
    assert_eq!(defined(&vf)[..2], ["writer", "reader"], "the owner starts first");
    let serves = define(&vf, "writer").every_statement().join("\n");
    assert!(
        serves.contains("nfs-kernel-server") && serves.contains("'10.84.0.11(rw,sync,no_subtree_check,no_root_squash)'"),
        "{serves}"
    );
    let mounts = define(&vf, "reader").every_statement().join("\n");
    assert!(mounts.contains("'10.84.0.10:/srv/shared /data/shared nfs defaults,_netdev 0 0'"), "{mounts}");
    // The clouds too: the reader's set-up installs the NFS client.
    let aws = generated(&shared()?, Target::CloudVm, "cloud-vm/aws/main.tf")?;
    assert!(set_up(&aws, "reader")?.contains("nfs-common"), "the clouds too");
    Ok(())
}

#[test]
fn shared_volumes_are_validated() {
    let spec = parse(
        "version: 1
name: bad
networks: { a: { cidr: 10.84.0.0/24 }, b: { cidr: 10.85.0.0/24 } }
volumes: { lonely: {}, data: { owner: ghost }, apart: {} }
machines:
  one:
    os: alpine
    networks: { a: 10 }
    volumes: { data: /d, apart: /x }
  two:
    os: alpine
    networks: { b: 10 }
    volumes: { apart: /y }
",
    )
    .unwrap();
    let problems: Vec<String> = Lab::new(spec).problems().iter().map(isoloom_spec::syntax::Problem::line).collect();
    let has = |at: &str, what: &str| problems.iter().any(|p| p.starts_with(at) && p.contains(what));
    assert!(has("volumes.lonely", "no machine mounts it"), "{problems:#?}");
    assert!(has("volumes.data.owner", "`ghost` doesn't mount it"), "{problems:#?}");
    assert!(has("machines.two.volumes.apart", "shares no network with `one`"), "{problems:#?}");
}
