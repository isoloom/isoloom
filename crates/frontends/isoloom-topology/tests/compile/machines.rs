//! Machines: what each kind of node becomes, its steps and its ports.

use isoloom_spec::syntax::Recipe;
use isoloom_topology::{Error, compile};

use crate::testing::{Outcome, accepted, container, files, spec, three_node, vm, yaml};

#[test]
fn a_container_is_an_app_from_its_image_and_a_vm_its_os_and_steps() -> Outcome {
    let c = accepted(&three_node()?)?;
    let client = &spec(&c).machines["client"];
    assert!(client.access);
    assert_eq!(client.os.map(|os| os.to_string()).as_deref(), Some("debian@12"));
    assert_eq!(client.app().map(|app| app.steps.len()), Some(1));
    let s = spec(&c);
    let router = s.machines["router"].app();
    assert_eq!(router.and_then(|r| r.container.clone()), Some(Recipe::Image("frrouting/frr:v8.5.2".into())));
    assert!(spec(&c).machines["router"].os.is_none());
    Ok(())
}

#[test]
fn the_yaml_is_the_new_format_with_defaults_left_out() -> Outcome {
    let yaml = accepted(&three_node()?)?.into_parts().0;
    for old in ["docker:", "vm:", "services:", "idle", "provision:", "null", "[]", "{}", "privileged", "arch"] {
        assert!(!yaml.contains(old), "`{old}` in:\n{yaml}");
    }
    assert!(yaml.contains("internet: false"), "{yaml}");
    assert!(yaml.contains("access: true"), "{yaml}");
    assert!(yaml.contains("steps:\n      - run: provision/x.sh"), "{yaml}");
    Ok(())
}

#[test]
fn an_idle_container_is_kept_running_by_its_own_command() -> Outcome {
    let mut topo = three_node()?;
    topo.nodes["router"].kind = container("alpine:3.20", true);
    let c = accepted(&topo)?;
    assert!(yaml(&c).contains("command:\n    - sleep\n    - infinity"), "{}", yaml(&c));
    assert!(spec(&c).machines["router"].command.is_some());
    Ok(())
}

#[test]
fn a_vm_without_steps_is_its_os_alone_and_a_playbook_is_a_playbook_step() -> Outcome {
    let mut topo = three_node()?;
    // The router's image has no VM form: a VM router, so the drawing renders on the VM targets.
    topo.nodes["router"].kind = vm("debian@12", &[])?;
    topo.nodes["client"].kind = vm("debian@12", &[])?;
    topo.nodes["server"].kind = vm("ubuntu@24.04", &["provision/web.sh", "provision/site.yml"])?;
    let c = accepted(&topo)?;
    assert!(spec(&c).machines["client"].app().is_none());
    assert!(
        yaml(&c).contains("- run: provision/web.sh\n      - playbook: provision/site.yml"),
        "{}",
        yaml(&c)
    );
    Ok(())
}

#[test]
fn ports_are_named_with_their_protocol() -> Outcome {
    let mut topo = three_node()?;
    // The router's image has no VM form: a VM router, so the drawing renders on the VM targets.
    topo.nodes["router"].kind = vm("debian@12", &[])?;
    topo.nodes["server"].services = serde_json::from_str(r#"[{ "port": 22 }, { "port": 443, "protocol": "https" }, { "port": 53, "protocol": "udp" }]"#)?;
    let c = accepted(&topo)?;
    assert!(
        yaml(&c).contains(
            "ports:\n      ssh: 22\n      https:\n        port: 443\n        protocol: https\n      dns-udp:\n        port: 53\n        protocol: udp\n"
        ),
        "{}",
        yaml(&c)
    );
    Ok(())
}

#[test]
fn a_windows_vm_runs_its_powershell_steps_and_the_controller_its_playbooks() -> Outcome {
    let mut topo = three_node()?;
    // The router's image has no VM form: a VM router, so the drawing renders on the VM targets.
    topo.nodes["router"].kind = vm("debian@12", &[])?;
    topo.nodes["server"].kind = vm("windows-server@2022", &["provision/iis.ps1", "provision/domain.yml"])?;
    let c = accepted(&topo)?;
    assert!(
        yaml(&c).contains("os: windows-server@2022\n    app:\n      steps:\n      - run: provision/iis.ps1\n    networks:"),
        "{}",
        yaml(&c)
    );
    assert!(yaml(&c).ends_with("provision:\n- ansible: provision/domain.yml\n"), "{}", yaml(&c));
    let starter = files(&c)
        .into_iter()
        .find(|f| f.path == "provision/domain.yml")
        .ok_or("the playbook's starter")?;
    assert!(starter.contents.contains("- hosts: server\n"), "{}", starter.contents);
    Ok(())
}

#[test]
fn a_windows_vm_with_only_playbooks_is_its_os_alone() -> Outcome {
    let mut topo = three_node()?;
    // The router's image has no VM form: a VM router, so the drawing renders on the VM targets.
    topo.nodes["router"].kind = vm("debian@12", &[])?;
    topo.nodes["server"].kind = vm("windows@11", &["provision/a.yml", "provision/b.yaml"])?;
    let c = accepted(&topo)?;
    assert!(spec(&c).machines["server"].app().is_none(), "{}", yaml(&c));
    assert!(
        yaml(&c).ends_with("provision:\n- ansible: provision/a.yml\n- ansible: provision/b.yaml\n"),
        "{}",
        yaml(&c)
    );
    Ok(())
}

/// Regression: a step path outside the project reached Isoloom's reader as it was, and failed as
/// `Unreadable` (a compiler bug) on Linux while a Windows playbook's compiled; it is the user's
/// mistake, named, whatever the OS. The fuzz seed `fuzz/seeds/drawing/step-outside.json` is one.
#[test]
fn a_step_outside_the_project_is_refused_naming_it() -> Outcome {
    for (os, path) in [
        ("debian@12", "/x.sh"),
        ("debian@12", "../x.sh"),
        ("debian@12", "provision/../../x.sh"),
        ("windows-server@2022", "/x.yml"),
    ] {
        let mut topo = three_node()?;
        topo.nodes["client"].kind = vm(os, &["provision/ok.sh", path])?;
        let e = compile(&topo).unwrap_err();
        assert!(
            matches!(&e, Error::StepPath { node, path: p, .. } if node == "client" && p == path),
            "{os} {path}: {e}"
        );
        assert!(e.to_string().starts_with(&format!("provisioning step `{path}` of `client`: ")), "{e}");
    }
    let seed = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../fuzz/seeds/drawing/step-outside.json"))?;
    assert!(matches!(compile(&serde_json::from_str(&seed)?), Err(Error::StepPath { .. })));
    Ok(())
}

#[test]
fn a_container_takes_the_forms_isoloom_has_of_its_image() -> Outcome {
    let mut topo = three_node()?;
    topo.nodes["router"].kind = container("alpine:3.24", true);
    topo.nodes["server"].kind = container("nginx:1.30", false);
    topo.nodes["server"].services = serde_json::from_str(r#"[{ "port": 80, "protocol": "http" }]"#)?;
    let c = accepted(&topo)?;
    let s = spec(&c);
    assert_eq!(s.machines["router"].os.map(|os| os.to_string()).as_deref(), Some("alpine@3.24"));
    assert!(s.machines["router"].app().is_none() && s.machines["router"].command.is_none(), "{}", yaml(&c));
    assert!(yaml(&c).contains("server:\n    os: debian@12\n    service: nginx@1.30\n"), "{}", yaml(&c));
    Ok(())
}

#[test]
fn a_drawing_no_target_runs_whole_is_refused_naming_each_machine_and_why() -> Outcome {
    let mut topo = three_node()?;
    topo.nodes["server"].kind = vm("windows-server@2022", &["provision/iis.ps1"])?;
    let refused = accepted(&topo).err().ok_or("a drawing no one target runs")?.to_string();
    assert!(refused.contains("no target is possible"), "{refused}");
    assert!(refused.contains("machines.router.app.image"), "the image with no VM form: {refused}");
    assert!(refused.contains("machines.server.os"), "the Windows VM with no container form: {refused}");
    Ok(())
}
