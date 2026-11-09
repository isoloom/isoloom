//! Defaults: layers merge leaf by leaf, the last one winning; `-s` overrides the spec or the
//! defaults; the clouds' built-in regions are what the generators write.

use isoloom_spec::defaults::{self, Layer, from_env, override_spec, resolve};
use isoloom_spec::{Target, load, parse};
use isoloom_targets::{Lab, generate};

use crate::testing::Outcome;

/// YAML text as a value.
fn yaml(text: &str) -> Outcome<serde_yaml_ng::Value> {
    Ok(serde_yaml_ng::from_str(text)?)
}

#[test]
fn later_layers_win_leaf_by_leaf() -> Outcome {
    let r = resolve(&[
        Layer {
            source: "user".into(),
            value: yaml("cloud: { aws: { region: eu-west-1 }, azure: { region: westeurope } }\nvagrant: { provider: libvirt }\n")?,
        },
        Layer {
            source: "project".into(),
            value: yaml("cloud: { aws: { region: us-east-1 } }\n")?,
        },
    ])
    .unwrap();
    assert_eq!(r.defaults.cloud["aws"].region.as_deref(), Some("us-east-1"));
    assert_eq!(r.defaults.cloud["azure"].region.as_deref(), Some("westeurope"));
    assert_eq!(r.defaults.vagrant.provider.as_deref(), Some("libvirt"));
    assert!(r.sources.contains(&("cloud.aws.region".into(), "project".into())));
    assert!(r.sources.contains(&("cloud.azure.region".into(), "user".into())));
    // A typo is an error that names the layer.
    let bad = resolve(&[Layer {
        source: "user".into(),
        value: yaml("vagrant: { provder: libvirt }\n")?,
    }]);
    assert!(bad.unwrap_err().to_string().contains("user"));
    Ok(())
}

#[test]
fn environment_and_sets_reach_the_defaults() {
    let env = from_env(
        vec![
            ("ISOLOOM_CLOUD__AWS__REGION".to_owned(), "eu-west-1".to_owned()),
            ("ISOLOOM_HOME".to_owned(), "/x".to_owned()),
            ("ISOLOOM_DERIVED".to_owned(), "0".to_owned()),
            ("ISOLOOM_VAGRANT__PROVIDER".to_owned(), "qemu".to_owned()),
        ]
        .into_iter(),
    );
    assert_eq!(
        env,
        vec![
            ("cloud.aws.region".to_owned(), "eu-west-1".to_owned()),
            ("vagrant.provider".to_owned(), "qemu".to_owned())
        ]
    );
    assert_eq!(defaults::parse_set("defaults.cloud.aws.region=x").unwrap(), ("defaults.cloud.aws.region", "x"));
    assert!(defaults::parse_set("nonsense").is_err());
    assert!(defaults::parse_set("a..b=1").is_err());
}

#[test]
fn sets_override_the_spec_before_it_is_parsed() {
    let text = std::fs::read_to_string(crate::testing::examples().join("hello-stack/isoloom.yml")).unwrap();
    let out = override_spec(
        &text,
        &[
            "machines.web.os=ubuntu@24.04".into(),
            "networks.app.internet=true".into(),
            "defaults.cloud.aws.region=eu-west-1".into(),
        ],
    )
    .unwrap();
    let spec = parse(&out).unwrap();
    assert_eq!(spec.machines["web"].os.map(isoloom_spec::syntax::OsRef::name), Some("ubuntu-24.04"));
    assert!(spec.networks["app"].internet);
    // Without spec overrides the text is untouched (comments and all).
    assert_eq!(override_spec(&text, &["defaults.cloud.aws.region=eu-west-1".into()]).unwrap(), text);
}

/// The default of `cloud-vm/<cloud>/main.tf`'s `region` variable, and of its run manifest's
/// `region` param (what a runner gives Terraform when nobody sets it).
fn regions(files: &[isoloom_targets::GeneratedFile], cloud: &str) -> Outcome<(String, String)> {
    let module = crate::testing::file(files, &format!("cloud-vm/{cloud}/main.tf"))?;
    let block = module
        .split("variable \"region\"")
        .nth(1)
        .and_then(|b| b.split("\n}").next())
        .unwrap_or_default();
    let variable = block.split("default").nth(1).and_then(|d| d.split('"').nth(1)).unwrap_or_default();
    let manifest = isoloom_manifest::parse(crate::testing::file(files, &format!("cloud-vm/{cloud}/run.json"))?)?;
    let param = manifest
        .params
        .iter()
        .find(|p| p.name.as_str() == "region")
        .and_then(|p| p.default.clone())
        .unwrap_or_default();
    Ok((variable.to_owned(), param))
}

#[test]
fn a_defaults_region_reaches_the_module_and_the_run_manifest_alike() -> Outcome {
    let lab = Lab::new(load(&crate::testing::examples().join("solo-web"))?);
    let files = generate(&lab, Target::CloudVm)?;
    for (cloud, region) in isoloom_targets::default_regions() {
        assert_eq!(regions(&files, cloud)?, (region.to_owned(), region.to_owned()), "{cloud}");
    }
    // The person's region, from their defaults: the run manifest's param defaulted to the
    // built-in one, which the runner gave Terraform, undoing it.
    let r = resolve(&[Layer {
        source: "t".into(),
        value: yaml("cloud: { aws: { region: eu-west-1 } }\n")?,
    }])?;
    let mine = generate(&lab.clone().with_regions(r.defaults.regions()), Target::CloudVm)?;
    assert_eq!(regions(&mine, "aws")?, ("eu-west-1".to_owned(), "eu-west-1".to_owned()));
    assert_eq!(
        regions(&files, "azure")?,
        ("swedencentral".to_owned(), "swedencentral".to_owned()),
        "other clouds keep theirs"
    );
    let docker = generate(&lab, Target::CloudDocker)?;
    assert!(crate::testing::file(&docker, "cloud-docker/aws/main.tf")?.contains("default = \"eu-west-3\""));
    Ok(())
}
