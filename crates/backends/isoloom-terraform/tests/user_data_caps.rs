//! A cloud that caps a machine's user data (AWS, Azure, Google Cloud) refuses a machine whose
//! user data is over its cap as the cloud counts it, naming the machine; the other clouds keep
//! their modules.

pub mod testing;

use isoloom_render::configure::cloud_init::Limit;
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule};
use testing::{Outcome, plan};

/// `bytes` of text gzip can't shrink much: hexadecimal from a fixed pseudo-random sequence.
fn noise(bytes: usize) -> String {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..bytes)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            char::from_digit(u32::try_from(x % 16).unwrap_or(0), 16).unwrap_or('0')
        })
        .collect()
}

/// A lab of one Linux machine, `web`, whose recipe runs a command carrying `bytes` of noise.
fn lab(bytes: usize) -> String {
    format!(
        "version: 1
name: caps
networks:
  lab: {{ cidr: 10.66.0.0/24, internet: true }}
machines:
  web:
    os: debian@12
    networks: {{ lab: 10 }}
    app:
      steps:
        - command: [sh, -c, \"echo {} > /srv/noise\"]
          creates: /srv/noise
",
        noise(bytes)
    )
}

/// Why `cloud` refuses `yaml`'s lab, if it does; and that rendering it says the same.
fn refusal(cloud: Cloud, yaml: &str) -> Outcome<Option<String>> {
    let env = plan(yaml, Target::CloudVm)?;
    let module = CloudVmModule::new(cloud);
    let why = module.refusal(&env);
    let rendered = module.render(&env).err().map(|e| e.to_string());
    assert_eq!(
        rendered.is_some(),
        why.is_some(),
        "{}: the refusal ({why:?}) and the render ({rendered:?}) agree",
        cloud.id()
    );
    if let (Some(why), Some(rendered)) = (&why, &rendered) {
        assert!(rendered.contains(why.as_str()), "{rendered} / {why}");
    }
    Ok(why)
}

#[test]
fn a_small_set_up_fits_every_cloud() -> Outcome {
    for cloud in Cloud::ALL {
        assert_eq!(refusal(cloud, &lab(100))?, None, "{}", cloud.id());
    }
    Ok(())
}

#[test]
fn aws_counts_the_gzip_and_refuses_over_16_kb_naming_the_machine() -> Outcome {
    // Hex gzips to about half: 40 KB of it is over AWS's 16 KB gzipped, under Azure's 64 KB.
    let yaml = lab(40_000);
    let why = refusal(Cloud::Aws, &yaml)?.ok_or("AWS takes it")?;
    assert!(why.starts_with("machine `web`: its user data is "), "{why}");
    assert!(why.contains(&format!("as AWS counts it, over its {}-byte limit", Limit::Aws.bytes())), "{why}");
    for cloud in [Cloud::Azure, Cloud::Gcp] {
        assert_eq!(refusal(cloud, &yaml)?, None, "{}", cloud.id());
    }
    Ok(())
}

#[test]
fn azure_refuses_over_its_custom_data_cap_and_gcp_takes_up_to_256_kb() -> Outcome {
    let yaml = lab(70_000);
    let why = refusal(Cloud::Azure, &yaml)?.ok_or("Azure takes it")?;
    assert!(why.contains("as Azure counts it, over its 65535-byte limit"), "{why}");
    assert_eq!(refusal(Cloud::Gcp, &yaml)?, None);
    let gcp = refusal(Cloud::Gcp, &lab(270_000))?.ok_or("GCP takes it")?;
    assert!(gcp.starts_with("machine `web`: "), "{gcp}");
    assert!(gcp.contains("as Google Cloud counts it, over its 262144-byte limit"), "{gcp}");
    Ok(())
}

#[test]
fn a_cloud_refusing_for_size_drops_only_its_module() -> Outcome {
    use isoloom_render::backend::Backend;
    let env = plan(&lab(40_000), Target::CloudVm)?;
    let files = isoloom_terraform::CloudVmBackend.render(&env)?;
    assert!(!files.iter().any(|f| f.path.contains("cloud-vm/aws/")), "AWS refuses it");
    assert!(files.iter().any(|f| f.path.ends_with("cloud-vm/azure/main.tf")), "Azure takes it");
    Ok(())
}
