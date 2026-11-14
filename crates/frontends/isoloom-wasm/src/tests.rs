//! The engine under its exports, natively: a topology's plan (its spec, Isoloom's problems,
//! every target's files, the machines' addresses), the project's own files, and the downloads.

use std::error::Error;
use std::io::{Cursor, Read};

use crate::{Plan, bundle_bytes, bundle_target_bytes, own_files, plan};

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// Two machines on a cable: a VM with a step, and a web server.
const PAIR: &str = include_str!("../tests/fixtures/pair.json");

/// Where a download puts the spec.
const SPEC_PATH: &str = "isoloom.yml";

/// The plan of `topology`, which must compile with no problem, and its spec's text.
fn clean_plan(topology: &str) -> Outcome<(Plan, String)> {
    let plan = plan(topology);
    if !plan.ok || !plan.problems.is_empty() {
        return Err(format!("{:?} {:?}", plan.error, plan.problems).into());
    }
    let yml = plan.isoloom_yml.clone().ok_or("a plan that compiled has its spec")?;
    Ok((plan, yml))
}

/// The paths in a zip, in order.
fn names(zip: Vec<u8>) -> Outcome<Vec<String>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip))?;
    (0..archive.len()).map(|i| Ok(archive.by_index(i)?.name().to_owned())).collect()
}

/// What file `path` of a zip holds.
fn read(zip: Vec<u8>, path: &str) -> Outcome<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip))?;
    let mut text = String::new();
    archive.by_name(path)?.read_to_string(&mut text)?;
    Ok(text)
}

#[test]
fn the_projects_own_files_are_its_starters() {
    let files = own_files(PAIR).unwrap();
    assert_eq!(files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["provision/client.sh"]);
    assert!(files[0].contents.starts_with("#!"));
    let refused = own_files("{ not json").unwrap_err().to_string();
    assert!(
        refused.starts_with("the topology is malformed"),
        "it says why, rather than give no file: {refused}"
    );
}

/// A drawing that doesn't compile gives no own files and no zip: each says the compiler's error.
#[test]
fn a_drawing_that_doesnt_compile_gives_no_files_and_says_why() {
    let t = r#"{"name":"project","networks":{"lan":{"cidr":"10.10.1.0/24"}},"nodes":{
        "a":{"kind":{"container":{"image":"alpine:3.20"}},"networks":{"lan":1}}}}"#;
    let why = "on LAN `lan` (10.10.1.0/24), `a` asks for .1, which no machine can take there";
    assert!(own_files(t).unwrap_err().to_string().contains(why));
    assert!(bundle_bytes(t).unwrap_err().to_string().contains(why));
    assert!(bundle_target_bytes(t, "docker").unwrap_err().to_string().contains(why));
}

/// A target's folder may be written with slashes around it (`/docker/`): the same zip.
#[test]
fn a_target_folder_with_slashes_around_it_is_that_target() -> Outcome {
    assert_eq!(names(bundle_target_bytes(PAIR, "/docker/")?)?, names(bundle_target_bytes(PAIR, "docker")?)?);
    Ok(())
}

#[test]
fn a_topology_becomes_a_spec_and_generated_files() -> Outcome {
    let (plan, yml) = clean_plan(PAIR)?;
    assert!(yml.contains("name: pair"));
    assert!(
        plan.files.iter().any(|f| f.path.ends_with("docker/compose.yml")),
        "{:?}",
        plan.files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn machines_get_addresses_on_their_lans() -> Outcome {
    let t = r#"{"name":"lan","networks":{"office":{"cidr":"10.10.1.0/24"}},"nodes":{
        "a":{"kind":{"container":{"image":"alpine:3.20"}},"networks":{"office":null}},
        "b":{"kind":{"container":{"image":"alpine:3.20"}},"networks":{"office":20}}}}"#;
    let (plan, _) = clean_plan(t)?;
    assert_eq!(plan.addresses["a"], vec![("office".to_owned(), "10.10.1.10".to_owned())]);
    assert_eq!(plan.addresses["b"], vec![("office".to_owned(), "10.10.1.20".to_owned())]);
    Ok(())
}

#[test]
fn a_bad_topology_says_why() {
    let bad = plan(r#"{"name":"x","nodes":{"a":{"kind":{"vm":{"os":"debian@12"}}}},"links":[{"a":{"node":"a","port":"e"},"b":{"node":"ghost","port":"e"}}]}"#);
    assert!(!bad.ok && bad.isoloom_yml.is_none());
    assert!(bad.error.unwrap_or_default().contains("ghost"));
    let old = plan(r#"{"name":"x","nodes":{"a":{"kind":{"vm":{"os":"debian-12"}}}}}"#);
    assert!(
        old.error.unwrap_or_default().contains("unknown OS `debian-12`"),
        "an OS is written as Isoloom writes it"
    );
    let malformed = plan("{ not json");
    assert!(malformed.error.unwrap_or_default().starts_with("the topology is malformed: "));
}

#[test]
fn problems_are_isoloom_s_with_their_code_line_and_fix() {
    // A rate with no unit: Isoloom says where and why.
    let rate = r#"{"name":"project","networks":{"lan":{"cidr":"10.10.1.0/24","tc":{"rate":"10"}}},"nodes":{
        "a":{"kind":{"container":{"image":"alpine:3.20"}},"networks":{"lan":null}}}}"#;
    let no_unit = plan(rate);
    assert!(no_unit.ok);
    assert_eq!(
        no_unit.problems,
        ["error[invalid-value] networks.lan.tc.rate  (isoloom.yml:7:7)\n  `10` isn't a rate like 10mbit or 512kbit"],
    );
}

#[test]
fn an_address_no_machine_can_take_is_the_compilers_error() {
    // The gateway's address: the compiler refuses it before Isoloom would.
    let t = r#"{"name":"project","networks":{"lan":{"cidr":"10.10.1.0/24"}},"nodes":{
        "a":{"kind":{"container":{"image":"alpine:3.20"}},"networks":{"lan":1}}}}"#;
    let gateway = plan(t);
    assert!(!gateway.ok && gateway.isoloom_yml.is_none());
    assert_eq!(
        gateway.error.as_deref(),
        Some("on LAN `lan` (10.10.1.0/24), `a` asks for .1, which no machine can take there")
    );
}

/// A LAN whose block doesn't read is the compiler's error, so no plan has a network left out of
/// `addresses` for its block.
#[test]
fn a_block_that_doesnt_read_is_the_compilers_error() {
    let t = r#"{"name":"project","networks":{"lan":{"cidr":"10.10.1.0/33"}},"nodes":{
        "a":{"kind":{"container":{"image":"alpine:3.20"}},"networks":{"lan":null}}}}"#;
    let refused = plan(t);
    assert!(!refused.ok && refused.addresses.is_empty(), "{refused:?}");
    assert!(refused.error.unwrap().starts_with("LAN `lan`: `10.10.1.0/33` isn't a block"));
}

#[test]
fn an_invalid_spec_is_reported_not_generated() {
    // A machine without a cable has no network: Isoloom's generators would panic on it.
    let t = r#"{"name":"project","nodes":{
        "a":{"kind":{"container":{"image":"alpine:3.20"}}},
        "b":{"kind":{"container":{"image":"alpine:3.20"}}},
        "lonely":{"kind":{"vm":{"os":"debian@12"}}}},
        "links":[{"a":{"node":"a","port":"eth0"},"b":{"node":"b","port":"eth0"}}]}"#;
    let invalid = plan(t);
    assert!(invalid.ok);
    assert!(invalid.problems.iter().any(|p| p.contains("lonely")), "{:?}", invalid.problems);
    assert_eq!(invalid.files, []);
    let refused = bundle_bytes(t).unwrap_err().to_string();
    assert!(refused.starts_with("fix the spec first: ") && refused.contains("lonely"), "{refused}");
}

#[test]
fn a_target_bundle_holds_only_that_target() -> Outcome {
    // A target this project does produce: the folder of its first generated file.
    let (plan, _) = clean_plan(PAIR)?;
    let first = plan.files.first().ok_or("files")?.path.clone();
    let target = first.trim_start_matches(".isoloom/").split('/').next().ok_or("a target folder")?.to_owned();
    let names = names(bundle_target_bytes(PAIR, &target)?)?;
    assert!(names.contains(&SPEC_PATH.to_owned()));
    let prefix = format!(".isoloom/{target}/");
    let own: Vec<String> = own_files(PAIR)?.into_iter().map(|f| f.path).collect();
    assert!(names.iter().all(|n| n == SPEC_PATH || n.starts_with(&prefix) || own.contains(n)), "{names:?}");
    assert!(names.len() > 1);
    assert_eq!(
        bundle_target_bytes(PAIR, "no-such-target").unwrap_err().to_string(),
        "this project has no files for `no-such-target`"
    );
    Ok(())
}

#[test]
fn the_bundle_holds_the_spec_and_the_generated_files() -> Outcome {
    let names = names(bundle_bytes(PAIR)?)?;
    assert_eq!(names.first().map(String::as_str), Some(SPEC_PATH), "the spec first");
    assert!(names.contains(&"provision/client.sh".to_owned()), "{names:?}");
    assert!(names.iter().any(|n| n.starts_with(".isoloom/docker/")), "{names:?}");
    Ok(())
}

/// Regression guard: the download takes the spec's text from the compiled parts; an empty or
/// other spec in the zip would pass a test that reads only its names.
#[test]
fn every_download_holds_the_plans_spec() -> Outcome {
    let (_, yml) = clean_plan(PAIR)?;
    assert_eq!(read(bundle_bytes(PAIR)?, SPEC_PATH)?, yml);
    assert_eq!(read(bundle_target_bytes(PAIR, "docker")?, SPEC_PATH)?, yml);
    Ok(())
}

#[test]
fn a_vm_project_comes_with_its_provisioning_scripts() -> Outcome {
    let vms = r#"{"name":"vms","networks":{"lan":{"cidr":"10.10.1.0/24"}},"nodes":{
        "db":{"kind":{"vm":{"os":"debian@12","provision":["provision/db.sh"]}},"networks":{"lan":null},"services":[{"port":5432}]},
        "app":{"kind":{"vm":{"os":"debian@12","provision":["provision/app.sh"]}},"networks":{"lan":null},"access":true}}}"#;
    assert!(names(bundle_bytes(vms)?)?.contains(&"provision/db.sh".to_owned()));
    assert!(names(bundle_target_bytes(vms, "vagrant")?)?.contains(&"provision/db.sh".to_owned()));
    // The client's steps build its container too: the Docker download has the script.
    assert!(names(bundle_target_bytes(PAIR, "docker")?)?.contains(&"provision/client.sh".to_owned()));
    // Every target's download has every one of the project's own files, the ones only another
    // target's files name included: which a target uses is Isoloom's to know.
    let own: Vec<String> = own_files(PAIR)?.into_iter().map(|f| f.path).collect();
    let (plan, _) = clean_plan(PAIR)?;
    let targets: std::collections::BTreeSet<String> = plan
        .files
        .iter()
        .filter_map(|f| Some(f.path.strip_prefix(".isoloom/")?.split_once('/')?.0.to_owned()))
        .collect();
    assert!(targets.len() > 1, "{targets:?}");
    for target in &targets {
        let got = names(bundle_target_bytes(PAIR, target)?)?;
        assert!(own.iter().all(|o| got.contains(o)), "{target}: {got:?}");
    }
    // An image runs no steps: a project of images downloads none.
    let images = r#"{"name":"imgs","networks":{"lan":{"cidr":"10.10.1.0/24"}},"nodes":{
        "a":{"kind":{"container":{"image":"alpine:3.20","idle":true}},"networks":{"lan":null}}}}"#;
    assert!(!names(bundle_target_bytes(images, "docker")?)?.iter().any(|n| n.starts_with("provision/")));
    Ok(())
}

#[test]
fn vlans_nest_under_their_switch_and_keep_the_editor_names() -> Outcome {
    let t = r#"{"name":"vlans","networks":{
        "vlan10":{"cidr":"10.10.10.0/24","switch":"sw1","vlan":10,"internet":false},
        "vlan20":{"cidr":"10.10.20.0/24","switch":"sw1","vlan":20,"internet":false}},
        "nodes":{
        "pc1":{"kind":{"container":{"image":"alpine:3.20","idle":true}},"networks":{"vlan10":null}},
        "srv":{"kind":{"container":{"image":"nginx:1.27-alpine"}},"networks":{"vlan20":null}},
        "r":{"kind":{"router":{}},"networks":{"vlan10":null,"vlan20":null}}}}"#;
    let (plan, yml) = clean_plan(t)?;
    assert!(yml.contains("  sw1:\n    cidr: 10.10.0.0/19\n    internet: false\n    vlans:\n"), "{yml}");
    assert!(yml.contains("sw1.vlan10: 10") && yml.contains("from: sw1.vlan10"), "{yml}");
    assert_eq!(plan.addresses["pc1"], vec![("vlan10".to_owned(), "10.10.10.10".to_owned())]);
    assert!(
        plan.files
            .iter()
            .any(|f| f.path.ends_with("docker/compose.yml") && f.contents.contains("sw1-vlan10"))
    );
    // The download parses it back the same way.
    assert!(bundle_bytes(t).is_ok());
    Ok(())
}

#[test]
fn a_cable_with_conditions_carries_them() -> Outcome {
    let t = r#"{"name":"wan","nodes":{
        "a":{"kind":{"container":{"image":"alpine:3.20","idle":true}}},
        "b":{"kind":{"container":{"image":"alpine:3.20","idle":true}}}},
        "links":[{"a":{"node":"a","port":"eth0"},"b":{"node":"b","port":"eth0"},"tc":{"delay":"40ms","loss":0.5}}]}"#;
    let (plan, yml) = clean_plan(t)?;
    assert!(yml.contains("tc:\n      delay: 40ms\n      loss: 0.5"), "{yml}");
    assert!(
        plan.files
            .iter()
            .any(|f| f.path.ends_with("docker/compose.yml") && f.contents.contains("netem delay 40ms loss 0.5%"))
    );
    Ok(())
}

#[test]
fn a_cisco_iol_router_is_its_lans_gateway_with_its_config() -> Outcome {
    let t = r#"{"name":"iol","networks":{"a":{"cidr":"10.30.1.0/24"},"b":{"cidr":"10.30.2.0/24"}},"nodes":{
        "r1":{"kind":{"container":{"image":"vrnetlab/cisco_iol:17.12.01","appliance":"cisco-iol","config":"router ospf 1\n network 10.30.0.0 0.0.255.255 area 0"}},"networks":{"a":null}},
        "r2":{"kind":{"container":{"image":"vrnetlab/cisco_iol:17.12.01","appliance":"cisco-iol"}},"networks":{"b":null}},
        "pc":{"kind":{"container":{"image":"alpine:3.20","idle":true}},"networks":{"a":null}}},
        "links":[{"a":{"node":"r1","port":"eth1"},"b":{"node":"r2","port":"eth1"}}]}"#;
    let (plan, yml) = clean_plan(t)?;
    assert!(yml.contains("gateway: r1") && yml.contains("gateway: r2"), "{yml}");
    assert!(yml.contains("appliance: cisco-iol") && yml.contains("config: configs/r1.cfg"), "{yml}");
    assert_eq!(plan.addresses["r1"][0], ("a".to_owned(), "10.30.1.1".to_owned()));
    let cfg = plan
        .files
        .iter()
        .find(|f| f.path.ends_with("appliances/r1/base.txt"))
        .ok_or("r1's base configuration")?;
    assert!(
        cfg.contents
            .contains("interface Ethernet0/1\n description a\n ip address 10.30.1.1 255.255.255.0"),
        "{}",
        cfg.contents
    );
    assert!(read(bundle_target_bytes(t, "docker")?, "configs/r1.cfg")?.starts_with("router ospf 1"));
    Ok(())
}

#[test]
fn a_windows_machines_playbook_runs_from_the_controller_and_the_vm_targets_take_it() -> Outcome {
    let t = r#"{"name":"domain","networks":{"corp":{"cidr":"10.50.1.0/24"}},"nodes":{
        "dc01":{"kind":{"vm":{"os":"windows-server@2022","provision":["provision/dc01.ps1","provision/dc01.yml"]}},"networks":{"corp":10}},
        "workstation":{"kind":{"vm":{"os":"debian"}},"access":true,"networks":{"corp":50}}}}"#;
    let (plan, yml) = clean_plan(t)?;
    assert!(yml.contains("steps:\n      - run: provision/dc01.ps1\n"), "{yml}");
    assert!(yml.ends_with("provision:\n- ansible: provision/dc01.yml\n"), "{yml}");
    assert!(plan.files.iter().any(|f| f.path.starts_with(".isoloom/vagrant/")), "{:?}", plan.skipped);
    let playbook = read(bundle_target_bytes(t, "vagrant")?, "provision/dc01.yml")?;
    assert!(playbook.contains("- hosts: dc01"), "{playbook}");
    Ok(())
}
