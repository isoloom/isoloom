//! The controller: the environment's playbooks run from it, its size, its halt, and the inventory
//! it writes.

use isoloom_spec::{Target, parse};
use isoloom_targets::Lab;

use crate::testing::{Outcome, example, from, generated, kali_as_debian, lab, problems, source};

#[test]
fn cloud_vm_gives_a_machine_on_several_networks_an_interface_on_each() -> Outcome {
    let mut spec = source("pivot-dmz")?;
    // Kali has no AWS image yet: the user lands on Debian here.
    kali_as_debian(&mut spec)?;
    let tf = generated(&Lab::new(spec), Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    assert!(tf.contains("resource \"aws_network_interface\" \"gateway_dmz\""), "{tf}");
    assert!(tf.contains("resource \"aws_network_interface\" \"gateway_internal\""));
    assert!(tf.contains("source_dest_check = false"));
    assert!(tf.contains("resource \"aws_network_interface_attachment\" \"gateway_internal\""));
    assert!(tf.contains("resource \"aws_eip\" \"gateway\""));
    // The set-up finds the extra interface by its MAC address: interpolated, not a literal.
    assert!(tf.contains("\\\"${lower(aws_network_interface.gateway_internal.mac_address)}\\\""), "{tf}");
    assert!(!tf.contains("$${lower("));
    Ok(())
}

#[test]
fn cloud_vm_runs_the_environments_playbooks_from_a_controller() -> Outcome {
    let spec = example("ansible-pair")?;
    let tf = generated(&spec, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    // A key of its own, authorized on every machine; the controller at the controller address.
    assert!(tf.contains("resource \"tls_private_key\" \"controller\""));
    assert!(tf.contains("${trimspace(tls_private_key.controller.public_key_openssh)}"), "{tf}");
    assert!(tf.contains("private_ips       = [\"10.63.0.253\"]"));
    // After every machine is set up, it runs the playbook with the groups and vars.
    assert!(tf.contains("depends_on = [terraform_data.web, terraform_data.cache]") || tf.contains("depends_on = [terraform_data.cache, terraform_data.web]"));
    assert!(tf.contains("ansible-playbook -i /etc/isoloom/inventory.ini -i /opt/isoloom/ansible/groups.ini"));
    assert!(tf.contains("[webservers]"));
    Ok(())
}

#[test]
fn playbook_vars_keep_their_types() -> Outcome {
    let spec = example("ansible-pair")?;
    let vars = &spec.source().provision[0].vars;
    assert_eq!(vars["tools"], serde_json::json!(["curl", "jq"]));
    assert_eq!(vars["cache_port"], serde_json::json!(6379));
    let vagrantfile = generated(&spec, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(
        vagrantfile.contains(r#"-e '{"greeting":"hello from ansible","tools":["curl","jq"],"cache_port":6379}'"#),
        "{vagrantfile}"
    );
    Ok(())
}

#[test]
fn the_controller_is_small_by_default_and_sized_by_the_spec() -> Outcome {
    // ansible-pair asks for 768 MB.
    let mut src = source("ansible-pair")?;
    let spec = Lab::new(src.clone());
    let vagrantfile = generated(&spec, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    let ctl = from(&vagrantfile, "config.vm.define \"isoloom-controller\"")?;
    assert!(
        ctl.contains("m.vm.box = RbConfig::CONFIG[\"host_cpu\"] =~ /arm|aarch64/ ? \"bento/debian-12\" : \"generic/alpine319\""),
        "{ctl}"
    );
    assert!(ctl.contains("o.vm.box = \"generic/alpine319\""), "libvirt has Alpine on both architectures");
    assert!(ctl.contains("v.memory = 768"));
    assert!(ctl.contains("command -v apk"));
    // Without `controller:`: 1 CPU, 512 MB, on every target with a controller.
    src.controller = None;
    let small_lab = Lab::new(src.clone());
    let small = generated(&small_lab, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    let small_ctl = from(&small, "config.vm.define \"isoloom-controller\"")?;
    assert!(small_ctl.contains("v.cpus = 1\n      v.memory = 512"), "{small_ctl}");
    assert!(small_ctl.contains("v.guest_memsize = 512"));
    let aws = generated(&small_lab, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    assert!(
        aws.contains("instance_type = \"t3.micro\"\n  key_name      = aws_key_pair.env.key_name\n  user_data"),
        "{aws}"
    );
    let proxmox = generated(&small_lab, Target::Proxmox, ".isoloom/proxmox/main.tf")?;
    assert!(proxmox.contains("dedicated = 512"));
    // A box of one's own, pinned.
    let other = parse(
        "version: 1\nname: t\nnetworks: {}\nmachines: {}\ncontroller: { resources: { cpus: 2 } }\noverrides: { vagrant: { controller: { box: bento/debian-12, box_version: \"202407.22.0\" } } }\n",
    )
    .unwrap();
    src.controller = other.controller;
    src.overrides = other.overrides;
    let pinned = generated(&Lab::new(src), Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(pinned.contains("m.vm.box = \"bento/debian-12\"\n    m.vm.box_version = \"202407.22.0\""));
    assert!(pinned.contains("v.cpus = 2"));
    Ok(())
}

#[test]
fn the_controller_halts_after_provisioning_unless_kept_running() -> Outcome {
    let mut src = source("ansible-pair")?;
    let vagrantfile = generated(&Lab::new(src.clone()), Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    let ctl = from(&vagrantfile, "config.vm.define \"isoloom-controller\"")?;
    // After the playbooks, before the on-demand checks; marked so a check run waits it out.
    let (ansible, halt) = (ctl.find("name: \"ansible\"").unwrap(), ctl.find("name: \"halt\"").unwrap());
    assert!(ansible < halt, "{ctl}");
    assert!(ctl.contains("touch /run/isoloom-halting; (sleep 5; poweroff)"));
    src.controller.get_or_insert_default().keep_running = true;
    let kept = generated(&Lab::new(src), Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(!kept.contains("name: \"halt\""));
    Ok(())
}

#[test]
fn the_resolved_snapshot_tells_infra_from_targets() -> Outcome {
    let spec = example("ansible-pair")?;
    let resolved = isoloom_targets::resolved::resolve(&spec);
    assert_eq!(resolved["machines"]["web"]["role"], "target");
    assert_eq!(resolved["controller"]["role"], "infra");
    assert_eq!(resolved["controller"]["keep_running"], false);
    Ok(())
}

#[test]
fn the_provision_script_can_be_limited_to_machines() -> Outcome {
    let spec = example("ansible-pair")?;
    let all = isoloom_render::controller::provision_script(&spec.spec(), &[]);
    // (The retry of unreachable hosts limits itself, from the retry file: only the runs count.)
    let runs = |s: &str| s.lines().filter(|l| l.starts_with("run_play ")).map(String::from).collect::<Vec<_>>();
    assert!(runs(&all).iter().all(|l| !l.contains("--limit")), "{all}");
    let some = isoloom_render::controller::provision_script(&spec.spec(), &["web".into(), "cache".into()]);
    assert!(runs(&some).iter().all(|l| l.contains(" --limit 'web,cache' ")), "{some}");
    Ok(())
}

/// The controller's playbook runs keep Ansible's retry files where `run_play` reads them, so a
/// play whose only failures are unreachable hosts runs again on those hosts (`--limit @…`).
#[test]
fn the_controller_retries_unreachable_hosts_from_the_retry_file() -> Outcome {
    let spec = example("ansible-pair")?;
    for (target, path) in [
        (Target::Vagrant, ".isoloom/vagrant/Vagrantfile"),
        (Target::Proxmox, ".isoloom/proxmox/main.tf"),
        (Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf"),
    ] {
        let text = generated(&spec, target, path)?;
        assert!(
            text.contains("export ANSIBLE_RETRY_FILES_ENABLED=True ANSIBLE_RETRY_FILES_SAVE_PATH=/tmp/isoloom-retry"),
            "{path}"
        );
        assert!(text.contains("--limit @$_isoloom_retry"), "{path}");
        assert!(text.contains("run_play ansible-playbook -i /etc/isoloom/inventory.ini"), "{path}");
    }
    Ok(())
}

#[test]
fn host_vars_go_on_each_machines_inventory_line_with_their_types() -> Outcome {
    let spec = example("ansible-pair")?;
    let pair = generated(&spec, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(pair.contains("cache ansible_host=10.63.0.20 maxmemory='\"64mb\"'"), "{pair}");
    assert!(pair.contains("web ansible_host=10.63.0.10\n"), "{pair}");
    // Python literals, single-quoted: Ansible reads these back with their types (checked with
    // ansible-inventory 2.x).
    let typed = lab(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: p.sh }] }\nprovision:\n  - ansible: site.yml\n    host_vars:\n      a: { code: \"10\", quote: \"it's\", n: 3, on: true, none: null, l: [x, 1], m: { k: v } }\n",
    )?;
    let vagrantfile = generated(&typed, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(
        vagrantfile.contains(r#"a ansible_host=10.9.0.10 code='"10"' quote='"it'"'"'s"' n='3' on='True' none='None' l='["x", 1]' m='{"k": "v"}'"#),
        "{vagrantfile}"
    );
    Ok(())
}

#[test]
fn host_vars_name_machines_and_variables() -> Outcome {
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: p.sh }] }\nprovision:\n  - ansible: site.yml\n    host_vars:\n      ghost: { x: 1 }\n      a: { \"bad name\": 1, 2go: 1, ok_1: 1 }\n",
    )?;
    let found = problems(&spec);
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(
        found
            .iter()
            .any(|p| p.contains("provision[0].host_vars.ghost") && p.contains("no machine named `ghost`"))
    );
    assert!(found.iter().any(|p| p.contains("host_vars.a.bad name")));
    assert!(found.iter().any(|p| p.contains("host_vars.a.2go")));
    Ok(())
}
