//! What the commands print and the exit code they give: validation, targets, generation with
//! a refused cloud, and profiles.

use crate::testing::{Outcome, example, isoloom, isoloom_with, project, scratch, validate_json};

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
    let (run, v) = validate_json(&dir, &dir)?;
    assert_eq!(run.code, Some(1), "{run:?}");
    assert_eq!(v["ok"], false);
    assert!(v["problems"].as_array().is_some_and(|p| !p.is_empty()), "{v}");
    Ok(())
}

#[test]
fn validate_json_reports_v1_and_the_isoloom_version_for_a_valid_spec() -> Outcome {
    let home = scratch("validate-json-v1")?;
    let (run, v) = validate_json(&home, &example("hello-stack"))?;
    assert_eq!(run.code, Some(0), "{run:?}");
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["specFormat"], "v1", "{v}");
    assert_eq!(v["isoloomVersion"], env!("CARGO_PKG_VERSION"), "{v}");
    Ok(())
}

/// A daemon installed with no `service` step for it is a warning: shown, in the JSON too, and
/// the spec still valid (exit 0).
#[test]
fn validate_shows_the_warnings_and_passes() -> Outcome {
    let dir = project(
        "warned-spec",
        "version: 1\nname: warned\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    app: { steps: [{ packages: [nginx] }] }\n",
    )?;
    let (run, v) = validate_json(&dir, &dir)?;
    assert_eq!((run.code, &v["ok"]), (Some(0), &serde_json::json!(true)), "{v}");
    let warnings = v["warnings"].as_array().ok_or("no warnings")?;
    assert_eq!(warnings.len(), 1, "{v}");
    assert_eq!(warnings[0]["path"], "machines.web.app.steps[0]", "{v}");
    let text = isoloom(&dir, &[&"validate", &dir, &"--skip-files"])?;
    assert_eq!(text.code, Some(0), "{text:?}");
    assert!(text.stdout.contains("! warning: machines.web.app.steps[0]"), "{}", text.stdout);
    assert!(text.stdout.trim_end().ends_with("✓ warned is valid"), "{}", text.stdout);
    Ok(())
}

#[test]
fn validate_json_reports_legacy_for_a_spec_with_a_removed_key() -> Outcome {
    let dir = project(
        "legacy-spec",
        "version: 1\nname: legacy-demo\nmachines:\n  web:\n    docker: { image: nginx }\n",
    )?;
    let (run, v) = validate_json(&dir, &dir)?;
    assert_eq!(run.code, Some(1), "{run:?}");
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["specFormat"], "legacy", "{v}");
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

/// A container with an alias, which Kubernetes refuses (`www` isn't a Service name it gives).
const ALIASED: &str = "version: 1\nname: named\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  web:\n    networks: { lab: 10 }\n    aliases: [www]\n    app: { image: \"nginx:1.27\" }\n";

#[test]
fn a_target_that_refuses_is_left_out_and_check_still_passes() -> Outcome {
    let dir = project("refusal-left-out", ALIASED)?;
    let generated = isoloom(&dir, &[&"generate", &dir])?;
    assert_eq!(generated.code, Some(0), "{generated:?}");
    assert!(generated.stdout.contains("· kubernetes: "), "{}", generated.stdout);
    let checked = isoloom(&dir, &[&"check", &dir])?;
    assert_eq!(checked.code, Some(0), "{checked:?}");
    let one = isoloom(&dir, &[&"check", &dir, &"--target", &"docker"])?;
    assert_eq!(one.code, Some(0), "{one:?}");
    assert!(one.stdout.contains("compose.yml") && !one.stdout.contains("Vagrantfile"), "{one:?}");
    let refused = isoloom(&dir, &[&"check", &dir, &"--target", &"kubernetes"])?;
    assert_eq!(refused.code, Some(1), "nothing to vouch for: {refused:?}");
    assert!(refused.stdout.contains("✗ kubernetes: "), "{refused:?}");
    Ok(())
}

#[test]
fn check_tells_a_missing_file_from_one_it_cant_read() -> Outcome {
    let dir = project("check-read-error", ALIASED)?;
    let generated = isoloom(&dir, &[&"generate", &dir])?;
    assert_eq!(generated.code, Some(0), "{generated:?}");
    // A folder where the Compose file was: it exists, but can't be read as a file.
    let compose = dir.join(".isoloom/docker/compose.yml");
    std::fs::remove_file(&compose)?;
    std::fs::create_dir(&compose)?;
    std::fs::remove_file(dir.join(".isoloom/docker/run.json"))?;
    let checked = isoloom(&dir, &[&"check", &dir])?;
    assert_eq!(checked.code, Some(1), "{checked:?}");
    assert!(checked.stdout.contains("✗ .isoloom/docker/compose.yml can't be read: "), "{}", checked.stdout);
    assert!(!checked.stdout.contains("compose.yml is missing"), "{}", checked.stdout);
    assert!(checked.stdout.contains("✗ .isoloom/docker/run.json is missing"), "{}", checked.stdout);
    Ok(())
}

#[test]
fn check_quiet_shows_only_failures_and_json_one_result_per_file() -> Outcome {
    let dir = project("check-quiet-json", ALIASED)?;
    let generated = isoloom(&dir, &[&"generate", &dir, &"--target", &"docker"])?;
    assert_eq!(generated.code, Some(0), "{generated:?}");
    let quiet = isoloom(&dir, &[&"check", &dir, &"--target", &"docker", &"--quiet"])?;
    assert_eq!(quiet.code, Some(0), "{quiet:?}");
    assert!(
        !quiet.stdout.contains("✓ .isoloom/") && quiet.stdout.contains(" files up to date"),
        "{}",
        quiet.stdout
    );
    std::fs::write(dir.join(".isoloom/docker/compose.yml"), "changed\n")?;
    let quiet = isoloom(&dir, &[&"check", &dir, &"--target", &"docker", &"-q"])?;
    assert_eq!(quiet.code, Some(1), "{quiet:?}");
    assert_eq!(
        quiet.stdout,
        "✗ .isoloom/docker/compose.yml is out of date\nrun `isoloom generate` and commit the result\n"
    );
    let json = isoloom(&dir, &[&"check", &dir, &"--target", &"docker", &"--json"])?;
    assert_eq!(json.code, Some(1), "{json:?}");
    let report: serde_json::Value = serde_json::from_str(&json.stdout)?;
    let files = report["files"].as_array().ok_or("no files")?;
    let compose = files.iter().find(|f| f["path"] == ".isoloom/docker/compose.yml").ok_or("no compose.yml")?;
    assert_eq!(compose["status"], "out_of_date");
    assert!(
        files
            .iter()
            .filter(|f| f["path"] != ".isoloom/docker/compose.yml")
            .all(|f| f["status"] == "current"),
        "{report:#}"
    );
    assert_eq!(
        report["summary"],
        serde_json::json!({ "ok": false, "checked": files.len(), "current": files.len() - 1, "failed": 1 })
    );
    let both = isoloom(&dir, &[&"check", &dir, &"--quiet", &"--json"])?;
    assert_eq!(both.code, Some(2), "{both:?}");
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

    // `check` compares what `generate` wrote: the pinned target's files only.
    let checked = isoloom(&home, &[&"--profile", &"big", &"check", &dir])?;
    assert_eq!(checked.code, Some(0), "{checked:?}");
    assert!(
        checked.stdout.contains("✓ .isoloom/vagrant/Vagrantfile") && !checked.stdout.contains("compose.yml"),
        "{checked:?}"
    );
    let unpinned = isoloom(&home, &[&"check", &dir, &"--profile", &"big", &"--target", &"docker"])?;
    assert_eq!(unpinned.code, Some(2), "{unpinned:?}");
    assert!(unpinned.stderr.contains("profile `big` runs on `vagrant`, not `docker`"), "{unpinned:?}");

    let unknown = isoloom(&home, &[&"validate", &dir, &"--profile", &"small"])?;
    assert_eq!(unknown.code, Some(2), "{unknown:?}");
    assert!(unknown.stderr.contains("no profile `small` (profiles here: big)"), "{unknown:?}");
    Ok(())
}

#[test]
fn a_users_default_region_reaches_the_module_and_the_run_manifest() -> Outcome {
    // The region the run manifest's `region` param defaulted to was the built-in one, which a
    // runner gives Terraform (TF_VAR_region), undoing the person's.
    let home = scratch("default-region-home")?;
    std::fs::write(home.join("defaults.yml"), "cloud: { aws: { region: eu-north-1 } }\n")?;
    let dir = scratch("default-region")?;
    for f in ["isoloom.yml", "build/web/Dockerfile", "provision/web.sh", "checks/web-answers.sh"] {
        let to = dir.join(f);
        std::fs::create_dir_all(to.parent().ok_or("a file at the root")?)?;
        std::fs::copy(example("solo-web").join(f), to)?;
    }
    let o = isoloom(&home, &[&"generate", &dir, &"--target", &"cloud-vm"])?;
    assert_eq!(o.code, Some(0), "{o:?}");
    let module = std::fs::read_to_string(dir.join(".isoloom/cloud-vm/aws/main.tf"))?;
    assert!(module.contains("default = \"eu-north-1\""), "{module}");
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(".isoloom/cloud-vm/aws/run.json"))?)?;
    let region = manifest["params"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"] == "region")
        .ok_or("no region param")?;
    assert_eq!(region["default"], "eu-north-1", "{region}");
    // `isoloom check` sees the same files.
    let checked = isoloom(&home, &[&"check", &dir, &"--target", &"cloud-vm"])?;
    assert_eq!(checked.code, Some(0), "{checked:?}");
    Ok(())
}

/// A Linux web VM; `os` for its OS.
fn one_vm(os: &str) -> String {
    format!(
        "version: 1\nname: p\nnetworks:\n  lan: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  web:\n    os: {os}\n    networks: {{ lan: 10 }}\n    app: {{ steps: [{{ run: p.sh }}] }}\n"
    )
}

#[test]
fn generate_removes_what_it_no_longer_writes_and_check_reports_it() -> Outcome {
    let home = scratch("stale-home")?;
    let dir = project("stale", &one_vm("debian@12"))?;
    std::fs::write(dir.join("p.sh"), "true\n")?;
    let first = isoloom(&home, &[&"generate", &dir])?;
    assert_eq!(first.code, Some(0), "{first:?}");
    let out = dir.join(".isoloom");
    for written in [
        "docker/compose.yml",
        "kubernetes/environment.yaml",
        "cloud-vm/gcp/main.tf",
        "docker-vm/Vagrantfile",
    ] {
        assert!(out.join(written).is_file(), "{written}");
    }
    // What a runner and its tools leave beside the generated files is theirs.
    std::fs::write(out.join("cloud-vm/gcp/.terraform.lock.hcl"), "lock\n")?;
    std::fs::create_dir_all(out.join("docker-vm/.vagrant/machines"))?;
    std::fs::write(out.join("docker-vm/.vagrant/machines/id"), "x\n")?;
    // The machine becomes Windows: no container form, no Windows on Google Cloud.
    std::fs::write(dir.join("isoloom.yml"), one_vm("windows-server@2022").replace("p.sh", "p.ps1"))?;
    std::fs::write(dir.join("p.ps1"), "$true\n")?;
    let checked = isoloom(&home, &[&"check", &dir])?;
    assert_eq!(checked.code, Some(1), "{checked:?}");
    assert!(
        checked.stdout.contains("✗ .isoloom/docker/compose.yml is no longer generated"),
        "{}",
        checked.stdout
    );
    let o = isoloom(&home, &[&"generate", &dir])?;
    assert_eq!(o.code, Some(0), "{o:?}");
    assert!(
        o.stdout.contains("− .isoloom/kubernetes/environment.yaml (no longer generated)"),
        "{}",
        o.stdout
    );
    for gone in [
        "docker",
        "kubernetes",
        "cloud-docker",
        "cloud-vm/digitalocean",
        "cloud-vm/linode",
        "cloud-vm/oci",
    ] {
        assert!(!out.join(gone).exists(), "{gone} is left");
    }
    assert!(!out.join("cloud-vm/gcp/main.tf").exists() && !out.join("cloud-vm/gcp/run.json").exists());
    assert!(out.join("cloud-vm/gcp/.terraform.lock.hcl").is_file(), "a tool's file is kept");
    assert!(out.join("docker-vm/.vagrant/machines/id").is_file() && !out.join("docker-vm/Vagrantfile").exists());
    assert!(out.join("vagrant/Vagrantfile").is_file() && out.join("cloud-vm/aws/main.tf").is_file());
    let clean = isoloom(&home, &[&"check", &dir])?;
    assert_eq!(clean.code, Some(0), "{clean:?}");
    Ok(())
}

#[test]
fn generate_writes_a_gitignore_for_the_tools_state_and_lists_it() -> Outcome {
    let home = scratch("gitignore-home")?;
    let dir = project("gitignore", &one_vm("debian@12"))?;
    std::fs::write(dir.join("p.sh"), "true\n")?;
    let run = isoloom(&home, &[&"generate", &dir])?;
    assert_eq!(run.code, Some(0), "{run:?}");
    let ignore = std::fs::read_to_string(dir.join(".isoloom/.gitignore"))?;
    for pattern in [".terraform/", "*.tfstate", "*.tfstate.*", ".vagrant/", ".isoloom-reach/"] {
        assert!(ignore.lines().any(|l| l == pattern), "{pattern}: {ignore}");
    }
    assert!(!ignore.contains(".terraform.lock.hcl"), "the lock file is committed: {ignore}");
    // Isoloom's like every file it writes: listed, so `generate` manages it.
    let list = std::fs::read_to_string(dir.join(".isoloom/.generated"))?;
    assert!(list.lines().any(|l| l == ".isoloom/.gitignore"), "{list}");
    Ok(())
}

#[test]
fn generating_one_target_leaves_the_others_files_alone() -> Outcome {
    let home = scratch("stale-one-home")?;
    let dir = project("stale-one", &one_vm("debian@12"))?;
    std::fs::write(dir.join("p.sh"), "true\n")?;
    assert_eq!(isoloom(&home, &[&"generate", &dir])?.code, Some(0));
    std::fs::write(dir.join("isoloom.yml"), one_vm("windows-server@2022").replace("p.sh", "p.ps1"))?;
    std::fs::write(dir.join("p.ps1"), "$true\n")?;
    let o = isoloom(&home, &[&"generate", &dir, &"--target", &"cloud-vm"])?;
    assert_eq!(o.code, Some(0), "{o:?}");
    let out = dir.join(".isoloom");
    assert!(!out.join("cloud-vm/gcp").exists(), "the target's own stale files go");
    assert!(out.join("docker/compose.yml").is_file(), "another target's stay until it is generated");
    Ok(())
}
