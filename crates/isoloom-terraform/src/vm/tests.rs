//! The VM core on small plans: the transports split the steps at a reboot, the limits refuse in
//! their order, the hooks land where they say.

use isoloom_plan::{Environment, Step};
use isoloom_render::hcl_builder::{Module, Template, string};
use isoloom_spec::model::Target;

use super::cloud_init::{self, CloudConfig};
use super::limits::Limits;
use super::linux::{self, Hooks};
use super::module::Variable;

const SPEC: &str = "
version: 1
name: core
inputs: [TOKEN]
networks:
  lab: { cidr: 10.9.0.0/24, internet: false }
machines:
  a:
    networks: { lab: 10 }
    services: [{ port: 80 }]
    inputs: [TOKEN]
    vm: { os: debian-12, provision: [one.sh, reboot, two.sh] }
  b:
    networks: { lab: 11 }
    depends_on: [a]
    vm: { os: debian-12, provision: [b.sh] }
";

fn env() -> Environment {
    let spec = isoloom_spec::parse(SPEC).unwrap();
    assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
    Environment::build(&spec, Target::CloudVm, None)
}

fn text(e: &isoloom_render::hcl_builder::Expression) -> String {
    e.to_string()
}

#[test]
fn remote_exec_splits_at_the_reboot_and_runs_the_hooks() {
    let env = env();
    let hooks = Hooks {
        at_boot: vec![Template::text("echo boot")],
        after_reboot: vec![Template::text("echo again")],
        before_egress: vec![Template::text("echo redirect")],
        skip: |s| matches!(s, Step::Routes { .. }),
    };
    let p = linux::provisioner(linux::Machine {
        env: &env,
        name: "a",
        replace_on: string("id"),
        host: string("host"),
        user: "admin".into(),
        hooks,
    });
    assert_eq!(p.parts.len(), 2);
    let first: Vec<String> = p.parts[0].iter().map(text).collect();
    let second: Vec<String> = p.parts[1].iter().map(text).collect();
    assert!(first.iter().any(|c| c.contains("echo boot")) && first.iter().any(|c| c.contains("one.sh")));
    assert!(first.last().unwrap().contains("boot-id"), "{first:?}");
    assert!(
        second[1].contains("did not restart") && second[2].contains("echo again") && second[3].contains("echo boot"),
        "{second:?}"
    );
    let redirect = second.iter().position(|c| c.contains("echo redirect")).unwrap();
    let egress = second.iter().position(|c| c.contains("isoloom-egress")).unwrap();
    assert!(second.iter().position(|c| c.contains("two.sh")).unwrap() < redirect && redirect < egress);
    // The inputs are uploaded with the project; `b` waits for `a`'s set-up.
    assert_eq!(p.uploads.len(), 2);
    let b = linux::depends_on(&env, "b");
    assert_eq!(b.iter().map(text).collect::<Vec<_>>(), ["terraform_data.a"]);
}

#[test]
fn cloud_init_runs_the_rest_at_the_next_boot() {
    let env = env();
    let mut c = CloudConfig::new("a", string("users"));
    cloud_init::machine(&env, "a", &mut c);
    assert!(c.restart);
    assert!(c.runcmd.iter().any(|r| r.contains("one.sh")) && !c.runcmd.iter().any(|r| r.contains("two.sh")));
    assert!(c.runcmd.last().unwrap().contains("next-boot"));
    // The inputs file, the unit, its script and one part.
    assert_eq!(c.files.len(), 4);
    let doc = text(&c.document());
    assert!(doc.starts_with("format(\"#cloud-config\\n%s\", yamlencode({"), "{doc}");
    assert!(doc.contains("power_state"), "{doc}");
}

#[test]
fn limits_refuse_in_their_order() {
    let spec = isoloom_spec::parse(&SPEC.replace("internet: false }", "internet: false, tc: { delay: 10ms } }")).unwrap();
    let env = Environment::build(&spec, Target::CloudVm, None);
    let limits = Limits {
        arm64: Some("no arm64"),
        gateways: Some("no gateways"),
        playbook_checks: None,
        tc: Some("no tc"),
        ipv6: None,
    };
    assert_eq!(limits.refusal(&env).as_deref(), Some("no tc"));
    assert_eq!(Limits::default().refusal(&env), None);
}

#[test]
fn a_variable_writes_its_attributes_in_order() {
    let mut m = Module::new("");
    m.block(Variable::new("x", "string").default(string("")).sensitive().describe("d").block());
    assert_eq!(
        m.render().unwrap(),
        "variable \"x\" {\n  type        = string\n  default     = \"\"\n  sensitive   = true\n  description = \"d\"\n}\n"
    );
}

#[test]
fn a_machine_waiting_for_secrets_announces_its_address_first() {
    let spec =
        isoloom_spec::parse(&format!("{SPEC}secrets:\n  db: {{ env: DB }}\n").replace("    inputs: [TOKEN]\n", "    inputs: [TOKEN]\n    secrets: [db]\n"))
            .unwrap();
    assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
    let env = Environment::build(&spec, Target::CloudVm, None);
    let machine = |name| {
        linux::provisioner(linux::Machine {
            env: &env,
            name,
            replace_on: string("id"),
            host: isoloom_render::hcl_builder::reference("aws_instance.a.public_ip"),
            user: "admin".into(),
            hooks: Hooks::default(),
        })
    };
    let a = machine("a");
    assert_eq!(a.announce.as_deref(), Some("a"));
    assert!(machine("b").announce.is_none());
    let first: Vec<String> = a.parts[0].iter().map(text).collect();
    assert!(first.iter().any(|c| c.contains("/var/lib/isoloom/secrets/.delivered")), "{first:?}");
    let mut m = Module::new("");
    m.block(a.block());
    let hcl = m.render().unwrap();
    let announce =
        hcl.find("mkdir -p \\\"${path.module}/.isoloom-reach\\\" && echo \\\"admin@${aws_instance.a.public_ip}\\\" > \\\"${path.module}/.isoloom-reach/a\\\"");
    let upload = hcl.find("provisioner \"file\"");
    assert!(announce.is_some() && announce < upload, "{hcl}");
}
