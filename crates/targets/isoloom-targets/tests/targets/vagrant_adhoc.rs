//! The hook for machines the runner adds to a running `vagrant` environment (its ad-hoc attack
//! VM, `isothrum attach`), as GNS3 adds a node to a live topology: every Vagrantfile carries
//! `IsoloomAdHoc`, what a VM needs to join each lab network on each provider, and loads the
//! runner's definitions from the run's state folder at the end of `Vagrant.configure`; on QEMU
//! every network's hub keeps a port for that VM.

use isoloom_spec::Target;

use crate::testing::{Outcome, example, generated, lab};

/// Two networks of different sizes, a machine with an alias.
const TWO_NETS: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n  dmz: { cidr: 10.8.0.0/25 }\nmachines:\n  web: { os: debian@12, networks: { lab: 10, dmz: 20 }, aliases: [www], app: { steps: [{ run: p.sh }] } }\n";

#[test]
fn every_vagrantfile_loads_the_runners_machines_and_says_how_they_join_each_network() -> Outcome {
    let vf = generated(&lab(TWO_NETS)?, Target::Vagrant, "Vagrantfile")?;
    assert!(vf.contains("module IsoloomAdHoc"), "{vf}");
    assert!(
        vf.contains("def self.define(vagrant, name, network:, ip:, box: nil, image: nil, cpus: 2, memory: 2048)"),
        "{vf}"
    );
    assert!(vf.contains("ADHOC_LABEL = \"t\""), "{vf}");
    assert!(
        vf.contains("\"lab\" => { \"name\" => \"isoloom-t-lab\", \"netmask\" => \"255.255.255.0\" }"),
        "{vf}"
    );
    assert!(
        vf.contains("\"dmz\" => { \"name\" => \"isoloom-t-dmz\", \"netmask\" => \"255.255.255.128\" }"),
        "{vf}"
    );
    // The added VM knows the lab's machines by name, as Isoloom's own VMs do.
    assert!(vf.contains("ADHOC_HOSTS = \"printf '%s\\\\n' '10.9.0.10 web www'"), "{vf}");
    // Its default box follows this host's CPU: Kali's box has no arm64 build.
    assert!(
        vf.contains("ADHOC_BOX = RbConfig::CONFIG[\"host_cpu\"] =~ /arm|aarch64/ ? \"cloud-image/debian-12\" : \"kalilinux/rolling\""),
        "{vf}"
    );
    assert!(vf.contains("picked = box || ENV[\"ISOLOOM_ACCESS_BOX\"]"), "the runner's box first: {vf}");
    // Loaded last, inside `Vagrant.configure`, so a definition can call `config.vm.define`.
    let loader = "  IsoloomAdHoc.files.each { |f| eval(File.read(f), binding, f) }\nend\n";
    assert!(vf.ends_with(loader), "{vf}");
    assert!(
        vf.contains("File.join(ENV.fetch(\"VAGRANT_DOTFILE_PATH\", File.join(__dir__, \".vagrant\")), \"adhoc\")"),
        "{vf}"
    );
    // Not started by `vagrant up`; on QEMU, on the port its network's hub keeps for it.
    assert!(vf.contains("vagrant.vm.define name, autostart: false do |m|"), "{vf}");
    assert!(vf.contains("QEMU_HUBS.dig(network, name)"), "{vf}");
    Ok(())
}

#[test]
fn the_hook_runs_a_container_image_as_a_vm_with_docker() -> Outcome {
    let vf = generated(&lab(TWO_NETS)?, Target::Vagrant, "Vagrantfile")?;
    // The image picks a Debian 12 box per provider unless one is named, and brings Docker.
    assert!(vf.contains("box = picked || (image ? ADHOC_IMAGE_BOX : ADHOC_BOX)"), "{vf}");
    assert!(vf.contains("ADHOC_IMAGE_BOX = \"bento/debian-12\""), "{vf}");
    assert!(
        vf.contains("\"libvirt\" => \"generic/debian12\"") && vf.contains("\"qemu\" => \"cloud-image/debian-12\""),
        "{vf}"
    );
    assert!(vf.contains("ADHOC_DOCKER = \"command -v docker >/dev/null || "), "{vf}");
    assert!(vf.contains("m.vm.provision \"shell\", name: \"docker\", inline: ADHOC_DOCKER"), "{vf}");
    // On QEMU the cloud image's 10 GB disk grows for the image.
    assert!(
        vf.contains("v.disk_resize = IsoloomQemu.disk(debian[\"qemu\"], ADHOC_IMAGE_DISK_GB) if defined?(IsoloomQemu)"),
        "{vf}"
    );
    // The image idle on the VM's own network, as the Compose access service runs it.
    let run = "docker run -d --name #{n} --restart unless-stopped --network host --cap-add NET_ADMIN --cap-add NET_RAW --entrypoint sleep #{i} infinity";
    assert!(vf.contains(run), "{vf}");
    Ok(())
}

#[test]
fn the_examples_vagrantfiles_keep_a_hub_port_for_the_runners_vm_on_every_network() -> Outcome {
    let vf = generated(&example("solo-web")?, Target::Vagrant, "Vagrantfile")?;
    // One machine and the controller: a hub of three ports, the last one the runner's.
    let hubs = vf.split("QEMU_HUBS = {\n").nth(1).and_then(|r| r.split('\n').next()).unwrap_or_default();
    assert!(hubs.contains("\"web\" => ") && hubs.contains("\"isoloom-controller\" => "), "{hubs}");
    assert!(hubs.contains(", \"isoloom-access\" => "), "the runner's port last: {hubs}");
    // The hybrid target's Vagrantfile has the hook too (its VMs are the vagrant target's).
    let hybrid = generated(&example("mixed-office")?, Target::Hybrid, "Vagrantfile")?;
    assert!(hybrid.contains("module IsoloomAdHoc") && hybrid.contains("IsoloomAdHoc.files.each"), "{hybrid}");
    Ok(())
}
