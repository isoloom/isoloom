//! Each Vagrant VM's `boot_digest` in the run manifest: a digest of what Vagrant applies only when
//! the VM boots (its synced folders, networks, forwarded ports, size, provider settings), so a
//! runner knows to reload a running VM an update changed that way. The same spec gives the same
//! digest; a host folder changes the digest of the VM that shares it, and only its; a change to a
//! VM's set-up alone (its provisioners) changes none.

use isoloom_manifest::{Manifest, Native};
use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, file, lab};

/// Two VMs, `web` with a published port.
const VMS: &str = "version: 1
name: pair
networks: { lan: { cidr: 10.32.0.0/24 } }
machines:
  web:
    os: debian@12
    networks: { lan: 10 }
    ports: { site: { port: 80, protocol: http, publish: 8080 } }
    app: { steps: [{ run: a.sh }] }
  db:
    os: debian@12
    networks: { lan: 11 }
";

/// One container, which `docker-vm` runs in its VM.
const CONTAINER: &str = "version: 1
name: one-box
networks: { lan: { cidr: 10.33.0.0/24 } }
machines:
  web:
    networks: { lan: 10 }
    ports: { site: { port: 80, protocol: http, publish: 8080 } }
    app: { image: nginx }
";

/// Each VM's name and boot digest in the run manifest `target` writes for `yaml` (at `run`).
fn digests(yaml: &str, target: Target, run: &str) -> Outcome<Vec<(String, String)>> {
    let files = generate(&lab(yaml)?, target)?;
    let m: Manifest = isoloom_manifest::parse(file(&files, run)?)?;
    let mut out = Vec::new();
    for machine in &m.machines {
        if let Some(Native::Vagrant { boot_digest, .. }) = &machine.native {
            out.push((machine.name.to_string(), boot_digest.clone().ok_or("a VM without a boot digest")?));
        }
    }
    Ok(out)
}

fn digest_of(all: &[(String, String)], name: &str) -> String {
    all.iter().find(|(n, _)| n == name).map(|(_, d)| d.clone()).unwrap_or_default()
}

#[test]
fn a_vm_runs_digest_is_stable_and_changes_with_the_vms_host_folders_only() -> Outcome {
    let run = ".isoloom/vagrant/run.json";
    let before = digests(VMS, Target::Vagrant, run)?;
    assert!(digest_of(&before, "web").starts_with("sha256:"), "{before:?}");
    assert_eq!(digests(VMS, Target::Vagrant, run)?, before, "the same spec, the same digests");
    let shared = VMS.replace(
        "    app: { steps: [{ run: a.sh }] }",
        "    app: { steps: [{ run: a.sh }] }\n    host_folders: [{ from: shared, path: /srv/shared }]",
    );
    let after = digests(&shared, Target::Vagrant, run)?;
    assert_ne!(digest_of(&after, "web"), digest_of(&before, "web"), "web shares a folder now");
    assert_eq!(digest_of(&after, "db"), digest_of(&before, "db"), "db boots as it did");
    let resized = VMS.replace("  db:\n    os: debian@12", "  db:\n    os: debian@12\n    resources: { cpus: 4 }");
    assert_ne!(digest_of(&digests(&resized, Target::Vagrant, run)?, "db"), digest_of(&before, "db"));
    Ok(())
}

#[test]
fn a_vms_set_up_alone_changes_no_digest() -> Outcome {
    let run = ".isoloom/vagrant/run.json";
    let other_steps = VMS.replace("run: a.sh", "run: b.sh");
    assert_eq!(digests(&other_steps, Target::Vagrant, run)?, digests(VMS, Target::Vagrant, run)?);
    Ok(())
}

/// Two VMs on two networks, `web` on the first, `db` on the second.
const TWO_NETWORKS: &str = "version: 1
name: pair
networks:
  front: { cidr: 10.34.0.0/24 }
  back: { cidr: 10.35.0.0/24 }
machines:
  web:
    os: debian@12
    networks: { front: 10 }
  db:
    os: debian@12
    networks: { back: 10 }
";

#[test]
fn a_network_inserted_ahead_changes_the_digest_of_each_vm_whose_port_group_moved() -> Outcome {
    // On ESXi, a VM's port groups are picked by its networks' places in the spec's order: one
    // inserted ahead of `back` moves db's port group, not web's.
    let run = ".isoloom/vagrant/run.json";
    let before = digests(TWO_NETWORKS, Target::Vagrant, run)?;
    let inserted = TWO_NETWORKS.replace("  back: {", "  middle: { cidr: 10.36.0.0/24 }\n  back: {");
    let after = digests(&inserted, Target::Vagrant, run)?;
    assert_eq!(digest_of(&after, "web"), digest_of(&before, "web"), "web's port group is where it was");
    assert_ne!(digest_of(&after, "db"), digest_of(&before, "db"), "db's port group moved");
    Ok(())
}

#[test]
fn docker_vms_vm_digest_changes_when_a_container_gets_a_host_folder() -> Outcome {
    let run = ".isoloom/docker-vm/run.json";
    let before = digests(CONTAINER, Target::DockerVm, run)?;
    assert_eq!(before.len(), 1, "{before:?}");
    assert_eq!(digests(CONTAINER, Target::DockerVm, run)?, before);
    let shared = format!("{CONTAINER}    host_folders: [{{ from: shared, path: /srv/shared }}]\n");
    let after = digests(&shared, Target::DockerVm, run)?;
    assert_eq!(after[0].0, before[0].0);
    assert_ne!(after[0].1, before[0].1, "the VM shares the container's folder now");
    Ok(())
}
