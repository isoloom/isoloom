//! What the Kubernetes backend refuses, from the plan or in its own words, and says why.

use isoloom_kubernetes::KubernetesBackend;
use isoloom_plan::Environment;
use isoloom_render::Error as RenderError;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

use crate::testing::{BASE, Outcome, load, parsed, plan, planned};

/// `tuned-web` without its TPM, which makes it a VM only: its container form, which Kubernetes
/// still refuses.
fn tuned_web_container() -> Outcome<Environment> {
    let mut tuned = load("tuned-web")?;
    let web = tuned.machines.get_mut("web").ok_or("no web machine")?;
    web.hardware.tpm = isoloom_spec::syntax::Tpm::Auto;
    planned(tuned)
}

/// What rendering `yaml` reports, as every backend reports it.
fn refusal(yaml: &str) -> Outcome<String> {
    let err = KubernetesBackend.render(&parsed(yaml)?).err().ok_or("rendered")?;
    Ok(err.to_string())
}

#[test]
fn each_unsupported_feature_is_refused_with_its_reason() -> Outcome {
    let cases = [
        (
            "    aliases: [www.lab.test]\n",
            "machine `aliases` (dotted DNS names) aren't Kubernetes Service names",
        ),
        (
            "    ulimits: { nofile: 1024 }\n",
            "machine `web` sets `ulimits`: Kubernetes has no per-container process limits",
        ),
    ];
    for (extra, why) in cases {
        assert_eq!(refusal(&format!("{BASE}{extra}"))?, format!("kubernetes: {why}"));
    }
    let tc = BASE.replace("{ cidr: 10.9.0.0/24 }", "{ cidr: 10.9.0.0/24, tc: { delay: 50ms } }");
    assert_eq!(
        refusal(&tc)?,
        "kubernetes: link impairment (`tc`) needs a router in the path; Kubernetes has none"
    );
    Ok(())
}

#[test]
fn a_gateway_machine_is_refused_in_the_plans_words() -> Outcome {
    // The plan words it for any target that lacks it, so Kubernetes says it as it is.
    let gateway = BASE
        .replace("{ cidr: 10.9.0.0/24 }", "{ cidr: 10.9.0.0/24, gateway: web }")
        .replace("{ lan: 10 }", "{ lan: 1 }");
    let why = "network `lan` is routed by a `gateway` machine, which this target doesn't route through (`isoloom matrix` lists the targets that do)";
    assert_eq!(refusal(&gateway)?, format!("kubernetes: {why}"));
    assert_eq!(KubernetesBackend.refusal(&parsed(&gateway)?).as_deref(), Some(why));
    Ok(())
}

/// An example Kubernetes can't run is refused by the plan before rendering, and the rendering
/// fails with the plan's refusal; one it runs isn't refused.
#[test]
fn the_refusal_comes_from_the_plan_without_rendering() -> Outcome {
    let tuned = tuned_web_container()?;
    let why = KubernetesBackend.refusal(&tuned).ok_or("not refused")?;
    assert_eq!(why, "machine `web` sets `ulimits`: Kubernetes has no per-container process limits");
    let err = KubernetesBackend.render(&tuned).err().ok_or("rendered")?;
    assert!(
        matches!(
            err,
            RenderError::Refused {
                target: Target::Kubernetes,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(err.to_string(), format!("kubernetes: {why}"));
    assert_eq!(KubernetesBackend.refusal(&plan("hello-stack")?), None);
    Ok(())
}
