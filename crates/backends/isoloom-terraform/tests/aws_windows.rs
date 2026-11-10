//! An AWS Windows machine answers WinRM with NTLM, never Basic (which sends the password with
//! every request): its first boot leaves Basic off, Terraform's set-up and the controller's
//! Ansible both sign in with NTLM.

pub mod testing;

use isoloom_terraform::Cloud;
use testing::{Outcome, WINDOWS_PROVISIONED, cloud_vm};

#[test]
fn an_aws_windows_machine_answers_winrm_with_ntlm_not_basic() -> Outcome {
    let tf = cloud_vm(Cloud::Aws, WINDOWS_PROVISIONED)?;
    assert!(!tf.contains("Auth\\\\Basic"), "Basic stays off: {tf}");
    let setup = tf.split("resource \"terraform_data\" \"dc\"").nth(1).ok_or("no set-up of dc")?;
    assert!(setup.contains("use_ntlm = true"), "Terraform signs in with NTLM: {setup}");
    assert!(
        tf.contains("ansible_winrm_transport=ntlm") && !tf.contains("ansible_winrm_transport=basic"),
        "the controller's Ansible signs in with NTLM: {tf}"
    );
    Ok(())
}
