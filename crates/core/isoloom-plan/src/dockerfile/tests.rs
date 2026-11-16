//! Golden fixtures, one folder per Dockerfile shape under the crate's
//! `tests/fixtures/dockerfile/`: its `Dockerfile`, and the VM form it translates to (`vm.yml`) or
//! why it is refused (`refused.txt`). A folder with a `base.Dockerfile` vendors it as the fetched
//! base its `FROM` names. `ISOLOOM_WRITE_FIXTURES=1` rewrites the expected files.

use std::path::{Path, PathBuf};

use super::*;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dockerfile")
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

/// `text` as the Dockerfile of `build/db`, with no argument and no `os`.
fn db(text: &str) -> Build<'_> {
    Build {
        path: "build/db/Dockerfile",
        text,
        context: "build/db",
        args: &[],
        os: None,
    }
}

/// The translation of [`db`]`(text)`.
fn translated_db(text: &str) -> Translation {
    translate(&db(text), &Sources::default()).unwrap()
}

/// The translation's steps as its YAML writes them (`app.steps`).
fn written(t: &Translation) -> Vec<serde_yaml_ng::Value> {
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&t.yaml).unwrap_or_else(|e| panic!("{e}\n{}", t.yaml));
    yaml["app"]["steps"].as_sequence().cloned().unwrap_or_default()
}

/// Each written step's kind: its one key.
fn kinds(t: &Translation) -> Vec<String> {
    written(t)
        .iter()
        .filter_map(|s| s.as_mapping().and_then(|m| m.keys().next()).and_then(|k| k.as_str()).map(str::to_owned))
        .collect()
}

/// What the written steps pin: each `packages` entry's version and each fetched `file`'s URL.
fn pinned(t: &Translation) -> Vec<String> {
    let mut out = Vec::new();
    for step in written(t) {
        for package in step["packages"].as_sequence().into_iter().flatten() {
            out.extend(package["version"].as_str().map(str::to_owned));
        }
        out.extend(step["file"]["url"].as_str().map(str::to_owned));
    }
    out
}

/// The lines of the written `command` steps' scripts.
fn script_lines(t: &Translation) -> Vec<String> {
    written(t)
        .iter()
        .filter_map(|s| s["command"].as_str())
        .flat_map(|script| script.lines().map(str::to_owned).collect::<Vec<_>>())
        .collect()
}

/// The services the written steps run, by name.
fn services(t: &Translation) -> Vec<String> {
    written(t).iter().filter_map(|s| s["service"]["name"].as_str().map(str::to_owned)).collect()
}

/// The text of the file the translation brings at `path`.
fn file<'t>(t: &'t Translation, path: &str) -> Option<&'t str> {
    t.files.iter().find(|(p, _)| p.as_str() == path).map(|(_, text)| text.as_str())
}

/// The translation writes the catalogue entry's recipe for `release` out as steps (no
/// `catalogue:` step a spec can't hold), pinning that release.
#[track_caller]
fn written_out(t: &Translation, release: &str) {
    isoloom_test_support::assert_every!(kinds(t), |k: &String| k != "catalogue");
    let pins = pinned(t);
    assert!(pins.iter().any(|p| p.contains(release)), "release {release} in none of {pins:?}");
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
    // The YAML's `app.steps` are the spec's own steps, the catalogue entry's recipe written out:
    // each parses as an authored spec's, and they are the translation's steps, in order.
    let reparsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&t.yaml).unwrap();
    let written: Vec<Step> = reparsed["app"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|s| serde_yaml_ng::from_value(s.clone()).unwrap_or_else(|e| panic!("{e}: {s:?}\n{}", t.yaml)))
        .collect();
    assert_eq!(written, t.steps, "{}", t.yaml);
    // As a machine's explicit VM form, it is a valid spec.
    let spec = format!(
        "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  web:\n    networks: {{ lan: 10 }}\n{}",
        t.yaml.lines().map(|l| format!("    {l}\n")).collect::<Vec<_>>().concat()
    );
    if let Err(e) = isoloom_spec::parse(&spec) {
        panic!("{e}\n{spec}");
    }
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
    assert!(script_lines(&t).contains(&"export FLAG=given".to_owned()), "{:?}", script_lines(&t));
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
    let t = translated_db("FROM postgres:10.5\nENV POSTGRES_PASSWORD=s3cret\nCOPY init.sql /docker-entrypoint-initdb.d/\n");
    assert_eq!(t.os.to_string(), "ubuntu@18.04");
    written_out(&t, "10.5");
    assert_eq!(file(&t, "catalogue/postgres/values/0"), Some("s3cret"), "the password's value file");
    // Without a password the image refuses to start: so does the translation.
    assert_eq!(
        translate(&db("FROM postgres:16\n"), &Sources::default()).unwrap_err().to_string(),
        "build/db/Dockerfile: the image's environment doesn't set `POSTGRES_PASSWORD`, which the catalogue's `postgres` needs"
    );
}

#[test]
fn a_catalogue_image_brings_its_entry_recipe_with_the_images_settings() {
    let text = std::fs::read_to_string(fixtures().join("mysql-catalogue/Dockerfile")).unwrap();
    let t = translated_db(&text);
    assert_eq!(t.os.to_string(), "debian@11");
    written_out(&t, "5.7.44");
    assert!(
        file(&t, "catalogue/mysql/.isoloom/catalogue/mysql/files/docker-entrypoint.sh").is_some(),
        "{:?}",
        t.files.keys()
    );
    // The root password from the image's environment, in its value file.
    assert_eq!(file(&t, "catalogue/mysql/values/0"), Some("trading_platform"));
    // The entry's own service runs: the Dockerfile keeps the image's command.
    assert_eq!(services(&t), ["mysql"]);
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
    written_out(&fpm, "7.4.33");
    assert!(
        file(&fpm, "catalogue/php/.isoloom/catalogue/php/files/php-fpm").is_some(),
        "{:?}",
        fpm.files.keys()
    );
    assert_eq!(services(&fpm), ["php-fpm"]);
    // A patch release the archive no longer carries: refused, naming the releases it has.
    let old = translate(&build("FROM php:7.1.3-apache\n"), &Sources::default()).unwrap_err().to_string();
    let (refusal, releases) = old.split_once(" (it has ").unwrap_or_else(|| panic!("{old}"));
    assert_eq!(
        refusal,
        "build/web/Dockerfile line 1 (`FROM`): the catalogue's `php` has no release 7.1.3 with a VM form"
    );
    assert!(releases.split(", ").any(|r| r == "7.1.33"), "the series' release among {releases}");
}

#[test]
fn a_mongo_image_installs_the_exact_build_and_its_service_reads_the_images_environment() {
    let t =
        translated_db("FROM mongo:4.0.4\nENV MONGO_INITDB_ROOT_USERNAME=admin MONGO_INITDB_ROOT_PASSWORD=s3cret\nCOPY seed.js /docker-entrypoint-initdb.d/\n");
    assert_eq!(t.os.to_string(), "ubuntu@18.04");
    written_out(&t, "4.0.4");
    // The entry's own service runs, and reads the environment the Dockerfile set, written to
    // IMAGE_ENV.
    assert_eq!(services(&t), ["mongodb"]);
    let written_env: Vec<String> = script_lines(&t)
        .iter()
        .filter_map(|l| {
            l.strip_prefix("printf %s ")?
                .strip_suffix(&format!(" | base64 -d > {IMAGE_ENV}"))
                .map(str::to_owned)
        })
        .map(|b64| String::from_utf8(isoloom_test_support::b64_decode(&b64).unwrap()).unwrap())
        .collect();
    assert_eq!(
        written_env,
        ["PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\nMONGO_INITDB_ROOT_USERNAME=admin\nMONGO_INITDB_ROOT_PASSWORD=s3cret\n"]
    );
    let newest = translate(&db("FROM mongo:7\n"), &Sources::default()).unwrap();
    written_out(&newest, "7.0.28");
}

#[test]
fn a_jenkins_image_installs_the_lts_release_and_runs_the_dockerfiles_steps_as_jenkins() {
    let text = std::fs::read_to_string(fixtures().join("jenkins-catalogue/Dockerfile")).unwrap();
    let build = Build {
        path: "build/jenkins/Dockerfile",
        text: &text,
        context: "build/jenkins",
        args: &[],
        os: None,
    };
    let t = translate(&build, &Sources::default()).unwrap();
    assert_eq!(t.os.to_string(), "debian@13");
    written_out(&t, "2.580.2");
    // `jenkins-plugin-cli` runs as the image's user, with its environment.
    assert!(
        script_lines(&t).contains(&"export JENKINS_HOME=/var/jenkins_home".to_owned()),
        "{:?}",
        script_lines(&t)
    );
    // The entry's service runs Jenkins: the Dockerfile keeps the image's command.
    assert_eq!(services(&t), ["jenkins"]);
}

#[test]
fn a_file_copied_out_of_composers_image_is_the_catalogue_entry_for_it() {
    let text = std::fs::read_to_string(fixtures().join("composer-copy/Dockerfile")).unwrap();
    let build = Build {
        path: "build/web/Dockerfile",
        text: &text,
        context: "build/web",
        args: &[],
        os: None,
    };
    let t = translate(&build, &Sources::default()).unwrap();
    // The series' newest release, on the VM's own OS (the php image's).
    written_out(&t, "2.8.12");
}

#[test]
fn a_dockerfile_a_vm_cant_be_is_refused_at_the_line_that_says_so() {
    let refused = |text: &str| translate(&db(text), &Sources::default()).map(|_| ()).unwrap_err().to_string();
    let found: Vec<String> = [
        "",
        "ARG V=1\nRUN make\n",
        "FROM debian:12\nCOPY ../shared/a.conf /etc/a.conf\n",
        "FROM debian:12\nCOPY sub\\\\a.conf /etc/a.conf\n",
        "FROM debian:12 AS build\nRUN make\nFROM debian:12\nCOPY --from=build / /srv/\n",
    ]
    .into_iter()
    .map(refused)
    .collect();
    assert_eq!(
        found,
        [
            "build/db/Dockerfile line 1 (`FROM`): the Dockerfile has no `FROM`",
            "build/db/Dockerfile line 1 (`FROM`): the Dockerfile has no `FROM`",
            "build/db/Dockerfile line 2 (`COPY`): `../shared/a.conf` goes out of the build context",
            "build/db/Dockerfile line 2 (`COPY`): `sub\\\\a.conf` goes out of the build context",
            "build/db/Dockerfile line 4 (`COPY`): it copies a stage's whole filesystem: a VM can't be two systems"
        ]
    );
}

#[test]
fn a_copy_with_chmod_sets_the_mode_of_what_it_placed() {
    let t = translated_db("FROM debian:12\nCOPY --chmod=0755 run.sh /usr/local/bin/\n");
    let modes: Vec<String> = script_lines(&t).into_iter().filter(|l| l.starts_with("chmod -R")).collect();
    assert_eq!(modes, ["chmod -R 0755 \"$dest\""]);
}
