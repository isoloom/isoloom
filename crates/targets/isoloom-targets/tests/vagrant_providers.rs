//! What the Vagrant target writes for one provider beyond the others: QEMU's hubs and disk
//! sizes, UTM's missing private networks, VMware's nested virtualization.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use isoloom_spec::{Target, parse};
use isoloom_targets::{generate, refusal, refusal_on};

/// Three Debian VMs on one network, the first sized.
const THREE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    resources: { disk_gb: 40 }\n    vm: { os: debian@12, provision: [p.sh] }\n  b:\n    networks: { lab: 11 }\n    vm: { os: debian@12, provision: [p.sh] }\n  c:\n    networks: { lab: 12 }\n    vm: { os: debian@12, provision: [p.sh] }\n";

fn vagrantfile(spec: &str) -> String {
    generate(&parse(spec).unwrap(), Target::Vagrant).unwrap().remove(0).contents
}

/// The `config.vm.define "<vm>"` block, up to its `end`.
fn define<'a>(vf: &'a str, vm: &str) -> &'a str {
    let start = vf.find(&format!("config.vm.define \"{vm}\"")).unwrap();
    &vf[start..start + vf[start..].find("\n  end\n").unwrap()]
}

/// The block of one provider inside one VM's define.
fn provider_block<'a>(vf: &'a str, vm: &str, provider: &str) -> &'a str {
    let d = define(vf, vm);
    let at = d.find(&format!("m.vm.provider \"{provider}\"")).unwrap();
    &d[at..at + d[at..].find("\n    end\n").unwrap()]
}

/// The port a VM's QEMU link connects to.
fn connects_to(block: &str) -> u16 {
    let rest = block.split("v.socket_opts = \"connect=127.0.0.1:").nth(1).unwrap();
    rest[..5].parse().unwrap()
}

#[test]
fn qemu_links_three_vms_through_a_hub() {
    let spec = parse(THREE).unwrap();
    assert_eq!(refusal_on(&spec, Target::Vagrant, Some("qemu")), None);
    let vf = vagrantfile(THREE);
    // Every VM connects to its own port of the network's hub.
    let ports: Vec<u16> = ["a", "b", "c"].iter().map(|vm| connects_to(provider_block(&vf, vm, "qemu"))).collect();
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
        let qemu = provider_block(&vf, vm, "qemu");
        assert!(qemu.contains("v.net_mode = :socket") && qemu.contains("v.advanced_network = true"), "{qemu}");
        assert!(define(&vf, vm).contains(start), "{}", define(&vf, vm));
        assert!(define(&vf, vm).contains(release), "{}", define(&vf, vm));
        assert!(!define(&vf, vm).contains("IsoloomQemu.stop("), "{}", define(&vf, vm));
    }
    // The hub is left running while another of its VMs keeps a Vagrant id.
    assert!(vf.contains("return if left.any? { |vm| env.local_data_path.join(\"machines\", vm, \"qemu\", \"id\").file? }"));
    // Other providers are untouched.
    assert!(!provider_block(&vf, "a", "virtualbox").contains("socket"));
}

#[test]
fn qemu_keeps_the_pair_for_two_vms_and_refuses_a_vm_on_two_networks() {
    let two = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10 }\n    vm: { os: debian@12, provision: [p.sh] }\n  b:\n    networks: { lab: 11 }\n    vm: { os: debian@12, provision: [p.sh] }\n";
    let vf = vagrantfile(two);
    assert!(provider_block(&vf, "a", "qemu").contains("v.socket_opts = \"listen=127.0.0.1:"));
    assert!(!vf.contains("IsoloomQemu"), "no hub, no disk size: no helper");

    let routed = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n  dmz: { cidr: 10.8.0.0/24 }\nmachines:\n  a:\n    networks: { lab: 10, dmz: 10 }\n    vm: { os: debian@12, provision: [p.sh] }\n";
    let spec = parse(routed).unwrap();
    let why = refusal_on(&spec, Target::Vagrant, Some("qemu")).expect("a VM on two networks");
    assert!(why.contains("`a` is on 2 networks, and QEMU gives a VM one"), "{why}");
    // Only QEMU: Vagrant on another provider, or whatever the provider, runs it.
    assert_eq!(refusal_on(&spec, Target::Vagrant, Some("virtualbox")), None);
    assert_eq!(refusal(&spec, Target::Vagrant), None);
    // Still generated (for every other provider); the QEMU block says why it has no network.
    let vf = vagrantfile(routed);
    assert!(provider_block(&vf, "a", "qemu").contains("# No private network on QEMU: `a` is on 2 networks"));
}

#[test]
fn qemu_sizes_the_disk_only_above_the_box() {
    let vf = vagrantfile(THREE);
    // The size is asked of the box itself: set above its disk, else the box's own is kept.
    let a = provider_block(&vf, "a", "qemu");
    assert!(a.contains("v.disk_resize = IsoloomQemu.disk(\"cloud-image/debian-12\", 40)"), "{a}");
    assert!(!provider_block(&vf, "b", "qemu").contains("disk_resize"));
    assert!(vf.contains("gb * 1024**3 > sizes.max ? \"#{gb}G\" : nil"));
}

#[test]
fn utm_refuses_vms_on_a_network() {
    let spec = parse(THREE).unwrap();
    let why = refusal_on(&spec, Target::Vagrant, Some("utm")).expect("UTM has no private networks");
    assert!(why.contains("`a` is on a network, and vagrant_utm gives a VM no private network"), "{why}");
    // Every machine is on a network (validation asks it), so UTM runs no environment's VMs yet;
    // the other providers do.
    assert_eq!(refusal_on(&spec, Target::Vagrant, Some("virtualbox")), None);
}

#[test]
fn vmware_runs_a_machine_with_nested_virtualization() {
    let spec = "version: 1\nname: t\nmachines:\n  a:\n    nested_virtualization: true\n    vm: { os: debian@12, provision: [p.sh] }\n  b:\n    vm: { os: debian@12, provision: [p.sh] }\n";
    let vf = vagrantfile(spec);
    assert!(provider_block(&vf, "a", "vmware_desktop").contains("v.vmx[\"vhv.enable\"] = \"TRUE\""));
    assert!(!provider_block(&vf, "b", "vmware_desktop").contains("vhv.enable"));
}
