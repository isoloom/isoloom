use super::*;

fn web() -> String {
    "web".to_owned()
}

fn lan() -> String {
    "lan".to_owned()
}

/// One requirement of every kind, and what its message must say: the field that asks, and
/// whom it is about.
fn every() -> Vec<(Requirement, &'static str)> {
    [environment_wide(), about_networks(), about_machines(), about_storage()].concat()
}

fn environment_wide() -> Vec<(Requirement, &'static str)> {
    vec![
        (Requirement::EnvironmentProvisioning, "`provision:`"),
        (Requirement::PlaybookChecks, "Ansible checks (.yml)"),
        (Requirement::ExecWithoutContainer { check: "c1".into() }, "`exec` check \"c1\" needs `from:`"),
        (Requirement::MachineAliases, "machine `aliases`"),
        (Requirement::TrafficControl, "(`tc`)"),
        (Requirement::PrivateDns { domain: "lab.internal".into() }, "(`dns.private`, `lab.internal`)"),
        (
            Requirement::VmDependsOnContainer {
                vm: web(),
                container: "db".into(),
            },
            "VM `web` depends on container `db`",
        ),
    ]
}

fn about_networks() -> Vec<(Requirement, &'static str)> {
    vec![
        (Requirement::Gateway { network: lan() }, "a `gateway` machine"),
        (Requirement::Ipv6 { network: lan() }, "network `lan` is IPv6 (`ipv6`)"),
        (Requirement::Ipv6Only { network: lan() }, "network `lan` is IPv6-only (`ipv6.only`)"),
        (
            Requirement::Ipv6Auto { network: lan() },
            "network `lan` leaves IPv6 addresses to the target (`ipv6.addresses: auto`)",
        ),
        (
            Requirement::ExternalNetwork {
                network: lan(),
                name: "corp".into(),
            },
            "network `lan` is an existing network (`external: corp`)",
        ),
        (
            Requirement::BridgedNetwork { network: lan() },
            "network `lan` is bridged to a host interface (`bridged`)",
        ),
        (Requirement::NetworkMtu { network: lan() }, "network `lan` sets its `mtu`"),
    ]
}

fn about_machines() -> Vec<(Requirement, &'static str)> {
    let more = |shape| Requirement::MoreAddresses {
        machine: web(),
        network: lan(),
        shape,
    };
    vec![
        (
            Requirement::NetworkAppliance { machine: web() },
            "machine `web` is a network appliance (`app.appliance`)",
        ),
        (Requirement::Ulimits { machine: web() }, "machine `web` sets `ulimits`"),
        (Requirement::Secrets { machine: web() }, "machine `web` gets `secrets`"),
        (Requirement::SecretOwner { machine: web() }, "machine `web` sets `uid` or `gid`"),
        (
            Requirement::Tpm { machine: web() },
            "machine `web` asks for a virtual TPM (`hardware.tpm: required`)",
        ),
        (Requirement::MachineProcess { machine: web() }, "machine `web` sets its own process"),
        (Requirement::OwnBoot { machine: web() }, "machine `web` boots its own way (`hardware.boot`)"),
        (
            Requirement::Hardware {
                machine: web(),
                need: Need::VmGpus,
            },
            "machine `web` asks for GPUs (`gpus`)",
        ),
        (Requirement::RateLimit { machine: web() }, "machine `web` sets a `rate_limit`"),
        (more(MachineShape::Vm), "which this target doesn't give a VM yet"),
        (more(MachineShape::Container), "a container has one address per network"),
        (more(MachineShape::External), "an existing host keeps the addresses it has"),
    ]
}

fn about_storage() -> Vec<(Requirement, &'static str)> {
    vec![
        (
            Requirement::VolumeAcrossShapes { volume: "data".into() },
            "volume `data` is shared by containers and VMs",
        ),
        (
            Requirement::OwnedProjectFile {
                machine: web(),
                path: "/etc/a".into(),
            },
            "machine `web`'s file `/etc/a` comes from the project with a `uid`, `gid` or `mode`",
        ),
        (Requirement::ExistingHostFiles { machine: web() }, "machine `web` has `files`"),
        (Requirement::PersistentVolume { volume: "data".into() }, "volume `data` is persistent"),
        (
            Requirement::ExternalVolume { volume: "data".into() },
            "volume `data` is an existing one (`external`)",
        ),
        (Requirement::DataDisks { machine: web() }, "machine `web` has data disks (`disks`)"),
        (Requirement::RawDisks { machine: web() }, "machine `web` has a raw disk (`fs: raw`)"),
        (Requirement::TmpfsModes { machine: web() }, "machine `web` sets a tmpfs `mode`"),
    ]
}

#[test]
fn every_requirement_says_why_naming_the_field_that_asks_and_whom() {
    for (r, says) in every() {
        let why = r.to_string();
        assert!(why.contains(says), "{r:?}: {why}");
        assert_eq!(Refusal { requirement: r }.to_string(), why);
    }
}

#[test]
fn a_backend_refuses_the_first_requirement_it_lacks_the_feature_for() {
    let requirements: Vec<Requirement> = every().into_iter().map(|(r, _)| r).collect();
    let all = Capabilities::of(requirements.iter().map(Requirement::feature));
    assert_eq!(refusal(&requirements, &all), None);
    for (i, r) in requirements.iter().enumerate() {
        let without = Capabilities::of(requirements.iter().map(Requirement::feature).filter(|f| *f != r.feature()));
        let first = requirements.iter().position(|o| o.feature() == r.feature());
        assert_eq!(
            refusal(&requirements, &without).map(|x| x.requirement),
            first.map(|p| requirements[p].clone()),
            "{i}"
        );
        assert!(!without.has(r.feature()) && all.has(r.feature()));
    }
}

/// The container checks' requirement of a lab with `machines` and `checks`.
fn container_checks_of(machines: &str, checks: &str) -> Option<Requirement> {
    container_checks(&crate::testing::spec(&format!(
        "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n{machines}checks: {checks}\n"
    )))
}

#[test]
fn an_exec_check_needs_a_container_where_it_runs_and_a_playbook_a_controller() {
    let web = "  web: { networks: { lan: 10 }, app: { image: nginx } }\n";
    let vm = "  vm: { networks: { lan: 11 }, os: debian@12, app: { steps: [{ run: a.sh }, reboot] } }\n";
    assert_eq!(container_checks_of(&format!("{web}{vm}"), "[{ exec: id, from: web }]"), None);
    assert_eq!(
        container_checks_of(&format!("{web}{vm}"), "[{ name: x, exec: id, from: vm }]"),
        Some(Requirement::ExecWithoutContainer { check: "x".into() })
    );
    assert_eq!(
        container_checks_of(web, "[{ name: y, exec: id }]"),
        Some(Requirement::ExecWithoutContainer { check: "y".into() }),
        "no access machine: from the networks"
    );
    assert_eq!(
        container_checks_of(web, "[checks/a.yml, { exec: id, from: vm }]"),
        Some(Requirement::PlaybookChecks)
    );
    assert_eq!(container_checks_of(web, "[checks/a.sh]"), None);
}

#[test]
fn a_secret_with_its_own_user_or_group_asks_for_owners() {
    let owners = |file: &str| {
        let spec = crate::testing::spec(&format!(
            "version: 1\nname: t\nsecrets: {{ db: {{ env: DB }} }}\nmachines:\n  web: {{ app: {{ image: nginx }}, files: [{file}] }}\n"
        ));
        of(&spec)
    };
    let secrets = Requirement::Secrets { machine: "web".into() };
    let owner = Requirement::SecretOwner { machine: "web".into() };
    assert_eq!(owners("{ path: /s, secret: db }"), std::slice::from_ref(&secrets));
    assert_eq!(owners("{ path: /s, secret: db, gid: 10 }"), [secrets.clone(), owner.clone()]);
    assert_eq!(owners("{ path: /s, secret: db, uid: 10 }"), [secrets, owner]);
}

#[test]
fn several_addresses_need_the_feature_of_the_machines_shape() {
    let more = |shape| Requirement::MoreAddresses {
        machine: "web".into(),
        network: "lan".into(),
        shape,
    };
    assert_eq!(more(MachineShape::Vm).feature(), Feature::VmMoreAddresses);
    assert_eq!(more(MachineShape::Container).feature(), Feature::ContainerMoreAddresses);
    assert_eq!(more(MachineShape::External).feature(), Feature::ContainerMoreAddresses);
}

#[test]
fn mixing_containers_and_vms_asks_for_a_volume_both_mount_and_a_vm_after_a_container() {
    let env = crate::testing::environment(
        "version: 1\nname: t\nvolumes: { data: { owner: db } }\nmachines:\n  db: { app: { image: postgres }, volumes: { data: /d } }\n  vm: { os: debian@12, app: { steps: [{ run: a.sh }, reboot] }, volumes: { data: /d }, depends_on: [db] }\n  other: { app: { image: nginx }, depends_on: [db] }\n",
        isoloom_spec::model::Target::Hybrid,
    );
    let mixed: Vec<&Requirement> = env
        .requirements
        .iter()
        .filter(|r| matches!(r, Requirement::VolumeAcrossShapes { .. } | Requirement::VmDependsOnContainer { .. }))
        .collect();
    assert_eq!(
        mixed,
        [
            &Requirement::VolumeAcrossShapes { volume: "data".into() },
            &Requirement::VmDependsOnContainer {
                vm: "vm".into(),
                container: "db".into()
            }
        ]
    );
    let docker = crate::testing::environment(
        "version: 1\nname: t\nvolumes: { data: { owner: a } }\nmachines:\n  a: { app: { image: x }, volumes: { data: /d } }\n  b: { app: { image: x }, volumes: { data: /d }, depends_on: [a] }\n",
        isoloom_spec::model::Target::Docker,
    );
    assert!(
        !docker
            .requirements
            .iter()
            .any(|r| matches!(r, Requirement::VolumeAcrossShapes { .. } | Requirement::VmDependsOnContainer { .. }))
    );
}

#[test]
fn a_vm_that_requires_a_tpm_asks_for_one_and_windows_11_merely_expects_it() {
    let tpm = |machines: &str| {
        let env = crate::testing::environment(&format!("version: 1\nname: t\nmachines:\n{machines}"), isoloom_spec::model::Target::Vagrant);
        env.requirements.into_iter().find(|r| matches!(r, Requirement::Tpm { .. }))
    };
    assert_eq!(
        tpm("  a: { os: debian@12 }\n  b: { os: debian@12, hardware: { tpm: required } }\n"),
        Some(Requirement::Tpm { machine: "b".into() })
    );
    assert_eq!(tpm("  w: { os: windows@11 }\n"), None);
}
