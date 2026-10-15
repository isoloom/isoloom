//! The image table: the runner's access machine, and boxes between the built-in ones and the
//! spec's.

use isoloom_spec::Target;
use isoloom_targets::{Lab, generate};

use crate::testing::{Outcome, example, generated, source};

#[test]
fn the_runner_supplies_the_access_machine_through_its_image_table() -> Outcome {
    // segmented's access machine without an `os` of its own: the runner supplies it.
    let mut supplied = source("segmented")?;
    supplied.machines["user"].os = None;
    let table = isoloom_spec::images::Table::parse("access:\n  docker: kalilinux/kali-rolling\n  vm: kali\n").unwrap();
    let applied = Lab::new(supplied).with_images(table.clone());
    let compose = generated(&applied, Target::Docker, ".isoloom/docker/compose.yml")?;
    // The access machine is a real container now (kept running idle), not the check stand-in.
    assert!(
        compose.contains("  user:\n    image: kalilinux/kali-rolling\n    platform: linux/amd64\n    entrypoint:\n    - sleep\n    - infinity\n"),
        "{compose}"
    );
    assert!(!compose.contains("isoloom-access"));
    // Its VM too.
    let vagrant = generated(&applied, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(
        vagrant.contains("config.vm.define \"user\" do |m|\n    m.vm.box = \"kalilinux/rolling\""),
        "{vagrant}"
    );
    // An access machine with its own `os` is the spec's, which wins over the table.
    let spec = example("segmented")?;
    let with_table = example("segmented")?.with_images(table);
    for t in [Target::Docker, Target::Vagrant] {
        assert_eq!(generate(&with_table, t).ok(), generate(&spec, t).ok(), "{t:?}");
    }
    // Without the table, nothing changes.
    assert_eq!(isoloom_spec::images::Table::default().apply(&spec.spec()), spec.spec());
    Ok(())
}

#[test]
fn the_image_table_sits_between_built_in_images_and_the_spec() -> Outcome {
    let table = isoloom_spec::images::Table::parse("os:\n  debian@12: { vagrant: my-org/debian-12, vagrant_version: \"1.2.0\" }\n").unwrap();
    let vagrant = generated(&example("hello-stack")?.with_images(table), Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(
        vagrant.contains("m.vm.box = \"my-org/debian-12\"\n    m.vm.box_version = \"1.2.0\""),
        "{vagrant}"
    );
    // A spec's own box (`overrides.vagrant`) still wins.
    let other = isoloom_spec::images::Table::parse("os:\n  windows-server@2019: { vagrant: other/box }\n").unwrap();
    let mut pinned = source("windows-hello")?;
    for m in pinned.overrides.vagrant.as_mut().unwrap().machines.values_mut() {
        m.box_name = Some("spec/box".into());
    }
    let own = generated(&Lab::new(pinned).with_images(other), Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(own.contains("\"spec/box\"") && !own.contains("other/box"));
    assert!(isoloom_spec::images::Table::parse("bogus: 1\n").is_err());
    // The OS is written as a spec writes it.
    let os = isoloom_spec::images::Table::parse("os:\n  debian-12: { vagrant: x }\n").unwrap_err();
    assert!(os.contains("os.debian-12: unknown OS `debian-12`"), "{os}");
    let access = isoloom_spec::images::Table::parse("access:\n  vm: ubuntu-24.04\n").unwrap_err();
    assert!(access.starts_with("access: unknown OS `ubuntu-24.04`"), "{access}");
    Ok(())
}
