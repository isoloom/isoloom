use serde_yaml_ng::Value;

use super::*;

fn spec(machines: &str) -> Result<Spec, String> {
    model(&format!(
        "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n{machines}"
    ))
}

#[test]
fn the_machines_os_reaches_its_vm_before_the_groups() {
    let mut doc: Value = serde_yaml_ng::from_str(
        "machines:\n  a: { os: debian@12 }\n  b: { os: windows, vm: { provision: [x.ps1] } }\n  c: { os: debian, vm: { os: debian@13 } }\n  d: { os: debian, docker: { image: x } }\n",
    )
    .unwrap();
    fill_vm(&mut doc);
    let vm = |m: &str| serde_yaml_ng::to_string(&doc["machines"][m].get("vm")).unwrap();
    assert_eq!(vm("a"), "os: debian@12\n");
    assert_eq!(vm("b"), "provision:\n- x.ps1\nos: windows\n");
    assert_eq!(vm("c"), "os: debian@13\n", "its own vm.os stays, for validation to compare");
    assert_eq!(vm("d"), "null\n", "a container keeps its `docker:` alone");
}

#[test]
fn a_shared_implementation_only_completes_one_a_machine_declares() {
    let s = model(
        "version: 1\nname: t\ncommon: { docker: { idle: true }, vm: { tpm: true } }\nmachines:\n  web: { docker: { image: nginx } }\n  dc: { vm: { os: windows-server@2022 } }\n",
    )
    .unwrap();
    assert!(s.machines["web"].docker.as_ref().is_some_and(|d| d.idle) && s.machines["web"].vm.is_none());
    assert!(s.machines["dc"].docker.is_none() && s.machines["dc"].vm.as_ref().is_some_and(|v| v.tpm == Some(true)));
}

#[test]
fn the_first_address_is_the_primary_one() {
    let s = spec("  web: { networks: { lan: [10, 11, 12] }, vm: { os: debian@12 } }\n  db: { networks: { lan: [20] }, vm: { os: debian@12 } }\n").unwrap();
    assert_eq!(s.machines["web"].networks["lan"], 10);
    assert_eq!(s.machines["web"].more_addresses["lan"], vec![11, 12]);
    assert_eq!(s.machines["db"].networks["lan"], 20);
    assert!(s.machines["db"].more_addresses.is_empty());
}

#[test]
fn a_list_is_numbers_and_the_field_is_not_written() {
    for (m, why) in [
        ("  web: { networks: { lan: [] } }\n", "an empty list"),
        ("  web: { networks: { lan: [10, x] } }\n", "each address is a number"),
        ("  web: { networks: { lan: 10 }, more_addresses: { lan: [11] } }\n", "not a field of the spec"),
    ] {
        let e = spec(m).unwrap_err();
        assert!(e.contains(why), "{e}");
    }
}

#[test]
fn validation_rejects_duplicates_and_addresses_outside_the_network() {
    let s = spec("  web: { networks: { lan: [10, 11, 10, 255, 20] }, docker: { image: nginx } }\n  db: { networks: { lan: 20 }, docker: { image: nginx } }\n")
        .unwrap();
    let problems: Vec<String> = crate::validate(&s).into_iter().map(|p| format!("{}: {}", p.at, p.message)).collect();
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems[0].contains("machines.web.networks.lan: address .10 is written twice"), "{problems:?}");
    assert!(problems[1].contains("255 isn't a usable address in 10.0.0.0/24"), "{problems:?}");
    assert!(
        problems[2].contains("machines.db.networks.lan: address .20 is already used by `web`"),
        "{problems:?}"
    );
}

#[test]
fn clones_shift_every_address() {
    let s = spec("  ws: { count: 2, networks: { lan: [10, 20] }, vm: { os: debian@12 } }\n").unwrap();
    assert_eq!(s.machines["ws-02"].networks["lan"], 11);
    assert_eq!(s.machines["ws-02"].more_addresses["lan"], vec![21]);
}
