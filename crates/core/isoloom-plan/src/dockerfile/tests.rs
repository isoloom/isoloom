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
    // The YAML's `app.steps` are the spec's own steps: they parse as an authored spec's, but
    // for the catalogue entry's recipe, which shows as one `catalogue:` line where its steps go.
    let reparsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&t.yaml).unwrap();
    let written = reparsed["app"]["steps"].as_sequence().unwrap();
    assert_eq!(written.iter().filter(|s| s.get("catalogue").is_some()).count(), 1, "{}", t.yaml);
    let own: Vec<Step> = written
        .iter()
        .filter(|s| s.get("catalogue").is_none())
        .map(|s| serde_yaml_ng::from_value(s.clone()).unwrap())
        .collect();
    // Each in order among the translation's steps, the recipe's around them.
    let mut rest = t.steps.iter();
    assert!(own.iter().all(|s| rest.any(|made| made == s)), "{}", t.yaml);
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
        text: "FROM python:3.12\n",
        context: ".",
        args: &[],
        os: Some("rocky@9"),
    };
    assert_eq!(translate(&build, &Sources::default()).unwrap().os.to_string(), "rocky@9");
}

#[test]
fn a_postgres_image_installs_the_exact_release_with_the_images_defaults() {
    let build = Build {
        path: "build/db/Dockerfile",
        text: "FROM postgres:10.5\nENV POSTGRES_PASSWORD=s3cret\nCOPY init.sql /docker-entrypoint-initdb.d/\n",
        context: "build/db",
        args: &[],
        os: None,
    };
    let t = translate(&build, &Sources::default()).unwrap();
    assert_eq!(t.os.to_string(), "ubuntu@18.04");
    assert!(t.yaml.contains("catalogue: postgres@10.5 on ubuntu@18.04"), "{}", t.yaml);
    assert!(t.files.values().any(|v| v == "s3cret"), "{:?}", t.files.keys());
    // Without a password the image refuses to start: so does the translation.
    let bare = Build {
        text: "FROM postgres:16\n",
        ..build
    };
    assert!(translate(&bare, &Sources::default()).unwrap_err().to_string().contains("POSTGRES_PASSWORD"));
}

#[test]
fn a_catalogue_image_brings_its_entry_recipe_with_the_images_settings() {
    let text = std::fs::read_to_string(fixtures().join("mysql-catalogue/Dockerfile")).unwrap();
    let build = Build {
        path: "build/db/Dockerfile",
        text: &text,
        context: "build/db",
        args: &[],
        os: None,
    };
    let t = translate(&build, &Sources::default()).unwrap();
    assert_eq!(t.os.to_string(), "debian@11");
    assert!(t.yaml.contains("catalogue: mysql@5.7.44 on debian@11"), "{}", t.yaml);
    let files: Vec<&str> = t.files.keys().map(ProjectPath::as_str).collect();
    assert!(
        files
            .iter()
            .any(|f| f.starts_with("catalogue/mysql/") && f.ends_with("files/docker-entrypoint.sh")),
        "{files:?}"
    );
    // The root password from the image's environment, in its value file.
    assert!(t.files.values().any(|v| v == "trading_platform"), "{files:?}");
    // The entry's own service runs: the Dockerfile keeps the image's command.
    assert!(!t.yaml.contains("isoloom-app"), "{}", t.yaml);
}

#[test]
fn a_php_image_installs_its_series_release_and_its_variants_server() {
    let build = |text: &'static str| Build {
        path: "build/web/Dockerfile",
        text,
        context: "build/web",
        args: &[],
        os: None,
    };
    let fpm = translate(&build("FROM php:7.4-fpm\nRUN docker-php-ext-install pdo_mysql\n"), &Sources::default()).unwrap();
    assert_eq!(fpm.os.to_string(), "debian@12");
    assert!(fpm.yaml.contains("catalogue: php@7.4.33 on debian@12"), "{}", fpm.yaml);
    let files: Vec<&str> = fpm.files.keys().map(ProjectPath::as_str).collect();
    assert!(files.iter().any(|f| f.ends_with("files/php-fpm")), "{files:?}");
    // A patch release the archive no longer carries: refused, naming the releases it has.
    let old = translate(&build("FROM php:7.1.3-apache\n"), &Sources::default()).unwrap_err().to_string();
    assert!(old.contains("no release 7.1.3") && old.contains("7.1.33"), "{old}");
}

#[test]
fn a_mongo_image_installs_the_exact_build_and_its_service_reads_the_images_environment() {
    let build = Build {
        path: "build/db/Dockerfile",
        text: "FROM mongo:4.0.4\nENV MONGO_INITDB_ROOT_USERNAME=admin MONGO_INITDB_ROOT_PASSWORD=s3cret\nCOPY seed.js /docker-entrypoint-initdb.d/\n",
        context: "build/db",
        args: &[],
        os: None,
    };
    let t = translate(&build, &Sources::default()).unwrap();
    assert_eq!(t.os.to_string(), "ubuntu@18.04");
    assert!(t.yaml.contains("catalogue: mongodb@4.0.4 on ubuntu@18.04"), "{}", t.yaml);
    // The entry's own service runs, and reads the environment the Dockerfile set.
    assert!(!t.yaml.contains("isoloom-app"), "{}", t.yaml);
    assert!(t.yaml.contains(IMAGE_ENV), "{}", t.yaml);
    let latest = Build {
        text: "FROM mongo:7\n",
        ..build
    };
    let newest = translate(&latest, &Sources::default()).unwrap();
    assert!(newest.yaml.contains("catalogue: mongodb@7.0.28 on ubuntu@20.04"), "{}", newest.yaml);
}
