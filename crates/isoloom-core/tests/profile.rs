//! Profiles: one file per customer or site, merged into the spec and the defaults, checked
//! against the spec, and pinning a target.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::{Path, PathBuf};

use isoloom_core::profile::{self, Profile};
use isoloom_core::{Target, defaults, load_profiled};

const SPEC: &str = "version: 1
name: shop
inputs: [REGISTRY]
networks:
  app: { cidr: 10.60.0.0/24 }
machines:
  web:
    networks: { app: 10 }
    services: [{ port: 80 }]
    docker: { image: nginx }
    vm: { os: debian-12 }
";

/// A project holding SPEC and the given profiles.
fn project(name: &str, profiles: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("isoloom-profile-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(profile::DIR)).unwrap();
    std::fs::write(dir.join("isoloom.yml"), SPEC).unwrap();
    for (n, text) in profiles {
        std::fs::write(dir.join(profile::DIR).join(format!("{n}.yml")), text).unwrap();
    }
    dir
}

fn parse(text: &str) -> Profile {
    profile::parse("p", PathBuf::from("profiles/p.yml"), text).unwrap()
}

fn spec_doc() -> serde_yaml_ng::Value {
    serde_yaml_ng::from_str(SPEC).unwrap()
}

#[test]
fn a_profile_changes_the_spec_and_set_still_wins() {
    let dir = project("merge", &[("big", "spec:\n  machines: { web: { resources: { cpus: 4, memory_mb: 8192 } } }\n")]);
    let p = profile::load(&dir, "big").unwrap();
    let spec = load_profiled(&dir, Some(&p), &[]).unwrap();
    let r = spec.machines["web"].resources.as_ref().unwrap();
    assert_eq!((r.cpus, r.memory_mb), (Some(4), Some(8192)));

    let spec = load_profiled(&dir, Some(&p), &["machines.web.resources.cpus=2".into()]).unwrap();
    let r = spec.machines["web"].resources.as_ref().unwrap();
    assert_eq!((r.cpus, r.memory_mb), (Some(2), Some(8192)), "-s wins over the profile, leaf by leaf");

    let plain = load_profiled(&dir, None, &[]).unwrap();
    assert!(plain.machines["web"].resources.is_none(), "without the profile, the spec as written");
}

#[test]
fn a_profile_is_a_defaults_layer_between_the_project_file_and_set() {
    let dir = project(
        "defaults",
        &[("site", "defaults:\n  cloud: { linode: { region: se-sto }, oci: { region: eu-stockholm-1 } }\n")],
    );
    std::fs::write(
        dir.join(defaults::PROJECT_FILE),
        "cloud: { linode: { region: us-east }, oci: { region: us-ashburn-1 } }\n",
    )
    .unwrap();
    let p = profile::load(&dir, "site").unwrap();
    let r = defaults::load_profiled(&dir, None, Some(&p), &["defaults.cloud.oci.region=eu-paris-1".into()]).unwrap();
    assert_eq!(
        r.defaults.cloud["linode"].region.as_deref(),
        Some("se-sto"),
        "the profile beats the project's file"
    );
    assert_eq!(r.defaults.cloud["oci"].region.as_deref(), Some("eu-paris-1"), "-s beats the profile");
    let source = &r.sources.iter().find(|(k, _)| k == "cloud.linode.region").unwrap().1;
    assert!(source.starts_with("profile site ("), "{source}");
}

#[test]
fn a_profile_names_only_what_the_spec_has() {
    let p =
        parse("spec:\n  machines: { webz: { resources: { cpus: 2 } } }\n  networks: { lan: { internet: false } }\ninputs: { REGISTRY: r.local, NOPE: x }\n");
    let problems = p.check(&spec_doc());
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems[0].contains("`spec.machines.webz`: the spec has no machine `webz`"), "{problems:?}");
    assert!(problems[1].contains("`spec.networks.lan`: the spec has no network `lan`"), "{problems:?}");
    assert!(problems[2].contains("input `NOPE`"), "{problems:?}");

    let dir = project("unknown", &[("bad", "spec:\n  machines: { webz: { resources: { cpus: 2 } } }\n")]);
    let p = profile::load(&dir, "bad").unwrap();
    let e = load_profiled(&dir, Some(&p), &[]).unwrap_err().to_string();
    assert!(e.contains("no machine `webz`"), "loading refuses it too: {e}");
}

#[test]
fn a_profile_file_is_strict() {
    let e = profile::parse("p", PathBuf::from("p.yml"), "targt: docker\n").unwrap_err();
    assert!(e.contains("unknown field `targt`"), "{e}");
    let e = profile::parse("p", PathBuf::from("p.yml"), "target: moon\n").unwrap_err();
    assert!(e.contains("moon"), "{e}");
    let e = profile::parse("p", PathBuf::from("p.yml"), "inputs: { REGISTRY: [a, b] }\n").unwrap_err();
    assert!(e.contains("single value"), "{e}");
    let p = parse("inputs: { PORT: 8080, DEBUG: true }\n");
    assert_eq!(p.inputs["PORT"], "8080");
    assert_eq!(p.inputs["DEBUG"], "true");
}

#[test]
fn a_profile_pins_its_target() {
    let p = parse("target: proxmox\n");
    assert_eq!(p.target, Some(Target::Proxmox));
    assert_eq!(p.pin(None).unwrap().as_deref(), Some("proxmox"));
    assert_eq!(p.pin(Some("proxmox")).unwrap().as_deref(), Some("proxmox"));
    assert!(p.pin(Some("docker")).unwrap_err().contains("runs on `proxmox`, not `docker`"));
    let any = parse("description: no target\n");
    assert_eq!(any.pin(Some("docker")).unwrap().as_deref(), Some("docker"));
    assert_eq!(any.pin(None).unwrap(), None);
}

#[test]
fn profiles_are_listed_and_found_by_name() {
    let dir = project("list", &[("b-site", "{}\n"), ("a-site", "{}\n")]);
    std::fs::write(dir.join(profile::DIR).join("Not_A_Name.yml"), "{}\n").unwrap();
    std::fs::write(dir.join(profile::DIR).join("notes.txt"), "").unwrap();
    assert_eq!(profile::list(&dir), ["a-site", "b-site"]);
    let e = profile::load(&dir, "c-site").unwrap_err();
    assert!(e.contains("no profile `c-site`") && e.contains("a-site, b-site"), "{e}");
    assert!(profile::load(&dir, "../etc").unwrap_err().contains("kebab-case"));
    std::fs::write(dir.join(profile::DIR).join("a-site.yaml"), "{}\n").unwrap();
    assert!(profile::load(&dir, "a-site").unwrap_err().contains("keep one"));
    assert_eq!(profile::list(Path::new("/nonexistent")), [] as [std::string::String; 0]);
}
