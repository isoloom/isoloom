//! The Vagrant backend's rendering of a few examples, as snapshots, its determinism, and its
//! refusals from the plan.

use std::error::Error;
use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_vagrant::VagrantBackend;

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// An example of the project, which must be valid, planned for Vagrant.
fn plan(example: &str) -> Outcome<Environment> {
    let spec = isoloom_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(example))?;
    let problems = isoloom_spec::validate(&spec);
    if !problems.is_empty() {
        return Err(format!("{example} isn't valid: {problems:?}").into());
    }
    Ok(Environment::build(&spec, Target::Vagrant, None))
}

/// The files the Vagrant backend renders for an example, from its plan.
fn render(example: &str) -> Outcome<Vec<GeneratedFile>> {
    Ok(VagrantBackend.render(&plan(example)?)?)
}

/// An example's Vagrantfile.
fn vagrantfile(example: &str) -> Outcome<String> {
    let file = render(example)?
        .into_iter()
        .find(|f| f.path.ends_with("/Vagrantfile"))
        .ok_or("no Vagrantfile")?;
    Ok(file.contents)
}

/// A spec written in a test, planned for Vagrant.
fn parsed(yaml: &str) -> Outcome<Environment> {
    Ok(Environment::build(&isoloom_spec::parse(yaml)?, Target::Vagrant, None))
}

#[test]
fn hello_stack() -> Outcome {
    insta::assert_snapshot!(vagrantfile("hello-stack")?);
    Ok(())
}

#[test]
fn segmented_routes_through_the_router() -> Outcome {
    insta::assert_snapshot!(vagrantfile("segmented")?);
    Ok(())
}

#[test]
fn windows_hello_over_winrm() -> Outcome {
    insta::assert_snapshot!(vagrantfile("windows-hello")?);
    Ok(())
}

#[test]
fn tuned_web_settings() -> Outcome {
    insta::assert_snapshot!(vagrantfile("tuned-web")?);
    Ok(())
}

#[test]
fn shared_files_over_nfs() -> Outcome {
    insta::assert_snapshot!(vagrantfile("shared-files")?);
    Ok(())
}

#[test]
fn ansible_pair_from_the_controller() -> Outcome {
    insta::assert_snapshot!(vagrantfile("ansible-pair")?);
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for example in ["hello-stack", "segmented", "edge-firewall", "vlan-office", "existing-hosts"] {
        assert_eq!(render(example)?, render(example)?, "{example}");
    }
    Ok(())
}

#[test]
fn a_renderable_example_has_no_refusal() -> Outcome {
    assert_eq!(VagrantBackend.refusal(&plan("hello-stack")?), None);
    Ok(())
}

#[test]
fn appliances_are_refused_from_the_capabilities() -> Outcome {
    let env = parsed(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  sw:\n    networks: { lab: 10 }\n    docker: { image: x, appliance: cisco-iol }\n    vm: { os: debian@12 }\n",
    )?;
    let why = VagrantBackend.refusal(&env).ok_or("not refused")?;
    assert!(why.contains("network appliance"), "{why}");
    assert!(VagrantBackend.render(&env).is_err());
    Ok(())
}

#[test]
fn a_machine_without_a_box_is_refused_without_rendering() -> Outcome {
    let mut spec = isoloom_spec::parse(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  m:\n    networks: { lab: 10 }\n    vm: { os: debian@12 }\n",
    )?;
    // Parsing refuses an OS it doesn't know; a spec built in code can still name one.
    spec.machines.get_mut("m").and_then(|m| m.vm.as_mut()).ok_or("a VM")?.os = "plan9".into();
    let env = Environment::build(&spec, Target::Vagrant, None);
    let why = VagrantBackend.refusal(&env).ok_or("not refused")?;
    assert!(why.contains("no Vagrant box for `plan9`"), "{why}");
    let refused = VagrantBackend.render(&env).err().ok_or("rendered")?;
    assert_eq!(refused.to_string(), format!("vagrant: {why}"));
    Ok(())
}
