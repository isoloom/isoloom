//! The `vagrant` target: machine order and names, the router, gateways, and each provider (libvirt,
//! QEMU, Hyper-V).

use isoloom_spec::Target;
use isoloom_targets::{generate, generate_instance};

use crate::testing::{Outcome, at, between, example, file, from, generated, lab};

#[test]
fn vagrant_orders_machines_and_names_them() -> Outcome {
    let spec = example("hello-stack")?;
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let cache = vf.find("config.vm.define \"cache\"").unwrap();
    let web = vf.find("config.vm.define \"web\"").unwrap();
    assert!(cache < web, "cache starts before web, which depends on it");
    assert!(vf.contains("'10.60.0.10 web'"));
    assert!(vf.contains("virtualbox__intnet: \"isoloom-hello-stack-app\""));
    Ok(())
}

#[test]
fn libvirt_domains_are_named_after_the_environment() -> Outcome {
    // Unset, vagrant-libvirt prefixes the folder's name ("vagrant_web"), the same for every lab.
    let spec = example("hello-stack")?;
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    assert!(vf.contains("v.default_prefix = \"hello-stack_\""), "{vf}");
    let second = file(&generate_instance(&spec, Target::Vagrant, 2)?, ".isoloom-2/vagrant/Vagrantfile")?.to_owned();
    assert!(second.contains("v.default_prefix = \"hello-stack-2_\""));
    let dvm = generated(&spec, Target::DockerVm, ".isoloom/docker-vm/Vagrantfile")?;
    assert!(dvm.contains("v.default_prefix = \"hello-stack_\""));
    Ok(())
}

#[test]
fn the_controller_gets_every_provider_block_the_machines_get() -> Outcome {
    let spec = example("ansible-pair")?;
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let controller = vf.split("config.vm.define \"isoloom-controller\"").nth(1).unwrap();
    for provider in ["virtualbox", "vmware_desktop", "parallels", "utm", "qemu", "vmware_esxi", "libvirt"] {
        assert!(
            controller.contains(&format!("m.vm.provider \"{provider}\"")),
            "controller has no {provider} block"
        );
    }
    Ok(())
}

#[test]
fn vagrant_router_waits_and_offline_blocks() -> Outcome {
    let spec = example("segmented")?;
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let router = vf.find("config.vm.define \"isoloom-router\"").unwrap();
    let cache = vf.find("config.vm.define \"cache\"").unwrap();
    let web = vf.find("config.vm.define \"web\"").unwrap();
    assert!(router < cache && cache < web);
    assert!(vf.contains("name: \"wait for cache\""));
    assert!(vf.contains("</dev/tcp/cache/6379"));
    assert!(vf.contains("name: \"no internet\""));
    assert!(vf.contains("isoloom-routes.service"));
    Ok(())
}

#[test]
fn vagrant_moves_machines_behind_their_gateway_after_provisioning() -> Outcome {
    let spec = example("edge-firewall")?;
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let fw = vf.find("config.vm.define \"fw\"").unwrap();
    let web = vf.find("config.vm.define \"web\"").unwrap();
    assert!(fw < web, "the gateway starts first");
    assert!(vf.contains("name: \"forwarding\""));
    let web_block = between(vf, "config.vm.define \"web\"", "config.vm.define \"cache\"")?;
    let routes = at(web_block, "name: \"routes\"")?;
    let provision = at(web_block, "name: \"provision/web.sh\"")?;
    let default = at(web_block, "name: \"default route\"")?;
    assert!(routes < provision && provision < default, "installs first, then moves behind the firewall");
    assert_eq!(web_block.matches("ip route replace default via 10.70.10.1").count(), 1, "{web_block}");
    assert!(from(web_block, "name: \"default route\"")?.contains("ip route replace default via 10.70.10.1"));
    Ok(())
}

/// The block of one provider inside one VM's define, from the Vagrantfile.
fn provider_block<'a>(vf: &'a str, vm: &str, provider: &str) -> Outcome<&'a str> {
    let define = from(vf, &format!("config.vm.define \"{vm}\""))?;
    between(define, &format!("m.vm.provider \"{provider}\""), "\n    end\n")
}

#[test]
fn qemu_emulates_another_cpu_and_links_a_pair_of_vms() -> Outcome {
    // GOAD-Mini's shape: one Windows DC and the controller running the lab's playbooks.
    let spec = lab(
        "version: 1\nname: mini\nnetworks:\n  lab: { cidr: 192.168.56.0/24 }\nmachines:\n  dc01:\n    os: windows-server@2019\n    networks: { lab: 10 }\n    ports: { ldap: 389 }\n    app: { steps: [{ run: p.ps1 }] }\nprovision:\n  - ansible: ansible/main.yml\n",
    )?;
    assert_eq!(isoloom_targets::refusal_on(&spec, Target::Vagrant, Some("qemu")), None);
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    assert!(vf.contains("HOST_ARCH = RbConfig::CONFIG[\"host_cpu\"]"));
    // QEMU boots machines in parallel unless told not to: the controller would run its play
    // before the DC is up, and the connecting side of the link could start before the listener.
    assert!(vf.contains("ENV[\"VAGRANT_NO_PARALLEL\"] = \"1\""));

    let dc = provider_block(vf, "dc01", "qemu")?;
    assert!(dc.contains("o.vm.box = \"peru/windows-server-2019-standard-x64-eval\""), "{dc}");
    assert!(dc.contains("o.vm.box_version = \">= 0\""), "the main box's pin isn't the QEMU box's: {dc}");
    assert!(dc.contains("o.vm.box_architecture = \"amd64\"") && dc.contains("v.arch = \"x86_64\""), "{dc}");
    // Emulated: one vCPU (a second only takes turns on QEMU's single emulation thread, and sees
    // the other's code rewrites half-done) and far longer timeouts.
    assert!(
        dc.contains("if \"x86_64\" != HOST_ARCH\n        v.smp = \"cpus=1\"\n        o.vm.boot_timeout = 3600"),
        "{dc}"
    );
    assert!(dc.contains("o.winrm.max_tries = 180"), "{dc}");
    assert!(
        dc.contains("New-NetIPAddress -InterfaceIndex $a.ifIndex -IPAddress 192.168.56.10 -PrefixLength 24"),
        "{dc}"
    );

    let ctl = provider_block(vf, "isoloom-controller", "qemu")?;
    assert!(ctl.contains("o.vm.box = \"cloud-image/debian-12\""), "{ctl}");
    assert!(!ctl.contains("v.arch"), "Isoloom's own VMs run on the host's CPU: {ctl}");
    // One listens, the other connects, on the same port; the DC is defined (and up) first.
    let port = |b: &str, side: &str| {
        b.split(&format!("v.socket_opts = \"{side}=127.0.0.1:"))
            .nth(1)
            .and_then(|r| r.get(..5))
            .map(str::to_owned)
    };
    assert!(port(dc, "listen").is_some() && port(dc, "listen") == port(ctl, "connect"), "{dc}\n{ctl}");

    // Other providers are untouched.
    assert!(!provider_block(vf, "dc01", "virtualbox")?.contains("socket"));
    Ok(())
}

/// The block `config.vm.define "<name>"` of a Vagrantfile, up to its `end`.
fn define_block<'a>(vf: &'a str, name: &str) -> Outcome<&'a str> {
    between(vf, &format!("config.vm.define \"{name}\""), "\n  end\n")
}

#[test]
fn vagrant_runs_on_hyperv_with_a_switch_per_network_and_addresses_set_in_the_guest() -> Outcome {
    let spec = example("segmented")?;
    let v = generated(&spec, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    // The first adapter's switch from ISOLOOM_HYPERV_SWITCH, the helper module once.
    assert_eq!(v.matches("HYPERV_SWITCH = ENV.fetch(\"ISOLOOM_HYPERV_SWITCH\", \"Default Switch\")").count(), 1);
    let web = define_block(&v, "web")?;
    // bento has no Hyper-V build: the roboxes box, pinned; the VM named after the environment.
    assert!(
        web.contains(
            "m.vm.provider \"hyperv\" do |v, o|\n      o.vm.box = \"generic/debian12\"\n      o.vm.box_version = \"4.3.12\"\n      v.vmname = \"segmented-web\""
        ),
        "{web}"
    );
    assert!(web.contains("o.vm.network \"private_network\", bridge: HYPERV_SWITCH"), "{web}");
    assert!(!web.contains("enable_virtualization_extensions"), "{web}");
    // Its network is a private switch, its adapter attached before every start (by a fixed MAC),
    // its address set in the guest once it answers, the switch gone with the last VM.
    assert!(web.contains("IsoloomHyperV.switches([\"isoloom-segmented-front\"])"), "{web}");
    assert!(web.contains("before :\"VagrantPlugins::HyperV::Action::StartInstance\""), "{web}");
    assert!(web.contains("IsoloomHyperV.attach(machine, [[\"isoloom-segmented-front\", \"0215"), "{web}");
    // The address step hangs on a Hyper-V action: other providers run (and print) no trigger.
    assert!(web.contains("m.trigger.after :\"VagrantPlugins::HyperV::Action::WaitForIPAddress\""), "{web}");
    assert!(!v.contains("Vagrant::Action::Builtin"), "a trigger fires on every provider");
    // Every network is tried, and a missing adapter fails the step.
    assert!(
        web.contains("rc=0\n") && web.contains(" 10.61.10.10/24 || rc=1\n") && web.contains("exit $rc\n"),
        "{web}"
    );
    assert!(
        web.contains("systemctl enable -q isoloom-lab-net.service") && web.contains("rc-update add local default"),
        "{web}"
    );
    assert!(web.contains("IsoloomHyperV.prune([\"isoloom-segmented-front\"])"), "{web}");
    hyperv_router_and_macs(&v, spec.source().networks.len())?;
    Ok(())
}

/// On Hyper-V the router is on every network (`networks` of them), with an adapter on each;
/// every adapter of the Vagrantfile `v` has its own MAC.
fn hyperv_router_and_macs(v: &str, networks: usize) -> Outcome {
    let router = define_block(v, "isoloom-router")?;
    assert_eq!(router.matches("\", \"0215").count(), networks, "{router}");
    let macs: Vec<&str> = v.match_indices("\"0215").filter_map(|(i, _)| v.get(i..i + 14)).collect();
    let mut unique = macs.clone();
    unique.sort_unstable();
    unique.dedup();
    assert!(macs.len() > networks && macs.len() == unique.len(), "{macs:?}");
    Ok(())
}

#[test]
fn vagrant_on_hyperv_takes_windows_and_privileged_machines() -> Outcome {
    // Windows: the box's Hyper-V build from `overrides.vagrant` (`boxes.hyperv`), a static
    // address through WMI.
    let win = example("windows-hello")?;
    let v = generated(&win, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(
        v.contains("      o.vm.box = \"StefanScherer/windows_2019\"\n      o.vm.box_version = \">= 0\"\n      v.vmname = \"windows-hello-web01\""),
        "{v}"
    );
    assert!(
        v.contains("IsoloomHyperV.address(machine, <<~'PS1')") && v.contains("EnableStatic(@($n.ip), @($n.mask))"),
        "{v}"
    );
    // Its published port isn't forwarded on Hyper-V: a warning says where it is.
    assert!(v.contains("IsoloomHyperV.ports(machine, [[80, 8082]])"), "{v}");
    assert!(v.contains("mac = '02:15:") && v.contains("ip = '192.168.57.10'; mask = '255.255.255.0'"), "{v}");

    // Nested virtualization comes from `nested_virtualization`, not from `privileged`.
    let hosts = example("existing-hosts")?;
    let plain = generated(&hosts, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(!define_block(&plain, "cache")?.contains("enable_virtualization_extensions"));
    let nested = lab(
        "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  hv: { os: debian@12, networks: { lab: 10 }, hardware: { nested_virtualization: true }, app: { steps: [{ run: a.sh }] } }\n",
    )?;
    let nesting = generated(&nested, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(define_block(&nesting, "hv")?.contains("v.enable_virtualization_extensions = true"), "{nesting}");
    Ok(())
}

#[test]
fn an_os_without_a_hyperv_box_is_refused_on_hyperv_until_the_spec_gives_one() -> Outcome {
    let yaml = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  box:\n    os: debian@13\n    networks: { lab: 10 }\n    app: { steps: [{ run: setup.sh }] }\n";
    let spec = lab(yaml)?;
    // Still generated for every other provider; the Hyper-V block says what is missing.
    let v = generated(&spec, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(
        v.contains("# No Hyper-V build of this box is known: give one with `overrides.vagrant.machines.<machine>.boxes.hyperv`."),
        "{v}"
    );
    assert_eq!(isoloom_targets::refusal_on(&spec, Target::Vagrant, Some("virtualbox")), None);
    let why = isoloom_targets::refusal_on(&spec, Target::Vagrant, Some("hyperv")).expect("refused on Hyper-V");
    assert!(why.contains("machine `box`") && why.contains("boxes.hyperv"), "{why}");
    // The spec's own box: its latest version with a Hyper-V build.
    let own = lab(&format!(
        "{yaml}overrides: {{ vagrant: {{ machines: {{ box: {{ boxes: {{ hyperv: my/debian13 }} }} }} }} }}\n"
    ))?;
    assert_eq!(isoloom_targets::refusal_on(&own, Target::Vagrant, Some("hyperv")), None);
    let given = generated(&own, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(given.contains("o.vm.box = \"my/debian13\"\n      o.vm.box_version = \">= 0\""), "{given}");
    Ok(())
}
