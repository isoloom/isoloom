//! What the Vagrant backend refuses from the plan, and says why.

use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_vagrant::VagrantBackend;

use crate::testing::{Outcome, parsed, plan};

#[test]
fn a_renderable_example_has_no_refusal() -> Outcome {
    assert_eq!(VagrantBackend.refusal(&plan("hello-stack")?), None);
    Ok(())
}

#[test]
fn appliances_are_refused_from_the_capabilities() -> Outcome {
    let env = parsed(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  sw:\n    networks: { lab: 10 }\n    os: debian@12\n    app: { appliance: cisco-iol, image: x, steps: [{ run: s.sh }] }\n",
    )?;
    let why = VagrantBackend.refusal(&env).ok_or("not refused")?;
    assert!(why.contains("network appliance"), "{why}");
    assert!(VagrantBackend.render(&env).is_err());
    Ok(())
}

#[test]
fn a_machine_without_a_box_is_refused_without_rendering() -> Outcome {
    let lab = isoloom_plan::Lab::parse(
        "version: 1\nname: a\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  m:\n    networks: { lab: 10 }\n    os: debian@12\n",
    )
    .map_err(|problems| format!("{problems:?}"))?;
    let mut spec = lab.for_target(Target::Vagrant);
    // Parsing refuses an OS it doesn't know; a spec built in code can still name one.
    spec.machines.get_mut("m").and_then(|m| m.vm.as_mut()).ok_or("a VM")?.os = "plan9".into();
    let env = lab.plan(&spec, Target::Vagrant, None)?;
    let why = VagrantBackend.refusal(&env).ok_or("not refused")?;
    assert!(why.contains("no Vagrant box for `plan9`"), "{why}");
    let refused = VagrantBackend.render(&env).err().ok_or("rendered")?;
    assert_eq!(refused.to_string(), format!("vagrant: {why}"));
    Ok(())
}
