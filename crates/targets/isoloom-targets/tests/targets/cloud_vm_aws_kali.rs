//! A Kali machine on AWS (`cloud-vm`): Kali's own image from the AWS Marketplace, pinned to its
//! product, logging in as `kali`, like any other OS of the image table.

use isoloom_spec::Target;

use isoloom_test_support::{hcl_blocks, hcl_body, hcl_nested, hcl_one, hcl_text, hcl_values};

use crate::testing::{Outcome, generated, lab};

/// `text` without its whitespace: an HCL expression however the reader lays it out.
fn compact(text: &str) -> String {
    text.split_whitespace().collect()
}

/// Whether one literal of `values` holds `text` (a script line of a user data template).
fn holds(values: &[String], text: &str) -> bool {
    values.iter().any(|v| v.contains(text))
}

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
    let body = hcl_body(&tf);
    let ami = hcl_blocks(&body, "data", &["aws_ami", "kali"]);
    let ami = ami.first().ok_or_else(|| format!("no data.aws_ami.kali: {tf}"))?.body();
    assert_eq!(hcl_values(ami, "owners"), ["aws-marketplace"], "{tf}");
    assert!(
        hcl_values(ami, "values").contains(&"7lgvy7mt78lgoi4lant0znp5h".to_owned()),
        "pinned to Kali's product: {tf}"
    );
    let vm = hcl_one(&body, "aws_instance", "workstation");
    assert_eq!(hcl_text(vm, "ami"), "data.aws_ami.kali.id", "{tf}");
    let user = hcl_blocks(&body, "output", &["ssh_user_workstation"]);
    assert_eq!(user.first().map(|o| hcl_text(o.body(), "value")).as_deref(), Some("kali"), "{tf}");
    // Named after itself, not after its image (Kali's names itself `kali`).
    assert!(holds(&hcl_values(vm, "user_data_base64"), "hostnamectl set-hostname workstation"), "{tf}");
    // Kali's image is a 25 GB snapshot: AWS refuses a smaller root volume (the default is 20).
    assert_eq!(hcl_values(vm, "volume_size"), ["25"], "{tf}");
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
    let body = hcl_body(&tf);
    let vm = hcl_one(&body, "aws_instance", "web");
    let lifecycles = hcl_nested(vm, "lifecycle");
    assert_eq!(
        lifecycles.iter().map(|l| compact(&hcl_text(l.body(), "ignore_changes"))).collect::<Vec<_>>(),
        ["[user_data_base64]"],
        "{tf}"
    );
    assert!(
        lifecycles.iter().all(|l| hcl_nested(l.body(), "precondition").is_empty()),
        "no TPM, no precondition: {tf}"
    );
    Ok(())
}

/// A machine that asks for a TPM and sets itself up once: both in the instance's one `lifecycle`
/// block, the NitroTPM precondition beside `ignore_changes`.
#[test]
fn an_aws_machine_with_a_tpm_has_one_lifecycle_block() -> Outcome {
    let tf = generated(&crate::testing::example("tuned-web")?, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    let body = hcl_body(&tf);
    let lifecycles = hcl_nested(hcl_one(&body, "aws_instance", "web"), "lifecycle");
    let [lifecycle] = lifecycles.as_slice() else {
        return Err(format!("{} lifecycle blocks: {tf}", lifecycles.len()).into());
    };
    assert_eq!(compact(&hcl_text(lifecycle.body(), "ignore_changes")), "[user_data_base64]", "{tf}");
    let conditions: Vec<String> = hcl_nested(lifecycle.body(), "precondition")
        .iter()
        .map(|p| hcl_text(p.body(), "condition"))
        .collect();
    assert_eq!(conditions, ["data.aws_ami.debian_12.tpm_support == \"v2.0\""], "{tf}");
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
    assert_eq!(hcl_values(&hcl_body(&tf), "volume_size"), ["40"], "{tf}");
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
    let vm = hcl_one(&hcl_body(&tf), "azurerm_linux_virtual_machine", "web").clone();
    assert!(holds(&hcl_values(&vm, "custom_data"), "hostnamectl set-hostname web"), "{tf}");
    let windows = generated(&crate::testing::example("windows-hello")?, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    let body = hcl_body(&windows);
    for name in isoloom_test_support::hcl_names(&body, "resource", "aws_instance") {
        assert!(
            !holds(&hcl_values(hcl_one(&body, "aws_instance", &name), "user_data"), "hostnamectl"),
            "{name}: {windows}"
        );
        assert!(
            !holds(&hcl_values(hcl_one(&body, "aws_instance", &name), "user_data_base64"), "hostnamectl"),
            "{name}: {windows}"
        );
    }
    Ok(())
}
