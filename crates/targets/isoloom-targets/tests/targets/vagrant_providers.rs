//! What the Vagrant target writes for one provider beyond the others: QEMU's hubs and disk
//! sizes, UTM's missing private networks, VMware's nested virtualization, ESXi's architecture.

use isoloom_spec::Target;
use isoloom_targets::{refusal, refusal_on};

use crate::testing::{Outcome, between, from, generated, lab};

/// Three Debian VMs on one network, the first sized.
const THREE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    os: debian@12\n    networks: { lab: 10 }\n    resources: { disk_gb: 40 }\n    app: { steps: [{ run: p.sh }] }\n  b:\n    os: debian@12\n    networks: { lab: 11 }\n    app: { steps: [{ run: p.sh }] }\n  c:\n    os: debian@12\n    networks: { lab: 12 }\n    app: { steps: [{ run: p.sh }] }\n";

/// The Vagrantfile of `spec`.
fn vagrantfile(spec: &str) -> Outcome<String> {
    generated(&lab(spec)?, Target::Vagrant, "Vagrantfile")
}

/// The `config.vm.define "<vm>"` block, up to its `end`.
fn define<'a>(vf: &'a str, vm: &str) -> Outcome<&'a str> {
    between(vf, &format!("config.vm.define \"{vm}\""), "\n  end\n")
}

/// The block of one provider inside one VM's define.
fn provider_block<'a>(vf: &'a str, vm: &str, provider: &str) -> Outcome<&'a str> {
    between(define(vf, vm)?, &format!("m.vm.provider \"{provider}\""), "\n    end\n")
}

/// The port a VM's QEMU link connects to.
fn connects_to(block: &str) -> Outcome<u16> {
    let rest = from(block, "v.socket_opts = \"connect=127.0.0.1:")?;
    let digits = rest.trim_start_matches("v.socket_opts = \"connect=127.0.0.1:").get(..5).ok_or("a short port")?;
    Ok(digits.parse()?)
}

#[test]
fn qemu_links_three_vms_through_a_hub() -> Outcome {
    let spec = lab(THREE)?;
    assert_eq!(refusal_on(&spec, Target::Vagrant, Some("qemu")), None);
    let vf = vagrantfile(THREE)?;
    // Every VM connects to its own port of the network's hub.
    let ports: Vec<u16> = ["a", "b", "c"]
        .iter()
        .map(|vm| connects_to(provider_block(&vf, vm, "qemu")?))
        .collect::<Outcome<_>>()?;
    assert_eq!(ports[1], ports[0] + 1, "{ports:?}");
    assert_eq!(ports[2], ports[0] + 2, "{ports:?}");
    let hubs = format!(
        "QEMU_HUBS = {{\n  \"lab\" => {{ \"a\" => {}, \"b\" => {}, \"c\" => {} }},\n}}",
        ports[0], ports[1], ports[2]
    );
    assert!(vf.contains(&hubs), "{vf}");
    assert!(vf.contains("module IsoloomQemu"));
    // Every VM starts the hub before QEMU starts it, unless it runs (only QEMU runs this
    // action), and every VM's destroy releases it: the hub stops once the network's last VM
    // is destroyed, whichever that is, so `vagrant destroy <vm>` leaves it to the others.
    let start = "m.trigger.before :\"VagrantPlugins::QEMU::Action::StartInstance\", type: :action do |t|\n      t.ruby { |_, _| IsoloomQemu.hub(\"lab\", QEMU_HUBS[\"lab\"].values) }";
    let release = "m.trigger.after :\"VagrantPlugins::QEMU::Action::Destroy\", type: :action do |t|\n      t.ruby { |env, machine| IsoloomQemu.release(\"lab\", QEMU_HUBS[\"lab\"].keys, env, machine.name) }";
    for vm in ["a", "b", "c"] {
        let qemu = provider_block(&vf, vm, "qemu")?;
        assert!(qemu.contains("v.net_mode = :socket") && qemu.contains("v.advanced_network = true"), "{qemu}");
        assert!(define(&vf, vm)?.contains(start), "{}", define(&vf, vm)?);
        assert!(define(&vf, vm)?.contains(release), "{}", define(&vf, vm)?);
        assert!(!define(&vf, vm)?.contains("IsoloomQemu.stop("), "{}", define(&vf, vm)?);
    }
    // The hub is left running while another of its VMs keeps a Vagrant id.
    assert!(vf.contains("return if left.any? { |vm| env.local_data_path.join(\"machines\", vm, \"qemu\", \"id\").file? }"));
    // Other providers are untouched.
    assert!(!provider_block(&vf, "a", "virtualbox")?.contains("socket"));
    Ok(())
}

#[test]
fn qemu_links_two_vms_through_a_hub_too_and_refuses_a_vm_on_two_networks() -> Outcome {
    // Two VMs link through a hub too, not a socket pair (one listening, the other connecting).
    let two = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    os: debian@12\n    networks: { lab: 10 }\n    app: { steps: [{ run: p.sh }] }\n  b:\n    os: debian@12\n    networks: { lab: 11 }\n    app: { steps: [{ run: p.sh }] }\n";
    let vf = vagrantfile(two)?;
    let (a, b) = (connects_to(provider_block(&vf, "a", "qemu")?)?, connects_to(provider_block(&vf, "b", "qemu")?)?);
    assert_eq!(b, a + 1);
    assert!(!vf.contains("v.socket_opts = \"listen="), "no socket pair: {vf}");
    assert!(!vf.contains("isoloom-access"), "no port kept for a VM the lab doesn't declare: {vf}");
    assert!(vf.contains("module IsoloomQemu"), "the hub needs the helper");

    let routed = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n  dmz: { cidr: 10.8.0.0/24 }\nmachines:\n  a:\n    os: debian@12\n    networks: { lab: 10, dmz: 10 }\n    app: { steps: [{ run: p.sh }] }\n";
    let spec = lab(routed)?;
    let why = refusal_on(&spec, Target::Vagrant, Some("qemu")).expect("a VM on two networks");
    assert!(why.contains("`a` is on 2 networks, and QEMU gives a VM one"), "{why}");
    // Only QEMU: Vagrant on another provider, or whatever the provider, runs it.
    assert_eq!(refusal_on(&spec, Target::Vagrant, Some("virtualbox")), None);
    assert_eq!(refusal(&spec, Target::Vagrant), None);
    // Still generated (for every other provider); the QEMU block says why it has no network.
    let routed_vf = vagrantfile(routed)?;
    assert!(provider_block(&routed_vf, "a", "qemu")?.contains("# No private network on QEMU: `a` is on 2 networks"));
    Ok(())
}

#[test]
fn qemu_sizes_the_disk_only_above_the_box() -> Outcome {
    let vf = vagrantfile(THREE)?;
    // The size is asked of the box itself: set above its disk, else the box's own is kept.
    let a = provider_block(&vf, "a", "qemu")?;
    assert!(a.contains("v.disk_resize = IsoloomQemu.disk(\"cloud-image/debian-12\", 40)"), "{a}");
    assert!(!provider_block(&vf, "b", "qemu")?.contains("disk_resize"));
    assert!(vf.contains("gb * 1024**3 > sizes.max ? \"#{gb}G\" : nil"));
    Ok(())
}

/// A container lab: its docker-vm helper VM.
const CONTAINERS: &str = "version: 1\nname: c\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  web:\n    networks: { lab: 10 }\n    ports: { site: { port: 80, protocol: http, publish: 8080 } }\n    app: { image: nginx }\n";

#[test]
fn docker_vm_boots_the_cloud_image_on_qemu_with_room_for_docker() -> Outcome {
    let vf = generated(&lab(CONTAINERS)?, Target::DockerVm, "Vagrantfile")?;
    // generic/debian12, the helper's box elsewhere, never comes up under vagrant-qemu: QEMU takes
    // the Debian cloud image the Vagrant target's VMs take there, its 10 GB disk grown for Docker.
    let qemu = between(&vf, "config.vm.provider \"qemu\"", "\n  end\n")?;
    assert!(qemu.contains("do |v, o|"), "{qemu}");
    assert!(qemu.contains("o.vm.box = \"cloud-image/debian-12\""), "{qemu}");
    assert!(
        qemu.contains("o.vm.box_version = \">= 0\""),
        "the main box's pin isn't the cloud image's: {qemu}"
    );
    assert!(qemu.contains("v.disk_resize = IsoloomQemu.disk(\"cloud-image/debian-12\", 40)"), "{qemu}");
    assert!(vf.contains("module IsoloomQemu"), "the disk helper is in the Vagrantfile");
    assert!(vf.contains("config.vm.box = \"generic/debian12\""), "the other providers keep their box");
    Ok(())
}

#[test]
fn the_vagrant_target_has_no_utm_provider() -> Outcome {
    // Every machine is on a network (validation asks it), and vagrant_utm gives a VM none: UTM
    // could run no environment's VMs, so the Vagrantfile has no block for it, the run manifest
    // doesn't offer it, and asking for it says so; the other providers run the environment.
    let spec = lab(THREE)?;
    let why = refusal_on(&spec, Target::Vagrant, Some("utm")).ok_or("UTM taken")?;
    assert!(
        why.starts_with("the `vagrant` target has no `utm` provider: vagrant_utm gives a VM no private network"),
        "{why}"
    );
    assert_eq!(refusal_on(&spec, Target::Vagrant, Some("virtualbox")), None);
    let vf = generated(&spec, Target::Vagrant, "Vagrantfile")?;
    assert!(!vf.contains("\"utm\"") && !vf.contains("ISOLOOM_NAT_ROUTE"), "{vf}");
    let manifest = isoloom_manifest::parse(&generated(&spec, Target::Vagrant, "vagrant/run.json")?)?;
    let isoloom_manifest::Identity::Vagrant(v) = manifest.identity else {
        return Err("a vagrant identity".into());
    };
    assert!(v.providers.iter().all(|p| p.id != isoloom_manifest::Provider::Utm), "{:?}", v.providers);
    Ok(())
}

#[test]
fn vmware_runs_a_machine_with_nested_virtualization() -> Outcome {
    let spec = "version: 1\nname: t\nmachines:\n  a:\n    os: debian@12\n    hardware: { nested_virtualization: true }\n    app: { steps: [{ run: p.sh }] }\n  b:\n    os: debian@12\n    app: { steps: [{ run: p.sh }] }\n";
    let vf = vagrantfile(spec)?;
    assert!(provider_block(&vf, "a", "vmware_desktop")?.contains("v.vmx[\"vhv.enable\"] = \"TRUE\""));
    assert!(!provider_block(&vf, "b", "vmware_desktop")?.contains("vhv.enable"));
    Ok(())
}

#[test]
fn on_utm_the_docker_vm_hosts_default_route_leaves_the_emulated_network() -> Outcome {
    let dvm = generated(&crate::testing::example("hello-stack")?, Target::DockerVm, "docker-vm/Vagrantfile")?;
    assert!(dvm.contains("ISOLOOM_NAT_ROUTE = <<~'SH'\nset -eu\ngw=10.0.2.2\n"), "{dvm}");
    let dvm_utm = between(&dvm, "config.vm.provider \"utm\"", "\n  end\n")?;
    assert!(dvm_utm.contains("preserve_order: true, inline: ISOLOOM_NAT_ROUTE"), "{dvm_utm}");
    let dvm_first = dvm
        .lines()
        .find(|l| l.trim_start().starts_with("config.vm.provision"))
        .ok_or("no provisioner")?;
    assert!(dvm_first.contains("\"nat-route\""), "{dvm_first}");
    Ok(())
}

/// An ESXi host is x86-64: an arm64 machine is refused there (a box's arm64 build, uploaded,
/// never runs), and only there, since every other provider runs on the computer Vagrant runs on.
#[test]
fn esxi_refuses_an_arm64_machine() -> Outcome {
    let arm = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  box:\n    os: ubuntu@24.04\n    networks: { lab: 10 }\n    hardware: { arch: arm64 }\n    app: { steps: [{ run: p.sh }] }\n";
    let spec = lab(arm)?;
    let why = refusal_on(&spec, Target::Vagrant, Some("vmware_esxi")).ok_or("ESXi takes an arm64 machine")?;
    assert_eq!(why, "machine `box` is arm64 (`hardware.arch`), and ESXi runs x86-64 VMs only");
    for p in ["virtualbox", "vmware_desktop", "parallels", "libvirt", "qemu"] {
        assert_eq!(refusal_on(&spec, Target::Vagrant, Some(p)), None, "{p}");
    }
    assert_eq!(refusal_on(&lab(THREE)?, Target::Vagrant, Some("vmware_esxi")), None);
    Ok(())
}
