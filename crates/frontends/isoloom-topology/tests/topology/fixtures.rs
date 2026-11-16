//! The fixture drawings, compiled: each spec as written (a snapshot), Isoloom's validation finds
//! nothing wrong with it, and Isoloom generates the files of the targets the drawing is for; or,
//! for a drawing no one target runs whole, Isoloom refuses it naming why.

use std::path::Path;

use isoloom_spec::syntax::Code;
use isoloom_topology::{Topology, compile};

use crate::testing::{Outcome, accepted, problems, yaml};

/// The drawing in `tests/fixtures/<name>.json`.
fn fixture(name: &str) -> Outcome<Topology> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(format!("{name}.json"));
    Ok(serde_json::from_str(&std::fs::read_to_string(&path)?)?)
}

/// The compiled spec's text, after checking Isoloom accepts it and generates the files of each
/// of `targets` (`docker`, `vagrant`...).
fn compiled(name: &str, targets: &[&str]) -> Outcome<String> {
    let compiled = accepted(&fixture(name)?)?;
    let project = isoloom_targets::Lab::parse(&yaml(&compiled)).map_err(|p| format!("{name}: {p:?}"))?;
    let (files, skipped) = isoloom_targets::generate_all(&project);
    for target in targets {
        let dir = format!(".isoloom/{target}/");
        if !files.iter().any(|f| f.path.starts_with(&dir)) {
            let skipped: Vec<String> = skipped.iter().map(ToString::to_string).collect();
            return Err(format!("{name}: nothing for {target}: {skipped:?}").into());
        }
    }
    Ok(yaml(&compiled))
}

#[test]
fn the_office_drawing_compiles_to_a_spec_isoloom_runs() -> Outcome {
    insta::assert_snapshot!("office", compiled("office", &["vagrant"])?);
    Ok(())
}

#[test]
fn the_vlan_drawing_compiles_to_a_spec_isoloom_runs() -> Outcome {
    insta::assert_snapshot!("vlans", compiled("vlans", &["docker"])?);
    Ok(())
}

#[test]
fn the_cisco_drawing_compiles_to_a_spec_isoloom_runs() -> Outcome {
    insta::assert_snapshot!("cisco-ospf", compiled("cisco-ospf", &["docker"])?);
    Ok(())
}

/// The WAN drawing mixes an image Isoloom has no VM form of (FRR's) with a VM whose steps install
/// services no container command runs: no one target runs it whole, and Isoloom's validation says
/// so, naming each machine and why, rather than generating a part of it.
#[test]
fn the_wan_drawing_is_refused_naming_the_machines_no_target_takes() -> Outcome {
    let compiled = compile(&fixture("wan-links")?)?;
    // The VM with no container form, the image with no VM form.
    assert_eq!(
        problems(&compiled)?,
        [(
            Code::InvalidValue,
            "machines".to_owned(),
            "no target is possible: machines.files.command: its steps install services, but no `service` step or `command` runs one in a container; machines.edge.app.image: an image's filesystem can't be reproduced on a fresh VM".to_owned()
        )]
    );
    insta::assert_snapshot!("wan-links", yaml(&compiled));
    Ok(())
}

#[test]
fn the_windows_drawing_compiles_to_a_spec_isoloom_runs() -> Outcome {
    insta::assert_snapshot!("windows-domain", compiled("windows-domain", &["vagrant"])?);
    Ok(())
}

#[test]
fn every_provisioning_step_gets_its_starter() -> Outcome {
    let compiled = compile(&fixture("windows-domain")?)?;
    let paths: Vec<String> = compiled.into_parts().2.into_iter().map(|f| f.path).collect();
    assert_eq!(paths, ["provision/dc01.ps1", "provision/intranet.sh", "provision/intranet.yml"]);
    Ok(())
}
