//! Golden fixtures, one folder per Dockerfile shape under `fixtures/`: its `Dockerfile`, and the
//! VM form it translates to (`vm.yml`) or why it is refused (`refused.txt`). A folder with a
//! `base.Dockerfile` vendors it as the fetched base its `FROM` names. `ISOLOOM_WRITE_FIXTURES=1`
//! rewrites the expected files.

use std::path::{Path, PathBuf};

use super::*;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dockerfile/fixtures")
}

/// The fetched bases a fixture vendors: `base.Dockerfile` as the image its `FROM` names.
fn sources(dir: &Path, dockerfile: &str) -> Sources {
    let mut out = Sources::default();
    let Ok(base) = std::fs::read_to_string(dir.join("base.Dockerfile")) else {
        return out;
    };
    for image in sources::wanted(dockerfile, &Sources::default()) {
        let origin = sources::publisher(&image).expect("a fixture's base has a publisher");
        out.insert(
            &image,
            Fetched {
                dockerfile: base.clone(),
                context: format!("vendor/bases/{}", image.replace(['/', ':'], "-")),
                args: origin.args.clone(),
                origin: format!("{}@{} {}", origin.repo, origin.commit, origin.file),
            },
        );
    }
    out
}

/// What fixture `dir` translates to: its VM form's YAML, or `refused: <why>` and the fix.
fn outcome(dir: &Path) -> (String, &'static str) {
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    let text = std::fs::read_to_string(dir.join("Dockerfile")).unwrap();
    let path = format!("build/{name}/Dockerfile");
    let build = Build {
        path: &path,
        text: &text,
        context: &format!("build/{name}"),
        args: &[],
        os: None,
    };
    match translate(&build, &sources(dir, &text)) {
        Ok(t) => (t.yaml, "vm.yml"),
        Err(u) => (format!("refused: {u}\nfix: {}\n", u.fix), "refused.txt"),
    }
}

#[test]
fn every_fixture_translates_to_its_golden_form() {
    let write = std::env::var_os("ISOLOOM_WRITE_FIXTURES").is_some();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(fixtures())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert!(dirs.len() >= 10, "the fixtures are there");
    let mut wrong = Vec::new();
    for dir in &dirs {
        let (got, file) = outcome(dir);
        let refused = dir.file_name().unwrap().to_string_lossy().starts_with("refused-");
        assert_eq!(file == "refused.txt", refused, "{}: {got}", dir.display());
        let expected = dir.join(file);
        if write {
            std::fs::write(&expected, &got).unwrap();
        } else if std::fs::read_to_string(&expected).ok().as_deref() != Some(got.as_str()) {
            wrong.push(expected.display().to_string());
        }
    }
    assert!(wrong.is_empty(), "not as expected (ISOLOOM_WRITE_FIXTURES=1 rewrites them): {wrong:?}");
}

#[test]
fn a_refusal_names_the_file_the_line_and_the_instruction() {
    let (copied, _) = outcome(&fixtures().join("refused-copy-from-image"));
    assert!(
        copied.starts_with("refused: build/refused-copy-from-image/Dockerfile line 2 (`COPY`): `--from=composer:2.8`"),
        "{copied}"
    );
    let (unknown, _) = outcome(&fixtures().join("refused-unknown-base"));
    assert!(
        unknown.contains("line 1 (`FROM`): `vulhub/tomcat:8.5` is in neither the service matrix nor the fetched bases"),
        "{unknown}"
    );
    assert!(unknown.contains("isoloom bases fetch"), "{unknown}");
}

#[test]
fn the_translation_is_an_explicit_vm_form_a_spec_can_hold() {
    let text = std::fs::read_to_string(fixtures().join("php-apache/Dockerfile")).unwrap();
    let build = Build {
        path: "build/dvwa/Dockerfile",
        text: &text,
        context: "build/dvwa",
        args: &[],
        os: None,
    };
    let t = translate(&build, &Sources::default()).unwrap();
    assert_eq!(t.os.to_string(), "debian@12");
    // The YAML's `app.steps` are the spec's own steps: they parse as an authored spec's.
    let reparsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&t.yaml).unwrap();
    let steps: Vec<Step> = serde_yaml_ng::from_value(reparsed["app"]["steps"].clone()).unwrap();
    assert_eq!(steps, t.steps);
    // The image's command is the VM's service, as the image's user and in its folder.
    assert!(matches!(t.steps.last(), Some(Step::Service(_))));
}

#[test]
fn a_build_argument_given_wins_over_the_default() {
    let text = "FROM debian:12\nARG FLAG=dev\nRUN echo \"$FLAG\" > /flag\n";
    let args = [("FLAG".to_owned(), "given".to_owned())];
    let build = Build {
        path: "Dockerfile",
        text,
        context: ".",
        args: &args,
        os: None,
    };
    let t = translate(&build, &Sources::default()).unwrap();
    assert!(t.yaml.contains("export FLAG=given"), "{}", t.yaml);
}

#[test]
fn the_machines_own_os_picks_where_a_service_installs() {
    let build = Build {
        path: "Dockerfile",
        text: "FROM mariadb:10.11\n",
        context: ".",
        args: &[],
        os: Some("rocky@9"),
    };
    assert_eq!(translate(&build, &Sources::default()).unwrap().os.to_string(), "rocky@9");
}
