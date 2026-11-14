//! The `vagrant` target: machine order and names, the router, gateways, and each provider (libvirt,
//! QEMU, Hyper-V).

use isoloom_spec::Target;
use isoloom_targets::generate_instance;

use isoloom_test_support::RubyBlock;

use crate::testing::{Outcome, define, defined, example, file, generated, lab, provisioners, vagrantfile};

/// Where `name` is in `names`.
fn place(names: &[String], name: &str) -> Outcome<usize> {
    names.iter().position(|n| n == name).ok_or_else(|| format!("no `{name}` in {names:?}").into())
}

/// The block of provider `provider` inside VM block `vm`.
fn provider<'v>(vm: &'v RubyBlock, provider: &str) -> &'v RubyBlock {
    vm.block(&format!("m.vm.provider \"{provider}\""))
}

/// Whether a statement anywhere in `block` (its blocks' opening lines included) holds `what`.
fn says(block: &RubyBlock, what: &str) -> bool {
    block.every_statement().iter().any(|s| s.contains(what))
}

#[test]
fn vagrant_orders_machines_and_names_them() -> Outcome {
    let spec = example("hello-stack")?;
    let vf = vagrantfile(&spec)?;
    let vms = defined(&vf);
    assert!(place(&vms, "cache")? < place(&vms, "web")?, "cache starts before web, which depends on it");
    assert!(says(define(&vf, "cache"), "'10.60.0.10 web'"), "cache names web");
    let network = define(&vf, "web").statements("m.vm.network \"private_network\"");
    assert!(
        network
            .iter()
            .any(|n| n.contains("virtualbox__intnet: \"isoloom-hello-stack-#{ISOLOOM_KEY}-app\"")),
        "{network:?}"
    );
    Ok(())
}

#[test]
fn libvirt_domains_are_named_after_the_environment() -> Outcome {
    // Unset, vagrant-libvirt prefixes the folder's name ("vagrant_web"), the same for every lab.
    let spec = example("hello-stack")?;
    let vf = vagrantfile(&spec)?;
    let libvirt = vf.block("config.vm.provider \"libvirt\"");
    assert_eq!(libvirt.statements("v.default_prefix"), ["v.default_prefix = \"hello-stack-#{ISOLOOM_KEY}_\""]);
    let second = file(&generate_instance(&spec, Target::Vagrant, 2)?, ".isoloom-2/vagrant/Vagrantfile")?.to_owned();
    let second = isoloom_test_support::ruby_blocks(&second);
    let libvirt = second.block("config.vm.provider \"libvirt\"");
    assert_eq!(libvirt.statements("v.default_prefix"), ["v.default_prefix = \"hello-stack-2-#{ISOLOOM_KEY}_\""]);
    Ok(())
}

#[test]
fn the_controller_gets_every_provider_block_the_machines_get() -> Outcome {
    let spec = example("ansible-pair")?;
    let vf = vagrantfile(&spec)?;
    let controller = define(&vf, "isoloom-controller");
    for name in ["virtualbox", "vmware_desktop", "parallels", "qemu", "vmware_esxi", "libvirt"] {
        assert_eq!(controller.find(&format!("m.vm.provider \"{name}\"")).len(), 1, "controller has no {name} block");
    }
    Ok(())
}

#[test]
fn vagrant_router_waits_and_offline_blocks() -> Outcome {
    let spec = example("segmented")?;
    let vf = vagrantfile(&spec)?;
    let vms = defined(&vf);
    assert!(
        place(&vms, "isoloom-router")? < place(&vms, "cache")? && place(&vms, "cache")? < place(&vms, "web")?,
        "{vms:?}"
    );
    let web = define(&vf, "web");
    assert!(provisioners(web).contains(&"wait for cache".to_owned()), "{:?}", provisioners(web));
    assert!(says(web, "</dev/tcp/cache/6379"));
    let cache = define(&vf, "cache");
    assert!(provisioners(cache).contains(&"no internet".to_owned()), "{:?}", provisioners(cache));
    assert!(says(web, "isoloom-routes.service"));
    Ok(())
}

#[test]
fn vagrant_moves_machines_behind_their_gateway_after_provisioning() -> Outcome {
    let spec = example("edge-firewall")?;
    let vf = vagrantfile(&spec)?;
    let vms = defined(&vf);
    assert!(place(&vms, "fw")? < place(&vms, "web")?, "the gateway starts first");
    assert!(provisioners(define(&vf, "fw")).contains(&"forwarding".to_owned()));
    let web = define(&vf, "web");
    let steps = provisioners(web);
    let (routes, provision, default) = (place(&steps, "routes")?, place(&steps, "provision/web.sh")?, place(&steps, "default route")?);
    assert!(
        routes < provision && provision < default,
        "installs first, then moves behind the firewall: {steps:?}"
    );
    let moves: Vec<&str> = web
        .every_statement()
        .into_iter()
        .filter(|s| s.contains("ip route replace default via 10.70.10.1"))
        .collect();
    assert_eq!(moves.len(), 1, "{moves:?}");
    assert!(moves[0].contains("name: \"default route\""), "{moves:?}");
    Ok(())
}

#[test]
fn qemu_emulates_another_cpu_and_links_the_vms_through_a_hub() -> Outcome {
    // GOAD-Mini's shape: one Windows DC and the controller running the lab's playbooks.
    let spec = lab(
        "version: 1\nname: mini\nnetworks:\n  lab: { cidr: 192.168.56.0/24 }\nmachines:\n  dc01:\n    os: windows-server@2019\n    networks: { lab: 10 }\n    ports: { ldap: 389 }\n    app: { steps: [{ run: p.ps1 }] }\nprovision:\n  - ansible: ansible/main.yml\n",
    )?;
    assert_eq!(isoloom_targets::refusal_on(&spec, Target::Vagrant, Some("qemu")).unwrap(), None);
    let text = generated(&spec, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    let vf = isoloom_test_support::ruby_blocks(&text);
    assert!(!vf.statements("HOST_ARCH = RbConfig::CONFIG[\"host_cpu\"]").is_empty());
    // QEMU boots machines in parallel unless told not to: the controller would run its play
    // before the DC is up, and the connecting side of the link could start before the listener.
    assert!(vf.statements.contains(&"ENV[\"VAGRANT_NO_PARALLEL\"] = \"1\"".to_owned()));

    let dc = provider(define(&vf, "dc01"), "qemu");
    let set = |b: &RubyBlock, s: &str| b.statements.contains(&s.to_owned());
    assert!(set(dc, "o.vm.box = \"peru/windows-server-2019-standard-x64-eval\""), "{dc:?}");
    assert!(set(dc, "o.vm.box_version = \">= 0\""), "the main box's pin isn't the QEMU box's: {dc:?}");
    assert!(set(dc, "o.vm.box_architecture = \"amd64\"") && set(dc, "v.arch = \"x86_64\""), "{dc:?}");
    // Emulated: one vCPU (a second only takes turns on QEMU's single emulation thread, and sees
    // the other's code rewrites half-done) and far longer timeouts.
    let emulated = dc.block("if \"x86_64\" != HOST_ARCH");
    assert_eq!(emulated.statements[..2], ["v.smp = \"cpus=1\"", "o.vm.boot_timeout = 3600"], "{emulated:?}");
    assert!(says(dc, "o.winrm.max_tries = 180"), "{dc:?}");
    // Its address on the adapter without a gateway (QEMU's first is its NAT), by the script
    // Hyper-V's adapters found by MAC run too.
    let address = [
        "@{ mac = ''; ip = '192.168.56.10'; mask = '255.255.255.0' }",
        "$_.IPEnabled -and -not $_.DefaultIPGateway",
    ];
    assert!(address.iter().all(|a| says(dc, a)), "{dc:?}");

    let ctl = provider(define(&vf, "isoloom-controller"), "qemu");
    assert!(set(ctl, "o.vm.box = \"cloud-image/debian-12\""), "{ctl:?}");
    assert!(!says(ctl, "v.arch"), "Isoloom's own VMs run on the host's CPU: {ctl:?}");
    // Both connect to the network's hub, each on its own port, in the order they are defined.
    let hubs = "QEMU_HUBS = IsoloomQemu.ports({ \"lab\" => [\"dc01\", \"isoloom-controller\"] })";
    let linked = [(dc, "dc01"), (ctl, "isoloom-controller")];
    assert!(
        vf.statements.contains(&hubs.to_owned()) && linked.iter().all(|(b, vm)| says(b, &hub_port("lab", vm))),
        "{dc:?}\n{ctl:?}"
    );
    assert!(!text.contains("isoloom-access"), "no port kept for a VM the lab doesn't declare: {text}");

    // Other providers are untouched.
    assert!(!says(provider(define(&vf, "dc01"), "virtualbox"), "socket"));
    Ok(())
}

/// A VM's QEMU link to its port of network `net`'s hub, as the Vagrantfile picked it.
fn hub_port(net: &str, vm: &str) -> String {
    format!("v.socket_opts = \"connect=127.0.0.1:#{{QEMU_HUBS[\"{net}\"][\"{vm}\"]}}\"")
}

#[test]
fn vagrant_runs_on_hyperv_with_a_switch_per_network_and_addresses_set_in_the_guest() -> Outcome {
    let spec = example("segmented")?;
    let v = generated(&spec, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    // The first adapter's switch from ISOLOOM_HYPERV_SWITCH, the helper module once.
    assert_eq!(v.matches("HYPERV_SWITCH = ENV.fetch(\"ISOLOOM_HYPERV_SWITCH\", \"Default Switch\")").count(), 1);
    let vf = isoloom_test_support::ruby_blocks(&v);
    let web_vm = define(&vf, "web");
    // bento has no Hyper-V build: the roboxes box, pinned; the VM named after the environment.
    let hyperv = provider(web_vm, "hyperv");
    assert_eq!(
        hyperv.statements[..3],
        [
            "o.vm.box = \"generic/debian12\"",
            "o.vm.box_version = \"4.3.12\"",
            "v.vmname = \"segmented-#{ISOLOOM_KEY}-web\""
        ],
        "{hyperv:?}"
    );
    assert!(
        hyperv
            .statements
            .contains(&"o.vm.network \"private_network\", bridge: HYPERV_SWITCH".to_owned()),
        "{hyperv:?}"
    );
    assert!(!says(web_vm, "enable_virtualization_extensions"), "{web_vm:?}");
    let web = web_vm.every_statement().join("\n");
    // Its network is a private switch, its adapter attached before every start (by a fixed MAC),
    // its address set in the guest once it answers, the switch gone with the last VM.
    assert!(web.contains("IsoloomHyperV.switches([\"isoloom-segmented-#{ISOLOOM_KEY}-front\"])"), "{web}");
    assert!(web.contains("before :\"VagrantPlugins::HyperV::Action::StartInstance\""), "{web}");
    assert!(
        web.contains("IsoloomHyperV.attach(machine, [[\"isoloom-segmented-#{ISOLOOM_KEY}-front\", \"0215"),
        "{web}"
    );
    // The address step hangs on a Hyper-V action: other providers run (and print) no trigger.
    assert!(web.contains("m.trigger.after :\"VagrantPlugins::HyperV::Action::WaitForIPAddress\""), "{web}");
    assert!(!v.contains("Vagrant::Action::Builtin"), "a trigger fires on every provider");
    // Every network is tried, and a missing adapter fails the step.
    assert!(
        web.contains("rc=0\n") && web.contains(" 10.61.10.10/24 || rc=1\n") && web.contains("exit $rc\n"),
        "{web}"
    );
    assert!(
        web.contains("systemctl enable -q isoloom-env-net.service") && web.contains("rc-update add local default"),
        "{web}"
    );
    assert!(web.contains("IsoloomHyperV.prune([\"isoloom-segmented-#{ISOLOOM_KEY}-front\"])"), "{web}");
    hyperv_router_and_macs(&v, spec.source().networks.len())?;
    Ok(())
}

/// On Hyper-V the router is on every network (`networks` of them), with an adapter on each;
/// every adapter of the Vagrantfile `v` has its own MAC.
fn hyperv_router_and_macs(v: &str, networks: usize) -> Outcome {
    let router = define(&isoloom_test_support::ruby_blocks(v), "isoloom-router").every_statement().join("\n");
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
        v.contains(
            "      o.vm.box = \"StefanScherer/windows_2019\"\n      o.vm.box_version = \">= 0\"\n      v.vmname = \"windows-hello-#{ISOLOOM_KEY}-web01\""
        ),
        "{v}"
    );
    assert!(
        v.contains("IsoloomHyperV.address(machine, <<~'PS1')") && v.contains("EnableStatic(@($n.ip), @($n.mask))"),
        "{v}"
    );
    // Hyper-V forwards no port: the published one is relayed from the host port its forward takes.
    let host = "ISOLOOM_HOSTPORTS.fetch(\"web01/web\", 8082)";
    assert!(v.contains(&format!("forwarded_port\", guest: 80, host: {host},")), "{v}");
    assert!(v.contains(&format!("IsoloomHyperV.ports(machine, [[80, {host}]])")), "{v}");
    assert!(v.contains("mac = '02:15:") && v.contains("ip = '192.168.57.10'; mask = '255.255.255.0'"), "{v}");

    // Nested virtualization comes from `nested_virtualization`, not from `privileged`.
    let hosts = example("existing-hosts")?;
    let plain = generated(&hosts, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(!says(
        define(&isoloom_test_support::ruby_blocks(&plain), "cache"),
        "enable_virtualization_extensions"
    ));
    let nested = lab(
        "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  hv: { os: debian@12, networks: { lab: 10 }, hardware: { nested_virtualization: true }, app: { steps: [{ run: a.sh }] } }\n",
    )?;
    let nesting = generated(&nested, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    let nesting = isoloom_test_support::ruby_blocks(&nesting);
    let hv = define(&nesting, "hv");
    assert!(
        provider(hv, "hyperv")
            .statements
            .contains(&"v.enable_virtualization_extensions = true".to_owned()),
        "{hv:?}"
    );
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
    assert_eq!(isoloom_targets::refusal_on(&spec, Target::Vagrant, Some("virtualbox")).unwrap(), None);
    let why = isoloom_targets::refusal_on(&spec, Target::Vagrant, Some("hyperv"))
        .unwrap()
        .expect("refused on Hyper-V");
    assert!(why.contains("machine `box`") && why.contains("boxes.hyperv"), "{why}");
    // The spec's own box: its latest version with a Hyper-V build.
    let own = lab(&format!(
        "{yaml}overrides: {{ vagrant: {{ machines: {{ box: {{ boxes: {{ hyperv: my/debian13 }} }} }} }} }}\n"
    ))?;
    assert_eq!(isoloom_targets::refusal_on(&own, Target::Vagrant, Some("hyperv")).unwrap(), None);
    let given = generated(&own, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(given.contains("o.vm.box = \"my/debian13\"\n      o.vm.box_version = \">= 0\""), "{given}");
    Ok(())
}
