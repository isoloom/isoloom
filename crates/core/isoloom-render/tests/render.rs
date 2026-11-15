//! isoloom-render as the backends call it: the inventory of a planned environment reached as a
//! target says, the check scripts' command run in a project, the files' header, PowerShell's
//! literals, the stable hash, and the capability check a backend makes before rendering.

use std::collections::BTreeMap;
use std::error::Error;

use isoloom_plan::{Capabilities, Environment, Lab};
use isoloom_render::backend::check;
use isoloom_render::inventory::{self, Access, vars};
use isoloom_render::{header, powershell, stable_hash};
use isoloom_spec::model::Target;

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// Two Linux VMs and a container, planned for `target`.
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
    let linux_vars = groups.get("linux:vars").ok_or("no [linux:vars]")?;
    assert!(
        linux_vars.contains(&"ansible_user=vagrant".to_owned()) && linux_vars.contains(&"ansible_become=true".to_owned()),
        "{text}"
    );
    Ok(())
}

#[test]
fn a_container_target_has_no_host_to_reach() -> Outcome {
    assert!(inventory::hosts(&planned(Target::Docker)?).is_empty());
    Ok(())
}

#[test]
fn winrm_says_its_port_scheme_and_transport() {
    use isoloom_spec::model::Winrm;
    let ssl: BTreeMap<String, String> = inventory::winrm_vars(Winrm::Ssl).into_iter().collect();
    assert_eq!(ssl.get("ansible_port").map(String::as_str), Some("5986"));
    assert_eq!(ssl.get("ansible_winrm_scheme").map(String::as_str), Some("https"));
    let plain: BTreeMap<String, String> = inventory::winrm_vars(Winrm::Plaintext).into_iter().collect();
    assert_eq!(plain.get("ansible_port").map(String::as_str), Some("5985"));
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

#[test]
fn every_header_line_is_a_comment_that_says_how_to_change_the_file() {
    for marker in ["#", "//"] {
        let text = header(marker);
        assert!(text.lines().all(|l| l.starts_with(marker)), "{text}");
        assert!(text.contains("isoloom generate"), "{text}");
    }
}

#[test]
fn a_powershell_literal_doubles_every_quote_powershell_reads() {
    assert_eq!(powershell::quote("plain"), "'plain'");
    assert_eq!(powershell::quote("it's"), "'it''s'");
    assert_eq!(powershell::quote("a\u{2019}b"), "'a\u{2019}\u{2019}b'");
    assert_eq!(powershell::quote("$env:PATH"), "'$env:PATH'");
}

#[test]
fn the_stable_hash_is_the_same_on_every_build_and_tells_names_apart() {
    assert_eq!(stable_hash::of("web"), stable_hash::of("web"));
    assert_ne!(stable_hash::of("web"), stable_hash::of("db"));
}

#[test]
fn a_backend_without_a_capability_the_environment_needs_is_refused_with_why() -> Outcome {
    let lab = Lab::new(isoloom_spec::parse(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12, aliases: [www] }\n",
    )?);
    let env = lab.plan(&lab.for_target(Target::Docker), Target::Docker, None)?;
    // A machine's aliases are a capability a target has or lacks.
    let needed: Vec<_> = env.requirements.iter().map(isoloom_plan::Requirement::feature).collect();
    assert_eq!(needed.len(), 1, "{:?}", env.requirements);
    assert!(check(Target::Docker, &env, &Capabilities::of(needed.clone())).is_ok());
    let without = Capabilities::of(needed.clone()).without(&needed[..1]);
    let refused = check(Target::Docker, &env, &without).err().ok_or("not refused")?;
    assert!(refused.to_string().contains("www") || refused.to_string().contains("alias"), "{refused}");
    Ok(())
}
