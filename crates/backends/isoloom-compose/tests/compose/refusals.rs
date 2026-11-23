//! What the Compose backend refuses from the plan, and says why.

use isoloom_compose::ComposeBackend;
use isoloom_plan::Lab;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

use crate::testing::Outcome;

#[test]
fn provisioning_is_refused_without_rendering() -> Outcome {
    let lab = Lab::parse(
        "version: 1\nname: p\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    os: debian@12\n    app: { image: alpine }\nprovision:\n  - ansible: site.yml\n",
    )
    .map_err(|problems| format!("{problems:?}"))?;
    let env = lab.plan(&lab.for_target(Target::Docker), Target::Docker, None)?;
    let refused = ComposeBackend::DOCKER.render(&env).err().ok_or("rendered")?;
    assert!(matches!(refused, isoloom_render::Error::Refused { .. }), "{refused:?}");
    assert_eq!(
        refused.to_string(),
        "docker: environment-level provisioning (`provision:`) runs on the VM targets only"
    );
    Ok(())
}
