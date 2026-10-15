//! The one Ansible inventory, rendered for examples as each target reaches its hosts:
//! snapshots, and determinism.

use std::error::Error;
use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::inventory::{self, Access, WinrmAuth};
use isoloom_spec::model::Target;

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// An example of the project, which must be valid, planned for `target`.
fn env(example: &str, target: Target) -> Outcome<Environment> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(example);
    let lab = isoloom_plan::Lab::new(isoloom_spec::load(&dir)?);
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(format!("{example} isn't valid: {problems:?}").into());
    }
    Ok(Environment::build(&lab.for_target(target), target, None))
}

/// AWS's image users, as the cloud-vm module names them.
fn aws_user(os: &str) -> String {
    if os.starts_with("ubuntu") { "ubuntu" } else { "admin" }.to_owned()
}

fn aws() -> Access<'static> {
    Access::cloud(aws_user, "${random_password.windows.result}", WinrmAuth::Basic)
}

#[test]
fn vagrant_ansible_pair() -> Outcome {
    insta::assert_snapshot!(inventory::render(&env("ansible-pair", Target::Vagrant)?, &Access::vagrant()));
    Ok(())
}

#[test]
fn vagrant_corp_ad_basics_windows() -> Outcome {
    insta::assert_snapshot!(inventory::render(&env("corp-ad-basics", Target::Vagrant)?, &Access::vagrant()));
    Ok(())
}

#[test]
fn proxmox_ansible_pair_keeps_the_groups() -> Outcome {
    insta::assert_snapshot!(inventory::render(
        &env("ansible-pair", Target::Proxmox)?,
        &Access::proxmox("${random_password.windows.result}")
    ));
    Ok(())
}

#[test]
fn aws_ansible_pair() -> Outcome {
    insta::assert_snapshot!(inventory::render(&env("ansible-pair", Target::CloudVm)?, &aws()));
    Ok(())
}

#[test]
fn aws_corp_ad_basics_windows() -> Outcome {
    insta::assert_snapshot!(inventory::render(&env("corp-ad-basics", Target::CloudVm)?, &aws()));
    Ok(())
}

#[test]
fn external_existing_hosts() -> Outcome {
    insta::assert_snapshot!(inventory::render(&env("existing-hosts", Target::External)?, &Access::external()));
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_inventory() -> Outcome {
    for (example, target) in [
        ("ansible-pair", Target::Vagrant),
        ("corp-ad-basics", Target::CloudVm),
        ("existing-hosts", Target::External),
    ] {
        let once = inventory::render(&env(example, target)?, &aws());
        assert_eq!(once, inventory::render(&env(example, target)?, &aws()), "{example}");
    }
    Ok(())
}

#[test]
fn azure_windows_hosts_authenticate_with_ntlm() -> Outcome {
    let azure = Access::cloud(|_| "isoloom".to_owned(), "${random_password.windows.result}", WinrmAuth::Ntlm);
    let inv = inventory::render(&env("corp-ad-basics", Target::CloudVm)?, &azure);
    assert!(inv.contains("ansible_winrm_transport=ntlm\n"), "{inv}");
    assert!(!inv.contains("ansible_winrm_transport=basic"), "{inv}");
    Ok(())
}

#[test]
fn proxmox_windows_hosts_get_the_isoloom_account_over_winrm() -> Outcome {
    let inv = inventory::render(&env("windows-hello", Target::Proxmox)?, &Access::proxmox("${random_password.windows.result}"));
    for v in [
        "ansible_user=isoloom",
        "ansible_password=${random_password.windows.result}",
        "ansible_connection=winrm",
    ] {
        assert!(inv.contains(&format!("{v}\n")), "{v}: {inv}");
    }
    Ok(())
}
