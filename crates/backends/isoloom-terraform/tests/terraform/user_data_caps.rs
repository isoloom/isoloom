//! Every cloud caps a machine's user data and refuses a machine whose user data is over its cap
//! as the cloud counts it, naming the machine; the other clouds keep their modules.

use crate::testing::{Outcome, plan};
use isoloom_spec::model::Target;
use isoloom_terraform::{Cloud, CloudVmModule};

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

/// An environment of one Linux machine, `web`, then `rest` (YAML at the top level or under `web`).
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

/// An environment of one Linux machine, `web`, whose recipe runs a command carrying `bytes` of noise.
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

/// Why `cloud` refuses `yaml`'s environment, if it does; and that rendering it says the same.
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
fn each_cloud_refuses_a_machines_user_data_over_its_cap_as_it_counts_it() -> Outcome {
    // Hex gzips to about half, base64 grows it by a third: the same noise is over one cloud's cap
    // and under another's.
    for (cloud, fits, over, limit) in [
        (Cloud::Aws, 100, 40_000, "as AWS counts it, over its 16384-byte limit"),
        (Cloud::Azure, 40_000, 70_000, "as Azure counts it, over its 65535-byte limit"),
        (Cloud::Gcp, 70_000, 270_000, "as Google Cloud counts it, over its 262144-byte limit"),
        (Cloud::DigitalOcean, 25_000, 70_000, "as DigitalOcean counts it, over its 65536-byte limit"),
        (Cloud::Linode, 25_000, 70_000, "as Linode counts it, over its 65535-byte limit"),
        // Oracle Cloud counts the base64 within its 32000 bytes of metadata.
        (Cloud::Oci, 100, 25_000, "as Oracle Cloud counts it, over its 30976-byte limit"),
    ] {
        assert_eq!(refusal(cloud, &lab(fits))?, None, "{cloud}: {fits} bytes fit");
        let why = refusal(cloud, &lab(over))?.ok_or(format!("{cloud} takes {over} bytes"))?;
        assert!(why.starts_with("machine `web`: its user data is ") && why.contains(limit), "{cloud}: {why}");
    }
    Ok(())
}

/// An environment whose environment-level playbook carries `bytes` of noise in its vars: the controller's
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
