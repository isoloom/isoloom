//! The `reboot` step of a Linux machine's `app.steps`: the machine restarts and the steps after
//! it run once it's back, on every VM target, each its own way.

use isoloom_spec::Target;
use isoloom_targets::generate;
use isoloom_test_support::hcl::{Expression, Object, ObjectKey};
use isoloom_test_support::{hcl_body, hcl_nested, hcl_one};

use crate::testing::{Outcome, at, define, generated, lab, provisioners, set_up, vagrantfile, valid};

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

/// A key of an HCL object as text.
fn key_text(key: &ObjectKey) -> String {
    match key {
        ObjectKey::Identifier(id) => id.to_string(),
        ObjectKey::Expression(Expression::String(s)) => s.clone(),
        other => format!("{other:?}"),
    }
}

/// The first object written in `expr`, a call's arguments searched (`format("…", yamlencode({…}))`).
fn object_in(expr: &Expression) -> Option<&Object<ObjectKey, Expression>> {
    match expr {
        Expression::Object(o) => Some(o),
        Expression::FuncCall(call) => call.args.iter().find_map(object_in),
        _ => None,
    }
}

/// Field `key` of `object`.
fn field<'o>(object: &'o Object<ObjectKey, Expression>, key: &str) -> Option<&'o Expression> {
    object.iter().find(|(k, _)| key_text(k) == key).map(|(_, v)| v)
}

/// VM `vm`'s cloud-config on Proxmox: the object its snippet file's `source_raw.data`
/// YAML-encodes.
fn cloud_config(tf: &str, vm: &str) -> Outcome<Object<ObjectKey, Expression>> {
    let body = hcl_body(tf);
    let file = hcl_one(&body, "proxmox_virtual_environment_file", vm);
    let raw = hcl_nested(file, "source_raw");
    let data = raw
        .first()
        .and_then(|r| isoloom_test_support::hcl_attribute(r.body(), "data"))
        .ok_or("no source_raw.data")?;
    Ok(object_in(data).ok_or("no object in the cloud-config")?.clone())
}

/// The strings of the list `key` of `object`.
fn strings(object: &Object<ObjectKey, Expression>, key: &str) -> Vec<String> {
    match field(object, key) {
        Some(Expression::Array(items)) => items
            .iter()
            .filter_map(|i| match i {
                Expression::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The content of each file the cloud-config writes (`write_files`, Isoloom's own after the
/// project's), by path.
fn written(object: &Object<ObjectKey, Expression>) -> Vec<(String, String)> {
    fn objects(e: &Expression, out: &mut Vec<Object<ObjectKey, Expression>>) {
        match e {
            Expression::Object(o) => out.push(o.clone()),
            Expression::Array(items) => items.iter().for_each(|i| objects(i, out)),
            Expression::FuncCall(call) => call.args.iter().for_each(|a| objects(a, out)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    if let Some(files) = field(object, "write_files") {
        objects(files, &mut found);
    }
    let text = |o: &Object<ObjectKey, Expression>, k: &str| match field(o, k) {
        Some(Expression::String(s)) => Some(s.clone()),
        _ => None,
    };
    found.iter().filter_map(|o| Some((text(o, "path")?, text(o, "content")?))).collect()
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
    let dir = tempfile::Builder::new().prefix("isoloom-reboot-").tempdir()?;
    let dir = dir.path();
    for f in ["kernel.sh", "middle.sh", "last.sh"] {
        std::fs::write(dir.join(f), "true\n")?;
    }
    let problems = spec.problems_with_files(dir);
    assert_eq!(problems, vec![]);
    Ok(())
}

#[test]
fn vagrant_restarts_the_vm_between_the_steps() -> Outcome {
    let vf = vagrantfile(&valid(SPEC)?)?;
    let vm = define(&vf, "box");
    let steps = provisioners(vm);
    let order: Vec<&str> = steps
        .iter()
        .map(String::as_str)
        .filter(|s| ["kernel.sh", "reboot", "middle.sh", "last.sh"].contains(s))
        .collect();
    assert_eq!(order, ["kernel.sh", "reboot", "middle.sh", "reboot", "last.sh"], "{steps:?}");
    let reboots = vm.statements("m.vm.provision \"shell\", name: \"reboot\"");
    isoloom_test_support::assert_every!(reboots, |s: &&str| *s == "m.vm.provision \"shell\", name: \"reboot\", reboot: true");
    Ok(())
}

#[test]
fn proxmox_runs_what_follows_a_reboot_at_the_next_boot() -> Outcome {
    let config = cloud_config(&file(Target::Proxmox, "proxmox/main.tf")?, "box")?;
    // cloud-init runs the steps before the first reboot, then restarts the machine.
    let runcmd = strings(&config, "runcmd");
    let has = |text: &str| runcmd.iter().any(|c| c.contains(text));
    assert!(has("kernel.sh"), "{runcmd:?}");
    assert!(
        !has("middle.sh") && !has("last.sh") && !has("echo ready > /var/lib/isoloom/ready"),
        "{runcmd:?}"
    );
    // Each step says it runs in the ready file, and a failing one ends the set-up there.
    let kernel = runcmd.iter().find(|c| c.contains("kernel.sh")).ok_or("no kernel.sh step")?;
    assert!(kernel.starts_with("echo 'running: kernel.sh' > /var/lib/isoloom/ready; "), "{kernel}");
    assert!(
        kernel.ends_with("|| { echo 'failed: kernel.sh' > /var/lib/isoloom/ready; exit 1; }"),
        "{kernel}"
    );
    assert_eq!(
        runcmd.last().map(String::as_str),
        Some("mkdir -p /var/lib/isoloom && echo 1 > /var/lib/isoloom/next-boot && systemctl enable isoloom-steps.service")
    );
    let power = field(&config, "power_state").and_then(object_in).ok_or("no power_state")?;
    assert!(matches!(field(power, "mode"), Some(Expression::String(m)) if m == "reboot"), "{power:?}");
    // The unit runs each part at the following boots: the first ends with the next reboot, the
    // last with the ready marker and the unit's end.
    let files = written(&config);
    let content = |path: &str| {
        files
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, c)| c.as_str())
            .ok_or_else(|| format!("no {path} among {:?}", files.iter().map(|(p, _)| p).collect::<Vec<_>>()))
    };
    content("/etc/systemd/system/isoloom-steps.service")?;
    let boot1 = content("/var/lib/isoloom/boot-1.sh")?;
    assert!(boot1.contains("middle.sh"), "{boot1}");
    assert!(
        boot1.ends_with("\necho 2 > /var/lib/isoloom/next-boot\nsystemctl --no-block reboot\n"),
        "{boot1}"
    );
    let boot2 = content("/var/lib/isoloom/boot-2.sh")?;
    assert!(boot2.contains("last.sh") && boot2.contains("failed: last.sh"), "{boot2}");
    assert!(
        at(boot2, "echo ready > /var/lib/isoloom/ready")? < at(boot2, "systemctl disable isoloom-steps.service")?,
        "{boot2}"
    );
    // Inputs are kept out of /tmp (emptied at boot).
    assert!(boot2.contains(". /var/lib/isoloom/inputs.env"), "{boot2}");
    Ok(())
}

#[test]
fn proxmox_without_a_reboot_is_unchanged() -> Outcome {
    let spec = lab(&SPEC.replace("reboot, ", ""))?;
    let config = cloud_config(&generated(&spec, Target::Proxmox, "proxmox/main.tf")?, "box")?;
    assert!(field(&config, "power_state").is_none(), "{config:?}");
    let files = written(&config);
    assert!(!files.iter().any(|(p, _)| p.contains("isoloom-steps")), "{files:?}");
    assert!(!strings(&config, "runcmd").iter().any(|c| c.contains("isoloom-steps")));
    Ok(())
}

#[test]
fn the_clouds_resume_the_set_up_after_each_reboot() -> Outcome {
    for cloud in ["aws", "azure"] {
        let module = file(Target::CloudVm, &format!("cloud-vm/{cloud}/main.tf"))?;
        // The machine sets itself up from its user data: no Terraform connection to it.
        let body = hcl_body(&module);
        assert!(
            isoloom_test_support::hcl_blocks(&body, "resource", &["terraform_data", "box"]).is_empty(),
            "{cloud}"
        );
        let tf = set_up(&module, "box")?;
        assert!(tf.contains("isoloom-setup.service"), "{cloud}");
        // Two restarts, each the end of a part its unit runs after the next boot.
        assert_eq!(tf.matches("systemctl reboot").count(), 2, "{cloud}");
        assert_eq!(tf.matches("if [ \"$part\" = ").count(), 3, "{cloud}");
        // The first part brings its second interface up; every later boot (a restart between
        // parts, or any restart once the set-up is ready) brings it up again before the parts.
        // The expiry is scheduled again at every boot, before anything else.
        assert_eq!(tf.matches("sudo ip addr add 10.70.1.10/24").count(), 2, "{cloud}");
        let again = at(&tf, "ready ] || [ \"$part\" != 1 ]; then")?;
        assert!(again < at(&tf, "sudo ip addr add 10.70.1.10/24")?, "{cloud}");
        assert!(at(&tf, "shutdown -h")? < at(&tf, "part=$(cat")?, "{cloud}");
        assert!(!tf.contains("systemctl disable isoloom-setup.service"), "{cloud}");
        // Steps in order, each after its restart; the ready marker last.
        let reboots: Vec<usize> = tf.match_indices("systemctl reboot").map(|(i, _)| i).collect();
        let kernel = at(&tf, "kernel.sh")?;
        assert!(kernel < reboots[0] && reboots[0] < at(&tf, "middle.sh")? && at(&tf, "middle.sh")? < reboots[1]);
        assert!(reboots[1] < at(&tf, "last.sh")? && at(&tf, "last.sh")? < at(&tf, "echo ready > /var/lib/isoloom/ready\nrm -f")?);
    }
    Ok(())
}

#[test]
fn every_cloud_driver_splits_the_set_up_at_a_reboot() -> Outcome {
    // One machine on one network: every cloud takes it, DigitalOcean included.
    let spec = lab(
        "version: 1\nname: solo\nnetworks: { lab: { cidr: 10.71.0.0/24 } }\nmachines:\n  box:\n    os: debian@12\n    networks: { lab: 10 }\n    app: { steps: [{ run: kernel.sh }, reboot, { run: last.sh }] }\n",
    )?;
    let files = generate(&spec, Target::CloudVm)?;
    for cloud in ["aws", "azure", "gcp", "linode", "oci", "digitalocean"] {
        let module = crate::testing::file(&files, &format!("cloud-vm/{cloud}/main.tf"))?;
        let tf = &set_up(module, "box")?;
        assert_eq!(tf.matches("systemctl reboot").count(), 1, "{cloud}");
        let reboot = at(tf, "systemctl reboot")?;
        assert!(at(tf, "kernel.sh")? < reboot && reboot < at(tf, "last.sh")?, "{cloud}");
        assert!(at(tf, "last.sh")? < at(tf, "echo ready > /var/lib/isoloom/ready\nrm -f")?, "{cloud}");
    }
    Ok(())
}

#[test]
fn a_cloud_docker_host_schedules_its_expiry_again_at_every_boot() -> Outcome {
    let spec =
        lab("version: 1\nname: d\nnetworks: { lab: { cidr: 10.72.0.0/24 } }\nmachines:\n  web: { networks: { lab: 10 }, app: { image: \"nginx:1.27\" } }\n")?;
    for cloud in ["aws", "azure", "gcp", "digitalocean", "linode", "oci"] {
        let tf = generated(&spec, Target::CloudDocker, &format!("cloud-docker/{cloud}/main.tf"))?;
        // Cloud-init runs the user data once: the set-up's unit, enabled for every boot, schedules
        // the expiry again before anything else.
        let host = set_up(&tf, "env")?;
        assert!(host.contains("systemctl enable isoloom-setup.service"), "{cloud}: {host}");
        assert!(host.contains("shutdown -h"), "{cloud}: {host}");
    }
    Ok(())
}
