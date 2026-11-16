use std::ffi::OsString;

use super::*;
use crate::testing::folder;

/// A test's outcome: a failed step is its report.
type Outcome = std::result::Result<(), Box<dyn std::error::Error>>;

/// A host with these environment variables.
fn host(vars: &[(&str, &str)]) -> Host {
    Host::new(vars.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))))
}

/// The region `d` gives `cloud`.
fn region<'d>(d: &'d Defaults, cloud: &str) -> Option<&'d str> {
    d.cloud.get(cloud)?.region.as_deref()
}

#[test]
fn the_users_file_is_in_isoloom_home() {
    assert_eq!(user_file(&host(&[("ISOLOOM_HOME", "/srv/iso")])), PathBuf::from("/srv/iso/defaults.yml"));
}

#[test]
fn layers_merge_in_order_and_each_leaf_names_the_last_that_set_it() -> Outcome {
    let (home, project) = (folder("defaults-layers-home")?, folder("defaults-layers-project")?);
    let (home, project) = (home.path(), project.path());
    std::fs::write(home.join("defaults.yml"), "cloud: { aws: { region: from-user }, gcp: { region: from-user } }\n")?;
    std::fs::write(project.join(PROJECT_FILE), "cloud: { aws: { region: from-project } }\n")?;
    let home_text = home.to_string_lossy().into_owned();
    let sets = ["defaults.vagrant.provider=libvirt".to_owned(), "machines.web.os=alpine".to_owned()];
    let resolved = load(&Sources {
        project,
        images_file: None,
        profile: None,
        sets: &sets,
        host: host(&[("ISOLOOM_HOME", &home_text), ("ISOLOOM_CLOUD__AZURE__REGION", "from-env")]),
    })?;
    let d = &resolved.defaults;
    assert_eq!(region(d, "aws"), Some("from-project"));
    assert_eq!(region(d, "gcp"), Some("from-user"), "a file that sets one region leaves the others alone");
    assert_eq!(region(d, "azure"), Some("from-env"));
    assert_eq!(d.vagrant.provider.as_deref(), Some("libvirt"));
    let user = format!("user file {}", home.join("defaults.yml").display());
    let sources: Vec<(&str, &str)> = resolved.sources.iter().map(|(k, s)| (k.as_str(), s.as_str())).collect();
    assert_eq!(
        sources,
        [
            ("cloud.gcp.region", user.as_str()),
            ("cloud.aws.region", &format!("project file {}", project.join(PROJECT_FILE).display())),
            ("cloud.azure.region", "environment ISOLOOM_CLOUD__AZURE__REGION"),
            ("vagrant.provider", "-s defaults.vagrant.provider=libvirt"),
        ],
        "a `-s` without the `defaults.` prefix is the spec's, not a layer"
    );
    Ok(())
}

#[test]
fn a_layer_file_that_cant_be_read_is_an_error_unlike_a_missing_one() -> Outcome {
    let project = folder("defaults-unreadable")?;
    let project = project.path();
    // A home folder that doesn't exist: no user file.
    let home = project.join("no-home").to_string_lossy().into_owned();
    let sources = |folder| Sources {
        project: folder,
        images_file: None,
        profile: None,
        sets: &[],
        host: host(&[("ISOLOOM_HOME", &home)]),
    };
    assert_eq!(load(&sources(project))?.defaults, Defaults::default(), "no file, no layer");
    std::fs::create_dir(project.join(PROJECT_FILE))?;
    assert!(matches!(load(&sources(project)), Err(Error::Read { path, .. }) if path == project.join(PROJECT_FILE)));
    Ok(())
}

#[test]
fn only_the_defaults_sections_are_read_from_the_environment() {
    let vars = [
        ("ISOLOOM_VAGRANT__PROVIDER", "qemu"),
        ("ISOLOOM_HOME", "/x"),
        ("PATH", "/bin"),
        ("ISOLOOM_IMAGES__ACCESS__VM", "ubuntu@24.04"),
        ("ISOLOOM_TAGS__TEAM", "ops"),
    ];
    assert_eq!(
        from_env(vars.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))),
        [
            ("images.access.vm".to_owned(), "ubuntu@24.04".to_owned()),
            ("tags.team".to_owned(), "ops".to_owned()),
            ("vagrant.provider".to_owned(), "qemu".to_owned())
        ]
    );
}

#[test]
fn a_tag_from_the_environment_or_a_set_keeps_its_text() {
    let env = from_env(
        [
            ("ISOLOOM_TAGS__ACCOUNT".to_owned(), "0x1F".to_owned()),
            ("ISOLOOM_TAGS__RUN".to_owned(), "1e3".to_owned()),
        ]
        .into_iter(),
    );
    let layers: Vec<Layer> = env
        .iter()
        .map(|(k, v)| Layer {
            source: "environment".into(),
            value: at_path(k, typed(k, v)),
        })
        .collect();
    let tags = resolve(&layers).unwrap().defaults.tags;
    assert_eq!((tags["account"].as_str(), tags["run"].as_str()), ("0x1F", "1e3"));
    let spec = override_spec_with("version: 1\n", None, &["tags.account=0x1F".to_owned(), "x.port=8080".to_owned()]).unwrap();
    let spec: Value = serde_yaml_ng::from_str(&spec).unwrap();
    assert_eq!(spec["tags"]["account"], Value::from("0x1F"), "a tag stays text");
    assert_eq!(spec["x"]["port"], Value::from(8080), "another value is as YAML reads it");
}

#[test]
fn a_tag_set_as_a_number_or_a_flag_is_its_text() {
    let layer = |key: &str, value: &str| Layer {
        source: format!("-s defaults.{key}"),
        value: at_path(key, typed(key, value)),
    };
    let resolved = resolve(&[layer("tags.cost", "12"), layer("tags.billable", "true"), layer("tags.team", "ops")]).unwrap();
    let tags: Vec<(&str, &str)> = resolved.defaults.tags.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(tags, [("cost", "12"), ("billable", "true"), ("team", "ops")]);
    let file: Defaults = serde_yaml_ng::from_str("tags: { cost-center: 1234, ratio: 1.5 }").unwrap();
    assert_eq!((file.tags["cost-center"].as_str(), file.tags["ratio"].as_str()), ("1234", "1.5"));
    let e = serde_yaml_ng::from_str::<Defaults>("tags: { team: [a, b] }").unwrap_err().to_string();
    assert!(e.contains("a tag's value: text, a number or true/false"), "a list is no tag: {e}");
}

/// The defaults of one layer from `source` setting each tag of `tags`.
fn tagged(source: &str, tags: &[(&str, &str)]) -> Result<Resolved> {
    let layers: Vec<Layer> = tags
        .iter()
        .map(|(k, v)| Layer {
            source: source.to_owned(),
            value: at_path(&format!("tags.{k}"), typed("tags", v)),
        })
        .collect();
    resolve(&layers)
}

#[test]
fn the_specs_tags_go_over_the_defaults() -> Outcome {
    let defaults = tagged("project file", &[("cost-center", "1"), ("team", "ops")])?;
    let own: IndexMap<String, String> = [("team".to_owned(), "platform".to_owned())].into_iter().collect();
    let merged = defaults.tags_under(&own)?;
    assert_eq!(merged["cost-center"], "1");
    assert_eq!(merged["team"], "platform");
    Ok(())
}

#[test]
fn a_tag_of_the_defaults_follows_the_specs_rules_and_names_its_source() -> Outcome {
    let wrong = tagged("environment ISOLOOM_TAGS__9X", &[("9x", "a")]).unwrap_err().to_string();
    assert!(
        wrong.starts_with("defaults (environment ISOLOOM_TAGS__9X): tag `9x`: a key of at most 63"),
        "{wrong}"
    );
    let own_tag = tagged("user file", &[("managed-by", "me")]).unwrap_err().to_string();
    assert!(own_tag.contains("Isoloom's own tag"), "{own_tag}");
    // One that clashes with the spec's is the defaults', named with its source.
    let team = tagged("environment ISOLOOM_TAGS__TEAM", &[("team", "dev")])?;
    let spec: IndexMap<String, String> = [("Team".to_owned(), "ops".to_owned())].into_iter().collect();
    let clash = team.tags_under(&spec).unwrap_err().to_string();
    assert_eq!(
        clash,
        "defaults (environment ISOLOOM_TAGS__TEAM): tag `team`: `team` and `Team` are one tag on Azure, whose tag names ignore case: name them apart"
    );
    Ok(())
}

#[test]
fn a_set_is_a_dotted_key_then_its_value() {
    assert!(matches!(parse_set("a.b=c=d"), Ok(("a.b", "c=d"))));
    assert!(matches!(parse_set("a.b"), Err(Error::SetShape { .. })));
    for s in ["=1", "a..b=1", ".a=1"] {
        assert!(matches!(parse_set(s), Err(Error::SetKey { .. })), "{s}");
    }
}

#[test]
fn no_spec_set_leaves_the_text_untouched() -> Outcome {
    let text = "# The lab.\nname: lab # its name\nmachines:\n  web: { os: debian@12 }\n";
    assert_eq!(override_spec(text, &["defaults.cloud.aws.region=eu-west-1".into()])?, text, "comments and all");
    let set: Value = serde_yaml_ng::from_str(&override_spec(text, &["machines.web.os=ubuntu@24.04".into()])?)?;
    assert_eq!(set["machines"]["web"]["os"], Value::from("ubuntu@24.04"));
    Ok(())
}

#[test]
fn an_empty_mapping_is_a_leaf_of_its_own() -> Outcome {
    let v: Value = serde_yaml_ng::from_str("cloud: { aws: {} }\nvagrant: { provider: qemu }\n")?;
    assert_eq!(leaves(&v, ""), ["cloud.aws", "vagrant.provider"]);
    Ok(())
}
