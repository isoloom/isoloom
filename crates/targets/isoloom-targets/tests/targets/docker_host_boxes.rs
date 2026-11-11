//! The box `docker-vm`'s VM boots has a build for each provider it is offered on and each
//! architecture of the computer running it: the rig found every Docker-in-VM environment failing
//! on Apple Silicon with VirtualBox ("platform architecture x86 is not supported on ARM"), the VM
//! booting `generic/debian12`, which has no arm64 VirtualBox build. So have Isoloom's own VMs of
//! the `vagrant` target. And the boxes a run manifest lists for a runner to download first are
//! those the Vagrantfile boots.

use std::collections::BTreeSet;

use isoloom_manifest::{Identity, Machine, Native, Provider, Role, VagrantBox};
use isoloom_spec::Target;
use isoloom_spec::model::Arch;
use isoloom_targets::generate;

use crate::testing::{Outcome, between, example, file, lab};

/// One container, published: the smallest `docker-vm` environment.
const CONTAINERS: &str = "version: 1\nname: c\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  web:\n    networks: { lab: 10 }\n    ports: { site: { port: 80, protocol: http, publish: 8080 } }\n    app: { image: nginx }\n";

/// The builds (box provider, architecture) of each box the VM may boot, at the version Vagrant
/// takes (the newest, none being pinned), as Vagrant Cloud listed them on 2026-11-11. A box
/// that loses a build there fails the rig, not this test: keep this table true when a box changes.
fn builds(name: &str) -> Option<BTreeSet<(&'static str, &'static str)>> {
    let both = |providers: &[&'static str]| providers.iter().flat_map(|p| [(*p, "amd64"), (*p, "arm64")]).collect();
    Some(match name {
        // 202510.26.0 (the first with an arm64 VirtualBox build: 202508.03.0).
        "bento/debian-12" => both(&["virtualbox", "vmware_desktop", "parallels"]),
        // 4.3.12, roboxes' boxes: arm64 for libvirt and QEMU only (the controller's Alpine too).
        "generic/debian12" | "generic/alpine319" => {
            let mut b: BTreeSet<_> = both(&["libvirt", "qemu"]);
            b.extend(["virtualbox", "vmware_desktop", "parallels", "hyperv"].map(|p| (p, "amd64")));
            b
        }
        "cloud-image/debian-12" => both(&["libvirt", "qemu", "virtualbox"]),
        "utm/bookworm" => both(&["utm"]),
        _ => return None,
    })
}

/// The box provider whose builds `provider` boots: ESXi's plugin boots VMware's.
fn box_provider(provider: Provider) -> &'static str {
    match provider {
        Provider::VmwareEsxi => "vmware_desktop",
        other => other.id(),
    }
}

/// The value of `<var>.vm.<key> = "<value>"` in `block`, if it sets it.
fn setting<'a>(block: &'a str, var: &str, key: &str) -> Option<&'a str> {
    let line = block.lines().find_map(|l| l.trim().strip_prefix(&format!("{var}.vm.{key} = ")))?;
    Some(line.trim_matches('"'))
}

/// The box a provider boots, and its build on a computer of architecture `host`: its block's
/// own box (`o.vm.box`), else the main one; its build pinned by `box_architecture`, else the
/// computer's (`:auto` too). ESXi's VMs run on the ESXi host, which is x86-64.
fn booted(vf: &str, provider: Provider, host: Arch) -> Outcome<(String, Arch)> {
    // Every block of the provider (libvirt's prefix has one of its own).
    let head = format!("config.vm.provider \"{}\"", provider.id());
    let blocks: Vec<&str> = vf
        .match_indices(&head)
        .map(|(at, _)| between(vf.get(at..).unwrap_or_default(), &head, "\n  end\n"))
        .collect::<Outcome<_>>()?;
    let block = blocks.concat();
    let main = setting(vf, "config", "box").ok_or("no main box")?;
    let name = setting(&block, "o", "box").unwrap_or(main);
    let pinned = setting(&block, "o", "box_architecture").or_else(|| setting(vf, "config", "box_architecture"));
    let arch = match pinned {
        Some("amd64") => Arch::Amd64,
        Some("arm64") => Arch::Arm64,
        Some(":auto") | None if provider == Provider::VmwareEsxi => Arch::Amd64,
        Some(":auto") | None => host,
        Some(other) => return Err(format!("{}: box_architecture {other}", provider.id()).into()),
    };
    Ok((name.to_owned(), arch))
}

#[test]
fn the_docker_hosts_box_has_a_build_for_every_provider_and_architecture() -> Outcome {
    let files = generate(&lab(CONTAINERS)?, Target::DockerVm)?;
    let vf = file(&files, ".isoloom/docker-vm/Vagrantfile")?;
    let manifest = isoloom_manifest::parse(file(&files, ".isoloom/docker-vm/run.json")?)?;
    let Identity::Vagrant(vagrant) = &manifest.identity else {
        return Err("not a Vagrant run".into());
    };
    let offered: Vec<Provider> = vagrant.providers.iter().map(|p| p.id).collect();
    assert_eq!(offered.len(), 7, "every provider but Hyper-V: {offered:?}");
    let mut missing = Vec::new();
    for &provider in &offered {
        for host in [Arch::Amd64, Arch::Arm64] {
            let (name, arch) = booted(vf, provider, host)?;
            let known = builds(&name).ok_or_else(|| format!("{}: box `{name}` isn't in the table", provider.id()))?;
            if !known.contains(&(box_provider(provider), arch.id())) {
                missing.push(format!("{} on {}: `{name}` has no {} build", provider.id(), host.id(), arch.id()));
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
    Ok(())
}

#[test]
fn the_docker_host_boots_debian_12s_boxes_as_a_debian_machine_does() -> Outcome {
    let vf = generate(&lab(CONTAINERS)?, Target::DockerVm)?;
    let vf = file(&vf, ".isoloom/docker-vm/Vagrantfile")?;
    // VirtualBox, VMware and Parallels: bento's, the main box, the computer's build.
    assert_eq!(setting(vf, "config", "box"), Some("bento/debian-12"));
    assert_eq!(setting(vf, "config", "box_architecture"), None, "the computer's build");
    for host in [Arch::Amd64, Arch::Arm64] {
        for (provider, name) in [
            (Provider::Virtualbox, "bento/debian-12"),
            (Provider::VmwareDesktop, "bento/debian-12"),
            (Provider::Parallels, "bento/debian-12"),
            (Provider::Libvirt, "generic/debian12"),
            (Provider::Qemu, "cloud-image/debian-12"),
            (Provider::Utm, "utm/bookworm"),
        ] {
            assert_eq!(booted(vf, provider, host)?, (name.to_owned(), host), "{}", provider.id());
        }
    }
    assert_eq!(booted(vf, Provider::VmwareEsxi, Arch::Arm64)?, ("bento/debian-12".to_owned(), Arch::Amd64));
    Ok(())
}

/// The box `m` lists for `provider` on a computer of architecture `host`, and the build Vagrant
/// takes: one, or none.
fn listed(m: &Machine, provider: Provider, host: Arch) -> Outcome<Option<(String, Arch)>> {
    let Some(Native::Vagrant { boxes, .. }) = &m.native else {
        return Err(format!("{}: not a Vagrant machine", m.name).into());
    };
    let arch = |a: isoloom_manifest::Arch| match a {
        isoloom_manifest::Arch::Amd64 => Arch::Amd64,
        isoloom_manifest::Arch::Arm64 => Arch::Arm64,
    };
    let on: Vec<&VagrantBox> = boxes
        .iter()
        .filter(|b| b.provider == provider && b.host_architecture.is_none_or(|h| arch(h) == host))
        .collect();
    match on.as_slice() {
        [] => Ok(None),
        [b] => Ok(Some((b.name.to_string(), b.architecture.map_or(host, arch)))),
        more => Err(format!("{}: {} boxes for {} on {}", m.name, more.len(), provider.id(), host.id()).into()),
    }
}

#[test]
fn the_docker_hosts_listed_boxes_are_the_ones_its_vagrantfile_boots() -> Outcome {
    let files = generate(&lab(CONTAINERS)?, Target::DockerVm)?;
    let vf = file(&files, ".isoloom/docker-vm/Vagrantfile")?;
    let manifest = isoloom_manifest::parse(file(&files, ".isoloom/docker-vm/run.json")?)?;
    let host = manifest.machines.iter().find(|m| m.role == Role::Infra).ok_or("no Docker host")?;
    let Identity::Vagrant(vagrant) = &manifest.identity else {
        return Err("not a Vagrant run".into());
    };
    for provider in vagrant.providers.iter().map(|p| p.id) {
        for arch in [Arch::Amd64, Arch::Arm64] {
            assert_eq!(
                listed(host, provider, arch)?,
                Some(booted(vf, provider, arch)?),
                "{} on {}",
                provider.id(),
                arch.id()
            );
        }
    }
    Ok(())
}

/// Isoloom's own VMs (the router, the tool shell, the controller) list, for every provider and
/// every architecture of the computer, a box with that build; Hyper-V on an x86-64 one only (no
/// arm64 Hyper-V is targeted).
#[test]
fn isoloom_s_own_vms_list_a_box_with_a_build_for_every_provider_and_architecture() -> Outcome {
    let mut seen = BTreeSet::new();
    let mut missing = Vec::new();
    for name in ["dmz-jump", "ansible-pair", "segmented", "slow-link"] {
        let files = generate(&example(name)?, Target::Vagrant)?;
        let manifest = isoloom_manifest::parse(file(&files, ".isoloom/vagrant/run.json")?)?;
        let Identity::Vagrant(vagrant) = &manifest.identity else {
            return Err("not a Vagrant run".into());
        };
        for m in manifest.machines.iter().filter(|m| matches!(m.role, Role::Infra | Role::Tool)) {
            seen.insert(m.name.to_string());
            for provider in vagrant.providers.iter().map(|p| p.id) {
                for host in [Arch::Amd64, Arch::Arm64] {
                    if provider == Provider::Hyperv && host == Arch::Arm64 {
                        continue;
                    }
                    let (bx, arch) = listed(m, provider, host)?.ok_or_else(|| format!("{name}/{}: no box for {}", m.name, provider.id()))?;
                    let known = builds(&bx).ok_or_else(|| format!("box `{bx}` isn't in the table"))?;
                    if !known.contains(&(box_provider(provider), arch.id())) {
                        missing.push(format!(
                            "{name}/{}: {} on {}: `{bx}` has no {} build",
                            m.name,
                            provider.id(),
                            host.id(),
                            arch.id()
                        ));
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
    let own: Vec<&str> = seen.iter().map(String::as_str).collect();
    assert_eq!(own, ["isoloom-controller", "isoloom-router", "isoloom-tool-shell"]);
    Ok(())
}
