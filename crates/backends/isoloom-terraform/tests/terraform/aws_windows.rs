//! An AWS Windows machine answers WinRM with NTLM, never Basic (which sends the password with
//! every request): its first boot leaves Basic off, Terraform's set-up and the controller's
//! Ansible both sign in with NTLM.

use crate::testing::{Outcome, WINDOWS_PROVISIONED, cloud_vm, texts};
use isoloom_terraform::Cloud;
use isoloom_test_support::{assert_every, hcl_nested, hcl_resource, hcl_text, ini_groups};

#[test]
fn an_aws_windows_machine_answers_winrm_with_ntlm_not_basic() -> Outcome {
    let tf = cloud_vm(Cloud::Aws, WINDOWS_PROVISIONED)?;
    // dc's first boot sets WinRM up without turning Basic on.
    let first_boot = texts(&tf, "aws_instance", "dc");
    assert!(
        first_boot.iter().any(|t| t.contains("AllowUnencrypted $true")),
        "WinRM is set up: {first_boot:?}"
    );
    assert_every!(&first_boot, |t: &&String| !t.contains("Basic $true"));
    // Terraform's set-up of dc signs in with NTLM.
    let setup = hcl_resource(&tf, "terraform_data", "dc");
    let connections = hcl_nested(&setup, "connection");
    assert_every!(&connections, |c: &&&hcl::Block| hcl_text(c.body(), "use_ntlm") == "true");
    // So does the controller's Ansible: its inventory's Windows group.
    let controller = texts(&tf, "aws_instance", "isoloom_controller");
    let inventory = controller.iter().find(|t| t.contains("[windows:vars]")).ok_or("no Windows group")?;
    let windows = ini_groups(inventory).remove("windows:vars").unwrap_or_default();
    let transports: Vec<&String> = windows.iter().filter(|l| l.starts_with("ansible_winrm_transport=")).collect();
    assert_eq!(transports, ["ansible_winrm_transport=ntlm"]);
    Ok(())
}

/// The administrator's set-up at first boot is the one cloudbase-init runs on the servers: the
/// user checked for first, put in the Administrators group by its SID (whose name a localised
/// Windows changes), and WinRM's port opened by one rule of one name.
#[test]
fn an_aws_windows_machine_makes_its_administrator_as_the_servers_do() -> Outcome {
    let aws = texts(&cloud_vm(Cloud::Aws, WINDOWS_PROVISIONED)?, "aws_instance", "dc");
    let server = texts(&crate::testing::proxmox(WINDOWS_PROVISIONED)?, "proxmox_virtual_environment_file", "dc");
    for (first_boot, on) in [(aws, "aws"), (server, "proxmox")] {
        let script = first_boot.concat();
        assert!(
            script.contains("if (Get-LocalUser -Name isoloom -ErrorAction SilentlyContinue)"),
            "{on}: {script}"
        );
        assert!(script.contains("Add-LocalGroupMember -SID S-1-5-32-544 -Member isoloom"), "{on}: {script}");
        assert!(!script.contains("-Group Administrators"), "{on}: by name");
        assert!(script.contains("-DisplayName 'isoloom winrm'"), "{on}: {script}");
        assert!(!script.contains("'isoloom WinRM'"), "{on}: another rule name");
    }
    Ok(())
}
