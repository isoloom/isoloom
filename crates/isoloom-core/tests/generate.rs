//! Generators against the examples: the committed files under each example's `.isoloom/`
//! are exactly what `generate` produces (what `isoloom check` enforces in CI), and specs
//! a generator can't handle yet are refused with the reason.

use std::path::{Path, PathBuf};

use isoloom_core::{GenerateError, Target, generate, generate_all, load, parse};

fn example(name: &str) -> (PathBuf, isoloom_core::Spec) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name);
    let spec = load(&dir).expect("example parses");
    (dir, spec)
}

fn assert_committed(name: &str) {
    let (dir, spec) = example(name);
    let (files, _) = generate_all(&spec);
    assert!(!files.is_empty(), "{name}: nothing generated");
    for f in files {
        let on_disk = std::fs::read_to_string(dir.join(&f.path)).unwrap_or_default();
        assert!(
            on_disk == f.contents,
            "{name}: {} is stale; run `cargo run -- generate examples/{name}`",
            f.path
        );
    }
}

#[test]
fn committed_outputs_are_up_to_date() {
    assert_committed("hello-stack");
    assert_committed("supplier-portal-api");
    assert_committed("segmented");
    assert_committed("pivot-dmz");
    assert_committed("edge-firewall");
    assert_committed("air-gapped");
    assert_committed("windows-hello");
    assert_committed("ansible-pair");
    assert_committed("mixed-office");
    assert_committed("arm-lab");
    assert_committed("arm-vm");
    assert_committed("slow-link");
    assert_committed("existing-hosts");
    assert_committed("workbench");
    assert_committed("vlan-office");
    assert_committed("cisco-iol");
    assert_committed("cisco-dynamips");
    assert_committed("cisco-qemu");
    assert_committed("solo-web");
}

#[test]
fn refusal_reports_what_a_generator_would_refuse() {
    // A target can be possible by its machines' editions yet refused by its generator; `targets`
    // used to show ✓ for those (Proxmox for a Windows lab, or one with environment provisioning).
    let (_, windows) = example("windows-hello");
    let why = isoloom_core::refusal(&windows, Target::Proxmox).expect("Windows has no Proxmox image yet");
    assert!(why.contains("Proxmox image"), "{why}");
    // Environment-level provisioning runs from a controller on Proxmox now, so it is not refused.
    let (_, ansible) = example("ansible-pair");
    assert_eq!(isoloom_core::refusal(&ansible, Target::Proxmox), None);
    // A plain Linux lab is generated: nothing to refuse.
    let (_, plain) = example("hello-stack");
    assert_eq!(isoloom_core::refusal(&plain, Target::Proxmox), None);
    assert_eq!(isoloom_core::refusal(&plain, Target::Docker), None);
}

#[test]
fn hybrid_runs_containers_beside_the_vms() {
    let (_, spec) = example("mixed-office");
    let files = generate(&spec, Target::Hybrid).unwrap();
    let vagrantfile = &files.iter().find(|f| f.path.ends_with("hybrid/Vagrantfile")).unwrap().contents;
    let compose = &files.iter().find(|f| f.path.ends_with("hybrid/compose.yml")).unwrap().contents;
    // The Windows server is a VM; the web app is a container, not a VM.
    assert!(vagrantfile.contains("config.vm.define \"files01\""));
    assert!(!vagrantfile.contains("config.vm.define \"intranet\""));
    assert!(vagrantfile.contains("config.vm.define \"isoloom-docker\""));
    assert!(vagrantfile.contains("--nicpromisc2"));
    assert!(vagrantfile.contains("docker network create -d macvlan --subnet 192.168.58.0/24"));
    assert!(vagrantfile.contains("guest: 8083, host: 8083, host_ip: \"127.0.0.1\""));
    // The containers join the VMs' network at their own address, and know the VMs by name.
    assert!(compose.contains("ipv4_address: 192.168.58.20"));
    assert!(compose.contains("name: isoloom-mixed-office-office"));
    assert!(compose.contains("files01:192.168.58.10"));
    assert!(!compose.contains("isoloom-check"));
}

#[test]
fn hybrid_is_offered_only_for_mixed_environments() {
    let (_, spec) = example("hello-stack");
    assert!(!isoloom_core::targets::derive(&spec).contains(&Target::Hybrid));
    let (_, spec) = example("mixed-office");
    assert!(isoloom_core::targets::derive(&spec).contains(&Target::Hybrid));
}

#[test]
fn compose_has_addresses_healthchecks_init_and_checks() {
    let (_, spec) = example("hello-stack");
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(compose.contains("ipv4_address: 10.60.0.20"));
    assert!(compose.contains("gateway: 10.60.0.1"));
    // The healthcheck runs Isoloom's own busybox (exec form): no shell needed in the image.
    assert!(compose.contains("/.isoloom-probe/busybox nc -z -w 2 127.0.0.1 6379"));
    assert!(compose.contains("- CMD\n"));
    assert!(compose.contains("isoloom-probe-amd64:/.isoloom-probe:ro"));
    assert!(compose.contains("image: busybox:1.37.0-musl"));
    assert!(compose.contains("cache-init-1:"));
    assert!(compose.contains("condition: service_completed_successfully"));
    assert!(compose.contains("isoloom-check:"));
    assert!(compose.starts_with("# Generated by isoloom"));
    // The runners: one on every network (no access machine), one per machine in its namespace.
    let files = generate(&spec, Target::Docker).unwrap();
    assert!(
        compose.contains("isoloom-check-web:") && compose.contains("network_mode: service:web"),
        "{compose}"
    );
    let runner = contents(&files, ".isoloom/docker/checks/networks.sh");
    assert!(runner.contains("cd /isoloom/project && sh checks/cache-seeded.sh"), "{runner}");
    assert!(runner.contains("_http_is 'http://web/' 200"), "{runner}");
}

#[test]
fn inputs_reach_only_the_machines_that_list_them() {
    let (_, spec) = example("supplier-portal-api");
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    let web = compose.split("\n  web:").nth(1).unwrap().split("\n  isoloom-check:").next().unwrap();
    assert!(compose.contains("LAUNCH_TOKEN: ${LAUNCH_TOKEN:-}"));
    assert!(!web.contains("LAUNCH_TOKEN"), "web must not receive the database's inputs");
}

#[test]
fn vagrant_orders_machines_and_names_them() {
    let (_, spec) = example("hello-stack");
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let cache = vf.find("config.vm.define \"cache\"").unwrap();
    let web = vf.find("config.vm.define \"web\"").unwrap();
    assert!(cache < web, "cache starts before web, which depends on it");
    assert!(vf.contains("'10.60.0.10 web'"));
    assert!(vf.contains("virtualbox__intnet: \"isoloom-hello-stack-app\""));
}

#[test]
fn libvirt_domains_are_named_after_the_environment() {
    // Unset, vagrant-libvirt prefixes the folder's name ("vagrant_web"), the same for every lab.
    let (_, spec) = example("hello-stack");
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    assert!(vf.contains("v.default_prefix = \"hello-stack_\""), "{vf}");
    let two = isoloom_core::instance::apply(&spec, 2).unwrap();
    let vf = &generate(&two, Target::Vagrant).unwrap()[0].contents;
    assert!(vf.contains("v.default_prefix = \"hello-stack-2_\""));
    let dvm = contents(&generate(&spec, Target::DockerVm).unwrap(), ".isoloom/docker-vm/Vagrantfile");
    assert!(dvm.contains("v.default_prefix = \"hello-stack_\""));
}

#[test]
fn the_controller_gets_every_provider_block_the_machines_get() {
    let (_, spec) = example("ansible-pair");
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let controller = vf.split("config.vm.define \"isoloom-controller\"").nth(1).unwrap();
    for provider in ["virtualbox", "vmware_desktop", "parallels", "utm", "qemu", "vmware_esxi", "libvirt"] {
        assert!(
            controller.contains(&format!("m.vm.provider \"{provider}\"")),
            "controller has no {provider} block"
        );
    }
}

#[test]
fn reach_rules_add_a_router_with_matching_rules() {
    let (_, spec) = example("segmented");
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(compose.contains("isoloom-router:"));
    assert!(compose.contains("ipv4_address: 10.61.20.254"), "router at the last address");
    assert!(compose.contains("ip saddr 10.61.99.0/24 ip daddr 10.61.10.0/24 accept"));
    assert!(compose.contains("th dport { 6379 }"));
    assert!(compose.contains("policy drop"));
    // Routes via the router, in the machine's own network namespace.
    assert!(compose.contains("network_mode: service:web"));
    assert!(compose.contains("ip route replace 10.61.20.0/24 via 10.61.10.254"));
    // Offline machines lose their default route; no Docker-internal network with a router.
    assert!(compose.contains("ip route del default"));
    assert!(!compose.contains("internal: true"));
    // Names across networks, and checks from the access side.
    assert!(compose.contains("cache:10.61.20.20"));
    assert!(compose.contains("network_mode: service:isoloom-access"));
}

#[test]
fn vagrant_router_waits_and_offline_blocks() {
    let (_, spec) = example("segmented");
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let router = vf.find("config.vm.define \"isoloom-router\"").unwrap();
    let cache = vf.find("config.vm.define \"cache\"").unwrap();
    let web = vf.find("config.vm.define \"web\"").unwrap();
    assert!(router < cache && cache < web);
    assert!(vf.contains("name: \"wait for cache\""));
    assert!(vf.contains("</dev/tcp/cache/6379"));
    assert!(vf.contains("name: \"no internet\""));
    assert!(vf.contains("isoloom-routes.service"));
}

#[test]
fn unsupported_features_are_refused_with_the_reason() {
    let (_, ad) = example("corp-ad-basics");
    assert_eq!(generate(&ad, Target::Docker), Err(GenerateError::NotPossible(Target::Docker)));
    assert!(
        matches!(generate(&ad, Target::Vagrant), Err(GenerateError::Unsupported { .. })),
        "Windows has no local image yet"
    );
    assert!(
        matches!(generate(&ad, Target::Proxmox), Err(GenerateError::Unsupported { ref what, .. }) if what.contains("windows-server-2022")),
        "no Windows image on Proxmox yet"
    );
    assert!(matches!(generate(&ad, Target::CloudVm), Err(GenerateError::Unsupported { ref what, .. }) if what.contains("Windows steps are .ps1 scripts")));
}

#[test]
fn an_arm64_access_machine_that_is_actually_built_is_still_refused_on_the_cloud() {
    // An access machine with no implementation isn't instantiated, so its arch is moot. One that
    // declares a VM is built, so arm64 must still be caught (the cloud images are x86-64 only).
    let spec = parse(
        "version: 1\nname: arm-access\nnetworks:\n  lab: { cidr: 10.60.0.0/24 }\nmachines:\n  kali:\n    networks: { lab: 10 }\n    access: true\n    arch: arm64\n    vm: { os: debian-12, provision: [p.sh] }\n",
    )
    .expect("parses");
    assert!(
        matches!(generate(&spec, Target::CloudVm), Err(GenerateError::Unsupported { ref what, .. }) if what.contains("arm64")),
        "an arm64 access machine with a VM must not slip past the cloud refusal"
    );
}

#[test]
fn a_gateway_routes_its_networks_instead_of_the_router() {
    let (_, spec) = example("edge-firewall");
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(!compose.contains("isoloom-router"), "no router: the firewall routes");
    // The firewall takes .1; Docker's bridge moves to the last address on its networks.
    assert!(compose.contains("ipv4_address: 10.70.10.1"));
    assert!(compose.contains("gateway: 10.70.10.254"));
    assert!(compose.contains("gateway: 10.70.0.1"));
    assert!(compose.contains("net.ipv4.ip_forward: '1'"));
    // Machines behind it send everything through it, once it answers.
    assert!(compose.contains("ip route replace default via 10.70.10.1"));
    assert!(compose.contains("ip route replace 10.70.10.0/24 via 10.70.0.2"));
    let web_routes = compose.split("\n  web-routes:").nth(1).unwrap().split("\n  cache:").next().unwrap();
    assert!(web_routes.contains("fw:\n        condition: service_healthy"));
    // Its own rules decide: no Docker-internal network, no route deletion.
    assert!(!compose.contains("internal: true"));
    assert!(!compose.contains("ip route del default"));
}

#[test]
fn vagrant_moves_machines_behind_their_gateway_after_provisioning() {
    let (_, spec) = example("edge-firewall");
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let fw = vf.find("config.vm.define \"fw\"").unwrap();
    let web = vf.find("config.vm.define \"web\"").unwrap();
    assert!(fw < web, "the gateway starts first");
    assert!(vf.contains("name: \"forwarding\""));
    let web_block = &vf[web..vf.find("config.vm.define \"cache\"").unwrap()];
    let provision = web_block.find("provision/web.sh").unwrap();
    let behind = web_block.find("name: \"through the gateway\"").unwrap();
    let default = web_block.find("ip route replace default via 10.70.10.1").unwrap();
    assert!(provision < behind && behind < default, "installs first, then moves behind the firewall");
    assert!(web_block.contains("th dport 53 accept"), "name lookups still work behind a gateway");
}

#[test]
fn networks_outside_10_move_into_10_on_docker_keeping_last_octets() {
    let spec = isoloom_core::parse(
        "version: 1\nname: t\nnetworks:\n  corp: { cidr: 192.168.20.0/24 }\n  dmz: { cidr: 172.18.5.0/24 }\n  taken: { cidr: 10.192.20.0/24 }\nmachines:\n  a: { networks: { corp: 10, dmz: 7, taken: 9 }, docker: { image: x }, vm: { os: debian-12, provision: [p.sh] } }\n",
    )
    .unwrap();
    assert_eq!(isoloom_core::validate(&spec), vec![]);
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    // 172.N.X -> 10.N.X; 192.168.X -> 10.192.X, taken here, so the next free block.
    assert!(compose.contains("subnet: 10.18.5.0/24"), "{compose}");
    assert!(compose.contains("ipv4_address: 10.18.5.7"));
    assert!(compose.contains("subnet: 10.240.0.0/24"));
    assert!(compose.contains("ipv4_address: 10.240.0.10"));
    assert!(compose.contains("# On Docker, network `corp` uses 10.240.0.0/24 instead of 192.168.20.0/24"));
    // VMs keep the spec's addresses.
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    assert!(vf.contains("ip: \"192.168.20.10\""));
}

#[test]
fn a_network_can_name_its_docker_block() {
    let base = "version: 1\nname: t\nnetworks:\n  corp: { cidr: 192.168.20.0/24, docker: { cidr: DOCKER } }\nmachines:\n  a: { networks: { corp: 10 }, docker: { image: x } }\n";
    let spec = isoloom_core::parse(&base.replace("DOCKER", "10.77.0.0/24")).unwrap();
    assert_eq!(isoloom_core::validate(&spec), vec![]);
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(compose.contains("ipv4_address: 10.77.0.10"));
    for bad in ["10.77.0.0/25", "172.20.0.0/24"] {
        let p = isoloom_core::validate(&isoloom_core::parse(&base.replace("DOCKER", bad)).unwrap());
        assert!(p.iter().any(|x| x.at == "networks.corp.docker.cidr"), "{bad}: {p:?}");
    }
}

#[test]
fn windows_machines_use_winrm_and_powershell() {
    let spec = isoloom_core::parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 192.168.56.0/24 }\nmachines:\n  dc01:\n    networks: { lab: 10 }\n    services: [{ port: 389 }]\n    vm:\n      os: windows-server-2019\n      provision: [provision/dc.ps1]\n  web:\n    networks: { lab: 20 }\n    depends_on: [dc01]\n    vm:\n      os: windows-server-2019\n      image: { vagrant: example/win2019, vagrant_version: '1.0' }\n      provision: [provision/web.ps1]\n",
    )
    .unwrap();
    assert_eq!(isoloom_core::validate(&spec), vec![]);
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let dc = &vf[vf.find("config.vm.define \"dc01\"").unwrap()..vf.find("config.vm.define \"web\"").unwrap()];
    assert!(dc.contains("m.vm.box = \"StefanScherer/windows_2019\""));
    assert!(dc.contains("m.vm.box_version = \"2021.05.15\""));
    assert!(dc.contains("m.vm.communicator = \"winrm\""));
    assert!(dc.contains("drivers\\\\etc\\\\hosts"), "the Windows hosts file: {dc}");
    assert!(dc.contains("path: File.join(ROOT, \"provision/dc.ps1\")"));
    let web = &vf[vf.find("config.vm.define \"web\"").unwrap()..];
    assert!(web.contains("m.vm.box = \"example/win2019\""), "the spec's image wins");
    assert!(web.contains("m.vm.box_version = \"1.0\""));
    assert!(web.contains("Test-NetConnection dc01 -Port $p"));
}

#[test]
fn windows_refuses_what_it_cant_do_yet() {
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  NAME:\n    networks: { lab: 10 }\n    vm: { os: windows-server-2019, provision: [STEP] }\n";
    for (name, step, why) in [("dc01", "setup.yml", "PowerShell scripts"), ("averyveryverylongname", "a.ps1", "15 characters")] {
        let spec = isoloom_core::parse(&base.replace("NAME", name).replace("STEP", step)).unwrap();
        let err = generate(&spec, Target::Vagrant).unwrap_err().to_string();
        assert!(err.contains(why), "{name}: {err}");
    }
}

fn contents(files: &[isoloom_core::GeneratedFile], path: &str) -> String {
    files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("{path} not generated"))
        .contents
        .clone()
}

#[test]
fn the_runner_supplies_the_access_machine_through_its_image_table() {
    let (_, spec) = example("segmented");
    let table = isoloom_core::images::Table::parse("access:\n  docker: kalilinux/kali-rolling\n  vm: kali\n").unwrap();
    let applied = table.apply(&spec);
    let compose = contents(&generate(&applied, Target::Docker).unwrap(), ".isoloom/docker/compose.yml");
    // The access machine is a real container now (kept running idle), not the check stand-in.
    assert!(
        compose.contains("  user:\n    image: kalilinux/kali-rolling\n    platform: linux/amd64\n    entrypoint:\n    - sleep\n    - infinity\n"),
        "{compose}"
    );
    assert!(!compose.contains("isoloom-access"));
    // Its VM part comes from the spec, which wins over the table.
    let vagrant = contents(&generate(&applied, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(vagrant.contains("config.vm.define \"user\" do |m|\n    m.vm.box = \"bento/debian-12\""));
    // Without the table, nothing changes.
    assert_eq!(isoloom_core::images::Table::default().apply(&spec), spec);
}

#[test]
fn the_image_table_sits_between_built_in_images_and_the_spec() {
    let (_, spec) = example("hello-stack");
    let table = isoloom_core::images::Table::parse("os:\n  debian-12: { vagrant: my-org/debian-12, vagrant_version: \"1.2.0\" }\n").unwrap();
    let vagrant = contents(&generate(&table.apply(&spec), Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(
        vagrant.contains("m.vm.box = \"my-org/debian-12\"\n    m.vm.box_version = \"1.2.0\""),
        "{vagrant}"
    );
    // A spec's own vm.image still wins.
    let (_, windows) = example("windows-hello");
    let table = isoloom_core::images::Table::parse("os:\n  windows-server-2019: { vagrant: other/box }\n").unwrap();
    let mut pinned = windows.clone();
    for m in pinned.machines.values_mut() {
        if let Some(vm) = &mut m.vm {
            vm.image = Some(isoloom_core::model::VmImage {
                vagrant: Some("spec/box".into()),
                vagrant_version: None,
                winrm: None,
            });
        }
    }
    let vagrant = contents(&generate(&table.apply(&pinned), Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(vagrant.contains("\"spec/box\"") && !vagrant.contains("other/box"));
    assert!(isoloom_core::images::Table::parse("bogus: 1\n").is_err());
}

#[test]
fn proxmox_forwards_published_ports_from_the_router() {
    let (_, spec) = example("hello-stack");
    let tf = contents(&generate(&spec, Target::Proxmox).unwrap(), ".isoloom/proxmox/main.tf");
    // From anything outside the lab to the machine, and the forward allowed through.
    assert!(tf.contains("tcp dport 8080 dnat ip to 10.60.0.10:"), "{tf}");
    assert!(tf.contains("ct status dnat accept"));
    // Where to connect: the router's uplink address, from the guest agent.
    assert!(tf.contains("output \"published\""));
    assert!(tf.contains("\"web/") && tf.contains(":8080\""));
}

#[test]
fn proxmox_cloud_init_uses_the_machines_declared_resolver() {
    // A machine that points at the lab's own resolver (e.g. an AD member at the domain
    // controller) must get that in cloud-init, not a forced public resolver.
    let spec = parse(
        "version: 1\nname: dns-lab\nnetworks:\n  lab: { cidr: 10.60.0.0/24 }\nmachines:\n  member:\n    networks: { lab: 10 }\n    dns: { servers: [10.60.0.2], domain: arm.lab }\n    vm: { os: debian-12, provision: [p.sh] }\n  plain:\n    networks: { lab: 11 }\n    vm: { os: debian-12, provision: [p.sh] }\n",
    )
    .expect("parses");
    let tf = contents(&generate(&spec, Target::Proxmox).unwrap(), ".isoloom/proxmox/main.tf");
    assert!(tf.contains("servers = [\"10.60.0.2\"]"), "{tf}");
    assert!(tf.contains("domain = \"arm.lab\""), "{tf}");
    // The machine with no resolver of its own still gets a public one so it can install software.
    assert!(tf.contains("servers = [\"1.1.1.1\"]"), "{tf}");
}

#[test]
fn kubernetes_turns_networks_and_reach_into_network_policies() {
    let (_, spec) = example("segmented");
    let files = generate(&spec, Target::Kubernetes).unwrap();
    let env = contents(&files, ".isoloom/kubernetes/environment.yaml");
    // Deny by default, each network's machines together, then the reach rules (with ports).
    assert!(env.contains("name: isoloom-default-deny"));
    assert!(env.contains("name: net-back"));
    // Reach policies carry their index so a dash in a network name can't collide two of them.
    assert!(env.contains("name: reach-0-access-front"));
    assert!(env.contains("name: reach-1-front-back"));
    assert!(env.contains("port: 6379"));
    // Names resolve as everywhere else: a hostname and a Service per serving machine.
    assert!(env.contains("hostname: web") && env.contains("kind: Service\nmetadata:\n  name: web"));
    // A volume is a claim.
    assert!(env.contains("kind: PersistentVolumeClaim"));
    // Checks: a Job per position, the user's with the access network's label (its policies apply),
    // the others carrying their machine's networks; each runs its own runner script.
    let job = contents(&files, ".isoloom/kubernetes/checks/job.yaml");
    assert!(job.contains("net.isoloom.com/access: member"), "{job}");
    assert!(job.contains("name: isoloom-check-web") && job.contains("/isoloom/run/web.sh"), "{job}");
    let k = contents(&files, ".isoloom/kubernetes/checks/kustomization.yaml");
    assert!(k.contains("- user.sh") && k.contains("- web.sh"), "{k}");
    // Machines are reached by name (Kubernetes picks the addresses).
    let web = contents(&files, ".isoloom/kubernetes/checks/web.sh");
    assert!(web.contains("_tcp 'cache' 6379"), "{web}");
}

#[test]
fn kubernetes_publishes_and_keeps_offline_machines_inside() {
    let (_, spec) = example("hello-stack");
    let env = contents(&generate(&spec, Target::Kubernetes).unwrap(), ".isoloom/kubernetes/environment.yaml");
    assert!(env.contains("type: LoadBalancer") && env.contains("port: 8080"), "{env}");
    assert!(env.contains("name: offline-"));
    // The checks stand on those offline networks: offline too.
    assert!(env.contains("name: offline-isoloom-check"));
    assert!(env.contains("policyTypes:\n    - Egress") || env.contains("policyTypes:\n  - Egress"));
}

#[test]
fn cloud_vm_gives_a_machine_on_several_networks_an_interface_on_each() {
    let (_, mut spec) = example("pivot-dmz");
    // Kali has no AWS image yet: the user lands on Debian here.
    for m in spec.machines.values_mut() {
        if let Some(vm) = &mut m.vm
            && vm.os == "kali"
        {
            vm.os = "debian-12".into();
        }
    }
    let tf = contents(&generate(&spec, Target::CloudVm).unwrap(), ".isoloom/cloud-vm/aws/main.tf");
    assert!(tf.contains("resource \"aws_network_interface\" \"gateway_dmz\""), "{tf}");
    assert!(tf.contains("resource \"aws_network_interface\" \"gateway_internal\""));
    assert!(tf.contains("source_dest_check = false"));
    assert!(tf.contains("resource \"aws_network_interface_attachment\" \"gateway_internal\""));
    assert!(tf.contains("resource \"aws_eip\" \"gateway\""));
    // The set-up finds the extra interface by its MAC address: interpolated, not a literal.
    assert!(tf.contains("\\\"${lower(aws_network_interface.gateway_internal.mac_address)}\\\""), "{tf}");
    assert!(!tf.contains("$${lower("));
}

#[test]
fn cloud_vm_runs_the_environments_playbooks_from_a_controller() {
    let (_, spec) = example("ansible-pair");
    let tf = contents(&generate(&spec, Target::CloudVm).unwrap(), ".isoloom/cloud-vm/aws/main.tf");
    // A key of its own, authorized on every machine; the controller at the controller address.
    assert!(tf.contains("resource \"tls_private_key\" \"controller\""));
    assert!(tf.contains("${trimspace(tls_private_key.controller.public_key_openssh)}"), "{tf}");
    assert!(tf.contains("private_ips       = [\"10.63.0.253\"]"));
    // After every machine is set up, it runs the playbook with the groups and vars.
    assert!(tf.contains("depends_on = [terraform_data.web, terraform_data.cache]") || tf.contains("depends_on = [terraform_data.cache, terraform_data.web]"));
    assert!(tf.contains("ansible-playbook -i /etc/isoloom/inventory.ini -i /opt/isoloom/ansible/groups.ini"));
    assert!(tf.contains("[webservers]"));
}

#[test]
fn cloud_vm_runs_windows_over_winrm_with_its_name() {
    let (_, spec) = example("windows-hello");
    let tf = contents(&generate(&spec, Target::CloudVm).unwrap(), ".isoloom/cloud-vm/aws/main.tf");
    // Amazon's image, no key pair (AWS refuses ED25519 on Windows), WinRM with a generated password.
    assert!(tf.contains("Windows_Server-2019-English-Full-Base-*"));
    assert!(tf.contains("resource \"random_password\" \"windows\""));
    assert!(tf.contains("type     = \"winrm\""));
    // Renamed and restarted before its set-up, which checks the name.
    assert!(tf.contains("resource \"time_sleep\" \"web01_restart\""));
    assert!(tf.contains("still named"));
    // Auto-stop as a scheduled task (a pending shutdown would block the rename's restart).
    assert!(tf.contains("isoloom-auto-stop"));
    // A Windows-only environment: the controller runs the checks.
    assert!(tf.contains("host = aws_eip.isoloom_controller.public_ip"));
}

#[test]
fn linux_machines_leave_windows_out_of_etc_hosts() {
    // A Linux member joining an AD domain must not have the Windows DC in /etc/hosts by its
    // short name: that shadows AD DNS and breaks the Kerberos SPN lookup on realm join.
    let spec = isoloom_core::parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 192.168.56.0/24 }\nmachines:\n  dc01:\n    networks: { lab: 10 }\n    services: [{ port: 389 }]\n    vm: { os: windows-server-2019, provision: [p.ps1] }\n  lx01:\n    networks: { lab: 12 }\n    services: [{ port: 22 }]\n    depends_on: [dc01]\n    vm: { os: ubuntu-24.04, provision: [p.sh] }\n",
    )
    .unwrap();
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let start = vf.find("config.vm.define \"lx01\"").unwrap();
    let end = vf[start + 20..].find("config.vm.define").map(|i| start + 20 + i).unwrap_or(vf.len());
    let lx = &vf[start..end];
    // The Windows DC isn't added to lx01's /etc/hosts by name.
    assert!(
        !lx.contains("192.168.56.10 dc01"),
        "Windows DC should be left out of the Linux host's /etc/hosts"
    );
    // And the depends_on wait on the Windows DC is by address, not name.
    assert!(lx.contains("/192.168.56.10/389"), "the Windows dependency is waited on by address: {lx}");
    assert!(!lx.contains("</dev/tcp/dc01/"), "not by the name that isn't in /etc/hosts");
}

#[test]
fn an_idle_container_is_kept_running() {
    let spec = parse(
        "version: 1\nname: idle\nnetworks:\n  lan: { cidr: 10.10.1.0/24 }\nmachines:\n  box:\n    networks: { lan: 10 }\n    docker: { image: \"alpine:3.20\", idle: true }\n  web:\n    networks: { lan: 11 }\n    services: [{ port: 80 }]\n    docker: { image: \"nginx:1.27-alpine\" }\n",
    )
    .expect("spec parses");
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(
        compose.contains("  box:\n    image: alpine:3.20\n    platform: linux/amd64\n    entrypoint:\n    - sleep\n    - infinity\n"),
        "{compose}"
    );
    // A machine that runs its own service keeps its image's command.
    let web = compose.split("  web:").nth(1).unwrap();
    assert!(!web.split("\n  ").next().unwrap().contains("entrypoint"), "{compose}");
    let k8s: String = generate(&spec, Target::Kubernetes).unwrap().iter().map(|f| f.contents.clone()).collect();
    assert!(
        k8s.contains("command:\n        - sleep\n        - infinity")
            || k8s.contains("command:\n          - sleep\n          - infinity")
            || k8s.contains("- sleep\n"),
        "{k8s}"
    );
}

#[test]
fn the_controller_is_small_by_default_and_sized_by_the_spec() {
    // ansible-pair asks for 768 MB.
    let (_, spec) = example("ansible-pair");
    let vagrantfile = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    let ctl = &vagrantfile[vagrantfile.find("config.vm.define \"isoloom-controller\"").unwrap()..];
    assert!(ctl.contains("m.vm.box = RbConfig::CONFIG[\"host_cpu\"] =~ /arm|aarch64/ ? \"bento/debian-12\" : \"generic/alpine319\""), "{ctl}");
    assert!(ctl.contains("o.vm.box = \"generic/alpine319\""), "libvirt has Alpine on both architectures");
    assert!(ctl.contains("v.memory = 768"));
    assert!(ctl.contains("command -v apk"));
    // Without `controller:`: 1 CPU, 512 MB, on every target with a controller.
    let mut spec = spec;
    spec.controller = None;
    let vagrantfile = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    let ctl = &vagrantfile[vagrantfile.find("config.vm.define \"isoloom-controller\"").unwrap()..];
    assert!(ctl.contains("v.cpus = 1\n      v.memory = 512"), "{ctl}");
    assert!(ctl.contains("v.guest_memsize = 512"));
    let tf = contents(&generate(&spec, Target::CloudVm).unwrap(), ".isoloom/cloud-vm/aws/main.tf");
    assert!(
        tf.contains("instance_type = \"t3.micro\"\n  key_name      = aws_key_pair.env.key_name\n  user_data"),
        "{tf}"
    );
    let tf = contents(&generate(&spec, Target::Proxmox).unwrap(), ".isoloom/proxmox/main.tf");
    assert!(tf.contains("dedicated = 512"));
    // A box of one's own, pinned.
    spec.controller = isoloom_core::parse(
        "version: 1\nname: t\nnetworks: {}\nmachines: {}\ncontroller: { image: { vagrant: bento/debian-12, vagrant_version: \"202407.22.0\" }, resources: { cpus: 2 } }\n",
    )
    .unwrap()
    .controller;
    let vagrantfile = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(vagrantfile.contains("m.vm.box = \"bento/debian-12\"\n    m.vm.box_version = \"202407.22.0\""));
    assert!(vagrantfile.contains("v.cpus = 2"));
}

/// An init job on a machine nothing depends on is a leaf job: `up --wait` would fail on its exit,
/// so the start plan waits for the rest and runs it after (#42).
#[test]
fn leaf_init_jobs_run_after_the_wait() {
    let spec = isoloom_core::parse(
        r#"
version: 1
name: leaf
networks: { lan: { cidr: 10.70.0.0/24 } }
machines:
  db:
    networks: { lan: 20 }
    services: [{ port: 5432 }]
    docker: { image: "postgres:17", init: [seed.sh] }
  web:
    networks: { lan: 10 }
    services: [{ port: 80 }]
    depends_on: [db]
    docker: { image: "nginx:1.27-alpine", init: [setup.sh, warm.sh] }
"#,
    )
    .unwrap();
    // db's job is waited for by web; web's two jobs by nothing.
    assert_eq!(isoloom_core::generate::leaf_jobs(&spec), ["web-init-1", "web-init-2"]);
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    let plan = isoloom_core::generate::start_plan(compose).unwrap();
    assert_eq!(plan.jobs, ["web-init-1", "web-init-2"]);
    assert!(plan.wait.contains(&"web".to_string()) && plan.wait.contains(&"db-init-1".to_string()));
    assert!(!plan.wait.iter().any(|w| w.starts_with("isoloom-check")), "profile services aren't started");
    let cmd = isoloom_core::generate::start_commands("docker compose", &plan.jobs, Some(900));
    assert_eq!(
        cmd,
        "docker compose up -d --build --wait --wait-timeout 900 $(docker compose config --services | grep -vx -e web-init-1 -e web-init-2) \
         && docker compose up --no-deps --exit-code-from web-init-1 web-init-1 \
         && docker compose up --no-deps --exit-code-from web-init-2 web-init-2"
    );
    // Nothing left over: the one command it always was.
    assert_eq!(
        isoloom_core::generate::start_commands("docker compose", &[], None),
        "docker compose up -d --build --wait"
    );
    let (_, hello) = example("hello-stack");
    let compose = &generate(&hello, Target::Docker).unwrap()[0].contents;
    assert_eq!(isoloom_core::generate::start_plan(compose).unwrap(), Default::default());
}

/// The check runners run with --no-deps: `compose run` would run completed init jobs again,
/// re-seeding the environment on every test (#47).
#[test]
fn checks_on_the_docker_vm_never_rerun_init_jobs() {
    let (_, spec) = example("hello-stack");
    let files = generate(&spec, Target::DockerVm).unwrap();
    let vf = files.iter().find(|f| f.path.ends_with("Vagrantfile")).unwrap();
    assert!(vf.contents.contains("--profile check run --rm --no-deps"));
}
