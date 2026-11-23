//! The controller: the environment's playbooks run from it, its size, its halt, and the inventory
//! it writes.

use isoloom_spec::{Target, parse};
use isoloom_targets::Lab;

use isoloom_test_support::{hcl_body, hcl_literals, hcl_names, hcl_one, hcl_text, hcl_values};

use crate::testing::{Outcome, define, example, generated, lab, problems, provisioners, set_up, source, vagrantfile};

/// The statements of `block` and the blocks inside it equal to `statement`.
fn count(block: &isoloom_test_support::RubyBlock, statement: &str) -> usize {
    block.every_statement().iter().filter(|s| **s == statement).count()
}

#[test]
fn cloud_vm_gives_a_machine_on_several_networks_an_interface_on_each() -> Outcome {
    let tf = generated(&Lab::new(source("dmz-jump")?), Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    let body = hcl_body(&tf);
    let nics = hcl_names(&body, "resource", "aws_network_interface");
    assert!(
        nics.contains(&"gateway_dmz".to_owned()) && nics.contains(&"gateway_internal".to_owned()),
        "{nics:?}"
    );
    // It forwards between them: no source/destination check on either.
    for nic in ["gateway_dmz", "gateway_internal"] {
        assert_eq!(hcl_text(hcl_one(&body, "aws_network_interface", nic), "source_dest_check"), "false", "{nic}");
    }
    assert_eq!(hcl_names(&body, "resource", "aws_network_interface_attachment"), ["gateway_internal"]);
    assert!(hcl_names(&body, "resource", "aws_eip").contains(&"gateway".to_owned()));
    // The set-up finds the extra interface by its MAC address: interpolated, not a literal.
    let scripts: Vec<String> = hcl_literals(&body).into_iter().filter(|l| l.contains('\n')).collect();
    assert!(
        scripts
            .iter()
            .any(|s| s.contains("${lower(aws_network_interface.gateway_internal.mac_address)}")),
        "{tf}"
    );
    assert!(!scripts.iter().any(|s| s.contains("$${lower(")), "{tf}");
    Ok(())
}

#[test]
fn cloud_vm_runs_the_environments_playbooks_from_a_controller() -> Outcome {
    let spec = example("ansible-pair")?;
    let tf = generated(&spec, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    // A key of its own, authorized on every machine; the controller at the controller address.
    let body = hcl_body(&tf);
    assert_eq!(hcl_names(&body, "resource", "tls_private_key"), ["controller"]);
    let literals = hcl_literals(&body);
    assert!(
        literals
            .iter()
            .any(|l| l.contains("${trimspace(tls_private_key.controller.public_key_openssh)}")),
        "{tf}"
    );
    assert!(
        hcl_values(&body, "private_ips").contains(&"10.63.0.253".to_owned()),
        "the controller at the controller address"
    );
    // Once every machine's own set-up is done (its ready file, over SSH with the controller's
    // key), it runs the playbook with the groups and vars.
    let script = set_up(&tf, "isoloom_controller")?;
    let (web, cache) = (
        script.find("running: waiting for web").ok_or("web")?,
        script.find("running: waiting for cache").ok_or("cache")?,
    );
    let playbook = script.find("ansible-playbook -i /etc/isoloom/inventory.ini -i ").ok_or("playbook")?;
    assert!(web < playbook && cache < playbook, "{script}");
    // The inventory it writes (base64-encoded by Terraform) has the spec's group; the project's
    // own groups file goes beside it.
    let inventory = script
        .split("base64encode(\"")
        .nth(1)
        .and_then(|rest| rest.split("\")}").next())
        .ok_or("no inventory")?
        .replace("\\n", "\n");
    assert!(inventory.lines().any(|l| l == "[webservers]"), "{inventory}");
    assert!(script.contains("-i '\\''/opt/isoloom/ansible/groups.ini'\\''"), "{script}");
    Ok(())
}

#[test]
fn playbook_vars_keep_their_types() -> Outcome {
    let spec = example("ansible-pair")?;
    let vars = &spec.source().provision[0].vars;
    assert_eq!(vars["tools"], serde_json::json!(["curl", "jq"]));
    assert_eq!(vars["cache_port"], serde_json::json!(6379));
    let vf = vagrantfile(&spec)?;
    let ansible = define(&vf, "isoloom-controller").statements("m.vm.provision \"shell\", name: \"ansible\"");
    assert!(
        matches!(ansible.as_slice(), [one] if one.contains(r#"-e '{"greeting":"hello from ansible","tools":["curl","jq"],"cache_port":6379}'"#)),
        "{ansible:?}"
    );
    Ok(())
}

#[test]
fn the_controller_is_small_by_default_and_sized_by_the_spec() -> Outcome {
    // ansible-pair asks for 768 MB.
    let mut src = source("ansible-pair")?;
    let spec = Lab::new(src.clone());
    let vf = vagrantfile(&spec)?;
    let ctl = define(&vf, "isoloom-controller");
    assert_eq!(
        ctl.statements("m.vm.box = "),
        ["m.vm.box = RbConfig::CONFIG[\"host_cpu\"] =~ /arm|aarch64/ ? \"bento/debian-12\" : \"generic/alpine319\""]
    );
    let libvirt = ctl.block("m.vm.provider \"libvirt\"");
    assert_eq!(
        libvirt.statements("o.vm.box = "),
        ["o.vm.box = \"generic/alpine319\""],
        "libvirt has Alpine on both architectures"
    );
    assert!(count(ctl, "v.memory = 768") > 0, "{ctl:?}");
    assert!(ctl.every_statement().iter().any(|s| s.contains("command -v apk")), "{ctl:?}");
    // Without `controller:`: 1 CPU, 512 MB, on every target with a controller.
    src.controller = None;
    let small_lab = Lab::new(src.clone());
    let small = vagrantfile(&small_lab)?;
    let small_ctl = define(&small, "isoloom-controller");
    let virtualbox = small_ctl.block("m.vm.provider \"virtualbox\"");
    assert_eq!(
        (virtualbox.statements("v.cpus = "), virtualbox.statements("v.memory = ")),
        (vec!["v.cpus = 1"], vec!["v.memory = 512"])
    );
    assert_eq!(count(small_ctl, "v.guest_memsize = 512"), 1, "{small_ctl:?}");
    let aws = hcl_body(&generated(&small_lab, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?);
    assert_eq!(hcl_text(hcl_one(&aws, "aws_instance", "isoloom_controller"), "instance_type"), "t3.micro");
    let proxmox = hcl_body(&generated(&small_lab, Target::Proxmox, ".isoloom/proxmox/main.tf")?);
    assert_eq!(
        hcl_values(hcl_one(&proxmox, "proxmox_virtual_environment_vm", "isoloom_controller"), "dedicated"),
        ["512"]
    );
    // A box of one's own, pinned.
    let other = parse(
        "version: 1\nname: t\nnetworks: {}\nmachines: {}\ncontroller: { resources: { cpus: 2 } }\noverrides: { vagrant: { controller: { box: bento/debian-12, box_version: \"202407.22.0\" } } }\n",
    )
    .unwrap();
    src.controller = other.controller;
    src.overrides = other.overrides;
    let pinned = vagrantfile(&Lab::new(src))?;
    let pinned_ctl = define(&pinned, "isoloom-controller");
    assert_eq!(
        pinned_ctl.statements("m.vm.box"),
        ["m.vm.box = \"bento/debian-12\"", "m.vm.box_version = \"202407.22.0\""]
    );
    assert!(count(pinned_ctl, "v.cpus = 2") > 0, "{pinned_ctl:?}");
    Ok(())
}

#[test]
fn the_controller_halts_after_provisioning_unless_kept_running() -> Outcome {
    let mut src = source("ansible-pair")?;
    let file = vagrantfile(&Lab::new(src.clone()))?;
    // After the playbooks, the last step; marked so a check run waits it out.
    let steps = provisioners(define(&file, "isoloom-controller"));
    let at = |name: &str| steps.iter().position(|s| s == name).ok_or_else(|| format!("no `{name}` step in {steps:?}"));
    assert!(at("ansible")? < at("halt")?, "{steps:?}");
    assert_eq!(steps.last().map(String::as_str), Some("halt"), "{steps:?}");
    let halt_steps = define(&file, "isoloom-controller").statements("m.vm.provision \"shell\", name: \"halt\"");
    assert!(
        matches!(halt_steps.as_slice(), [one] if one.contains("touch /run/isoloom-halting; (sleep 5; poweroff)")),
        "{halt_steps:?}"
    );
    src.controller.get_or_insert_default().keep_running = true;
    let kept = vagrantfile(&Lab::new(src))?;
    assert!(!provisioners(define(&kept, "isoloom-controller")).contains(&"halt".to_owned()));
    Ok(())
}

#[test]
fn the_resolved_snapshot_tells_infra_from_members() -> Outcome {
    let spec = example("ansible-pair")?;
    let resolved = isoloom_targets::resolved::resolve(&spec);
    assert_eq!(resolved["machines"]["web"]["role"], "member");
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
    isoloom_test_support::assert_every!(runs(&all), |l: &String| !l.contains("--limit"));
    let some = isoloom_render::controller::provision_script(&spec.spec(), &["web".into(), "cache".into()]);
    isoloom_test_support::assert_every!(runs(&some), |l: &String| l.contains(" --limit 'web,cache' "));
    Ok(())
}

/// The controller's playbook runs keep Ansible's retry files where `run_play` reads them, so a
/// play whose only failures are unreachable hosts runs again on those hosts (`--limit @…`).
#[test]
fn the_controller_retries_unreachable_hosts_from_the_retry_file() -> Outcome {
    let spec = example("ansible-pair")?;
    let vf = vagrantfile(&spec)?;
    let vagrant = define(&vf, "isoloom-controller")
        .statements("m.vm.provision \"shell\", name: \"ansible\"")
        .concat();
    // Proxmox's controller sets up from the snippets of its cloud-init file.
    let proxmox_tf = hcl_body(&generated(&spec, Target::Proxmox, ".isoloom/proxmox/main.tf")?);
    let proxmox = hcl_literals(hcl_one(&proxmox_tf, "proxmox_virtual_environment_file", "isoloom_controller")).join("\n");
    let aws = set_up(&generated(&spec, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?, "isoloom_controller")?;
    for (path, text) in [("vagrant", vagrant), ("proxmox", proxmox), ("cloud-vm", aws)] {
        assert!(
            text.contains("export ANSIBLE_RETRY_FILES_ENABLED=True ANSIBLE_RETRY_FILES_SAVE_PATH=/tmp/isoloom-retry"),
            "{path}"
        );
        // Added to the command's arguments once (HCL escapes its quotes).
        let limit = "set -- \"$@\" --limit \"@$_isoloom_retry\"";
        assert!(text.replace("\\\"", "\"").contains(limit), "{path}");
        assert!(text.contains("run_play ansible-playbook -i /etc/isoloom/inventory.ini"), "{path}");
    }
    Ok(())
}

#[test]
fn host_vars_go_on_each_machines_inventory_line_with_their_types() -> Outcome {
    let spec = example("ansible-pair")?;
    let pair = vagrantfile(&spec)?;
    let inventory: Vec<String> = define(&pair, "isoloom-controller")
        .every_statement()
        .iter()
        .flat_map(|s| s.lines().map(|l| l.trim().to_owned()).collect::<Vec<_>>())
        .collect();
    assert!(
        inventory.iter().any(|l| l == "cache ansible_host=10.63.0.20 maxmemory='\"64mb\"'"),
        "{inventory:?}"
    );
    assert!(inventory.iter().any(|l| l == "web ansible_host=10.63.0.10"), "{inventory:?}");
    // Python literals, single-quoted: Ansible reads these back with their types (checked with
    // ansible-inventory 2.x).
    let typed = lab(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: p.sh }] }\nprovision:\n  - ansible: site.yml\n    host_vars:\n      a: { code: \"10\", quote: \"it's\", n: 3, on: true, none: null, l: [x, 1], m: { k: v } }\n",
    )?;
    let typed_vf = vagrantfile(&typed)?;
    let typed_inventory = define(&typed_vf, "isoloom-controller").every_statement().join("\n");
    assert!(
        typed_inventory
            .lines()
            .any(|l| l.trim() == r#"a ansible_host=10.9.0.10 code='"10"' quote='"it'"'"'s"' n='3' on='True' none='None' l='["x", 1]' m='{"k": "v"}'"#),
        "{typed_inventory}"
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
