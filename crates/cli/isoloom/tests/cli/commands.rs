//! What the commands print and the exit code they give: validation, targets, generation with
//! a refused cloud, and profiles.

use crate::testing::{Outcome, example, isoloom, isoloom_with, project, scratch};

#[test]
fn an_example_validates() -> Outcome {
    let home = scratch("example-validates")?;
    let o = isoloom(&home, &[&"validate", &example("hello-stack")])?;
    assert_eq!(o.code, Some(0), "{o:?}");
    assert!(o.stdout.contains("hello-stack is valid"), "{o:?}");
    Ok(())
}

#[test]
fn targets_lists_the_refused_clouds_under_cloud_vm() -> Outcome {
    let home = scratch("targets-refused-clouds")?;
    let o = isoloom(&home, &[&"targets", &example("windows-hello")])?;
    assert_eq!(o.code, Some(0), "{o:?}");
    let (_, after) = o.stdout.split_once("✓ cloud-vm\n").ok_or("cloud-vm isn't generated")?;
    let under: Vec<&str> = after.lines().take_while(|l| l.starts_with("    ")).collect();
    assert_eq!(under.len(), 4, "{}", o.stdout);
    assert!(
        under.first().is_some_and(|l| l.starts_with("    ✗ gcp (") && l.contains("Windows on GCP")),
        "{}",
        o.stdout
    );
    assert!(!o.stdout.contains("✗ aws") && !o.stdout.contains("✗ azure"), "{}", o.stdout);
    Ok(())
}

#[test]
fn targets_gives_the_tier_of_each_target_generated_and_its_evidence() -> Outcome {
    let home = scratch("targets-tiers")?;
    let text = isoloom(&home, &[&"targets", &example("hello-stack")])?;
    assert_eq!(text.code, Some(0), "{text:?}");
    let (_, tiers) = text.stdout.split_once("\nTiers (").ok_or("no tiers after the listing")?;
    assert!(tiers.lines().any(|l| l.starts_with("  docker ")), "{}", text.stdout);
    let o = isoloom(&home, &[&"targets", &example("hello-stack"), &"--json"])?;
    let listed: serde_json::Value = serde_json::from_str(&o.stdout)?;
    let targets = listed.as_array().ok_or("not a list")?;
    assert!(targets.iter().any(|t| t["id"] == "vagrant"), "{listed}");
    for t in targets {
        let entries = t["tiers"].as_array().ok_or("no tiers")?;
        assert!(!entries.is_empty(), "{t}");
        for e in entries {
            assert!(matches!(e["tier"].as_u64(), Some(1..=3)), "{e}");
            assert!(e["evidence"].as_str().is_some_and(|s| !s.is_empty()), "{e}");
            assert_eq!(e["target"], t["id"], "{e}");
        }
    }
    Ok(())
}

#[test]
fn matrix_shows_each_field_against_each_target_with_the_tiers() -> Outcome {
    let home = scratch("matrix")?;
    let text = isoloom(&home, &[&"matrix"])?;
    assert_eq!(text.code, Some(0), "{text:?}");
    assert!(text.stdout.starts_with("Capability matrix: "), "{}", text.stdout);
    assert!(text.stdout.contains("\n  vagrant/libvirt "), "the tiers");
    assert!(text.stdout.contains("\nmachines.*.app.image: "), "a row");
    let json = isoloom(&home, &[&"matrix", &"--json"])?;
    let v: serde_json::Value = serde_json::from_str(&json.stdout)?;
    assert_eq!(v["columns"][0]["id"], "docker", "{v}");
    let row = v["rows"].as_array().and_then(|r| r.first()).ok_or("no rows")?;
    assert_eq!(row["cells"].as_object().map(serde_json::Map::len), v["columns"].as_array().map(Vec::len));
    let page = isoloom(&home, &[&"docs", &"--matrix"])?;
    assert!(page.stdout.starts_with("# Capability matrix\n"), "{}", page.stdout);
    let neither = isoloom(&home, &[&"docs"])?;
    assert_eq!(neither.code, Some(2), "docs needs --spec or --matrix: {neither:?}");
    Ok(())
}

#[test]
fn validate_lists_an_invalid_specs_problems_as_json_and_gives_1() -> Outcome {
    let dir = project(
        "invalid-spec",
        "version: 1\nname: broken\nnetworks:\n  lan: { cidr: 10.0.0.0/24 }\nmachines:\n  a:\n    networks: { wan: 5 }\n    app: { image: alpine }\n",
    )?;
    let o = isoloom(&dir, &[&"validate", &dir, &"--json"])?;
    assert_eq!(o.code, Some(1), "{o:?}");
    let v: serde_json::Value = serde_json::from_str(&o.stdout)?;
    assert_eq!(v["ok"], false);
    assert!(v["problems"].as_array().is_some_and(|p| !p.is_empty()), "{v}");
    Ok(())
}

/// A spec AWS refuses on `cloud-vm` (a /29 network: AWS subnets are /28 or larger) while other
/// clouds run it.
const SMALL_SUBNET: &str = "version: 1\nname: small\nnetworks:\n  lab: { cidr: 10.9.0.0/29 }\nmachines:\n  w:\n    os: debian@12\n    networks: { lab: 2 }\n    app: { steps: [{ run: setup.sh }] }\n";

#[test]
fn a_refusal_by_aws_drops_only_its_module() -> Outcome {
    let dir = project("aws-refuses", SMALL_SUBNET)?;
    std::fs::write(dir.join("setup.sh"), "#!/bin/sh\ntrue\n")?;
    let listed = isoloom(&dir, &[&"targets", &dir])?;
    assert_eq!(listed.code, Some(0), "{listed:?}");
    let (_, after) = listed.stdout.split_once("✓ cloud-vm\n").ok_or("cloud-vm isn't generated")?;
    assert!(after.contains("    ✗ aws (AWS subnets are /28 or larger)"), "{}", listed.stdout);
    let generated = isoloom(&dir, &[&"generate", &dir])?;
    assert_eq!(generated.code, Some(0), "{generated:?}");
    assert!(!dir.join(".isoloom/cloud-vm/aws").exists());
    assert!(dir.join(".isoloom/cloud-vm/gcp/main.tf").exists());
    Ok(())
}

const PROFILED: &str = "version: 1
name: shop
networks:
  app: { cidr: 10.60.0.0/24 }
machines:
  web:
    os: debian@12
    networks: { app: 10 }
    ports: { web: 80 }
    app: { image: nginx, steps: [{ run: setup.sh }] }
";

/// A project with [`PROFILED`] and the given profiles.
fn profiled(name: &str, profiles: &[(&str, &str)]) -> Outcome<std::path::PathBuf> {
    let dir = project(name, PROFILED)?;
    std::fs::write(dir.join("setup.sh"), "apt-get install -y nginx\n")?;
    std::fs::create_dir_all(dir.join("profiles"))?;
    for (n, text) in profiles {
        std::fs::write(dir.join("profiles").join(format!("{n}.yml")), text)?;
    }
    Ok(dir)
}

#[test]
fn profiles_lists_and_checks_each_profile() -> Outcome {
    let home = scratch("profiles-home")?;
    let dir = profiled(
        "profiles-list",
        &[
            (
                "big",
                "description: a bigger web server\ntarget: vagrant\nspec:\n  machines: { web: { resources: { cpus: 4 } } }\n",
            ),
            ("typo", "spec:\n  machines: { wbe: { resources: { cpus: 4 } } }\n"),
            ("nowhere", "target: kubernetes\nspec:\n  machines: { web: { hardware: { tpm: required } } }\n"),
        ],
    )?;
    let o = isoloom(&home, &[&"profiles", &dir])?;
    assert_eq!(o.code, Some(1), "{o:?}");
    assert!(o.stdout.contains("✓ big") && o.stdout.contains("a bigger web server"), "{}", o.stdout);
    assert!(o.stdout.contains("✗ typo") && o.stdout.contains("no machine `wbe`"), "{}", o.stdout);
    assert!(o.stdout.contains("✗ nowhere") && o.stdout.contains("can't run on `kubernetes`"), "{}", o.stdout);

    std::fs::remove_file(dir.join("profiles/typo.yml"))?;
    std::fs::remove_file(dir.join("profiles/nowhere.yml"))?;
    let json = isoloom(&home, &[&"profiles", &dir, &"--json"])?;
    assert_eq!(json.code, Some(0), "{json:?}");
    let v: serde_json::Value = serde_json::from_str(&json.stdout)?;
    assert_eq!(v[0]["name"], "big");
    assert_eq!(v[0]["target"], "vagrant");
    assert_eq!(v[0]["ok"], true);
    Ok(())
}

#[test]
fn a_profile_applies_to_every_command_and_pins_its_target() -> Outcome {
    let home = scratch("profile-apply-home")?;
    let dir = profiled(
        "profile-apply",
        &[("big", "target: vagrant\nspec:\n  machines: { web: { resources: { cpus: 4 } } }\n")],
    )?;
    let asked = isoloom(&home, &[&"resources", &dir, &"--profile", &"big"])?;
    assert!(asked.stdout.contains("4 CPUs"), "{asked:?}");
    let from_env = isoloom_with(&home, &[&"resources", &dir], &[("ISOLOOM_PROFILE", "big")])?;
    assert!(from_env.stdout.contains("4 CPUs"), "ISOLOOM_PROFILE too: {from_env:?}");
    let as_written = isoloom(&home, &[&"resources", &dir])?;
    assert!(as_written.stdout.contains("1 CPU"), "without it, the spec as written: {as_written:?}");

    let pinned = isoloom(&home, &[&"--profile", &"big", &"generate", &dir])?;
    assert_eq!(pinned.code, Some(0), "{pinned:?}");
    assert!(
        pinned.stdout.contains(".isoloom/vagrant/Vagrantfile") && !pinned.stdout.contains("compose.yml"),
        "only the pinned target: {pinned:?}"
    );

    let other = isoloom(&home, &[&"generate", &dir, &"--profile", &"big", &"--target", &"docker"])?;
    assert_eq!(other.code, Some(2), "{other:?}");
    assert!(other.stderr.contains("profile `big` runs on `vagrant`, not `docker`"), "{other:?}");

    let unknown = isoloom(&home, &[&"validate", &dir, &"--profile", &"small"])?;
    assert_eq!(unknown.code, Some(2), "{unknown:?}");
    assert!(unknown.stderr.contains("no profile `small` (profiles here: big)"), "{unknown:?}");
    Ok(())
}
