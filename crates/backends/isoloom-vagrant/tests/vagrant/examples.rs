//! The examples' Vagrantfiles as snapshots, and their rendering's determinism.

use isoloom_render::backend::Backend;
use isoloom_vagrant::VagrantBackend;

use crate::testing::{Outcome, parsed, render, vagrantfile};

#[test]
fn hello_stack() -> Outcome {
    insta::assert_snapshot!(vagrantfile("hello-stack")?);
    Ok(())
}

#[test]
fn segmented_routes_through_the_router() -> Outcome {
    insta::assert_snapshot!(vagrantfile("segmented")?);
    Ok(())
}

#[test]
fn windows_hello_over_winrm() -> Outcome {
    insta::assert_snapshot!(vagrantfile("windows-hello")?);
    Ok(())
}

#[test]
fn tuned_web_settings() -> Outcome {
    insta::assert_snapshot!(vagrantfile("tuned-web")?);
    Ok(())
}

#[test]
fn shared_files_over_nfs() -> Outcome {
    insta::assert_snapshot!(vagrantfile("shared-files")?);
    Ok(())
}

#[test]
fn ansible_pair_from_the_controller() -> Outcome {
    insta::assert_snapshot!(vagrantfile("ansible-pair")?);
    Ok(())
}

/// A VM mounting three folders of the computer that runs the environment: one of the project, one of the
/// home folder (read-only), one absolute.
const SHARED_DESK: &str = "version: 1\nname: shared-desk\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nmachines:\n  desk: { os: debian@12, networks: { lan: 10 }, host_folders: [{ from: shared, path: /srv/shared }, { from: '~/notes', path: /home/notes, read_only: true }, { from: /srv/drop, path: /srv/drop }] }\n";

#[test]
fn a_vm_shares_folders_of_the_computer_with_each_providers_own_shared_folder() -> Outcome {
    let files = VagrantBackend.render(&parsed(SHARED_DESK)?)?;
    let vagrantfile = &files.iter().find(|f| f.path.ends_with("/Vagrantfile")).ok_or("no Vagrantfile")?.contents;
    let start = vagrantfile.find("    # This computer's folders").ok_or("no host folders")?;
    let end = vagrantfile.find("    m.vm.provision \"isoloom_project\"").ok_or("no project")?;
    insta::assert_snapshot!(vagrantfile.get(start..end).ok_or("out of order")?);
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for example in ["hello-stack", "segmented", "edge-firewall", "vlan-office", "existing-hosts"] {
        assert_eq!(render(example)?, render(example)?, "{example}");
    }
    Ok(())
}
