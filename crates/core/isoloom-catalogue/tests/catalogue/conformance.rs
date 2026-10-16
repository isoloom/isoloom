//! Each case of a contract as a lab: the service machine with the case's settings, its helpers,
//! the probe, the checks filled in for the case, and each fact as a check of its own.

use isoloom_catalogue::conformance::{LabError, MACHINE, PROBE};
use isoloom_catalogue::{CaseName, Catalogue};
use isoloom_spec::model::Check;

fn nginx() -> Option<(&'static Catalogue, &'static isoloom_catalogue::Entry)> {
    let catalogue = Catalogue::embedded().ok()?;
    Some((catalogue, catalogue.entry("nginx")?))
}

#[test]
fn a_case_is_a_lab_of_the_service_its_probe_and_its_checks() {
    let (catalogue, nginx) = nginx().unwrap();
    let release = nginx.definition().release(&nginx.definition().default).unwrap();
    let case: CaseName = "site".parse().unwrap();
    let lab = nginx.conformance_lab(catalogue, &case, release, "debian@12".parse().unwrap()).unwrap();
    let spec = isoloom_spec::syntax::parse(&lab.spec).unwrap();
    let web = &spec.machines[MACHINE];
    assert!(matches!(&web.kind, isoloom_spec::syntax::Kind::Service(s) if s.services[0].as_str() == "nginx@1.30.6"));
    assert!(spec.machines[PROBE].access);
    assert_eq!(lab.project.get("site/index.html").map(String::as_str), Some("parity-ok\n"));
    assert_eq!(
        lab.checks,
        ["the port answers", "nginx reports the release asked for", "the root's index is what is served"]
    );
    assert_eq!(lab.facts, ["the OS", "nginx's build"]);
    let declared: Vec<_> = spec
        .checks
        .iter()
        .filter_map(|c| if let Check::Declared(d) = c { Some(d) } else { None })
        .collect();
    assert_eq!(declared[0].http.as_deref(), Some("http://web:80/"), "the default port filled in");
    assert_eq!(declared[0].from.as_deref(), Some(PROBE));
    assert_eq!(
        (declared[1].exec.as_deref(), declared[1].from.as_deref()),
        (Some("nginx -v 2>&1"), Some(MACHINE))
    );
    assert_eq!(declared[1].expect, Some(isoloom_spec::Expect::Text("nginx/1.30.6".into())));
    assert_eq!(declared[3].name.as_deref(), Some("fact: the OS"));
}

#[test]
fn a_case_whose_helper_needs_a_missing_entry_isnt_run_yet() {
    let (catalogue, nginx) = nginx().unwrap();
    let release = nginx.definition().release(&nginx.definition().default).unwrap();
    let case: CaseName = "php".parse().unwrap();
    let wrong = nginx.conformance_lab(catalogue, &case, release, "debian@12".parse().unwrap()).unwrap_err();
    assert!(matches!(&wrong, LabError::MissingEntry { service, .. } if service == "php-fpm"), "{wrong}");
}

#[test]
fn the_default_case_lab_of_nginx() {
    let (catalogue, nginx) = nginx().unwrap();
    let release = nginx.definition().release(&nginx.definition().default).unwrap();
    let lab = nginx
        .conformance_lab(catalogue, &"default".parse().unwrap(), release, "debian@12".parse().unwrap())
        .unwrap();
    insta::assert_snapshot!(lab.spec);
}
