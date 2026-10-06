//! Defaults: layers merge leaf by leaf, the last one winning; `-s` overrides the spec or the
//! defaults; the clouds' built-in regions are what the generators write.

use std::path::Path;

use isoloom_core::defaults::{self, BUILTIN_REGIONS, Layer, apply_to_files, from_env, override_spec, resolve, set_region};
use isoloom_core::{Target, generate, load, parse};

fn yaml(text: &str) -> serde_yaml_ng::Value {
    serde_yaml_ng::from_str(text).unwrap()
}

#[test]
fn later_layers_win_leaf_by_leaf() {
    let r = resolve(vec![
        Layer {
            source: "user".into(),
            value: yaml("cloud: { aws: { region: eu-west-1 }, azure: { region: westeurope } }\nvagrant: { provider: libvirt }\n"),
        },
        Layer {
            source: "project".into(),
            value: yaml("cloud: { aws: { region: us-east-1 } }\n"),
        },
    ])
    .unwrap();
    assert_eq!(r.defaults.cloud["aws"].region.as_deref(), Some("us-east-1"));
    assert_eq!(r.defaults.cloud["azure"].region.as_deref(), Some("westeurope"));
    assert_eq!(r.defaults.vagrant.provider.as_deref(), Some("libvirt"));
    assert!(r.sources.contains(&("cloud.aws.region".into(), "project".into())));
    assert!(r.sources.contains(&("cloud.azure.region".into(), "user".into())));
    // A typo is an error that names the layer.
    let bad = resolve(vec![Layer {
        source: "user".into(),
        value: yaml("vagrant: { provder: libvirt }\n"),
    }]);
    assert!(bad.unwrap_err().contains("user"));
}

#[test]
fn environment_and_sets_reach_the_defaults() {
    let env = from_env(
        vec![
            ("ISOLOOM_CLOUD__AWS__REGION".to_string(), "eu-west-1".to_string()),
            ("ISOLOOM_HOME".to_string(), "/x".to_string()),
            ("ISOLOOM_DERIVED".to_string(), "0".to_string()),
            ("ISOLOOM_VAGRANT__PROVIDER".to_string(), "qemu".to_string()),
        ]
        .into_iter(),
    );
    assert_eq!(
        env,
        vec![
            ("cloud.aws.region".to_string(), "eu-west-1".to_string()),
            ("vagrant.provider".to_string(), "qemu".to_string())
        ]
    );
    assert_eq!(defaults::parse_set("defaults.cloud.aws.region=x").unwrap(), ("defaults.cloud.aws.region", "x"));
    assert!(defaults::parse_set("nonsense").is_err());
    assert!(defaults::parse_set("a..b=1").is_err());
}

#[test]
fn sets_override_the_spec_before_it_is_parsed() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-stack/isoloom.yml")).unwrap();
    let out = override_spec(
        &text,
        &[
            "machines.web.vm.os=ubuntu-24.04".into(),
            "networks.app.internet=true".into(),
            "defaults.cloud.aws.region=eu-west-1".into(),
        ],
    )
    .unwrap();
    let spec = parse(&out).unwrap();
    assert_eq!(spec.machines["web"].vm.as_ref().unwrap().os, "ubuntu-24.04");
    assert!(spec.networks["app"].internet);
    // Without spec overrides the text is untouched (comments and all).
    assert_eq!(override_spec(&text, &["defaults.cloud.aws.region=eu-west-1".into()]).unwrap(), text);
}

#[test]
fn builtin_regions_are_what_the_generators_write_and_can_be_replaced() {
    let spec = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/solo-web")).unwrap();
    let files = generate(&spec, Target::CloudVm).unwrap();
    for (cloud, region) in BUILTIN_REGIONS {
        let Some(f) = files.iter().find(|f| f.path == format!(".isoloom/cloud-vm/{cloud}/main.tf")) else {
            continue;
        };
        assert!(
            f.contents.contains(&format!("default = \"{region}\"")),
            "{cloud}: {}",
            f.contents.lines().take(40).collect::<Vec<_>>().join("\n")
        );
    }
    let r = resolve(vec![Layer {
        source: "t".into(),
        value: yaml("cloud: { aws: { region: eu-west-1 } }\n"),
    }])
    .unwrap();
    let out = apply_to_files(files, &r.defaults);
    let aws = out.iter().find(|f| f.path == ".isoloom/cloud-vm/aws/main.tf").unwrap();
    assert!(aws.contents.contains("default = \"eu-west-1\"") && !aws.contents.contains("default = \"eu-west-3\""));
    // Other clouds untouched.
    let az = out.iter().find(|f| f.path == ".isoloom/cloud-vm/azure/main.tf").unwrap();
    assert!(az.contents.contains("default = \"swedencentral\""));
    assert_eq!(set_region("no region here", "x"), "no region here");
}
