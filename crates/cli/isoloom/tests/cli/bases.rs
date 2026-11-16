//! `isoloom bases fetch`: the base images a project's builds start `FROM` that the service matrix
//! doesn't have, vendored from their published sources with `git`, a stand-in here: no test
//! reaches the network.

use isoloom_test_support::{FakeBin, yaml, yaml_str};

use crate::testing::{Outcome, isoloom_with, project, scratch};

/// The commit the stand-in `git` checks out.
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// A stand-in `git`: docker-library's manifests list `frob:1.0` (built from `1.0/` of its
/// repository), and the checkout holds that Dockerfile, which starts from an image the matrix has.
/// Each call is logged.
fn git(bin: &FakeBin) -> Outcome<()> {
    let log = bin.log();
    bin.stand_in(
        "git",
        &format!(
            r#"echo "git $*" >> '{log}'
case "$1" in
  clone) mkdir -p "$6/library"; printf 'GitRepo: https://example.com/frob.git\n\nTags: 1.0\nGitCommit: {COMMIT}\nDirectory: 1.0\n' > "$6/library/frob" ;;
  init) mkdir -p "$3" ;;
  checkout) mkdir -p 1.0; printf 'FROM debian:12\n' > 1.0/Dockerfile ;;
  rev-parse) echo {COMMIT} ;;
esac"#,
            log = log.display()
        ),
    )?
    .link_real(&["mkdir"])?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_base_with_a_published_source_is_vendored_and_one_without_is_named() -> Outcome {
    let home = scratch("bases-home")?;
    let dir = project(
        "bases",
        "version: 1\nname: b\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  app: { networks: { lan: 10 }, app: { build: app } }\n  tool: { networks: { lan: 11 }, app: { build: tool } }\n",
    )?;
    for (machine, from) in [("app", "frob:1.0"), ("tool", "registry.example.com/team/tool:2")] {
        std::fs::create_dir_all(dir.join(machine))?;
        std::fs::write(dir.join(machine).join("Dockerfile"), format!("FROM {from}\n"))?;
    }
    let bin = FakeBin::named("bases-git")?;
    git(&bin)?;
    let path = bin.path();
    let o = isoloom_with(&home, &[&"bases", &"fetch", &dir], &[("PATH", path.to_str().ok_or("a PATH that isn't UTF-8")?)])?;
    // One image can't be fetched: the command says no, after vendoring the other.
    assert_eq!(o.code, Some(1), "{o:?}");
    let lines: Vec<&str> = o.stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "✗ registry.example.com/team/tool:2: no published source Isoloom knows (give its machine `os` and `app.steps`)",
            "✓ frob:1.0: https://example.com/frob@0123456789ab Dockerfile",
        ],
        "{o:?}"
    );
    let index = yaml(&std::fs::read_to_string(dir.join("vendor/bases/index.yml"))?);
    assert_eq!(yaml_str(&index, "`frob:1.0`.origin.repo"), "https://example.com/frob");
    assert_eq!(yaml_str(&index, "`frob:1.0`.origin.commit"), COMMIT);
    assert_eq!(yaml_str(&index, "`frob:1.0`.context"), "vendor/bases/frob-1.0");
    assert_eq!(std::fs::read_to_string(dir.join("vendor/bases/frob-1.0/Dockerfile"))?, "FROM debian:12\n");
    // The manifests are cloned once; the image's repository fetched at its commit.
    let calls = bin.calls()?;
    let verbs: Vec<&str> = calls.iter().filter_map(|c| c.get(1).map(String::as_str)).collect();
    assert_eq!(verbs, ["clone", "init", "fetch", "checkout", "rev-parse"], "{calls:?}");
    assert!(
        calls
            .iter()
            .any(|c| c.iter().any(|w| w == "https://example.com/frob") && c.iter().any(|w| w == COMMIT)),
        "{calls:?}"
    );
    Ok(())
}
