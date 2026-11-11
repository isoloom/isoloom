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
        under
            .first()
            .is_some_and(|l| l.starts_with("    ✗ gcp (") && l.contains("no Windows VM on Google Cloud")),
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
    if compose.exists() {
        std::fs::remove_file(&compose)?;
    }
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
    let failing = isoloom(&dir, &[&"check", &dir, &"--target", &"docker", &"-q"])?;
    assert_eq!(failing.code, Some(1), "{failing:?}");
    assert_eq!(
        failing.stdout,
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
    std::fs::write(dir.join("profiles/Bank_FR.yml"), "{}\n")?;
    let json = isoloom(&home, &[&"profiles", &dir, &"--json"])?;
    assert_eq!(json.code, Some(0), "{json:?}");
    assert!(
        json.stderr.contains("Bank_FR.yml is skipped: a profile's file name is kebab-case"),
        "{}",
        json.stderr
    );
    let v: serde_json::Value = serde_json::from_str(&json.stdout)?;
    assert_eq!(v[0]["name"], "big");
    assert_eq!(v[0]["target"], "vagrant");
    assert_eq!(v[0]["ok"], true);
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_profile_variable_that_isnt_text_is_an_error_not_no_profile() -> Outcome {
    use std::os::unix::ffi::OsStrExt;
    let home = scratch("profile-not-text-home")?;
    let dir = profiled("profile-not-text", &[("big", "spec:\n  machines: { web: { resources: { cpus: 4 } } }\n")])?;
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_isoloom"))
        .args([std::ffi::OsStr::new("resources"), dir.as_os_str()])
        .env("ISOLOOM_HOME", &home)
        .env("ISOLOOM_PROFILE", std::ffi::OsStr::from_bytes(b"b\xffig"))
        .output()?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "not run without the profile: {stderr}");
    assert!(stderr.contains("ISOLOOM_PROFILE isn't text"), "{stderr}");
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

/// `isoloom defaults` shows every layer the configuration reference lists, the image table given
/// to `--images` and the `-s defaults.<key>=<value>` overrides included, each with its source: it
/// once refused `--images` and left out `-s`.
#[test]
fn defaults_shows_the_image_table_and_the_command_lines_layers() -> Outcome {
    let home = scratch("defaults-layers-home")?;
    let dir = scratch("defaults-layers")?;
    std::fs::copy(example("solo-web").join("isoloom.yml"), dir.join("isoloom.yml"))?;
    let table = dir.join("table.yml");
    std::fs::write(&table, "os: { \"debian@12\": { vagrant: my-org/debian-12 } }\n")?;
    let o = isoloom(&home, &[&"defaults", &dir, &"--images", &table, &"-s", &"defaults.cloud.aws.region=eu-north-1"])?;
    assert_eq!(o.code, Some(0), "{o:?}");
    let line = |key: &str| o.stdout.lines().find(|l| l.starts_with(key)).map(str::to_owned).unwrap_or_default();
    let image = line("images.os.debian@12.vagrant");
    assert!(image.contains("my-org/debian-12") && image.contains("--images "), "{}", o.stdout);
    let region = line("cloud.aws.region");
    assert!(region.contains("eu-north-1") && region.contains("-s defaults.cloud.aws.region"), "{}", o.stdout);
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
    for pattern in [".terraform/", "*.tfstate", "*.tfstate.*", ".vagrant/"] {
        assert!(ignore.lines().any(|l| l == pattern), "{pattern}: {ignore}");
    }
    assert!(!ignore.contains(".terraform.lock.hcl"), "the lock file is committed: {ignore}");
    // Isoloom's like every file it writes: listed, so `generate` manages it.
    let list = std::fs::read_to_string(dir.join(".isoloom/.generated"))?;
    assert!(list.lines().any(|l| l == ".isoloom/.gitignore"), "{list}");
    Ok(())
}

#[test]
fn check_compares_the_list_of_generated_files_like_any_generated_file() -> Outcome {
    let home = scratch("check-list-home")?;
    let dir = project("check-list", &one_vm("debian@12"))?;
    std::fs::write(dir.join("p.sh"), "true\n")?;
    assert_eq!(isoloom(&home, &[&"generate", &dir])?.code, Some(0));
    assert_eq!(isoloom(&home, &[&"check", &dir])?.code, Some(0));
    let list = dir.join(".isoloom/.generated");
    let text = std::fs::read_to_string(&list)?;
    std::fs::remove_file(&list)?;
    let missing = isoloom(&home, &[&"check", &dir, &"--quiet"])?;
    assert_eq!(missing.code, Some(1), "{missing:?}");
    assert!(missing.stdout.contains("✗ .isoloom/.generated is missing"), "{}", missing.stdout);
    std::fs::write(
        &list,
        text.lines().filter(|l| *l != ".isoloom/.gitignore").flat_map(|l| [l, "\n"]).collect::<String>(),
    )?;
    let short = isoloom(&home, &[&"check", &dir, &"--quiet"])?;
    assert!(short.stdout.contains("✗ .isoloom/.generated is out of date"), "{}", short.stdout);
    assert_eq!(isoloom(&home, &[&"generate", &dir])?.code, Some(0));
    assert_eq!(std::fs::read_to_string(&list)?, text, "generate writes the list check expects");
    assert_eq!(isoloom(&home, &[&"check", &dir])?.code, Some(0));
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

/// A one-VM project `name` with its `docker` files generated: its folder.
fn generated(name: &str) -> Outcome<std::path::PathBuf> {
    let home = scratch(&format!("{name}-home"))?;
    let dir = project(name, &one_vm("debian@12"))?;
    std::fs::write(dir.join("p.sh"), "true\n")?;
    assert_eq!(isoloom(&home, &[&"generate", &dir, &"--target", &"docker"])?.code, Some(0));
    Ok(dir)
}

/// A generated project whose list also names `entry`, and the file under `dir` it would reach.
fn listing(name: &str, entry: &str) -> Outcome<std::path::PathBuf> {
    let dir = generated(name)?;
    let list = dir.join(".isoloom/.generated");
    let text = std::fs::read_to_string(&list)?;
    std::fs::write(&list, format!("{text}{entry}\n"))?;
    Ok(dir)
}

/// `generate` and `check` on `dir` both refuse its list, naming `entry`, and leave `outside` be;
/// `generate` writes nothing before it refuses (a generated file removed stays removed).
fn refuses_the_list(dir: &std::path::Path, entry: &str, outside: &std::path::Path) -> Outcome {
    // A home of the project's own: the tests calling this run at once.
    let project = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let home = scratch(&format!("{project}-refused-home"))?;
    let compose = dir.join(".isoloom/docker/compose.yml");
    if compose.exists() {
        std::fs::remove_file(&compose)?;
    }
    for command in ["generate", "check"] {
        let o = isoloom(&home, &[&command, &dir])?;
        assert_eq!(o.code, Some(2), "{command}: {o:?}");
        assert!(o.stderr.contains(&format!("`.isoloom/.generated` lists `{entry}`")), "{command}: {}", o.stderr);
        assert!(outside.is_file(), "{command} removed {}", outside.display());
        assert!(!compose.exists(), "{command} wrote files before refusing the list");
    }
    Ok(())
}

#[test]
fn generate_refuses_a_listed_path_that_climbs_out_of_the_output() -> Outcome {
    let entry = ".isoloom/../outside.txt";
    let dir = listing("list-dotdot", entry)?;
    let outside = dir.join("outside.txt");
    std::fs::write(&outside, "the project's own\n")?;
    refuses_the_list(&dir, entry, &outside)
}

#[test]
fn generate_refuses_a_listed_absolute_path() -> Outcome {
    let outside = scratch("list-absolute-outside")?.join("outside.txt");
    std::fs::write(&outside, "someone else's\n")?;
    let entry = outside.to_string_lossy().into_owned();
    let dir = listing("list-absolute", &entry)?;
    refuses_the_list(&dir, &entry, &outside)
}

#[cfg(unix)]
#[test]
fn generate_refuses_a_listed_file_reached_through_a_symbolic_link() -> Outcome {
    let elsewhere = scratch("list-link-target")?;
    let outside = elsewhere.join("outside.txt");
    std::fs::write(&outside, "someone else's\n")?;
    let entry = ".isoloom/linked/outside.txt";
    let dir = listing("list-link", entry)?;
    std::os::unix::fs::symlink(&elsewhere, dir.join(".isoloom/linked"))?;
    refuses_the_list(&dir, entry, &outside)
}

#[cfg(unix)]
#[test]
fn generate_refuses_to_remove_through_a_linked_output_folder() -> Outcome {
    let elsewhere = scratch("list-linked-output-target")?;
    let outside = elsewhere.join("outside.txt");
    std::fs::write(&outside, "someone else's\n")?;
    let entry = ".isoloom/outside.txt";
    let dir = listing("list-linked-output", entry)?;
    let list = std::fs::read_to_string(dir.join(".isoloom/.generated"))?;
    std::fs::remove_dir_all(dir.join(".isoloom"))?;
    std::fs::write(elsewhere.join(".generated"), list)?;
    std::os::unix::fs::symlink(&elsewhere, dir.join(".isoloom"))?;
    refuses_the_list(&dir, entry, &outside)
}

/// A generated project (`docker`) whose `.isoloom/docker/<file>` is replaced by a link to
/// `outside`; its folder.
#[cfg(unix)]
fn with_linked_output(name: &str, file: &str, outside: &std::path::Path) -> Outcome<std::path::PathBuf> {
    let dir = generated(name)?;
    let linked = dir.join(".isoloom/docker").join(file);
    std::fs::remove_file(&linked)?;
    std::os::unix::fs::symlink(outside, &linked)?;
    Ok(dir)
}

#[cfg(unix)]
#[test]
fn generate_and_check_refuse_to_write_through_a_linked_generated_file() -> Outcome {
    let victim = scratch("linked-file-victim")?.join("keys");
    std::fs::write(&victim, "someone else's\n")?;
    let dir = with_linked_output("linked-file", "run.json", &victim)?;
    let home = scratch("linked-file-refused-home")?;
    for command in ["generate", "check"] {
        let o = isoloom(&home, &[&command, &dir, &"--target", &"docker"])?;
        assert_eq!(o.code, Some(2), "{command}: {o:?}");
        assert!(
            o.stderr.contains(".isoloom/docker/run.json is reached through a symbolic link"),
            "{command}: {}",
            o.stderr
        );
        assert_eq!(std::fs::read_to_string(&victim)?, "someone else's\n", "{command} wrote through the link");
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn generate_refuses_to_write_through_a_linked_output_folder() -> Outcome {
    let elsewhere = scratch("linked-folder-victim")?;
    let home = scratch("linked-folder-home")?;
    let dir = project("linked-folder", &one_vm("debian@12"))?;
    std::fs::write(dir.join("p.sh"), "true\n")?;
    std::fs::create_dir_all(dir.join(".isoloom"))?;
    std::os::unix::fs::symlink(&elsewhere, dir.join(".isoloom/docker"))?;
    let o = isoloom(&home, &[&"generate", &dir, &"--target", &"docker"])?;
    assert_eq!(o.code, Some(2), "{o:?}");
    assert!(o.stderr.contains("is reached through a symbolic link"), "{}", o.stderr);
    assert_eq!(std::fs::read_dir(&elsewhere)?.count(), 0, "written through the link");
    assert!(!dir.join(".isoloom/.generated").exists(), "nothing is written before the refusal");
    Ok(())
}

#[test]
fn generate_replaces_each_file_rather_than_writing_into_it() -> Outcome {
    let home = scratch("replaced-home")?;
    let dir = project("replaced", &one_vm("debian@12"))?;
    std::fs::write(dir.join("p.sh"), "true\n")?;
    assert_eq!(isoloom(&home, &[&"generate", &dir, &"--target", &"docker"])?.code, Some(0));
    let compose = dir.join(".isoloom/docker/compose.yml");
    let written = std::fs::read_to_string(&compose)?;
    std::fs::write(&compose, "stale\n")?;
    std::fs::write(dir.join(".isoloom/docker/compose.yml.isoloom-new"), "a half-written file left over\n")?;
    assert_eq!(isoloom(&home, &[&"generate", &dir, &"--target", &"docker"])?.code, Some(0));
    assert_eq!(std::fs::read_to_string(&compose)?, written);
    assert!(
        !dir.join(".isoloom/docker/compose.yml.isoloom-new").exists(),
        "the fresh file is renamed into place"
    );
    Ok(())
}

#[test]
fn every_command_reads_the_defaults_tags_and_names_where_a_wrong_one_comes_from() -> Outcome {
    let home = scratch("default-tags-home")?;
    let dir = project("default-tags", &format!("{}tags: {{ Team: ops }}\n", one_vm("debian@12")))?;
    std::fs::write(dir.join("p.sh"), "true\n")?;
    // A tag the defaults add, clashing with the spec's: each command says so, naming its source.
    for command in ["validate", "targets", "resources", "generate"] {
        let o = isoloom_with(&home, &[&command, &dir], &[("ISOLOOM_TAGS__TEAM", "dev")])?;
        assert_eq!(o.code, Some(2), "{command}: {o:?}");
        assert!(
            o.stderr
                .contains("defaults (environment ISOLOOM_TAGS__TEAM): tag `team`: `team` and `Team` are one tag on Azure"),
            "{command}: {}",
            o.stderr
        );
    }
    // One no cloud takes is refused where it is loaded, before the spec.
    let o = isoloom_with(&home, &[&"validate", &dir], &[("ISOLOOM_TAGS__9X", "dev")])?;
    assert_eq!(o.code, Some(2), "{o:?}");
    assert!(o.stderr.contains("tag `9x`: a key of at most 63"), "{}", o.stderr);
    Ok(())
}

#[test]
fn when_every_target_refuses_generate_and_check_say_no_and_nothing_is_removed() -> Outcome {
    let home = scratch("all-refused-home")?;
    let dir = project("all-refused", &format!("{}targets: [docker]\n", one_vm("debian@12")))?;
    std::fs::write(dir.join("p.sh"), "true\n")?;
    assert_eq!(isoloom(&home, &[&"generate", &dir])?.code, Some(0));
    let compose = dir.join(".isoloom/docker/compose.yml");
    assert!(compose.is_file());
    // Every target left refuses the lab now (cloud-docker: an arm64 machine).
    let refused = one_vm("debian@12").replace("    os: debian@12\n", "    os: debian@12\n    hardware: { arch: arm64 }\n");
    std::fs::write(dir.join("isoloom.yml"), format!("{refused}targets: [cloud-docker]\n"))?;
    for command in ["generate", "check"] {
        let o = isoloom(&home, &[&command, &dir])?;
        assert_eq!(o.code, Some(1), "{command}: {o:?}");
        assert!(o.stdout.contains("arm64"), "{command} says why: {}", o.stdout);
    }
    assert!(compose.is_file(), "nothing is removed when nothing is generated");
    Ok(())
}

#[test]
fn schema_prints_the_committed_schema_with_the_catalogues_rules() -> Outcome {
    let home = scratch("schema-home")?;
    let run = isoloom(&home, &[&"schema"])?;
    assert_eq!(run.code, Some(0), "{run:?}");
    let printed: serde_json::Value = serde_json::from_str(&run.stdout)?;
    let committed: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../schemas/spec-v1.schema.json"),
    )?)?;
    assert_eq!(printed, committed);
    assert!(printed.pointer("/$defs/Machine/allOf").is_some(), "each entry's `with:` rule");
    Ok(())
}

#[test]
fn the_cli_reference_page_is_what_docs_cli_prints() -> Outcome {
    let home = scratch("docs-cli-home")?;
    let run = isoloom(&home, &[&"docs", &"--cli"])?;
    assert_eq!(run.code, Some(0), "{run:?}");
    let page = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/reference/cli.md");
    let on_disk = std::fs::read_to_string(&page).unwrap_or_default();
    assert!(
        on_disk == run.stdout,
        "docs/reference/cli.md is stale; run `cargo run -q -p isoloom -- docs --cli > docs/reference/cli.md`"
    );
    for command in ["validate", "generate", "check", "import compose", "docs"] {
        assert!(run.stdout.contains(&format!("## `isoloom {command}`")), "{command}");
    }
    assert!(run.stdout.contains("| `--quiet`") || run.stdout.contains("| `-q, --quiet`"), "check's options");
    Ok(())
}

/// The access machine left to the runner: a target refuses it unless the image table gives it a
/// form there. Every command that judges the environment takes the table and `-s` as `generate` does, so
/// none says ✗ for a target `generate --images` writes.
#[test]
fn every_command_judges_the_lab_generate_writes() -> Outcome {
    let home = scratch("judges-what-generate-writes")?;
    let dir = project(
        "judges-what-generate-writes",
        "version: 1\nname: supplied\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@12, networks: { lan: 10 }, ports: { web: 80 }, app: { image: nginx } }\n  user: { access: true, networks: { lan: 99 } }\n",
    )?;
    let table = dir.join("images.yml");
    std::fs::write(&table, "access: { docker: registry.example.com/workstation:1 }\n")?;
    let bare = isoloom(&home, &[&"targets", &dir])?;
    assert!(bare.stdout.contains("✗ docker ("), "{bare:?}");
    let with = isoloom(&home, &[&"targets", &dir, &"--images", &table])?;
    assert!(with.stdout.contains("✓ docker\n"), "{with:?}");
    let generated = isoloom(&home, &[&"generate", &dir, &"--target", &"docker", &"--images", &table])?;
    assert_eq!(generated.code, Some(0), "{generated:?}");
    // The snapshot's targets, the same way.
    let inspected = isoloom(&home, &[&"inspect", &"targets.generated", &dir, &"--images", &table])?;
    assert_eq!(inspected.code, Some(0), "{inspected:?}");
    assert!(inspected.stdout.contains("\"docker\""), "{inspected:?}");
    // `-s` too, on the commands that had none.
    let set = isoloom(&home, &[&"resources", &dir, &"-s", &"machines.web.resources.cpus=3", &"--json"])?;
    assert!(set.stdout.contains("\"cpus\":"), "{set:?}");
    let renamed = isoloom(&home, &[&"validate", &dir, &"--skip-files", &"-s", &"name=renamed"])?;
    assert!(renamed.stdout.contains("✓ renamed is valid"), "{renamed:?}");
    Ok(())
}

/// A copy of example `name` in a fresh folder: its project files, none of the outputs a local
/// generation left there.
fn example_copy(name: &str, into: &str) -> Outcome<std::path::PathBuf> {
    let dir = scratch(into)?;
    let from = example(name);
    let mut stack = vec![std::path::PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for entry in std::fs::read_dir(from.join(&rel))? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(".isoloom") {
                continue;
            }
            let path = rel.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                std::fs::create_dir_all(dir.join(&path))?;
                stack.push(path);
            } else {
                std::fs::copy(entry.path(), dir.join(&path))?;
            }
        }
    }
    Ok(dir)
}

#[test]
fn generate_fails_on_a_template_or_check_script_it_cant_read_with_the_read_error() -> Outcome {
    let dir = example_copy("configured-app", "missing-template")?;
    std::fs::remove_file(dir.join("conf/site.conf"))?;
    let o = isoloom(&dir, &[&"generate", &dir, &"--target", &"docker"])?;
    assert_ne!(o.code, Some(0), "{o:?}");
    assert!(o.stderr.contains("conf/site.conf") && o.stderr.contains("No such file"), "{}", o.stderr);
    assert!(!dir.join(".isoloom/docker").exists(), "nothing is written");

    let no_script = example_copy("configured-app", "missing-check-script")?;
    std::fs::remove_file(no_script.join("checks/api-answers.sh"))?;
    let k = isoloom(&no_script, &[&"generate", &no_script, &"--target", &"kubernetes"])?;
    assert_ne!(k.code, Some(0), "{k:?}");
    assert!(k.stderr.contains("checks/api-answers.sh") && k.stderr.contains("No such file"), "{}", k.stderr);
    // `validate` still lists it as a problem of the spec, not as an error.
    let v = isoloom(&no_script, &[&"validate", &no_script])?;
    assert_eq!(v.code, Some(1), "{v:?}");
    assert!(v.stdout.contains("checks/api-answers.sh"), "{}", v.stdout);
    Ok(())
}

#[test]
fn profiles_checks_the_lab_generate_writes_its_defaults_tags_included() -> Outcome {
    let home = scratch("profiles-tags-home")?;
    let dir = profiled("profiles-tags", &[("p", "target: docker\n")])?;
    let spec = std::fs::read_to_string(dir.join("isoloom.yml"))?;
    std::fs::write(dir.join("isoloom.yml"), format!("{spec}tags: {{ team: b }}\n"))?;
    std::fs::write(dir.join("isoloom.defaults.yml"), "tags: { Team: a }\n")?;
    let generate = isoloom(&home, &[&"--profile", &"p", &"generate", &dir])?;
    assert_ne!(generate.code, Some(0), "{generate:?}");
    let o = isoloom(&home, &[&"profiles", &dir])?;
    assert_eq!(o.code, Some(1), "{o:?}");
    assert!(o.stdout.contains("✗ p") && o.stdout.contains("`Team` and `team`"), "{}", o.stdout);
    Ok(())
}
