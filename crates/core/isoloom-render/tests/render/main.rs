//! isoloom-render as the backends call it: the inventory of a planned environment reached as a
//! target says, and the check scripts' command run in a project.

use std::error::Error;

use isoloom_plan::{Environment, Lab};
use isoloom_render::inventory::{self, Access, vars};
use isoloom_spec::model::{Target, Winrm};

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// Two Linux VMs, planned for `target`.
fn planned(target: Target) -> Outcome<Environment> {
    let lab = Lab::new(isoloom_spec::parse(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\n  db: { networks: { lan: 20 }, os: debian@12 }\n",
    )?);
    Ok(lab.plan(&lab.for_target(target), target, None)?)
}

#[test]
fn the_inventory_lists_each_vm_at_its_first_address_with_the_targets_access() -> Outcome {
    let env = planned(Target::Vagrant)?;
    let hosts: Vec<(&str, String)> = inventory::hosts(&env).iter().map(|h| (h.name, h.address.clone())).collect();
    assert_eq!(hosts, [("web", "10.9.0.10".to_owned()), ("db", "10.9.0.20".to_owned())]);
    let access = Access::new(vars(&[("ansible_user", "vagrant")]), Vec::new(), |h| vars(&[("isoloom_name", h.name)]));
    let text = inventory::render(&env, &access);
    let groups = isoloom_test_support::ini_groups(&text);
    assert_eq!(
        groups.get("linux").map(Vec::as_slice),
        Some(
            [
                "web ansible_host=10.9.0.10 isoloom_name=web".to_owned(),
                "db ansible_host=10.9.0.20 isoloom_name=db".to_owned()
            ]
            .as_slice()
        ),
        "{text}"
    );
    assert_eq!(
        groups.get("linux:vars").map(Vec::as_slice),
        Some(["ansible_user=vagrant".to_owned(), "ansible_become=true".to_owned()].as_slice()),
        "{text}"
    );
    Ok(())
}

#[test]
fn a_container_target_has_no_host_to_reach() -> Outcome {
    assert_eq!(inventory::hosts(&planned(Target::Docker)?).len(), 0);
    Ok(())
}

#[test]
fn winrm_says_its_port_scheme_and_transport() {
    for (winrm, expected) in [
        (
            Winrm::Ssl,
            [("ansible_port", "5986"), ("ansible_winrm_scheme", "https"), ("ansible_winrm_transport", "ntlm")],
        ),
        (
            Winrm::Plaintext,
            [("ansible_port", "5985"), ("ansible_winrm_scheme", "http"), ("ansible_winrm_transport", "basic")],
        ),
    ] {
        assert_eq!(inventory::winrm_vars(winrm), vars(&expected), "{winrm:?}");
    }
}

#[cfg(unix)]
#[test]
#[expect(clippy::disallowed_methods, reason = "the test runs the command the crate writes, in sh")]
fn a_check_scripts_command_runs_it_from_the_project_even_with_a_space_in_its_path() -> Outcome {
    let project = tempfile::Builder::new().prefix("isoloom-render-project-").tempdir()?;
    std::fs::create_dir_all(project.path().join("my checks"))?;
    std::fs::write(project.path().join("my checks/up.sh"), "pwd; echo up\n")?;
    let command = isoloom_render::checks::script_command_in(&project.path().to_string_lossy(), "my checks/up.sh");
    let out = std::process::Command::new("sh").arg("-c").arg(&command).output()?;
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let printed = String::from_utf8(out.stdout)?;
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.last(), Some(&"up"));
    assert_eq!(std::fs::canonicalize(lines.first().ok_or("no pwd")?)?, std::fs::canonicalize(project.path())?);
    Ok(())
}
