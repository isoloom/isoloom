//! The `reboot` step of a Linux machine's `app.steps`: the machine restarts and the steps after
//! it run once it's back, on every VM target, each its own way.

use isoloom_spec::Target;

use isoloom_targets::generate;

use crate::testing::{Outcome, at, before, from, generated, lab, valid};

/// A Linux machine on two networks, with an input, restarting twice between its steps.
const SPEC: &str = r"
version: 1
name: boots
inputs: { TOKEN: { sensitive: false } }
networks:
  front: { cidr: 10.70.0.0/24 }
  back: { cidr: 10.70.1.0/24 }
machines:
  box:
    os: debian@12
    networks: { front: 10, back: 10 }
    environment: { TOKEN: { input: TOKEN } }
    app:
      steps: [{ run: kernel.sh }, reboot, { run: middle.sh }, reboot, { run: last.sh }]
";

/// The file of [`SPEC`] on `target` whose path ends with `suffix`.
fn file(target: Target, suffix: &str) -> Outcome<String> {
    generated(&valid(SPEC)?, target, suffix)
}

#[test]
fn a_windows_machine_restarts_from_the_playbooks() -> Outcome {
    let spec = lab(
        "version: 1\nname: w\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines:\n  dc:\n    os: windows-server@2022\n    networks: { lab: 10 }\n    app: { steps: [{ run: a.ps1 }, reboot, { run: b.ps1 }] }\n",
    )?;
    let problems = spec.problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].path.as_str(), "machines.dc.app.steps[1]");
    assert!(problems[0].reason.contains("restarts from its own script"), "{}", problems[0].reason);
    Ok(())
}

#[test]
fn reboot_is_a_step_not_a_file() -> Outcome {
    let spec = lab(SPEC)?;
    let dir = std::env::temp_dir().join(format!("isoloom-reboot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["kernel.sh", "middle.sh", "last.sh"] {
        std::fs::write(dir.join(f), "true\n").unwrap();
    }
    let problems = spec.problems_with_files(&dir);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(problems, vec![]);
    Ok(())
}

#[test]
fn vagrant_restarts_the_vm_between_the_steps() -> Outcome {
    let v = file(Target::Vagrant, "Vagrantfile")?;
    let reboot = "m.vm.provision \"shell\", name: \"reboot\", reboot: true\n";
    assert_eq!(v.matches(reboot).count(), 2);
    let first = at(&v, reboot)?;
    assert!(at(&v, "sh kernel.sh")? < first);
    assert!(first < at(&v, "sh middle.sh")?);
    assert!(at(&v, "sh middle.sh")? < v.rfind(reboot).unwrap());
    assert!(v.rfind(reboot).unwrap() < at(&v, "sh last.sh")?);
    Ok(())
}

#[test]
fn proxmox_runs_what_follows_a_reboot_at_the_next_boot() -> Outcome {
    let tf = file(Target::Proxmox, "proxmox/main.tf")?;
    let tf = from(&tf, "resource \"proxmox_virtual_environment_file\" \"box\"")?;
    let tf = before(tf, "\n}\n")?;
    // cloud-init runs the steps before the first reboot, then restarts the machine.
    let runcmd = from(tf, "runcmd = [")?;
    let runcmd = before(runcmd, "]\n      power_state = {")?;
    assert!(runcmd.contains("sh kernel.sh"));
    assert!(!runcmd.contains("middle.sh") && !runcmd.contains("last.sh") && !runcmd.contains("echo ready > /var/lib/isoloom/ready"));
    // Each step says it runs in the ready file, and a failing one ends the set-up there.
    assert!(runcmd.contains("echo 'running: kernel.sh' > /var/lib/isoloom/ready; "), "{runcmd}");
    assert!(runcmd.contains("|| { echo 'failed: kernel.sh' > /var/lib/isoloom/ready; exit 1; }"), "{runcmd}");
    assert!(runcmd.contains("echo 1 > /var/lib/isoloom/next-boot && systemctl enable isoloom-steps.service"));
    assert!(tf.contains("mode      = \"reboot\""), "{tf}");
    // The unit runs each part at the following boots: the first ends with the next reboot, the
    // last with the ready marker and the unit's end.
    assert!(tf.contains("\"/etc/systemd/system/isoloom-steps.service\""));
    let boot1 = from(tf, "boot-1.sh\"")?;
    let boot1 = before(boot1, "\n        }")?;
    assert!(boot1.contains("sh middle.sh"), "{boot1}");
    assert!(
        boot1.contains("\\necho 2 > /var/lib/isoloom/next-boot\\nsystemctl --no-block reboot"),
        "{boot1}"
    );
    let boot2 = from(tf, "boot-2.sh\"")?;
    let boot2 = before(boot2, "\n        }")?;
    assert!(boot2.contains("sh last.sh"), "{boot2}");
    assert!(
        at(boot2, "echo ready > /var/lib/isoloom/ready")? < at(boot2, "systemctl disable isoloom-steps.service")?,
        "{boot2}"
    );
    assert!(boot2.contains("failed: last.sh"), "{boot2}");
    // Inputs are kept out of /tmp (emptied at boot).
    assert!(boot2.contains(". /var/lib/isoloom/inputs.env"), "{boot2}");
    Ok(())
}

#[test]
fn proxmox_without_a_reboot_is_unchanged() -> Outcome {
    let spec = lab(&SPEC.replace("reboot, ", ""))?;
    let tf = generate(&spec, Target::Proxmox)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with("main.tf"))
        .unwrap()
        .contents;
    assert!(!tf.contains("power_state") && !tf.contains("isoloom-steps"));
    Ok(())
}

#[test]
fn the_clouds_resume_the_set_up_after_each_reboot() -> Outcome {
    for cloud in ["aws", "azure"] {
        let tf = file(Target::CloudVm, &format!("cloud-vm/{cloud}/main.tf"))?;
        // The machine sets itself up from its user data: no Terraform connection to it.
        assert!(!tf.contains("resource \"terraform_data\" \"box\""), "{cloud}");
        assert!(tf.contains("isoloom-setup.service"), "{cloud}");
        // Two restarts, each the end of a part its unit runs after the next boot.
        assert_eq!(tf.matches("systemctl reboot").count(), 2, "{cloud}");
        assert_eq!(tf.matches("if [ \\\"$part\\\" = ").count(), 3, "{cloud}");
        // Each part after a restart brings its second interface up again (the first part brings
        // it up too); the expiry is scheduled again at every boot, before any part.
        assert_eq!(tf.matches("sudo ip addr add 10.70.1.10/24").count(), 3, "{cloud}");
        assert!(at(&tf, "shutdown -h")? < at(&tf, "part=$(cat")?, "{cloud}");
        // Steps in order, each after its restart; the ready marker last.
        let reboots: Vec<usize> = tf.match_indices("systemctl reboot").map(|(i, _)| i).collect();
        let kernel = at(&tf, "sh kernel.sh")?;
        assert!(kernel < reboots[0] && reboots[0] < at(&tf, "sh middle.sh")? && at(&tf, "sh middle.sh")? < reboots[1]);
        assert!(reboots[1] < at(&tf, "sh last.sh")? && at(&tf, "sh last.sh")? < at(&tf, "echo ready > /var/lib/isoloom/ready\\nrm -f")?);
    }
    Ok(())
}

#[test]
fn every_cloud_driver_splits_the_set_up_at_a_reboot() -> Outcome {
    // One machine on one network: every cloud takes it, DigitalOcean included.
    let spec = lab(
        "version: 1\nname: solo\nnetworks: { lab: { cidr: 10.71.0.0/24 } }\nmachines:\n  box:\n    os: debian@12\n    networks: { lab: 10 }\n    app: { steps: [{ run: kernel.sh }, reboot, { run: last.sh }] }\n",
    )?;
    let files = generate(&spec, Target::CloudVm).unwrap();
    for cloud in ["aws", "azure", "gcp", "linode", "oci", "digitalocean"] {
        let tf = &files
            .iter()
            .find(|f| f.path.ends_with(&format!("cloud-vm/{cloud}/main.tf")))
            .unwrap_or_else(|| panic!("{cloud}"))
            .contents;
        assert_eq!(tf.matches("systemctl reboot").count(), 1, "{cloud}");
        let reboot = at(tf, "systemctl reboot")?;
        assert!(at(tf, "sh kernel.sh")? < reboot && reboot < at(tf, "sh last.sh")?, "{cloud}");
        assert!(at(tf, "sh last.sh")? < at(tf, "echo ready > /var/lib/isoloom/ready\\nrm -f")?, "{cloud}");
    }
    Ok(())
}
