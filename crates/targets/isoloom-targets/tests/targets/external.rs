//! The `external` target: machines that already exist, described with their SSH endpoints.

use isoloom_spec::Target;
use isoloom_targets::{Lab, generate};

use crate::testing::{Outcome, lab};

/// The `existing-hosts` example.
fn example() -> Outcome<Lab> {
    crate::testing::example("existing-hosts")
}

#[test]
fn a_spec_with_addresses_on_every_machine_gets_the_external_target() -> Outcome {
    let spec = example()?;
    assert_eq!(spec.problems(), vec![]);
    let ids: Vec<&str> = spec.possible().into_iter().map(Target::id).collect();
    assert!(ids.contains(&"external") && ids.contains(&"vagrant"), "{ids:?}");
    let files = generate(&spec, Target::External).unwrap();
    let inv = &files.iter().find(|f| f.path == ".isoloom/external/inventory.ini").unwrap().contents;
    assert!(
        inv.contains("web ansible_host=192.168.1.20 ansible_user=florian ansible_ssh_private_key_file=~/.ssh/id_ed25519"),
        "{inv}"
    );
    let machines = &files.iter().find(|f| f.path == ".isoloom/external/machines.json").unwrap().contents;
    assert!(machines.contains("\"port\": 22"), "{machines}");
    // Derived checks run from each machine; probes only.
    let web = &files.iter().find(|f| f.path == ".isoloom/external/checks/web.sh").unwrap().contents;
    assert!(web.contains("_tcp '192.168.1.21' 6379"), "{web}");
    assert!(files.iter().any(|f| f.path == ".isoloom/resolved.json"));
    Ok(())
}

#[test]
fn external_blocks_are_checked_and_optional() -> Outcome {
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n";
    let spec = lab(&format!(
        "{base}  a: {{ networks: {{ lab: 5 }}, app: {{ image: x }} }}\noverrides: {{ external: {{ machines: {{ a: {{ ssh: {{ address: '' }} }} }} }} }}\n"
    ))?;
    // The problem is placed at the machine's `ssh` block (the lowered field's leaf isn't kept).
    let first = spec.problems()[0].line();
    assert!(first.starts_with("overrides.external.machines.a.ssh: an address"), "{first}");
    let bad_port = lab(&format!(
        "{base}  a: {{ networks: {{ lab: 5 }}, app: {{ image: x }} }}\noverrides: {{ external: {{ machines: {{ a: {{ ssh: {{ address: 10.0.0.5, port: 0 }} }} }} }} }}\n"
    ))?;
    assert!(
        bad_port.problems().iter().any(|p| p.line() == "overrides.external.machines.a.ssh: 1 to 65535"),
        "{:?}",
        bad_port.problems()
    );
    // One machine without an address: no external target, and it says so.
    let missing = lab(&format!(
        "{base}  a: {{ networks: {{ lab: 5 }}, app: {{ image: x }} }}\n  b: {{ networks: {{ lab: 6 }}, app: {{ image: x }} }}\noverrides: {{ external: {{ machines: {{ a: {{ ssh: {{ address: 10.0.0.5 }} }} }} }} }}\n"
    ))?;
    assert!(!missing.possible().contains(&Target::External));
    assert_eq!(
        missing.refusal(Target::External).map(|p| p.path.to_string()).as_deref(),
        Some("overrides.external.machines.b")
    );
    Ok(())
}

#[test]
fn existing_hosts_behind_a_gateway_machine_are_still_generated() -> Outcome {
    let spec = lab(
        "version: 1\nname: gw\nnetworks:\n  lan: { cidr: 10.9.0.0/24, gateway: fw }\n  wan: { cidr: 10.8.0.0/24 }\nmachines:\n  fw:\n    os: debian@12\n    networks: { wan: 2, lan: 1 }\n    app: { steps: [{ run: fw.sh }] }\n  pc:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: pc.sh }] }\noverrides:\n  external:\n    machines:\n      fw: { ssh: { address: 192.168.1.2 } }\n      pc: { ssh: { address: 192.168.1.10 } }\n",
    )?;
    assert_eq!(spec.problems(), vec![]);
    assert!(spec.possible().contains(&Target::External));
    assert_eq!(isoloom_targets::refusal(&spec, Target::External), None);
    assert!(generate(&spec, Target::External).is_ok());
    Ok(())
}
