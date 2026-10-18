//! `isoloom import vagrant`: from what a Vagrantfile set when it ran (recorded by the CLI's
//! `vagrant_record.rb`; the fixtures hold its output).

use isoloom_import::NoteKind;
use isoloom_import::vagrant::{Recording, draft};
use isoloom_spec::syntax::Octets;

use crate::testing::{Outcome, fixture, has, ports, spec, steps, validate, validate_in};

/// The recording `name` of the classic fixture's folder, as JSON (some tests change it).
fn recorded_json(name: &str) -> Outcome<serde_json::Value> {
    Ok(serde_json::from_str(&std::fs::read_to_string(fixture("vagrant-classic").join(name))?)?)
}

/// The recording `name` of the classic fixture's folder.
fn recorded(name: &str) -> Outcome<Recording> {
    Ok(Recording::try_from(recorded_json(name)?)?)
}

#[test]
fn a_vagrantfile_with_a_loop_becomes_a_draft_that_passes_validate() -> Outcome {
    let d = draft(&recorded("recorded.json")?, "classic", "Vagrantfile")?;
    assert_eq!(validate_in(&d, &fixture("vagrant-classic"))?, Vec::<String>::new(), "{}", d.yaml);
    let spec = spec(&d)?;
    assert_eq!(spec.networks["lab"].cidr, "192.168.33.0/24");
    let web = &spec.machines["web"];
    assert_eq!(web.networks["lab"], Octets::One(10));
    assert_eq!(ports(&spec, "web").first().map(|p| (p.1, p.2)), Some((80, Some(8080))));
    assert_eq!(web.resources.and_then(|r| r.memory_mb), Some(1024));
    assert_eq!(spec.machines["db"].resources.and_then(|r| r.memory_mb), Some(2048));
    let boxes = &spec.overrides.vagrant.as_ref().ok_or("no vagrant overrides")?.machines;
    assert_eq!(boxes["web"].box_name.as_deref(), Some("ubuntu/jammy64"), "an unknown box is kept");
    assert_eq!(steps(&spec, "web"), ["site.yml", "provision/web.sh"], "global provisioners run first");
    assert!(has(&d, NoteKind::InImage, "define web.vm.provision", "inline script"));
    Ok(())
}

#[test]
fn a_udp_forwarded_port_is_imported_as_a_udp_port() -> Outcome {
    let mut json = recorded_json("recorded.json")?;
    let calls = json
        .pointer_mut("/children/vm/calls/1/block/children/vm/calls")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("the fixture's web machine has no calls")?;
    calls.insert(
        2,
        serde_json::json!({"name": "network", "args": ["forwarded_port", {"guest": 514, "host": 1514, "protocol": "udp"}], "block": null}),
    );
    let d = draft(&Recording::try_from(json)?, "classic", "Vagrantfile")?;
    let spec = spec(&d)?;
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    assert_eq!(
        ports(&spec, "web").get(1),
        Some(&("syslog-udp".to_owned(), 514, Some(1514), true)),
        "{}",
        d.yaml
    );
    Ok(())
}

#[test]
fn importing_an_isoloom_vagrantfile_gives_back_the_environment() -> Outcome {
    let d = draft(&recorded("segmented-recorded.json")?, "segmented", "Vagrantfile")?;
    let spec = spec(&d)?;
    let original = isoloom_spec::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/segmented"))?;
    let cidrs = |s: &isoloom_spec::Spec| {
        let mut v: Vec<String> = s.networks.values().map(|n| n.cidr.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(cidrs(&spec), cidrs(&original));
    for name in ["cache", "web", "user"] {
        assert!(spec.machines.contains_key(name), "{name} comes back");
    }
    assert!(!spec.machines.contains_key("isoloom-router"), "Isoloom's own router isn't a machine");
    assert_eq!(spec.machines["web"].os.map(|o| o.to_string()).as_deref(), Some("debian@12"));
    assert!(spec.overrides.vagrant.is_none(), "a built-in box needs no override");
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    Ok(())
}

#[test]
fn a_reboot_after_a_script_is_kept_as_a_step() -> Outcome {
    let recorded: Recording = r#"{"settings":{},"children":{"vm":{"settings":{"box":"bento/debian-12"},"children":{},"calls":[
          {"name":"define","args":["web"],"block":{"settings":{},"children":{"vm":{"settings":{},"children":{},"calls":[
            {"name":"network","args":["private_network",{"ip":"192.168.40.10"}],"block":null},
            {"name":"provision","args":["shell",{"path":"kernel.sh","reboot":true}],"block":null},
            {"name":"provision","args":["shell",{"path":"web.sh"}],"block":null}]}},"calls":[]}}]}},"calls":[]}"#
        .parse()?;
    let d = draft(&recorded, "lab", "Vagrantfile")?;
    let lab = isoloom_plan::Lab::new(spec(&d)?);
    assert_eq!(
        lab.spec().machines["web"].vm.as_ref().ok_or("web has no VM form")?.provision,
        ["kernel.sh", "reboot", "web.sh"]
    );
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    Ok(())
}

#[test]
fn a_vagrantfile_that_defines_nothing_of_its_own_is_an_error() -> Outcome {
    let only_isoloom: Recording = r#"{"children":{"vm":{"calls":[{"name":"define","args":["isoloom-router"]}]}}}"#.parse()?;
    assert!(matches!(draft(&only_isoloom, "x", "Vagrantfile"), Err(isoloom_import::Error::NoVagrantMachine)));
    Ok(())
}
