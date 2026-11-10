//! Every cloud caps a machine's user data and refuses a machine whose user data is over its cap
//! as the cloud counts it, naming the machine; the other clouds keep their modules.

pub mod testing;

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

/// A lab of one Linux machine, `web`, then `rest` (YAML at the top level or under `web`).
fn one_machine(rest: &str) -> String {
    format!(
        "version: 1
name: caps
networks:
  lab: {{ cidr: 10.66.0.0/24, internet: true }}
machines:
  web:
    os: debian@12
    networks: {{ lab: 10 }}
{rest}"
    )
}

/// A lab of one Linux machine, `web`, whose recipe runs a command carrying `bytes` of noise.
fn lab(bytes: usize) -> String {
    one_machine(&format!(
        "    app:
      steps:
        - command: [sh, -c, \"echo {} > /srv/noise\"]
          creates: /srv/noise
",
        noise(bytes)
    ))
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
    assert!(why.contains("as AWS counts it, over its 16384-byte limit"), "{why}");
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
fn digitalocean_linode_and_oracle_cloud_refuse_over_their_caps() -> Outcome {
    // Oracle Cloud counts the base64 within its 32000 bytes of metadata: 25 KB is over.
    let oci = refusal(Cloud::Oci, &lab(25_000))?.ok_or("Oracle Cloud takes it")?;
    assert!(oci.starts_with("machine `web`: "), "{oci}");
    assert!(oci.contains("as Oracle Cloud counts it, over its 30976-byte limit"), "{oci}");
    for cloud in [Cloud::DigitalOcean, Cloud::Linode] {
        assert_eq!(refusal(cloud, &lab(25_000))?, None, "{}", cloud.id());
    }
    let yaml = lab(70_000);
    let digitalocean = refusal(Cloud::DigitalOcean, &yaml)?.ok_or("DigitalOcean takes it")?;
    assert!(digitalocean.contains("as DigitalOcean counts it, over its 65536-byte limit"), "{digitalocean}");
    let linode = refusal(Cloud::Linode, &yaml)?.ok_or("Linode takes it")?;
    assert!(linode.contains("as Linode counts it, over its 65535-byte limit"), "{linode}");
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

/// A lab whose environment-level playbook carries `bytes` of noise in its vars: the controller's
/// set-up runs it, so its user data carries them.
fn controller_lab(bytes: usize) -> String {
    one_machine(&format!(
        "provision:
  - ansible: site.yml
    vars: {{ noise: \"{}\" }}
",
        noise(bytes)
    ))
}

#[test]
fn the_controllers_user_data_is_capped_too_naming_it() -> Outcome {
    let yaml = controller_lab(40_000);
    let aws = refusal(Cloud::Aws, &yaml)?.ok_or("AWS takes it")?;
    assert!(aws.starts_with("the controller: its user data is "), "{aws}");
    assert!(aws.contains("as AWS counts it, over its 16384-byte limit"), "{aws}");
    let bigger = controller_lab(70_000);
    let azure = refusal(Cloud::Azure, &bigger)?.ok_or("Azure takes it")?;
    assert!(azure.starts_with("the controller: ") && azure.contains("as Azure counts it"), "{azure}");
    let gcp = refusal(Cloud::Gcp, &controller_lab(270_000))?.ok_or("GCP takes it")?;
    assert!(gcp.starts_with("the controller: ") && gcp.contains("as Google Cloud counts it"), "{gcp}");
    Ok(())
}
