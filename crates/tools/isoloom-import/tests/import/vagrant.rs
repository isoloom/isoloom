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
    assert_eq!(spec.networks["net"].cidr, "192.168.33.0/24");
    let web = &spec.machines["web"];
    assert_eq!(web.networks["net"], Octets::One(10));
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

/// One machine whose `vm` makes `calls` (JSON), on a private network besides.
fn one_machine(os_box: &str, calls: &str) -> Outcome<Recording> {
    Ok(format!(
        r#"{{"children":{{"vm":{{"calls":[{{"name":"define","args":["web"],"block":{{"children":{{"vm":{{"settings":{{"box":"{os_box}"}},"calls":[
            {{"name":"network","args":["private_network",{{"ip":"192.168.40.10"}}]}},{calls}]}}}}}}}}]}}}}}}"#
    )
    .parse()?)
}

#[test]
fn a_public_network_is_a_bridged_network() -> Outcome {
    let fixed = one_machine(
        "bento/debian-12",
        r#"{"name":"network","args":["public_network",{"ip":"192.168.1.50","bridge":"en0: Wi-Fi"}]}"#,
    )?;
    let d = draft(&fixed, "lab", "Vagrantfile")?;
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    let drafted = spec(&d)?;
    let lan = &drafted.networks["lan"];
    assert_eq!(lan.cidr, "192.168.1.0/24");
    let bridged = lan.bridged.as_ref().ok_or("not bridged")?;
    assert_eq!((bridged.interface.as_deref(), bridged.dhcp), (Some("en0: Wi-Fi"), false));
    assert_eq!(drafted.machines["web"].networks["lan"], Octets::One(50));
    // By DHCP, with the LAN's own default route: the block is a placeholder, said.
    let dhcp = one_machine(
        "bento/debian-12",
        r#"{"name":"network","args":["public_network",{"use_dhcp_assigned_default_route":true}]}"#,
    )?;
    let by_dhcp = draft(&dhcp, "lab", "Vagrantfile")?;
    assert_eq!(validate(&by_dhcp)?, Vec::<String>::new(), "{}", by_dhcp.yaml);
    let leased = spec(&by_dhcp)?;
    let lease = leased.networks["lan"].bridged.as_ref().ok_or("not bridged")?;
    assert!(lease.dhcp && lease.default_route && lease.interface.is_none(), "{}", by_dhcp.yaml);
    assert_eq!(leased.machines["web"].networks["lan"], Octets::One(0));
    assert!(
        has(&by_dhcp, NoteKind::Changed, "define web.vm.network", "set it to the LAN's"),
        "{:?}",
        by_dhcp.notes
    );
    assert!(!by_dhcp.notes.iter().any(|n| n.kind == NoteKind::NotYet), "{:?}", by_dhcp.notes);
    Ok(())
}

#[test]
fn a_synced_folder_is_a_host_folder() -> Outcome {
    let shared = one_machine(
        "bento/debian-12",
        r#"{"name":"synced_folder","args":["data","/srv/data",{"mount_options":["ro"]}]},
           {"name":"synced_folder","args":["notes","/srv/notes",{"type":"rsync"}]},
           {"name":"synced_folder","args":[".","/vagrant"]},
           {"name":"synced_folder","args":["off","/srv/off",{"disabled":true}]}"#,
    )?;
    let d = draft(&shared, "lab", "Vagrantfile")?;
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    let folders: Vec<(String, String, bool)> = spec(&d)?.machines["web"]
        .host_folders
        .iter()
        .map(|f| (f.from.clone(), f.path.clone(), f.read_only))
        .collect();
    assert_eq!(
        folders,
        [
            ("data".to_owned(), "/srv/data".to_owned(), true),
            ("notes".to_owned(), "/srv/notes".to_owned(), false)
        ],
        "{}",
        d.yaml
    );
    let at = "define web.vm.synced_folder";
    assert!(has(
        &d,
        NoteKind::Changed,
        at,
        "`notes`: each target shares a host folder its own way; not kept: type: rsync"
    ));
    assert!(has(&d, NoteKind::Equivalent, at, "the project: Isoloom copies it"), "{:?}", d.notes);
    // A Windows machine has none: left out, and said.
    let windows = one_machine("gusztavvargadr/windows-server", r#"{"name":"synced_folder","args":["data","C:/data"]}"#)?;
    let on_windows = draft(&windows, "lab", "Vagrantfile")?;
    assert!(spec(&on_windows)?.machines["web"].host_folders.is_empty(), "{}", on_windows.yaml);
    assert!(
        has(&on_windows, NoteKind::Changed, at, "a Windows machine has no host folders"),
        "{:?}",
        on_windows.notes
    );
    Ok(())
}
