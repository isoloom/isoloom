use super::{Problem, validate};

fn spec(yaml: &str) -> crate::model::Spec {
    crate::testing::model(yaml).unwrap()
}

#[test]
fn a_problem_knows_its_section() {
    assert_eq!(Problem::new("machines.web.depends_on[0]", "x").section(), "machines");
    assert_eq!(Problem::new("checks[2].http", "x").section(), "checks");
    assert_eq!(Problem::new("name", "x").section(), "name");
}

#[test]
fn sections_report_in_a_fixed_order() {
    // One problem in several sections: they come out in the validators' order, whatever the
    // order of the keys in the file.
    let s = spec(
        "machines:\n  web: { networks: { lab: 255 }, docker: { image: nginx } }\nreach: [{ from: lab, to: lab }]\nnetworks: { lab: { cidr: 10.9.0.0/24, tc: { loss: 200 } } }\nname: Bad\nversion: 2\n",
    );
    let at: Vec<String> = validate(&s).into_iter().map(|p| p.at).collect();
    assert_eq!(at, ["version", "name", "reach[0]", "machines.web.networks.lab", "networks.lab.tc.loss"]);
}
