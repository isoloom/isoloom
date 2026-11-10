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

/// The administrator's set-up at first boot is the one cloudbase-init runs on the servers: the
/// user checked for first, put in the Administrators group by its SID (whose name a localised
/// Windows changes), and WinRM's port opened by one rule of one name.
#[test]
fn an_aws_windows_machine_makes_its_administrator_as_the_servers_do() -> Outcome {
    let aws = cloud_vm(Cloud::Aws, WINDOWS_PROVISIONED)?;
    let server = testing::proxmox(WINDOWS_PROVISIONED)?;
    for (tf, on) in [(&aws, "aws"), (&server, "proxmox")] {
        assert!(tf.contains("if (Get-LocalUser -Name isoloom -ErrorAction SilentlyContinue)"), "{on}: {tf}");
        assert!(tf.contains("Add-LocalGroupMember -SID S-1-5-32-544 -Member isoloom"), "{on}: {tf}");
        assert!(!tf.contains("-Group Administrators"), "{on}: by name");
        assert!(tf.contains("-DisplayName 'isoloom winrm'"), "{on}: {tf}");
        assert!(!tf.contains("'isoloom WinRM'"), "{on}: another rule name");
    }
    Ok(())
}
