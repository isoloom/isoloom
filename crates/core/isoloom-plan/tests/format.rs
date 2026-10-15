//! A lab's problems and the targets its machines' forms allow (the examples are checked in the
//! `examples` test).

use isoloom_plan::Lab;
use isoloom_spec::syntax::{self, Code, Problem, Problems};
use isoloom_spec::{LoadError, Target};

fn ids(targets: Vec<Target>) -> Vec<&'static str> {
    targets.into_iter().map(Target::id).collect()
}

/// The lab of `yaml`, or the problems that stop it from being read.
fn lab(yaml: &str) -> Result<Lab, Problems> {
    Lab::parse(yaml)
}

fn problems(yaml: &str) -> Vec<String> {
    isoloom_plan::problems_in(yaml).iter().map(Problem::line).collect()
}

/// The problems the parser refuses `yaml` with.
fn refused(yaml: &str) -> Vec<Problem> {
    syntax::parse(yaml).err().unwrap_or_default()
}

const BASE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n";

#[test]
fn asking_for_an_impossible_target_names_the_machines() {
    let p = problems(&format!(
        "{BASE}targets: [docker]\nmachines:\n  dc: {{ networks: {{ lab: 5 }}, ports: {{ ldap: 389 }}, os: windows-server@2022, app: {{ steps: [{{ run: p.ps1 }}] }} }}\n"
    ));
    assert_eq!(p, ["targets[0]: `docker` needs a `docker:` implementation on every machine; missing on: dc"]);
}

#[test]
fn narrowing_targets_keeps_only_those() {
    let lab = lab(&format!(
        "{BASE}targets: [docker, vagrant]\nmachines:\n  a: {{ networks: {{ lab: 5 }}, os: debian@12, app: {{ image: nginx, steps: [{{ run: p.sh }}] }} }}\n"
    ))
    .unwrap();
    assert_eq!(ids(lab.possible()), ["docker", "vagrant"]);
}

#[test]
fn addresses_and_networks_are_checked() {
    let p = problems(
        "version: 1\nname: t\nnetworks:\n  a: { cidr: 8.8.8.0/24 }\n  b: { cidr: 10.1.0.0/24 }\n  c: { cidr: 10.1.0.0/25 }\nmachines:\n  x: { networks: { b: 1 }, app: { image: nginx } }\n  y: { networks: { b: 7, nope: 3 }, app: { image: nginx } }\n  z: { networks: { b: 7 }, app: { image: nginx } }\n",
    );
    assert!(p.iter().any(|m| m.starts_with("networks.a.cidr: use a private block")), "{p:?}");
    assert!(p.iter().any(|m| m == "networks.c.cidr: overlaps network `b`"), "{p:?}");
    assert!(p.iter().any(|m| m.starts_with("machines.x.networks.b: 1 isn't a usable address")), "{p:?}");
    assert!(p.iter().any(|m| m == "machines.y.networks.nope: no network named `nope`"), "{p:?}");
    assert!(p.iter().any(|m| m == "machines.z.networks.b: address .7 is already used by `y`"), "{p:?}");
}

#[test]
fn dependencies_must_exist_have_services_and_not_loop() {
    let p = problems(&format!(
        "{BASE}machines:\n  a: {{ networks: {{ lab: 5 }}, ports: {{ one: 1 }}, depends_on: [b], app: {{ image: x }} }}\n  b: {{ networks: {{ lab: 6 }}, ports: {{ two: 2 }}, depends_on: [a], app: {{ image: x }} }}\n  c: {{ networks: {{ lab: 7 }}, depends_on: [ghost], app: {{ image: x }} }}\n"
    ));
    assert!(p.iter().any(|m| m == "machines: depends_on forms a cycle: a -> b -> a"), "{p:?}");
    assert!(p.iter().any(|m| m == "machines.c.depends_on[0]: no machine named `ghost`"), "{p:?}");
}

#[test]
fn inputs_reach_only_machines_that_declare_them() {
    let found = lab(&format!(
        "{BASE}inputs: [TOKEN]\nmachines:\n  a: {{ networks: {{ lab: 5 }}, files: [{{ path: /run/secret, input: SECRET }}], app: {{ image: x }} }}\n"
    ))
    .unwrap()
    .problems();
    let found: Vec<(Code, &str)> = found.iter().map(|p| (p.code, p.path.as_str())).collect();
    assert_eq!(found, [(Code::UnknownReference, "machines.a.files[0].input")]);
}

#[test]
fn unknown_fields_are_rejected() {
    let found = refused(&format!("{BASE}machines:\n  a: {{ networks: {{ lab: 5 }}, flag: x, app: {{ image: x }} }}\n"));
    let found: Vec<(Code, &str)> = found.iter().map(|p| (p.code, p.path.as_str())).collect();
    assert_eq!(found, [(Code::UnknownField, "machines.a.flag")]);
}

#[test]
fn a_machine_needs_an_os_unless_it_is_the_access_machine() {
    let p = problems(&format!(
        "{BASE}machines:\n  a: {{ networks: {{ lab: 5 }} }}\n  u: {{ access: true, networks: {{ lab: 6 }} }}\n"
    ));
    assert!(
        p.contains(&"machines.a.os: a machine says what it is: its OS, plus a service or an app".to_owned()),
        "{p:?}"
    );
    assert!(!p.iter().any(|m| m.starts_with("machines.u")), "{p:?}");
}

#[test]
fn spec_file_is_isoloom_yml_or_yaml_but_not_both() {
    let dir = std::env::temp_dir().join(format!("isoloom-find-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(matches!(isoloom_spec::find(&dir), Err(LoadError::NotFound(_))));
    std::fs::write(dir.join("isoloom.yaml"), "x").unwrap();
    assert!(isoloom_spec::find(&dir).unwrap().ends_with("isoloom.yaml"));
    std::fs::write(dir.join("isoloom.yml"), "x").unwrap();
    assert!(matches!(isoloom_spec::find(&dir), Err(LoadError::Ambiguous(_))));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_last_address_is_reserved_for_the_router() {
    let p = problems(&format!("{BASE}machines:\n  a: {{ networks: {{ lab: 254 }}, app: {{ image: x }} }}\n"));
    assert!(p.iter().any(|m| m.starts_with("machines.a.networks.lab: 254 isn't a usable address")), "{p:?}");
}

#[test]
fn a_gateway_is_a_machine_on_the_network_at_the_gateway_address() {
    let base = |nets: &str, fw: &str| {
        format!(
            "version: 1\nname: t\nnetworks:\n{nets}\nmachines:\n  fw: {{ networks: {fw}, app: {{ image: a }} }}\n  web: {{ networks: {{ dmz: 10 }}, app: {{ image: a }} }}\n"
        )
    };
    let ok = base("  out: { cidr: 10.1.0.0/24 }\n  dmz: { cidr: 10.1.1.0/24, gateway: fw }", "{ out: 2, dmz: 1 }");
    assert_eq!(problems(&ok), Vec::<String>::new());

    let unknown = base("  dmz: { cidr: 10.1.1.0/24, gateway: nope }", "{ dmz: 2 }");
    assert!(
        problems(&unknown)
            .iter()
            .any(|p| p.starts_with("networks.dmz.gateway: no machine named `nope`"))
    );

    let detached = base("  out: { cidr: 10.1.0.0/24 }\n  dmz: { cidr: 10.1.1.0/24, gateway: fw }", "{ out: 2 }");
    assert!(problems(&detached).iter().any(|p| p.contains("attach `fw` to `dmz` at the gateway address")));

    let wrong_octet = base("  dmz: { cidr: 10.1.1.0/24, gateway: fw }", "{ dmz: 5 }");
    assert!(problems(&wrong_octet).iter().any(|p| p.contains("takes the gateway address: use 1")));

    // Only the gateway may take .1.
    let squatter = "version: 1\nname: t\nnetworks:\n  a: { cidr: 10.1.0.0/24 }\nmachines:\n  m: { networks: { a: 1 }, app: { image: a } }\n";
    assert!(problems(squatter).iter().any(|p| p.contains("isn't a usable address")));
}

#[test]
fn machines_behind_a_gateway_start_after_it() {
    let looped = "version: 1\nname: t\nnetworks:\n  dmz: { cidr: 10.1.1.0/24, gateway: fw }\nmachines:\n  fw: { networks: { dmz: 1 }, depends_on: [web], app: { image: a } }\n  web: { networks: { dmz: 10 }, ports: { web: 80 }, app: { image: a } }\n";
    assert!(problems(looped).iter().any(|p| p.contains("cycle: fw -> web -> fw")));
}

#[test]
fn volumes_are_absolute_paths_named_in_kebab_case() {
    let p = problems(&format!(
        "{BASE}machines:\n  a: {{ networks: {{ lab: 5 }}, volumes: {{ data: /data, Bad: /x, rel: data/x, twice: /data/, spaced: \"/var/my data\", coloned: \"/var/a:b\" }}, app: {{ image: x }} }}\n"
    ));
    assert!(p.iter().any(|m| m.starts_with("machines.a.volumes.Bad: volume names are kebab-case")), "{p:?}");
    assert!(p.iter().any(|m| m.contains("`data/x` isn't a safe absolute path")), "{p:?}");
    assert!(p.iter().any(|m| m.contains("`/data/` is already a volume")), "{p:?}");
    // Spaces and ':' corrupt the Docker short mount syntax and the provisioners' `mkdir -p`.
    assert!(p.iter().any(|m| m.contains("`/var/my data` isn't a safe absolute path")), "{p:?}");
    assert!(p.iter().any(|m| m.contains("`/var/a:b` isn't a safe absolute path")), "{p:?}");
}

#[test]
fn names_leave_room_for_the_kubernetes_affixes() {
    // The name becomes `isoloom-<name>` (namespace): 56 characters overflows the 63-char label.
    let long = "a".repeat(56);
    let p = problems(&format!(
        "version: 1\nname: {long}\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  a: {{ networks: {{ lab: 5 }}, app: {{ image: x }} }}\n"
    ));
    assert!(p.iter().any(|m| m.contains("name: at most 55 characters")), "{p:?}");

    // A 54-char machine name overflows `<name>-published`; a machine+volume pair overflows the claim.
    let m54 = "m".repeat(54);
    let v60 = "v".repeat(63);
    let overflow = problems(&format!(
        "{BASE}machines:\n  {m54}: {{ networks: {{ lab: 5 }}, app: {{ image: x }} }}\n  b: {{ networks: {{ lab: 6 }}, volumes: {{ {v60}: /data }}, app: {{ image: x }} }}\n"
    ));
    assert!(overflow.iter().any(|m| m.contains("at most 53 characters")), "{overflow:?}");
    assert!(overflow.iter().any(|m| m.contains("at most 62 characters")), "{overflow:?}");
    // A volume name that fits the claim but not its seed init container (`seed-<volume>`).
    let v59 = "v".repeat(59);
    let seeded = problems(&format!(
        "{BASE}machines:\n  b: {{ networks: {{ lab: 6 }}, volumes: {{ {v59}: /data }}, app: {{ image: x }} }}\n"
    ));
    assert!(seeded.iter().any(|m| m.contains("at most 58 characters")), "{seeded:?}");
    let v58 = "v".repeat(58);
    let fits = problems(&format!(
        "{BASE}machines:\n  b: {{ networks: {{ lab: 6 }}, volumes: {{ {v58}: /data }}, app: {{ image: x }} }}\n"
    ));
    assert!(fits.is_empty(), "{fits:?}");
}

/// A container can be small (a Go service capped at 64 MB); a VM needs room to boot.
#[test]
fn the_size_floor_is_a_vms() {
    let spec = |app: &str| {
        format!(
            "version: 1\nname: t\nnetworks:\n  lan: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  svc:\n    networks: {{ lan: 10 }}\n    resources: {{ cpus: 1, memory_mb: 64 }}\n{app}"
        )
    };
    assert_eq!(problems(&spec("    app: { image: nginx }\n")), Vec::<String>::new());
    let both = problems(&spec("    os: debian@12\n    app: { image: nginx, steps: [{ run: x.sh }] }\n"));
    assert!(both.iter().any(|p| p.contains("too small for a VM")), "{both:?}");
    let tiny = problems(&spec("    app: { image: nginx }\n").replace("memory_mb: 64", "memory_mb: 8"));
    assert!(tiny.iter().any(|p| p.contains("at least 1 cpu and 16 MB")), "{tiny:?}");
}

/// Cloud-services variables take any value, and `{{ inputs.NAME }}` a declared launch-time input.
#[test]
fn cloud_vars_are_typed_and_can_take_inputs() {
    let base = "version: 1\nname: t\ncloud:\n  provider: aws\n  terraform: tf\n";
    let ok = lab(&format!(
        "{base}  vars: {{ enabled: true, count: 3, ips: [\"10.0.0.1/32\"], cidr: \"{{{{ inputs.CLIENT_CIDR }}}}\" }}\ninputs: [CLIENT_CIDR]\n"
    ))
    .unwrap();
    let vars = &ok.source().cloud.as_ref().unwrap().vars;
    assert_eq!(vars["enabled"], serde_json::json!(true));
    assert_eq!(vars["ips"], serde_json::json!(["10.0.0.1/32"]));
    assert_eq!(isoloom_spec::CloudServices::input_of(&vars["cidr"]), Some("CLIENT_CIDR"));
    assert_eq!(ok.problems(), []);
    // At any depth, inside a string too.
    let nested = lab(&format!(
        "{base}  vars: {{ allow: [\"{{{{ inputs.IP }}}}/32\", {{ v: \"{{{{inputs.NOPE}}}}\" }}] }}\ninputs: [IP]\n"
    ))
    .unwrap();
    let allow = &nested.source().cloud.as_ref().unwrap().vars["allow"];
    assert_eq!(isoloom_spec::CloudServices::inputs_in(allow), ["IP", "NOPE"]);
    let filled = isoloom_spec::CloudServices::fill_inputs(allow, &mut |n| (n == "IP").then(|| "203.0.113.7".to_owned()));
    assert_eq!(filled, serde_json::json!(["203.0.113.7/32", { "v": "{{inputs.NOPE}}" }]));
    assert!(nested.problems().iter().any(|p| p.reason.contains("`NOPE` isn't declared")));
    // Other placeholders stay as written.
    let other = serde_json::json!("{{ cloud.outputs.x }} and {{ inputs.IP }}");
    assert_eq!(isoloom_spec::CloudServices::inputs_in(&other), ["IP"]);
    let kept = isoloom_spec::CloudServices::fill_inputs(&other, &mut |_| Some("1".into()));
    assert_eq!(kept, serde_json::json!("{{ cloud.outputs.x }} and 1"));
    let undeclared = lab(&format!("{base}  vars: {{ cidr: \"{{{{ inputs.NOPE }}}}\" }}\n")).unwrap();
    assert!(undeclared.problems().iter().any(|p| p.reason.contains("`NOPE` isn't declared")));
}
