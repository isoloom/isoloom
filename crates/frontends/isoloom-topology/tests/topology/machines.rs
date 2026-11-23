//! Machines: what each kind of node becomes, its steps and its ports.

use isoloom_spec::syntax::{Code, Recipe};
use isoloom_topology::{Error, compile};

use crate::testing::{Outcome, accepted, at, container, files, keys_and_empties, parsed, problems, spec, three_node, vm, yaml};

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
    let c = accepted(&three_node()?)?;
    let written = keys_and_empties(&parsed(&c));
    for old in ["docker", "vm", "services", "idle", "provision", "<empty>", "privileged", "arch"] {
        assert!(!written.iter().any(|k| k == old), "`{old}` in:\n{}", yaml(&c));
    }
    assert_eq!(
        ["networks.link-0.internet", "machines.client.access", "machines.client.app.steps.0.run"].map(|path| at(&c, path)),
        ["false", "true", "provision/x.sh"]
    );
    Ok(())
}

#[test]
fn an_idle_container_is_kept_running_by_its_own_command() -> Outcome {
    let mut topo = three_node()?;
    topo.nodes["router"].kind = container("alpine:3.20", true);
    let c = accepted(&topo)?;
    assert_eq!(isoloom_test_support::yaml_strs(&parsed(&c), "machines.router.command"), ["sleep", "infinity"]);
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
    assert_eq!(
        ["machines.server.app.steps.0.run", "machines.server.app.steps.1.playbook"].map(|path| at(&c, path)),
        ["provision/web.sh", "provision/site.yml"]
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
    let ports = "machines.server.ports";
    assert_eq!(isoloom_test_support::yaml_keys_at(&parsed(&c), ports), ["ssh", "https", "dns-udp"]);
    assert_eq!(
        ["ssh", "https.port", "https.protocol", "dns-udp.port", "dns-udp.protocol"].map(|p| at(&c, &format!("{ports}.{p}"))),
        ["22", "443", "https", "53", "udp"],
        "plain TCP as its number"
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
    assert_eq!(
        ["machines.server.os", "machines.server.app.steps.0.run", "provision.0.ansible"].map(|path| at(&c, path)),
        ["windows-server@2022", "provision/iis.ps1", "provision/domain.yml"]
    );
    assert_eq!(
        isoloom_test_support::yaml_strs(&parsed(&c), "machines.server.app.steps").len(),
        1,
        "the playbook isn't the VM's step"
    );
    let starter = files(&c)
        .into_iter()
        .find(|f| f.path == "provision/domain.yml")
        .ok_or("the playbook's starter")?;
    let play = isoloom_test_support::yaml(&starter.contents);
    assert_eq!(
        isoloom_test_support::yaml_str(&play, "0.hosts"),
        "server",
        "it runs from the controller, on its machine"
    );
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
    assert_eq!(
        ["provision.0.ansible", "provision.1.ansible"].map(|path| at(&c, path)),
        ["provision/a.yml", "provision/b.yaml"]
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
    assert_eq!(
        ["machines.server.os", "machines.server.service"].map(|path| at(&c, path)),
        ["debian@12", "nginx@1.30"]
    );
    Ok(())
}

#[test]
fn a_drawing_no_target_runs_whole_is_refused_naming_each_machine_and_why() -> Outcome {
    let mut topo = three_node()?;
    topo.nodes["server"].kind = vm("windows-server@2022", &["provision/iis.ps1"])?;
    // The Windows VM has no container form, the router's image no VM form.
    assert_eq!(
        problems(&compile(&topo)?)?,
        [(
            Code::InvalidValue,
            "machines".to_owned(),
            "no target is possible: machines.server.os: Windows has no container form; machines.router.app.image: an image's filesystem can't be reproduced on a fresh VM".to_owned()
        )]
    );
    Ok(())
}
