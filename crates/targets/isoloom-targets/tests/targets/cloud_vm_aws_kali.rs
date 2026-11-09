//! A Kali machine on AWS (`cloud-vm`): Kali's own image from the AWS Marketplace, pinned to its
//! product, logging in as `kali`, like any other OS of the image table.

use isoloom_spec::Target;

use crate::testing::{Outcome, generated, lab};

#[test]
fn a_kali_machine_boots_kalis_own_image_pinned_to_its_product() -> Outcome {
    let lab = lab("version: 1
name: kali
networks: { lan: { cidr: 10.70.0.0/24 } }
machines:
  web: { os: debian@12, networks: { lan: 10 } }
  workstation: { os: kali@rolling, access: true, networks: { lan: 20 } }
")?;
    let tf = generated(&lab, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    // Compared with the alignment's spaces squeezed: `ami = …`, whatever the block's widest name.
    let tf = tf.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    assert!(tf.contains("data \"aws_ami\" \"kali\""), "{tf}");
    assert!(
        tf.contains("owners      = [\"aws-marketplace\"]") || tf.contains("owners = [\"aws-marketplace\"]"),
        "{tf}"
    );
    assert!(tf.contains("values = [\"7lgvy7mt78lgoi4lant0znp5h\"]"), "pinned to Kali's product: {tf}");
    assert!(tf.contains("ami = data.aws_ami.kali.id"), "{tf}");
    assert!(tf.contains("output \"ssh_user_workstation\" {\n value = \"kali\""), "{tf}");
    // Named after itself, not after its image (Kali's names itself `kali`).
    assert!(tf.contains("hostnamectl set-hostname workstation"), "{tf}");
    // Kali's image is a 25 GB snapshot: AWS refuses a smaller root volume (the default is 20).
    assert!(tf.contains("volume_size = 25"), "{tf}");
    // The access machine keeps its role in the run manifest.
    let run = generated(&lab, Target::CloudVm, ".isoloom/cloud-vm/aws/run.json")?;
    let run: serde_json::Value = serde_json::from_str(&run).unwrap();
    let role = |name: &str| {
        run["machines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["name"] == name)
            .map(|m| m["role"].clone())
    };
    assert_eq!(role("workstation"), Some("access".into()));
    assert_eq!(role("web"), Some("target".into()));
    Ok(())
}

/// Every AWS machine sets itself up once, at its first boot: a later apply with other user data
/// (a run brought up to date without the launch's values) leaves it running as it is.
#[test]
fn an_aws_machine_sets_itself_up_once() -> Outcome {
    let lab = lab("version: 1
name: once
networks: { lan: { cidr: 10.70.0.0/24 } }
machines:
  web: { os: debian@12, networks: { lan: 10 } }
")?;
    let tf = generated(&lab, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    assert!(tf.contains("lifecycle {\n    ignore_changes = [user_data_base64]\n  }"), "{tf}");
    Ok(())
}

/// The `lifecycle` blocks of `tf`'s resource `head` (Terraform takes one per resource).
fn lifecycles(tf: &str, head: &str) -> Outcome<usize> {
    let block = tf.split(head).nth(1).ok_or_else(|| format!("no {head}"))?;
    let block = block.split("\n}\n").next().unwrap_or_default();
    Ok(block.matches("\n  lifecycle {").count())
}

/// A machine that asks for a TPM and sets itself up once: both in the instance's one `lifecycle`
/// block, the NitroTPM precondition beside `ignore_changes`.
#[test]
fn an_aws_machine_with_a_tpm_has_one_lifecycle_block() -> Outcome {
    let tf = generated(&crate::testing::example("tuned-web")?, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    assert_eq!(lifecycles(&tf, "resource \"aws_instance\" \"web\" {")?, 1, "{tf}");
    let block = crate::testing::between(&tf, "resource \"aws_instance\" \"web\" {", "\n}\n")?;
    let lifecycle = crate::testing::from(block, "\n  lifecycle {")?;
    assert!(
        lifecycle.starts_with("\n  lifecycle {\n    ignore_changes = [user_data_base64]\n    precondition {"),
        "{lifecycle}"
    );
    assert!(lifecycle.contains("data.aws_ami.debian_12.tpm_support == \"v2.0\""), "{lifecycle}");
    Ok(())
}

/// A spec disk larger than the image's snapshot is the root volume's size; the image's is a floor.
#[test]
fn a_root_disk_larger_than_the_images_is_the_specs() -> Outcome {
    let lab = lab("version: 1
name: big-kali
networks: { lan: { cidr: 10.70.0.0/24 } }
machines:
  workstation: { os: kali@rolling, access: true, networks: { lan: 20 }, resources: { disk_gb: 40 } }
")?;
    let tf = generated(&lab, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    let tf = tf.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    assert!(tf.contains("volume_size = 40"), "{tf}");
    assert!(!tf.contains("volume_size = 25"), "{tf}");
    Ok(())
}

/// The host name is every cloud's set-up, not AWS's alone; a Windows machine names itself from
/// its own set-up, with no Linux command.
#[test]
fn a_linux_machine_is_named_after_itself_on_another_cloud_and_windows_gets_no_linux_command() -> Outcome {
    let lab = lab("version: 1
name: named
networks: { lan: { cidr: 10.70.0.0/24 } }
machines:
  web: { os: debian@12, networks: { lan: 10 } }
")?;
    let tf = generated(&lab, Target::CloudVm, ".isoloom/cloud-vm/azure/main.tf")?;
    assert!(tf.contains("hostnamectl set-hostname web"), "{tf}");
    let windows = generated(&crate::testing::example("windows-hello")?, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    assert!(!windows.contains("hostnamectl"), "{windows}");
    Ok(())
}
