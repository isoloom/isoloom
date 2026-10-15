use super::{Problem, starts_after, validate};
use crate::testing::{lab, problems};

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

/// Networks `lan` and `dmz` on 10.0.0.0/23, `dmz` given `lan`'s block on Docker, and a machine on
/// both.
const DOCKER_OVERLAP: &str = "networks:\n  lan: { cidr: 10.0.0.0/24 }\n  dmz: { cidr: 10.0.1.0/24, docker: { cidr: 10.0.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10, dmz: 10 }, docker: { image: nginx } }\n";

#[test]
fn docker_blocks_are_compared_once_the_spec_is_otherwise_sound() {
    assert_eq!(
        problems(&format!("version: 1\nname: t\n{DOCKER_OVERLAP}")),
        ["networks.dmz.docker.cidr: on Docker, overlaps network `lan`; choose another block"]
    );
    let other = problems(&format!("version: 1\nname: t\ninputs: [1A]\n{DOCKER_OVERLAP}"));
    assert_eq!(
        other,
        ["inputs[0]: `1A`: inputs are environment variable names (letters, digits and `_`, not starting with a digit)"]
    );
}

#[test]
fn machine_and_network_names_are_dns_labels_starting_with_a_letter() {
    let found = problems(
        "version: 1\nname: t\nnetworks:\n  1lan: { cidr: 10.0.0.0/24 }\n  Dmz: { cidr: 10.0.1.0/24 }\nmachines:\n  1web: { networks: { 1lan: 10 }, docker: { image: nginx } }\n  Db: { networks: { Dmz: 10 }, docker: { image: x } }\n",
    );
    assert_eq!(
        found,
        [
            "networks.1lan: network names are kebab-case and start with a letter",
            "networks.Dmz: network names are kebab-case and start with a letter",
            "machines.1web: machine names are DNS names: kebab-case, starting with a letter",
            "machines.Db: machine names are DNS names: kebab-case, starting with a letter",
        ]
    );
}

#[test]
fn a_machine_starts_after_its_dependencies_gateways_and_volume_owner_once_each() {
    let s = spec(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.0.0.0/24, gateway: r }\n  dmz: { cidr: 10.0.1.0/24, gateway: r }\nvolumes: { www: { owner: nas } }\nmachines:\n  r: { networks: { lan: 1, dmz: 1 }, docker: { image: x } }\n  nas: { networks: { lan: 2 }, docker: { image: x }, volumes: { www: /srv/www } }\n  db: { networks: { lan: 3 }, docker: { image: x }, depends_on: [r] }\n  web: { networks: { lan: 10, dmz: 10 }, docker: { image: nginx }, depends_on: [db], volumes: { www: /var/www } }\n",
    );
    let after = |name: &str| starts_after(&s, name, &s.machines[name]);
    assert_eq!(after("web"), ["db", "r", "nas"]);
    assert_eq!(after("db"), ["r"]);
    assert_eq!(after("nas"), ["r"]);
    assert_eq!(after("r"), Vec::<&str>::new());
}

#[test]
fn a_start_order_cycle_is_named_along_its_path() {
    let chain = |c: &str| {
        let machine =
            |n: u8, after: &str| format!("{{ networks: {{ lan: {n} }}, docker: {{ image: x }}, ports: [{{ port: 80, name: http }}], depends_on: [{after}] }}");
        problems(&lab(&format!(
            "machines:\n  a: {}\n  b: {}\n  c: {}\n",
            machine(10, "b"),
            machine(11, "c"),
            machine(12, c)
        )))
    };
    assert_eq!(chain(""), Vec::<String>::new());
    assert_eq!(chain("b"), ["machines: depends_on forms a cycle: b -> c -> b"]);
}
