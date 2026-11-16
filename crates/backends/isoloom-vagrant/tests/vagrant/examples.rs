//! The examples' Vagrantfiles as snapshots, and their rendering's determinism.

use isoloom_render::backend::Backend;
use isoloom_vagrant::VagrantBackend;

use crate::testing::{Outcome, parsed, render, vagrantfile};

/// The examples whose Vagrantfile a reviewer reads whole, each its snapshot: a stack of
/// services, networks routed through a router, Windows over WinRM, the kernel's settings, files
/// shared over NFS, and Ansible run from the controller.
#[test]
fn each_examples_vagrantfile_is_its_snapshot() -> Outcome {
    for (snapshot, example) in [
        ("hello_stack", "hello-stack"),
        ("segmented_routes_through_the_router", "segmented"),
        ("windows_hello_over_winrm", "windows-hello"),
        ("tuned_web_settings", "tuned-web"),
        ("shared_files_over_nfs", "shared-files"),
        ("ansible_pair_from_the_controller", "ansible-pair"),
    ] {
        insta::assert_snapshot!(snapshot, vagrantfile(example)?);
    }
    Ok(())
}

/// A VM mounting three folders of the computer that runs the environment: one of the project
/// and one absolute, which it writes in, and one of the home folder, read-only.
const SHARED_DESK: &str = "version: 1\nname: shared-desk\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nmachines:\n  desk: { os: debian@12, networks: { lan: 10 }, host_folders: [{ from: shared, path: /srv/shared, read_only: false }, { from: '~/notes', path: /home/notes }, { from: /srv/drop, path: /srv/drop, read_only: false }] }\n";

/// The spec's host folders reach the VM's define as each provider's own shared folder, each
/// read-only or not as the spec says it, and a provider that can't share them (QEMU: one is
/// read-only) stops `vagrant up` with the reason.
#[test]
fn a_vm_shares_folders_of_the_computer_as_the_spec_says_with_each_providers_own_shared_folder() -> Outcome {
    let files = VagrantBackend.render(&parsed(SHARED_DESK)?)?;
    let file = &files.iter().find(|f| f.path.ends_with("/Vagrantfile")).ok_or("no Vagrantfile")?.contents;
    let r = isoloom_test_support::ruby_blocks(file);
    let desk = r.block("config.vm.define \"desk\"");
    let folders = |provider: &str| -> Vec<&str> {
        desk.find(&format!("m.vm.provider \"{provider}\" do |"))
            .into_iter()
            .flat_map(|b| b.statements("o.vm.synced_folder "))
            .collect()
    };
    let folder = |from: &str, to: &str, rest: &str| format!("o.vm.synced_folder File.expand_path(\"{from}\", ROOT), \"{to}\", {rest}");
    assert_eq!(
        folders("virtualbox"),
        [
            folder(
                "shared",
                "/srv/shared",
                "type: \"virtualbox\", create: true, mount_options: [\"dmode=0775\", \"fmode=0664\"]"
            ),
            folder("~/notes", "/home/notes", "type: \"virtualbox\", create: true, mount_options: [\"ro\"]"),
            folder(
                "/srv/drop",
                "/srv/drop",
                "type: \"virtualbox\", create: true, mount_options: [\"dmode=0775\", \"fmode=0664\"]"
            ),
        ]
    );
    assert_eq!(
        folders("libvirt"),
        [
            folder("shared", "/srv/shared", "type: \"virtiofs\", create: true"),
            folder("~/notes", "/home/notes", "type: \"virtiofs\", create: true, mount_opts: \"ro\""),
            folder("/srv/drop", "/srv/drop", "type: \"virtiofs\", create: true"),
        ]
    );
    assert_eq!(folders("qemu"), Vec::<&str>::new(), "QEMU mounts every share read-write");
    // The trigger refusing the providers that don't share them: its table of reasons.
    let reasons: Vec<&str> = desk
        .find("m.trigger.before :up do |t|")
        .into_iter()
        .flat_map(|t| t.find("t.ruby do |_, machine|"))
        .flat_map(|t| t.statements("why = {"))
        .collect();
    assert!(
        matches!(reasons.as_slice(), [why] if why.contains(":qemu => \"host folders (`host_folders`): one is read-only (`read_only`)")),
        "{reasons:?}"
    );
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for example in ["hello-stack", "segmented", "edge-firewall", "vlan-office", "existing-hosts"] {
        assert_eq!(render(example)?, render(example)?, "{example}");
    }
    Ok(())
}
