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
  attacker: { os: kali@rolling, networks: { lan: 20 } }
")?;
    let tf = generated(&lab, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    assert!(tf.contains("data \"aws_ami\" \"kali\""), "{tf}");
    assert!(
        tf.contains("owners      = [\"aws-marketplace\"]") || tf.contains("owners = [\"aws-marketplace\"]"),
        "{tf}"
    );
    assert!(tf.contains("values = [\"7lgvy7mt78lgoi4lant0znp5h\"]"), "pinned to Kali's product: {tf}");
    assert!(tf.contains("ami                         = data.aws_ami.kali.id"), "{tf}");
    assert!(tf.contains("output \"ssh_user_attacker\" {\n  value = \"kali\""), "{tf}");
    Ok(())
}
