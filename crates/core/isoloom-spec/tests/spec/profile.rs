//! Profiles: one file per customer or site, merged into the spec and the defaults, checked
//! against the spec, and pinning a target.

use std::path::PathBuf;

use isoloom_spec::host::Host;
use isoloom_spec::profile::{self, Profile};
use isoloom_spec::{Error, Target, defaults, load_profiled};

use crate::testing::Outcome;

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

/// A project holding SPEC and the given profiles, in a fresh folder removed when it goes out of
/// scope.
fn project(name: &str, profiles: &[(&str, &str)]) -> Outcome<tempfile::TempDir> {
    let scratch = tempfile::Builder::new().prefix(&format!("isoloom-profile-{name}-")).tempdir()?;
    let dir = scratch.path();
    std::fs::create_dir_all(dir.join(profile::DIR))?;
    std::fs::write(dir.join("isoloom.yml"), SPEC)?;
    for (n, text) in profiles {
        std::fs::write(dir.join(profile::DIR).join(format!("{n}.yml")), text)?;
    }
    Ok(scratch)
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
    let dir = dir.path();
    let p = profile::load(dir, "big")?;
    assert_eq!(web_size(&load_profiled(dir, Some(&p), &[])?), Some((Some(4), Some(8192))));
    let set = load_profiled(dir, Some(&p), &["machines.web.resources.cpus=2".into()])?;
    assert_eq!(web_size(&set), Some((Some(2), Some(8192))), "-s wins over the profile, leaf by leaf");
    let plain = load_profiled(dir, None, &[])?;
    assert_eq!(web_size(&plain), None, "without the profile, the spec as written");
    Ok(())
}

#[test]
fn a_profile_is_a_defaults_layer_between_the_project_file_and_set() -> Outcome {
    let dir = project(
        "defaults",
        &[("site", "defaults:\n  cloud: { linode: { region: se-sto }, oci: { region: eu-stockholm-1 } }\n")],
    )?;
    let dir = dir.path();
    std::fs::write(
        dir.join(defaults::PROJECT_FILE),
        "cloud: { linode: { region: us-east }, oci: { region: us-ashburn-1 } }\n",
    )?;
    let p = profile::load(dir, "site")?;
    let sets = ["defaults.cloud.oci.region=eu-paris-1".to_owned()];
    let r = defaults::load(&defaults::Sources {
        project: dir,
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
    let dir = dir.path();
    let bad = profile::load(dir, "bad")?;
    let refused = load_profiled(dir, Some(&bad), &[]).unwrap_err().to_string();
    assert!(refused.contains("no machine `webz`"), "loading refuses it too: {refused}");
    Ok(())
}

#[test]
fn a_profile_gives_inputs_the_spec_declares_as_a_map() -> Outcome {
    let spec: serde_yaml_ng::Value = serde_yaml_ng::from_str(&SPEC.replace(
        "inputs: [REGISTRY]",
        "inputs: { REGISTRY: { sensitive: false, description: the mirror }, TOKEN: {} }",
    ))?;
    let p = parse("inputs: { REGISTRY: r.local, TOKEN: t, NOPE: x }\n")?;
    let lines: Vec<String> = p.check(&spec).iter().map(ToString::to_string).collect();
    assert_eq!(lines, ["profile `p`: input `NOPE` isn't one of the spec's `inputs`"]);
    Ok(())
}

#[test]
fn a_profile_file_is_strict() -> Outcome {
    let refused = |text: &str| profile::parse("p", PathBuf::from("p.yml"), text).unwrap_err();
    for (text, word) in [("targt: docker\n", "unknown field `targt`"), ("target: moon\n", "moon")] {
        let e = refused(text);
        assert!(matches!(e, Error::ProfileYaml { .. }) && e.to_string().contains(word), "{text}: {e}");
    }
    let list = refused("inputs: { REGISTRY: [a, b] }\n");
    assert!(matches!(&list, Error::ProfileInput { input, .. } if input == "REGISTRY"), "{list}");
    assert!(list.to_string().contains("single value"), "{list}");
    for (text, section) in [("spec: 5\n", "spec"), ("defaults: [a]\n", "defaults")] {
        let e = refused(text);
        assert!(matches!(e, Error::ProfileMapping { key, .. } if key == section), "{text}: {e}");
    }
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
    let dir = dir.path();
    std::fs::write(dir.join(profile::DIR).join("Not_A_Name.yml"), "{}\n")?;
    std::fs::write(dir.join(profile::DIR).join("notes.txt"), "")?;
    let listed = profile::list(dir)?;
    assert_eq!(listed.names, ["a-site", "b-site"]);
    assert_eq!(listed.skipped, [dir.join(profile::DIR).join("Not_A_Name.yml")], "said, not dropped");
    let missing = profile::load(dir, "c-site").unwrap_err().to_string();
    assert!(missing.contains("no profile `c-site`") && missing.contains("a-site, b-site"), "{missing}");
    assert!(profile::load(dir, "../etc").unwrap_err().to_string().contains("kebab-case"));
    for wrong in ["a--b", "-a", "a-", "A-site", &"a".repeat(64)] {
        let e = profile::load(dir, wrong).unwrap_err().to_string();
        assert!(e.contains("kebab-case"), "{wrong}: {e}");
    }
    assert!(profile::load(dir, &"a".repeat(63)).unwrap_err().to_string().contains("no profile"));
    std::fs::write(dir.join(profile::DIR).join("a-site.yaml"), "{}\n")?;
    assert!(profile::load(dir, "a-site").unwrap_err().to_string().contains("keep one"));
    assert_eq!(profile::list(&dir.join("nowhere"))?, profile::Listed::default(), "a project without profiles");
    Ok(())
}

/// A `profiles` that is a file can't be listed, for any user (root included, who reads a folder
/// whatever its mode): an error, not "no profiles".
#[cfg(unix)]
#[test]
fn a_profiles_folder_that_cant_be_read_is_an_error_not_no_profiles() -> Outcome {
    let dir = project("unreadable", &[])?;
    let dir = dir.path();
    let folder = dir.join(profile::DIR);
    std::fs::remove_dir(&folder)?;
    std::fs::write(&folder, "not a folder\n")?;
    let e = profile::list(dir).expect_err("a profiles file isn't a folder of profiles").to_string();
    assert!(e.starts_with("can't read ") && e.contains("profiles"), "{e}");
    Ok(())
}
