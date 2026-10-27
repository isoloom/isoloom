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
    // The access machine is a real container now, kept running idle under the engine's init, not
    // the check stand-in. It is on demand: its image is the runner-overridable `ISOLOOM_ACCESS_IMAGE`
    // (its table image the fallback) and it sits in the `access` profile, so the initial `up` skips it.
    assert!(
        compose
            .contains("  user:\n    image: ${ISOLOOM_ACCESS_IMAGE:-kalilinux/kali-rolling}\n    platform: linux/amd64\n    profiles:\n    - access\n    init: true\n    entrypoint:\n    - sleep\n    - infinity\n"),
        "{compose}"
    );
    // A check that runs from its position can't depend on it (it is on demand, in the `access`
    // profile), so the check-profile stand-in `isoloom-access` stands in at its addresses; the
    // access box keeps its own `user` service above and is never collapsed into the stand-in.
    assert!(compose.contains("\n  isoloom-access:\n"), "{compose}");
    // Its VM too: defined but not started by `up` (`autostart: false`), its box overridable.
    let vagrant = generated(&applied, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(
        vagrant.contains("config.vm.define \"user\", autostart: false do |m|\n    m.vm.box = ENV.fetch(\"ISOLOOM_ACCESS_BOX\", \"kalilinux/rolling\")"),
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
    let os = isoloom_spec::images::Table::parse("os:\n  debian-12: { vagrant: x }\n")
        .unwrap_err()
        .to_string();
    assert!(os.contains("os.debian-12: unknown OS `debian-12`"), "{os}");
    let access = isoloom_spec::images::Table::parse("access:\n  vm: ubuntu-24.04\n").unwrap_err().to_string();
    assert!(access.starts_with("access: unknown OS `ubuntu-24.04`"), "{access}");
    Ok(())
}
