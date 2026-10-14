//! The one Ansible inventory, rendered for examples as each target reaches its hosts:
//! snapshots, and determinism.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::inventory::{self, Access};
use isoloom_spec::model::Target;

fn env(example: &str, target: Target) -> Environment {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    Environment::build(&spec, target, None)
}

/// AWS's image users, as the cloud-vm module names them.
fn aws_user(os: &str) -> String {
    if os.starts_with("ubuntu") { "ubuntu" } else { "admin" }.to_string()
}

fn aws() -> Access<'static> {
    Access::cloud(aws_user, "${random_password.windows.result}")
}

#[test]
fn vagrant_ansible_pair() {
    insta::assert_snapshot!(inventory::render(&env("ansible-pair", Target::Vagrant), &Access::vagrant()));
}

#[test]
fn vagrant_corp_ad_basics_windows() {
    insta::assert_snapshot!(inventory::render(&env("corp-ad-basics", Target::Vagrant), &Access::vagrant()));
}

#[test]
fn proxmox_ansible_pair_keeps_the_groups() {
    insta::assert_snapshot!(inventory::render(&env("ansible-pair", Target::Proxmox), &Access::proxmox()));
}

#[test]
fn aws_ansible_pair() {
    insta::assert_snapshot!(inventory::render(&env("ansible-pair", Target::CloudVm), &aws()));
}

#[test]
fn aws_corp_ad_basics_windows() {
    insta::assert_snapshot!(inventory::render(&env("corp-ad-basics", Target::CloudVm), &aws()));
}

#[test]
fn external_existing_hosts() {
    insta::assert_snapshot!(inventory::render(&env("existing-hosts", Target::External), &Access::external()));
}

#[test]
fn rendering_twice_gives_the_same_inventory() {
    for (example, target) in [
        ("ansible-pair", Target::Vagrant),
        ("corp-ad-basics", Target::CloudVm),
        ("existing-hosts", Target::External),
    ] {
        let once = inventory::render(&env(example, target), &aws());
        assert_eq!(once, inventory::render(&env(example, target), &aws()), "{example}");
    }
}
