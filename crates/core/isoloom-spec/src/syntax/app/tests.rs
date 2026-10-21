use super::*;

fn app(yaml: &str) -> Result<App, String> {
    serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())
}

#[test]
fn an_app_is_an_image_a_build_an_appliance_or_steps() {
    assert_eq!(app("{ image: nginx:1.27 }").unwrap().container, Some(Recipe::Image("nginx:1.27".into())));
    let Some(Recipe::Build(folder)) = app("{ build: build/web }").unwrap().container else {
        panic!()
    };
    assert_eq!((folder.context.as_str(), folder.dockerfile), ("build/web", None));
    let Some(Recipe::Build(full)) = app("{ build: { context: build/web, dockerfile: w.Dockerfile, args: { TAG: x } } }")
        .unwrap()
        .container
    else {
        panic!()
    };
    assert_eq!(full.dockerfile.as_deref(), Some("w.Dockerfile"));
    let appliance = app("{ appliance: cisco-iol, image: vrnetlab/cisco_iol:17, config: r1.cfg }").unwrap();
    assert!(matches!(
        appliance.container,
        Some(Recipe::Appliance(ApplianceRecipe { kind: Appliance::CiscoIol, .. }))
    ));
    let steps = app("{ steps: [{ packages: [nginx] }, { copy: { from: site/, to: /var/www/ } }, { run: p.sh, creates: /done }, reboot, { playbook: p.yml }] }")
        .unwrap()
        .steps;
    assert_eq!(steps.len(), 5);
    assert_eq!(steps[3], Step::Reboot);
    assert!(steps[3].container_refusal().is_some() && steps[0].container_refusal().is_none());
}

#[test]
fn two_recipes_give_each_form_its_own() {
    let a = app("{ image: example/board:2.1, steps: [{ run: provision/board.sh }] }").unwrap();
    assert!(a.container.is_some() && a.steps.len() == 1);
}

#[test]
fn an_image_beside_a_build_tags_the_build_as_that_published_name() {
    let Some(Recipe::Build(b)) = app("{ image: ghcr.io/x/y:1, build: build/web }").unwrap().container else {
        panic!("a build tagged with the published image")
    };
    assert_eq!((b.image.as_deref(), b.context.as_str()), (Some("ghcr.io/x/y:1"), "build/web"));
    let Some(Recipe::Build(full)) = app("{ image: ghcr.io/x/y:1, build: { context: web, target: prod } }").unwrap().container else {
        panic!("the full build keeps its fields and gains the image")
    };
    assert_eq!((full.image.as_deref(), full.target.as_deref()), (Some("ghcr.io/x/y:1"), Some("prod")));
}

#[test]
fn recipes_that_cannot_go_together_are_refused() {
    for (yaml, why) in [
        ("{ appliance: cisco-iol, image: x, build: b }", "not a `build`"),
        ("{ appliance: cisco-iol, build: b }", "not a `build`"),
        ("{}", "needs a recipe"),
        ("{ appliance: cisco-iol }", "runs its `image`"),
        ("{ appliance: cisco-dynamips, image: x, firmware: f }", "no `image`"),
        ("{ appliance: cisco-dynamips }", "needs the `firmware`"),
        ("{ image: a, config: c }", "goes with `appliance`"),
        ("{ image: a, firmware: f }", "cisco-dynamips"),
        ("{ steps: [{ run: a.sh, copy: { from: a, to: /a } }] }", "does one thing"),
        ("{ steps: [{ creates: /a }] }", "goes with `run`"),
        ("{ steps: [a.sh] }", "{ run: a.sh }"),
        ("{ steps: [{ shell: a }] }", "unknown field `shell`"),
    ] {
        let e = app(yaml).unwrap_err();
        assert!(e.contains(why), "{yaml}: {e}");
    }
}

#[test]
fn a_build_or_a_step_of_the_wrong_shape_says_what_is_expected() {
    let build = app("{ build: [a] }").unwrap_err();
    assert!(build.contains("a project folder, or `{ context, dockerfile, target, args }`"), "{build}");
    let step = app("{ steps: [[a]] }").unwrap_err();
    assert!(
        step.contains("`reboot`, or a map with one of `run`, `copy`, `packages`, `playbook`, `repository`"),
        "{step}"
    );
    let nothing = app("{ steps: [{}] }").unwrap_err();
    assert!(nothing.contains("a step needs one of `run`, `copy`, `packages`, `playbook`"), "{nothing}");
}

#[test]
fn a_step_without_a_container_form_says_why() {
    let refusal = |yaml: &str| app(yaml).unwrap().steps[0].container_refusal();
    assert_eq!(refusal("{ steps: [{ playbook: p.yml }] }"), Some("a playbook step runs in a VM only in 1.0"));
    assert_eq!(
        refusal("{ steps: [{ run: iis.PS1 }] }"),
        Some("a PowerShell script means Windows, which has no container form in 1.0")
    );
    assert_eq!(refusal("{ steps: [{ run: web.sh }] }"), None);
    assert_eq!(refusal("{ steps: [{ copy: { from: a, to: /a } }] }"), None);
}
