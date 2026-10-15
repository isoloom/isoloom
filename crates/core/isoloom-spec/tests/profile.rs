//! Profiles: one file per customer or site, merged into the spec and the defaults, checked
//! against the spec, and pinning a target.

use std::error::Error;
use std::path::{Path, PathBuf};

use isoloom_spec::host::Host;
use isoloom_spec::profile::{self, Profile};
use isoloom_spec::{Target, defaults, load_profiled};

const SPEC: &str = "version: 1
name: shop
inputs: [REGISTRY]
networks:
  app: { cidr: 10.60.0.0/24 }
machines:
  web:
    os: debian@12
    networks: { app: 10 }
    ports: { web: 80 }
    app: { image: nginx }
";

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// A project holding SPEC and the given profiles, in a fresh folder.
fn project(name: &str, profiles: &[(&str, &str)]) -> Outcome<PathBuf> {
    let dir = std::env::temp_dir().join(format!("isoloom-profile-{}-{name}", std::process::id()));
    match std::fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    std::fs::create_dir_all(dir.join(profile::DIR))?;
    std::fs::write(dir.join("isoloom.yml"), SPEC)?;
    for (n, text) in profiles {
        std::fs::write(dir.join(profile::DIR).join(format!("{n}.yml")), text)?;
    }
    Ok(dir)
}

/// A profile read from `text`.
fn parse(text: &str) -> Outcome<Profile> {
    Ok(profile::parse("p", PathBuf::from("profiles/p.yml"), text)?)
}

/// SPEC as YAML, as a profile is checked against it.
fn spec_doc() -> Outcome<serde_yaml_ng::Value> {
    Ok(serde_yaml_ng::from_str(SPEC)?)
}

/// The web machine's CPUs and memory.
fn web_size(spec: &isoloom_spec::Spec) -> Option<(Option<u32>, Option<u32>)> {
    spec.machines.get("web")?.resources.as_ref().map(|r| (r.cpus, r.memory_mb))
}

#[test]
fn a_profile_changes_the_spec_and_set_still_wins() -> Outcome {
    let dir = project("merge", &[("big", "spec:\n  machines: { web: { resources: { cpus: 4, memory_mb: 8192 } } }\n")])?;
    let p = profile::load(&dir, "big")?;
    assert_eq!(web_size(&load_profiled(&dir, Some(&p), &[])?), Some((Some(4), Some(8192))));
    let set = load_profiled(&dir, Some(&p), &["machines.web.resources.cpus=2".into()])?;
    assert_eq!(web_size(&set), Some((Some(2), Some(8192))), "-s wins over the profile, leaf by leaf");
    let plain = load_profiled(&dir, None, &[])?;
    assert_eq!(web_size(&plain), None, "without the profile, the spec as written");
    Ok(())
}

#[test]
fn a_profile_is_a_defaults_layer_between_the_project_file_and_set() -> Outcome {
    let dir = project(
        "defaults",
        &[("site", "defaults:\n  cloud: { linode: { region: se-sto }, oci: { region: eu-stockholm-1 } }\n")],
    )?;
    std::fs::write(
        dir.join(defaults::PROJECT_FILE),
        "cloud: { linode: { region: us-east }, oci: { region: us-ashburn-1 } }\n",
    )?;
    let p = profile::load(&dir, "site")?;
    let sets = ["defaults.cloud.oci.region=eu-paris-1".to_owned()];
    let r = defaults::load(&defaults::Sources {
        project: &dir,
        images_file: None,
        profile: Some(&p),
        sets: &sets,
        host: Host::default(),
    })?;
    assert_eq!(
        r.defaults.cloud["linode"].region.as_deref(),
        Some("se-sto"),
        "the profile beats the project's file"
    );
    assert_eq!(r.defaults.cloud["oci"].region.as_deref(), Some("eu-paris-1"), "-s beats the profile");
    let source = &r.sources.iter().find(|(k, _)| k == "cloud.linode.region").ok_or("no source")?.1;
    assert!(source.starts_with("profile site ("), "{source}");
    Ok(())
}

#[test]
fn a_profile_names_only_what_the_spec_has() -> Outcome {
    let p =
        parse("spec:\n  machines: { webz: { resources: { cpus: 2 } } }\n  networks: { lan: { internet: false } }\ninputs: { REGISTRY: r.local, NOPE: x }\n")?;
    let problems = p.check(&spec_doc()?);
    assert_eq!(problems.len(), 3, "{problems:?}");
    let lines: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert!(lines[0].contains("`spec.machines.webz`: the spec has no machine `webz`"), "{lines:?}");
    assert!(lines[1].contains("`spec.networks.lan`: the spec has no network `lan`"), "{lines:?}");
    assert!(lines[2].contains("input `NOPE`"), "{lines:?}");

    let dir = project("unknown", &[("bad", "spec:\n  machines: { webz: { resources: { cpus: 2 } } }\n")])?;
    let bad = profile::load(&dir, "bad")?;
    let refused = load_profiled(&dir, Some(&bad), &[]).unwrap_err().to_string();
    assert!(refused.contains("no machine `webz`"), "loading refuses it too: {refused}");
    Ok(())
}

#[test]
fn a_profile_file_is_strict() -> Outcome {
    let refused = |text: &str| profile::parse("p", PathBuf::from("p.yml"), text).unwrap_err().to_string();
    let typo = refused("targt: docker\n");
    assert!(typo.contains("unknown field `targt`"), "{typo}");
    let target = refused("target: moon\n");
    assert!(target.contains("moon"), "{target}");
    let list = refused("inputs: { REGISTRY: [a, b] }\n");
    assert!(list.contains("single value"), "{list}");
    let p = parse("inputs: { PORT: 8080, DEBUG: true }\n")?;
    assert_eq!(p.inputs["PORT"], "8080");
    assert_eq!(p.inputs["DEBUG"], "true");
    Ok(())
}

#[test]
fn a_profile_pins_its_target() -> Outcome {
    let p = parse("target: proxmox\n")?;
    assert_eq!(p.target, Some(Target::Proxmox));
    assert_eq!(p.pin(None)?.as_deref(), Some("proxmox"));
    assert_eq!(p.pin(Some("proxmox"))?.as_deref(), Some("proxmox"));
    assert!(p.pin(Some("docker")).unwrap_err().to_string().contains("runs on `proxmox`, not `docker`"));
    let any = parse("description: no target\n")?;
    assert_eq!(any.pin(Some("docker"))?.as_deref(), Some("docker"));
    assert_eq!(any.pin(None)?, None);
    Ok(())
}

#[test]
fn profiles_are_listed_and_found_by_name() -> Outcome {
    let dir = project("list", &[("b-site", "{}\n"), ("a-site", "{}\n")])?;
    std::fs::write(dir.join(profile::DIR).join("Not_A_Name.yml"), "{}\n")?;
    std::fs::write(dir.join(profile::DIR).join("notes.txt"), "")?;
    assert_eq!(profile::list(&dir), ["a-site", "b-site"]);
    let missing = profile::load(&dir, "c-site").unwrap_err().to_string();
    assert!(missing.contains("no profile `c-site`") && missing.contains("a-site, b-site"), "{missing}");
    assert!(profile::load(&dir, "../etc").unwrap_err().to_string().contains("kebab-case"));
    std::fs::write(dir.join(profile::DIR).join("a-site.yaml"), "{}\n")?;
    assert!(profile::load(&dir, "a-site").unwrap_err().to_string().contains("keep one"));
    assert_eq!(profile::list(Path::new("/nonexistent")), Vec::<String>::new());
    Ok(())
}
