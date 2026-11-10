//! Defaults: layers merge leaf by leaf, the last one winning; `-s` overrides the spec or the
//! defaults; the clouds' built-in regions are what the generators write.

use std::ffi::OsString;

use isoloom_spec::defaults::{self, Layer, Sources, from_env, resolve};
use isoloom_spec::host::Host;
use isoloom_spec::{Target, load, load_profiled};
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
fn environment_and_sets_reach_the_defaults() -> Outcome {
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
    let project = crate::testing::examples().join("hello-stack");
    let with_sets = |sets: &[String]| {
        defaults::load(&Sources {
            project: &project,
            images_file: None,
            profile: None,
            sets,
            host: Host::new([(OsString::from("ISOLOOM_HOME"), OsString::from("/nonexistent/isoloom-home"))]),
        })
    };
    let set = with_sets(&["defaults.cloud.aws.region=x".into()])?;
    assert_eq!(set.defaults.cloud["aws"].region.as_deref(), Some("x"));
    assert!(with_sets(&["nonsense".into()]).is_err(), "a set without `=`");
    assert!(with_sets(&["a..b=1".into()]).is_err(), "a key with an empty part");
    Ok(())
}

#[test]
fn sets_override_the_spec_before_it_is_parsed() -> Outcome {
    let sets = [
        "machines.web.os=ubuntu@24.04".into(),
        "networks.app.internet=true".into(),
        "defaults.cloud.aws.region=eu-west-1".into(),
    ];
    let spec = load_profiled(&crate::testing::examples().join("hello-stack"), None, &sets)?;
    assert_eq!(spec.machines["web"].os.map(isoloom_spec::syntax::OsRef::name), Some("ubuntu-24.04"));
    assert!(spec.networks["app"].internet);
    Ok(())
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
