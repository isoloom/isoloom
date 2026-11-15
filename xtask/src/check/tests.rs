use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Parser;

use super::outputs::{differences, phases, target_of};
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
    assert!(rust_version("[package]\nname = \"x\"\n").is_err());
}

#[test]
fn a_1x_release_checks_the_changes_as_a_major_and_a_later_one_as_the_version_says() {
    let answer = r#"{"crate":{"name":"isoloom-spec","max_stable_version":"1.4.2"}}"#;
    assert_eq!(latest_release(answer).ok().as_deref(), Some("1.4.2"));
    assert!(latest_release(r#"{"errors":[]}"#).is_err());
    assert_eq!(release_type("1.4.2"), ["--release-type", "major"]);
    assert_eq!(release_type("2.0.0"), Vec::<&str>::new());
    assert!(release_type("10.0.0").is_empty(), "a 10.x isn't a 1.x");
}

#[test]
fn the_first_module_of_each_target_fills_the_cache_and_the_others_follow() {
    let modules: Vec<PathBuf> = [
        "a/.isoloom/cloud-vm/aws",
        "a/.isoloom/proxmox",
        "b/.isoloom/cloud-vm/aws",
        "b/.isoloom/cloud-vm/gcp",
        "c/.isoloom/proxmox",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    let (first, rest) = phases(&modules);
    let text = |v: Vec<&std::path::Path>| v.iter().map(|p| target_of(p)).collect::<Vec<_>>();
    assert_eq!(text(first), ["cloud-vm/aws", "proxmox", "cloud-vm/gcp"]);
    assert_eq!(text(rest), ["cloud-vm/aws", "proxmox"]);
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
