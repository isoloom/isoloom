use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Parser;

use super::outputs::{differences, phases, provider_entry, prune_unbuilt};
use super::{Args, SEMVER_CRATES, STEPS, chosen, latest_release, release_type, rust_version};

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    args: Args,
}

fn names(argv: &[&str]) -> Vec<&'static str> {
    let cli = Cli::try_parse_from([&["check"], argv].concat()).unwrap_or_else(|e| panic!("{e}"));
    chosen(&cli.args).unwrap_or_else(|e| panic!("{e}")).iter().map(|s| s.name).collect()
}

#[test]
fn fast_is_the_short_list_in_order() {
    assert_eq!(names(&["--fast"]), ["lockfile", "fmt", "clippy", "test", "windows-gnu"]);
}

#[test]
fn the_full_run_has_every_step_once() {
    let all = names(&[]);
    assert_eq!(all.len(), STEPS.len());
    let mut unique = all.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), all.len(), "a step named twice: {all:?}");
}

#[test]
fn only_and_skip_choose_by_name_and_keep_the_order() {
    assert_eq!(names(&["--only", "terraform,fmt"]), ["fmt", "terraform"]);
    assert_eq!(names(&["--fast", "--skip", "windows-gnu,lockfile"]), ["fmt", "clippy", "test"]);
}

#[test]
fn a_step_that_does_not_exist_is_an_error_naming_the_steps() {
    let cli = Cli::try_parse_from(["check", "--only", "clipy"]).unwrap_or_else(|e| panic!("{e}"));
    let error = chosen(&cli.args).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(error.starts_with("no step `clipy`; the steps: lockfile, fmt,"), "{error}");
}

#[test]
fn the_rust_version_is_the_workspace_one() {
    let manifest = "[workspace]\nmembers = []\n[workspace.package]\nrust-version = \"1.89\"\n";
    assert_eq!(rust_version(manifest).ok().as_deref(), Some("1.89"));
    let none = rust_version("[package]\nname = \"x\"\n").err().map(|e| e.to_string());
    assert_eq!(none.as_deref(), Some("Cargo.toml sets no workspace.package.rust-version"));
}

#[test]
fn a_1x_release_checks_the_changes_as_a_major_and_a_later_one_as_the_version_says() {
    let answer = r#"{"crate":{"name":"isoloom-spec","max_stable_version":"1.4.2"}}"#;
    assert_eq!(latest_release(answer).ok().as_deref(), Some("1.4.2"));
    let none = latest_release(r#"{"errors":[]}"#).err().map(|e| e.to_string());
    assert_eq!(none.as_deref(), Some("crates.io's answer names no max_stable_version"));
    assert_eq!(release_type("1.4.2"), ["--release-type", "major"]);
    assert_eq!(release_type("2.0.0"), Vec::<&str>::new());
    assert!(release_type("10.0.0").is_empty(), "a 10.x isn't a 1.x");
}

#[test]
fn the_modules_that_bring_a_provider_fill_the_cache_and_the_others_follow() {
    let module = |path: &str, providers: &[&str]| (PathBuf::from(path), providers.iter().map(|p| (*p).to_owned()).collect());
    let modules: Vec<(PathBuf, Vec<String>)> = vec![
        module("a/aws", &["hashicorp/aws ~> 6", "hashicorp/local ~> 2"]),
        module("a/proxmox", &["bpg/proxmox ~> 0.1"]),
        module("b/aws", &["hashicorp/aws ~> 6", "hashicorp/local ~> 2"]),
        // The same target, one provider more: it fills the cache with it before the others run.
        module("c/aws", &["hashicorp/aws ~> 6", "hashicorp/local ~> 2", "hashicorp/tls ~> 4"]),
        module("d/aws", &["hashicorp/aws ~> 6", "hashicorp/tls ~> 4"]),
        module("d/proxmox", &["bpg/proxmox ~> 0.1"]),
    ];
    let (first, rest) = phases(&modules);
    let text = |v: Vec<&std::path::Path>| v.iter().map(|p| p.display().to_string()).collect::<Vec<_>>();
    assert_eq!(text(first), ["a/aws", "a/proxmox", "c/aws"]);
    assert_eq!(text(rest), ["b/aws", "d/aws", "d/proxmox"]);
}

#[test]
fn a_provider_entry_is_its_source_and_version_and_nothing_else_is_one() {
    assert_eq!(
        provider_entry(r#"    aws   = { source = "hashicorp/aws", version = "~> 6.67.0" }"#).as_deref(),
        Some("hashicorp/aws ~> 6.67.0")
    );
    assert_eq!(provider_entry(r#"    source      = "${path.module}/.isoloom-outputs-web.tgz""#), None);
    assert_eq!(provider_entry(r#"  required_version = ">= 1.6""#), None);
}

#[test]
fn differences_name_a_changed_a_missing_and_an_added_file() {
    let tree = |files: &[(&str, &str)]| -> BTreeMap<PathBuf, Vec<u8>> { files.iter().map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec())).collect() };
    let one = tree(&[("same", "x"), ("changed", "1"), ("gone", "g")]);
    let two = tree(&[("same", "x"), ("changed", "2"), ("new", "n")]);
    let found: Vec<String> = differences(&one, &two).iter().map(|p| p.display().to_string()).collect();
    assert_eq!(found, ["changed", "gone", "new"]);
    assert_eq!(differences(&one, &one), Vec::<&std::path::Path>::new());
}

#[test]
fn the_api_crates_are_the_workspaces_in_their_group_folders() {
    let root = crate::repo::root();
    for name in SEMVER_CRATES {
        let found: Vec<PathBuf> = std::fs::read_dir(root.join("crates"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|group| group.path().join(name).join("Cargo.toml"))
            .filter(|m| m.is_file())
            .collect();
        assert_eq!(found.len(), 1, "`{name}` is at crates/<group>/{name}/Cargo.toml: {found:?}");
    }
}

#[test]
fn a_windows_binary_importing_the_visual_cpp_runtime_is_named() {
    let listing = "  DLL Name: KERNEL32.dll\n  DLL Name: VCRUNTIME140.dll\n  DLL Name: msvcp140.dll\n  DLL Name: ws2_32.dll\n";
    assert_eq!(super::vc_runtime_imports(listing), ["VCRUNTIME140.dll", "msvcp140.dll"]);
    assert_eq!(super::vc_runtime_imports("  DLL Name: KERNEL32.dll\n"), Vec::<String>::new());
}

/// Each `run` of workflow `file`'s steps, across its jobs, parsed.
fn workflow_runs(file: &str) -> (serde_yaml_ng::Value, Vec<String>) {
    let text = std::fs::read_to_string(crate::repo::root().join(".github/workflows").join(file)).unwrap();
    let doc = isoloom_test_support::yaml(&text);
    let runs = doc["jobs"]
        .as_mapping()
        .into_iter()
        .flat_map(|jobs| jobs.values())
        .flat_map(|job| job["steps"].as_sequence().into_iter().flatten())
        .filter_map(|step| step["run"].as_str().map(str::to_owned))
        .collect();
    (doc, runs)
}

/// gate.yml runs every step `cargo xtask check` has (main and a release tag get the whole check),
/// and ci.yml runs the short list on pull requests into dev: CI and the command can't drift.
#[test]
fn the_gate_runs_every_step_and_ci_the_short_list() {
    let (_, gate) = workflow_runs("gate.yml");
    let gate: std::collections::BTreeSet<&str> = gate
        .iter()
        .filter_map(|run| run.strip_prefix("cargo run -q -p xtask --locked -- check --only "))
        .flat_map(|list| list.split(','))
        .collect();
    let missing: Vec<&str> = STEPS.iter().map(|s| s.name).filter(|n| !gate.contains(n)).collect();
    assert_eq!(missing, Vec::<&str>::new(), "steps gate.yml doesn't run");
    let unknown: Vec<&&str> = gate.iter().filter(|n| !STEPS.iter().any(|s| s.name == **n)).collect();
    assert_eq!(unknown, Vec::<&&str>::new(), "gate.yml names steps cargo xtask check doesn't have");
    let (ci, runs) = workflow_runs("ci.yml");
    assert!(
        runs.iter().any(|r| r == "cargo run -q -p xtask --locked -- check ${{ matrix.check }}"),
        "{runs:?}"
    );
    let checks = isoloom_test_support::yaml_values(&ci["jobs"]["fast"]["strategy"]["matrix"], "check");
    isoloom_test_support::assert_every!(&checks, |c: &String| c == "--fast" || c.starts_with("--fast --skip "));
}

/// A cached provider whose folder lost its binary is removed, so Terraform fetches it again; a
/// whole one stays.
#[test]
fn a_cached_provider_without_its_binary_is_removed_and_a_whole_one_stays() {
    let cache = tempfile::tempdir().unwrap();
    let whole = cache.path().join("registry.terraform.io/hashicorp/aws/6.67.0/linux_amd64");
    let empty = cache.path().join("registry.terraform.io/hashicorp/local/2.9.1/linux_amd64");
    std::fs::create_dir_all(&whole).unwrap();
    std::fs::write(whole.join("terraform-provider-aws_v6.67.0_x5"), "").unwrap();
    std::fs::create_dir_all(&empty).unwrap();
    std::fs::write(empty.join("LICENSE.txt"), "").unwrap();
    let removed = prune_unbuilt(cache.path()).unwrap();
    assert_eq!(removed, [std::path::PathBuf::from("registry.terraform.io/hashicorp/local/2.9.1/linux_amd64")]);
    assert!(whole.is_dir() && !empty.exists());
}
