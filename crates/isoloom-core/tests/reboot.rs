//! The `reboot` step of a Linux machine's `vm.provision`: the machine restarts and the steps
//! after it run once it's back, on every VM target, each its own way.

use isoloom_core::{Target, generate, parse, validate};

/// A Linux machine on two networks, with an input, restarting twice between its steps.
const SPEC: &str = r#"
version: 1
name: boots
inputs: [TOKEN]
networks:
  front: { cidr: 10.70.0.0/24 }
  back: { cidr: 10.70.1.0/24 }
machines:
  box:
    networks: { front: 10, back: 10 }
    inputs: [TOKEN]
    vm:
      os: debian-12
      provision: [kernel.sh, reboot, middle.sh, reboot, last.sh]
"#;

fn file(target: Target, suffix: &str) -> String {
    let spec = parse(SPEC).unwrap();
    assert_eq!(validate(&spec), vec![]);
    generate(&spec, target)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with(suffix))
        .unwrap_or_else(|| panic!("no {suffix}"))
        .contents
}

fn at(text: &str, what: &str) -> usize {
    text.find(what).unwrap_or_else(|| panic!("`{what}` missing in:\n{text}"))
}

#[test]
fn a_windows_machine_restarts_from_the_playbooks() {
    let spec = parse(
        "version: 1\nname: w\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines:\n  dc:\n    networks: { lab: 10 }\n    vm: { os: windows-server-2022, provision: [a.ps1, reboot, b.ps1] }\n",
    )
    .unwrap();
    let problems = validate(&spec);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].at, "machines.dc.vm.provision[1]");
    assert!(problems[0].message.contains("win_reboot"), "{}", problems[0].message);
}

#[test]
fn reboot_is_a_step_not_a_file() {
    let spec = parse(SPEC).unwrap();
    let dir = std::env::temp_dir().join(format!("isoloom-reboot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["kernel.sh", "middle.sh", "last.sh"] {
        std::fs::write(dir.join(f), "true\n").unwrap();
    }
    let problems = isoloom_core::validate_files(&spec, &dir);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(problems, vec![]);
}

#[test]
fn vagrant_restarts_the_vm_between_the_steps() {
    let v = file(Target::Vagrant, "Vagrantfile");
    let reboot = "m.vm.provision \"shell\", name: \"reboot\", reboot: true\n";
    assert_eq!(v.matches(reboot).count(), 2);
    let first = at(&v, reboot);
    assert!(at(&v, "sh kernel.sh") < first);
    assert!(first < at(&v, "sh middle.sh"));
    assert!(at(&v, "sh middle.sh") < v.rfind(reboot).unwrap());
    assert!(v.rfind(reboot).unwrap() < at(&v, "sh last.sh"));
}

#[test]
fn proxmox_runs_what_follows_a_reboot_at_the_next_boot() {
    let tf = file(Target::Proxmox, "proxmox/main.tf");
    let tf = &tf[at(&tf, "resource \"proxmox_virtual_environment_file\" \"box\"")..];
    let tf = &tf[..at(tf, "\n}\n")];
    // cloud-init runs the steps before the first reboot, then restarts the machine.
    let runcmd = &tf[at(tf, "runcmd = [")..];
    let runcmd = &runcmd[..at(runcmd, "]\n      power_state")];
    assert!(runcmd.contains("sh kernel.sh"));
    assert!(!runcmd.contains("middle.sh") && !runcmd.contains("last.sh") && !runcmd.contains("/var/lib/isoloom/ready"));
    assert!(runcmd.contains("echo 1 > /var/lib/isoloom/next-boot && systemctl enable isoloom-steps.service"));
    assert!(tf.contains("power_state = { mode = \"reboot\""));
    // The unit runs each part at the following boots: the first ends with the next reboot, the
    // last with the ready marker and the unit's end.
    assert!(tf.contains("path = \"/etc/systemd/system/isoloom-steps.service\""));
    let boot1 = &tf[at(tf, "boot-1.sh\", permissions = \"0700\", content = ")..];
    let boot1 = &boot1[..at(boot1, " }")];
    assert!(boot1.contains("sh middle.sh\\necho 2 > /var/lib/isoloom/next-boot\\nsystemctl --no-block reboot"));
    let boot2 = &tf[at(tf, "boot-2.sh\", permissions = \"0700\", content = ")..];
    let boot2 = &boot2[..at(boot2, " }")];
    assert!(boot2.contains("sh last.sh\\nmkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready\\nsystemctl disable isoloom-steps.service"));
    // Inputs stay in /etc/isoloom (kept across boots).
    assert!(boot2.contains(". /etc/isoloom/inputs.env"));
}

#[test]
fn proxmox_without_a_reboot_is_unchanged() {
    let spec = parse(&SPEC.replace("reboot, ", "")).unwrap();
    let tf = generate(&spec, Target::Proxmox)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with("main.tf"))
        .unwrap()
        .contents;
    assert!(!tf.contains("power_state") && !tf.contains("isoloom-steps"));
}

#[test]
fn the_clouds_reconnect_after_each_reboot() {
    for cloud in ["aws", "azure"] {
        let tf = file(Target::CloudVm, &format!("cloud-vm/{cloud}/main.tf"));
        let box_ = &tf[at(&tf, "resource \"terraform_data\" \"box\"")..];
        let box_ = &box_[..at(box_, "\n}\n")];
        // Two restarts: each a provisioner whose dropped connection is expected.
        assert_eq!(box_.matches("on_failure = continue").count(), 2, "{cloud}");
        assert_eq!(box_.matches("sudo systemctl --no-block reboot").count(), 2, "{cloud}");
        assert!(box_.contains("\"sudo touch /run/nologin\""), "{cloud}: no new login while it goes down");
        // Each part after a restart checks the boot changed, brings its second interface up
        // again and schedules the auto-stop again.
        assert_eq!(box_.matches("the machine did not restart").count(), 2, "{cloud}");
        assert_eq!(box_.matches("sudo ip addr add 10.70.1.10/24").count(), 3, "{cloud}");
        assert_eq!(box_.matches("sudo shutdown -h +${var.auto_stop_minutes}").count(), 2, "{cloud}");
        // The inputs leave /tmp (emptied at boot) before the first restart.
        let kernel = at(box_, "sh kernel.sh");
        assert!(
            at(box_, "sudo install -m 600 /tmp/isoloom-inputs.env /var/lib/isoloom/inputs.env") < kernel,
            "{cloud}"
        );
        assert!(!box_.contains(". /tmp/isoloom-inputs.env"), "{cloud}");
        // Steps in order, each after its restart; the ready marker last.
        let reboots: Vec<usize> = box_.match_indices("on_failure = continue").map(|(i, _)| i).collect();
        assert!(kernel < reboots[0] && reboots[0] < at(box_, "sh middle.sh") && at(box_, "sh middle.sh") < reboots[1]);
        assert!(reboots[1] < at(box_, "sh last.sh") && at(box_, "sh last.sh") < at(box_, "/var/lib/isoloom/ready"));
    }
}

#[test]
fn every_cloud_driver_splits_the_set_up_at_a_reboot() {
    // One machine on one network: every cloud takes it, DigitalOcean included.
    let spec = parse(
        "version: 1\nname: solo\nnetworks: { lab: { cidr: 10.71.0.0/24 } }\nmachines:\n  box:\n    networks: { lab: 10 }\n    vm: { os: debian-12, provision: [kernel.sh, reboot, last.sh] }\n",
    )
    .unwrap();
    let files = generate(&spec, Target::CloudVm).unwrap();
    for cloud in ["aws", "azure", "gcp", "linode", "oci", "digitalocean"] {
        let tf = &files
            .iter()
            .find(|f| f.path.ends_with(&format!("cloud-vm/{cloud}/main.tf")))
            .unwrap_or_else(|| panic!("{cloud}"))
            .contents;
        assert_eq!(tf.matches("on_failure = continue").count(), 1, "{cloud}");
        let reboot = at(tf, "on_failure = continue");
        assert!(at(tf, "sh kernel.sh") < reboot && reboot < at(tf, "sh last.sh"), "{cloud}");
        assert!(at(tf, "sh last.sh") < at(tf, "/var/lib/isoloom/ready"), "{cloud}");
    }
}

#[test]
fn importing_a_vagrant_reboot_keeps_it_as_a_step() {
    let recorded: serde_json::Value = serde_json::from_str(
        r#"{"settings":{},"children":{"vm":{"settings":{"box":"bento/debian-12"},"children":{},"calls":[
          {"name":"define","args":["web"],"block":{"settings":{},"children":{"vm":{"settings":{},"children":{},"calls":[
            {"name":"network","args":["private_network",{"ip":"192.168.40.10"}],"block":null},
            {"name":"provision","args":["shell",{"path":"kernel.sh","reboot":true}],"block":null},
            {"name":"provision","args":["shell",{"path":"web.sh"}],"block":null}]}},"calls":[]}}]}},"calls":[]}"#,
    )
    .unwrap();
    let d = isoloom_core::import::vagrant::draft(&recorded, "lab", "Vagrantfile").unwrap();
    let spec = parse(&d.yaml).expect("the draft parses");
    assert_eq!(spec.machines["web"].vm.as_ref().unwrap().provision, ["kernel.sh", "reboot", "web.sh"]);
    assert_eq!(validate(&spec), vec![], "{}", d.yaml);
}
