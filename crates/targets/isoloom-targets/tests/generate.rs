//! Generators against the examples and small specs: what each target writes, and specs a
//! generator can't handle yet refused with the reason.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_spec::{Target, load, parse};
use isoloom_targets::{GenerateError, Lab, generate, generate_all, generate_instance};

fn source(name: &str) -> isoloom_spec::Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name)).expect("example parses")
}

fn example(name: &str) -> Lab {
    Lab::new(source(name))
}

fn lab(yaml: &str) -> Lab {
    Lab::new(parse(yaml).expect("parses"))
}

/// The lab's problems, each as `path: reason`.
fn problems(lab: &Lab) -> Vec<String> {
    lab.problems().iter().map(isoloom_spec::syntax::Problem::line).collect()
}

#[test]
fn an_example_every_generator_refuses_is_refused_with_reasons() {
    // corp-ad-basics: every generator refuses it (Windows provisioning with an Ansible playbook,
    // no Proxmox image for Windows).
    let spec = example("corp-ad-basics");
    let (files, skipped) = generate_all(&spec);
    assert!(files.is_empty(), "{:?}", files.iter().map(|f| &f.path).collect::<Vec<_>>());
    assert_ne!(skipped, [] as [isoloom_targets::GenerateError; 0]);
}

#[test]
fn refusal_reports_what_a_generator_would_refuse() {
    // A target can be possible by its machines' editions yet refused by its generator; `targets`
    // used to show ✓ for those (Proxmox for a Windows lab whose steps are playbooks, or one with
    // environment provisioning).
    let ad = example("corp-ad-basics");
    let why = isoloom_targets::refusal(&ad, Target::Proxmox).expect("Windows steps are PowerShell on Proxmox");
    assert!(why.contains("PowerShell scripts"), "{why}");
    // A Windows lab of PowerShell steps clones a template on Proxmox.
    let windows = example("windows-hello");
    assert_eq!(isoloom_targets::refusal(&windows, Target::Proxmox), None);
    // Environment-level provisioning runs from a controller on Proxmox now, so it is not refused.
    let ansible = example("ansible-pair");
    assert_eq!(isoloom_targets::refusal(&ansible, Target::Proxmox), None);
    // A plain Linux lab is generated: nothing to refuse.
    let plain = example("hello-stack");
    assert_eq!(isoloom_targets::refusal(&plain, Target::Proxmox), None);
    assert_eq!(isoloom_targets::refusal(&plain, Target::Docker), None);
}

const BAD_CIDR: &str = "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/33 }\nmachines:\n  web: { networks: { app: 10 }, app: { image: nginx } }\n";
const UNDECLARED_NETWORK: &str =
    "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/24 }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n";

/// `refusal` on a spec nobody validated: every target refused with the spec's problem, no panic.
fn assert_refused_everywhere(yaml: &str, problem: &str) {
    let spec = lab(yaml);
    for t in Target::ALL {
        let why = isoloom_targets::refusal(&spec, t).unwrap_or_else(|| panic!("{t:?} not refused"));
        assert!(why.contains(problem), "{t:?}: {why}");
    }
}

#[test]
fn refusal_of_an_invalid_cidr_does_not_panic() {
    assert_refused_everywhere(BAD_CIDR, "networks.app.cidr");
}

#[test]
fn refusal_of_an_undeclared_network_does_not_panic() {
    assert_refused_everywhere(UNDECLARED_NETWORK, "no network named `lan`");
}

#[test]
fn checks_plan_of_an_undeclared_network_does_not_panic() {
    let _ = isoloom_plan::checks::plan(&lab(UNDECLARED_NETWORK).spec());
}

#[test]
fn hybrid_runs_containers_beside_the_vms() {
    let spec = example("mixed-office");
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
    assert!(!example("hello-stack").possible().contains(&Target::Hybrid));
    assert!(example("mixed-office").possible().contains(&Target::Hybrid));
}

#[test]
fn compose_has_addresses_healthchecks_init_and_checks() {
    let spec = example("hello-stack");
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
    let spec = example("supplier-portal-api");
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    let web = compose.split("\n  web:").nth(1).unwrap().split("\n  isoloom-check:").next().unwrap();
    assert!(compose.contains("API_URL: ${API_URL:-}"), "{compose}");
    assert!(
        !web.contains("API_URL") && !web.contains("launch-token"),
        "web must not receive the database's inputs"
    );
    // A sensitive input is a file, read from the runner's environment: never an environment
    // variable of the machine.
    assert!(compose.contains("  input-launch-token:\n    environment: LAUNCH_TOKEN"), "{compose}");
    assert!(!compose.contains("LAUNCH_TOKEN: ${LAUNCH_TOKEN"), "{compose}");
}

#[test]
fn vagrant_orders_machines_and_names_them() {
    let spec = example("hello-stack");
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
    let spec = example("hello-stack");
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    assert!(vf.contains("v.default_prefix = \"hello-stack_\""), "{vf}");
    let vf = contents(&generate_instance(&spec, Target::Vagrant, 2).unwrap(), ".isoloom-2/vagrant/Vagrantfile");
    assert!(vf.contains("v.default_prefix = \"hello-stack-2_\""));
    let dvm = contents(&generate(&spec, Target::DockerVm).unwrap(), ".isoloom/docker-vm/Vagrantfile");
    assert!(dvm.contains("v.default_prefix = \"hello-stack_\""));
}

#[test]
fn the_controller_gets_every_provider_block_the_machines_get() {
    let spec = example("ansible-pair");
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
    let spec = example("segmented");
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
    // Names across networks, and checks from the access machine (a Debian container here).
    assert!(compose.contains("cache:10.61.20.20"));
    let check = compose
        .split("\n  isoloom-check:\n")
        .nth(1)
        .unwrap()
        .split("\n  isoloom-check-")
        .next()
        .unwrap();
    assert!(check.contains("network_mode: service:user") && !compose.contains("isoloom-access"), "{compose}");
}

#[test]
fn vagrant_router_waits_and_offline_blocks() {
    let spec = example("segmented");
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
    let ad = example("corp-ad-basics");
    // Windows has no container form: the Docker target refuses it at the field that rules it out.
    assert_eq!(
        generate(&ad, Target::Docker),
        Err(GenerateError::Unsupported {
            target: Target::Docker,
            what: "machines.dc01.os: Windows has no container form in 1.0".into()
        })
    );
    assert!(
        matches!(generate(&ad, Target::Vagrant), Err(GenerateError::Unsupported { .. })),
        "Windows has no local image yet"
    );
    assert!(
        matches!(generate(&ad, Target::Proxmox), Err(GenerateError::Unsupported { ref what, .. }) if what.contains("PowerShell scripts")),
        "Windows steps on Proxmox are PowerShell scripts"
    );
    assert!(matches!(generate(&ad, Target::CloudVm), Err(GenerateError::Unsupported { ref what, .. }) if what.contains("Windows steps are .ps1 scripts")));
}

#[test]
fn an_arm64_access_machine_that_is_actually_built_is_still_refused_on_the_cloud() {
    // An access machine with no implementation isn't instantiated, so its arch is moot. One that
    // declares a VM is built, so arm64 must still be caught (the cloud images are x86-64 only).
    let spec = lab(
        "version: 1\nname: arm-access\nnetworks:\n  lab: { cidr: 10.60.0.0/24 }\nmachines:\n  kali:\n    os: debian@12\n    networks: { lab: 10 }\n    access: true\n    hardware: { arch: arm64 }\n    app: { steps: [{ run: p.sh }] }\n",
    );
    assert!(
        matches!(generate(&spec, Target::CloudVm), Err(GenerateError::Unsupported { ref what, .. }) if what.contains("arm64")),
        "an arm64 access machine with a VM must not slip past the cloud refusal"
    );
}

#[test]
fn a_gateway_routes_its_networks_instead_of_the_router() {
    let spec = example("edge-firewall");
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(!compose.contains("isoloom-router"), "no router: the firewall routes");
    // The firewall takes .1; Docker's bridge moves to the last address on its networks.
    assert!(compose.contains("ipv4_address: 10.70.10.1"));
    assert!(compose.contains("gateway: 10.70.10.254"));
    assert!(compose.contains("gateway: 10.70.0.1"));
    assert!(compose.contains("net.ipv4.ip_forward: '1'"));
    // Machines behind it send everything through it, once it answers.
    assert!(compose.contains("ip route replace default via 10.70.10.1"));
    assert!(compose.contains("ip route replace 10.70.10.0/24 via 10.70.0.5"));
    let web_routes = compose.split("\n  web-routes:").nth(1).unwrap().split("\n  cache:").next().unwrap();
    assert!(web_routes.contains("fw:\n        condition: service_healthy"));
    // Its own rules decide: no Docker-internal network, no route deletion.
    assert!(!compose.contains("internal: true"));
    assert!(!compose.contains("ip route del default"));
}

#[test]
fn vagrant_moves_machines_behind_their_gateway_after_provisioning() {
    let spec = example("edge-firewall");
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let fw = vf.find("config.vm.define \"fw\"").unwrap();
    let web = vf.find("config.vm.define \"web\"").unwrap();
    assert!(fw < web, "the gateway starts first");
    assert!(vf.contains("name: \"forwarding\""));
    let web_block = &vf[web..vf.find("config.vm.define \"cache\"").unwrap()];
    let routes = web_block.find("name: \"routes\"").unwrap();
    let provision = web_block.find("name: \"provision/web.sh\"").unwrap();
    let default = web_block.find("name: \"default route\"").unwrap();
    assert!(routes < provision && provision < default, "installs first, then moves behind the firewall");
    assert_eq!(web_block.matches("ip route replace default via 10.70.10.1").count(), 1, "{web_block}");
    assert!(web_block[default..].contains("ip route replace default via 10.70.10.1"));
}

#[test]
fn networks_outside_10_move_into_10_on_docker_keeping_last_octets() {
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  corp: { cidr: 192.168.20.0/24 }\n  dmz: { cidr: 172.18.5.0/24 }\n  taken: { cidr: 10.192.20.0/24 }\nmachines:\n  a: { os: debian@12, networks: { corp: 10, dmz: 7, taken: 9 }, app: { image: x, steps: [{ run: p.sh }] } }\n",
    );
    assert_eq!(spec.problems(), vec![]);
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
    let base = "version: 1\nname: t\nnetworks:\n  corp: { cidr: 192.168.20.0/24 }\nmachines:\n  a: { networks: { corp: 10 }, app: { image: x } }\noverrides: { docker: { networks: { corp: { cidr: DOCKER } } } }\n";
    let spec = lab(&base.replace("DOCKER", "10.77.0.0/24"));
    assert_eq!(spec.problems(), vec![]);
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(compose.contains("ipv4_address: 10.77.0.10"));
    for bad in ["10.77.0.0/25", "172.20.0.0/24"] {
        let p = lab(&base.replace("DOCKER", bad)).problems();
        assert!(p.iter().any(|x| x.path.as_str() == "overrides.docker.networks.corp.cidr"), "{bad}: {p:?}");
    }
}

#[test]
fn windows_machines_use_winrm_and_powershell() {
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 192.168.56.0/24 }\nmachines:\n  dc01:\n    os: windows-server@2019\n    networks: { lab: 10 }\n    ports: { ldap: 389 }\n    app:\n      steps: [{ run: provision/dc.ps1 }]\n  web:\n    os: windows-server@2019\n    networks: { lab: 20 }\n    depends_on: [dc01]\n    app:\n      steps: [{ run: provision/web.ps1 }]\noverrides:\n  vagrant:\n    machines:\n      web: { box: example/win2019, box_version: '1.0' }\n",
    );
    assert_eq!(spec.problems(), vec![]);
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let dc = &vf[vf.find("config.vm.define \"dc01\"").unwrap()..vf.find("config.vm.define \"web\"").unwrap()];
    assert!(dc.contains("m.vm.box = \"StefanScherer/windows_2019\""));
    assert!(dc.contains("m.vm.box_version = \"2021.05.15\""));
    assert!(dc.contains("m.vm.communicator = \"winrm\""));
    assert!(dc.contains("drivers\\\\etc\\\\hosts"), "the Windows hosts file: {dc}");
    // The project in C:\isoloom first, then each step run from there.
    let project = dc.find("m.vm.provision \"isoloom_project\", name: \"project\"").expect("the project step");
    let step = dc
        .find("name: \"provision/dc.ps1\", inline: \"Set-Location C:\\\\isoloom; & powershell -NoProfile -ExecutionPolicy Bypass -File 'provision/dc.ps1'; if ($LASTEXITCODE) { exit $LASTEXITCODE }\"")
        .expect("the step, from the project");
    assert!(project < step);
    let web = &vf[vf.find("config.vm.define \"web\"").unwrap()..];
    assert!(web.contains("m.vm.box = \"example/win2019\""), "the spec's image wins");
    assert!(web.contains("m.vm.box_version = \"1.0\""));
    assert!(web.contains("Test-NetConnection 'dc01' -Port $p"));
}

#[test]
fn windows_refuses_what_it_cant_do_yet() {
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  NAME:\n    os: windows-server@2019\n    networks: { lab: 10 }\n    app: { steps: [STEP] }\n";
    for (name, step, why) in [
        ("dc01", "{ playbook: setup.yml }", "PowerShell scripts"),
        ("averyveryverylongname", "{ run: a.ps1 }", "15 characters"),
    ] {
        let spec = lab(&base.replace("NAME", name).replace("STEP", step));
        let err = generate(&spec, Target::Vagrant).unwrap_err().to_string();
        assert!(err.contains(why), "{name}: {err}");
    }
}

fn contents(files: &[isoloom_targets::GeneratedFile], path: &str) -> String {
    files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("{path} not generated"))
        .contents
        .clone()
}

#[test]
fn the_runner_supplies_the_access_machine_through_its_image_table() {
    // segmented's access machine without an `os` of its own: the runner supplies it.
    let mut supplied = source("segmented");
    supplied.machines["user"].os = None;
    let table = isoloom_spec::images::Table::parse("access:\n  docker: kalilinux/kali-rolling\n  vm: kali\n").unwrap();
    let applied = Lab::new(supplied).with_images(table.clone());
    let compose = contents(&generate(&applied, Target::Docker).unwrap(), ".isoloom/docker/compose.yml");
    // The access machine is a real container now (kept running idle), not the check stand-in.
    assert!(
        compose.contains("  user:\n    image: kalilinux/kali-rolling\n    platform: linux/amd64\n    entrypoint:\n    - sleep\n    - infinity\n"),
        "{compose}"
    );
    assert!(!compose.contains("isoloom-access"));
    // Its VM too.
    let vagrant = contents(&generate(&applied, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(
        vagrant.contains("config.vm.define \"user\" do |m|\n    m.vm.box = \"kalilinux/rolling\""),
        "{vagrant}"
    );
    // An access machine with its own `os` is the spec's, which wins over the table.
    let spec = example("segmented");
    let with_table = example("segmented").with_images(table);
    for t in [Target::Docker, Target::Vagrant] {
        assert_eq!(generate(&with_table, t), generate(&spec, t), "{t:?}");
    }
    // Without the table, nothing changes.
    assert_eq!(isoloom_spec::images::Table::default().apply(&spec.spec()), spec.spec());
}

#[test]
fn the_image_table_sits_between_built_in_images_and_the_spec() {
    let table = isoloom_spec::images::Table::parse("os:\n  debian@12: { vagrant: my-org/debian-12, vagrant_version: \"1.2.0\" }\n").unwrap();
    let vagrant = contents(
        &generate(&example("hello-stack").with_images(table), Target::Vagrant).unwrap(),
        ".isoloom/vagrant/Vagrantfile",
    );
    assert!(
        vagrant.contains("m.vm.box = \"my-org/debian-12\"\n    m.vm.box_version = \"1.2.0\""),
        "{vagrant}"
    );
    // A spec's own box (`overrides.vagrant`) still wins.
    let table = isoloom_spec::images::Table::parse("os:\n  windows-server@2019: { vagrant: other/box }\n").unwrap();
    let mut pinned = source("windows-hello");
    for m in pinned.overrides.vagrant.as_mut().unwrap().machines.values_mut() {
        m.box_name = Some("spec/box".into());
    }
    let vagrant = contents(
        &generate(&Lab::new(pinned).with_images(table), Target::Vagrant).unwrap(),
        ".isoloom/vagrant/Vagrantfile",
    );
    assert!(vagrant.contains("\"spec/box\"") && !vagrant.contains("other/box"));
    assert!(isoloom_spec::images::Table::parse("bogus: 1\n").is_err());
    // The OS is written as a spec writes it.
    let e = isoloom_spec::images::Table::parse("os:\n  debian-12: { vagrant: x }\n").unwrap_err();
    assert!(e.contains("os.debian-12: unknown OS `debian-12`"), "{e}");
    let e = isoloom_spec::images::Table::parse("access:\n  vm: ubuntu-24.04\n").unwrap_err();
    assert!(e.starts_with("access: unknown OS `ubuntu-24.04`"), "{e}");
}

#[test]
fn proxmox_forwards_published_ports_from_the_router() {
    let spec = example("hello-stack");
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
    let spec = lab(
        "version: 1\nname: dns-lab\nnetworks:\n  lab: { cidr: 10.60.0.0/24 }\nmachines:\n  member:\n    os: debian@12\n    networks: { lab: 10 }\n    dns: { servers: [10.60.0.2], domain: arm.lab }\n    app: { steps: [{ run: p.sh }] }\n  plain:\n    os: debian@12\n    networks: { lab: 11 }\n    app: { steps: [{ run: p.sh }] }\n",
    );
    let tf = contents(&generate(&spec, Target::Proxmox).unwrap(), ".isoloom/proxmox/main.tf");
    assert!(tf.contains("servers = [\"10.60.0.2\"]"), "{tf}");
    assert!(tf.contains("domain  = \"arm.lab\""), "{tf}");
    // The machine with no resolver of its own still gets a public one so it can install software.
    assert!(tf.contains("servers = [\"1.1.1.1\"]"), "{tf}");
}

#[test]
fn kubernetes_turns_networks_and_reach_into_network_policies() {
    let spec = example("segmented");
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
fn kubernetes_seeds_a_volume_from_the_image_before_the_init_jobs() {
    // `volumes: { ctf: /chroot }` over an image path: on Docker the named volume starts with the
    // image's files; the claim does too, from an init container of the machine's image.
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/24 }\nmachines:\n  chal:\n    networks: { app: 10 }\n    volumes: { ctf: /chroot }\n    app: { image: \"gcr.io/distroless/static\" }\n    setup: [flag.sh]\n",
    );
    let env = contents(&generate(&spec, Target::Kubernetes).unwrap(), ".isoloom/kubernetes/environment.yaml");
    let seed = env.find("- name: seed-ctf").expect("a seed init container");
    let init = env.find("initContainers:").expect("init containers");
    let containers = env.find("containers:\n      - name: chal").expect("the machine");
    // An init container: done before the machine and its init jobs (a container of the pod) start.
    assert!(init < seed && seed < containers, "{env}");
    let block = &env[seed..containers];
    // The machine's image, run by Isoloom's busybox (no shell needed from the image), as root,
    // the claim at a path of its own, copied into only while empty (a `lost+found` aside).
    assert!(block.contains("image: gcr.io/distroless/static"), "{block}");
    assert!(block.contains("- /.isoloom-probe/busybox\n        - sh"), "{block}");
    assert!(block.contains("runAsUser: 0"), "{block}");
    assert!(block.contains("name: vol-ctf\n          mountPath: /isoloom-seed"), "{block}");
    // Its script, as Kubernetes reads it.
    let docs: Vec<serde_yaml_ng::Value> = env.split("\n---\n").filter_map(|d| serde_yaml_ng::from_str(d).ok()).collect();
    let deploy = docs.iter().find(|d| d["kind"] == "Deployment").expect("the machine's Deployment");
    let inits = deploy["spec"]["template"]["spec"]["initContainers"].as_sequence().unwrap();
    let seed_c = inits.iter().find(|c| c["name"] == "seed-ctf").unwrap();
    let script = seed_c["command"][3].as_str().unwrap();
    assert!(script.contains("[ -d '/chroot' ] || exit 0"), "{script}");
    assert!(script.contains("lost+found"), "{script}");
    assert!(script.contains("$b cp -a '/chroot/.' \"$d/\""), "{script}");
    // A marker stands while copying: a failed copy is cleared and done again, not taken as seeded.
    assert!(script.contains(": > \"$m\" && $b cp -a") && script.ends_with("&& $b rm -f \"$m\""), "{script}");
    assert!(script.contains("[ -e \"$m\" ] || exit 0; $b rm -rf \"$f\""), "{script}");
    // The busybox it runs is copied in first, even though the machine has no services to probe.
    assert!(env[init..seed].contains("- name: isoloom-probe\n        image: busybox"), "{env}");
    assert!(env.contains("- name: isoloom-probe\n        emptyDir: {}"), "{env}");
}

#[test]
fn kubernetes_publishes_and_keeps_offline_machines_inside() {
    let spec = example("hello-stack");
    let env = contents(&generate(&spec, Target::Kubernetes).unwrap(), ".isoloom/kubernetes/environment.yaml");
    assert!(env.contains("type: LoadBalancer") && env.contains("port: 8080"), "{env}");
    assert!(env.contains("name: offline-"));
    // The checks stand on those offline networks: offline too.
    assert!(env.contains("name: offline-isoloom-check"));
    assert!(env.contains("policyTypes:\n    - Egress") || env.contains("policyTypes:\n  - Egress"));
}

#[test]
fn cloud_vm_gives_a_machine_on_several_networks_an_interface_on_each() {
    let mut spec = source("pivot-dmz");
    // Kali has no AWS image yet: the user lands on Debian here.
    for m in spec.machines.values_mut() {
        if m.os.is_some_and(|os| os.name() == "kali") {
            m.os = Some("debian@12".parse().unwrap());
        }
    }
    let tf = contents(&generate(&Lab::new(spec), Target::CloudVm).unwrap(), ".isoloom/cloud-vm/aws/main.tf");
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
    let spec = example("ansible-pair");
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
fn playbook_vars_keep_their_types() {
    let spec = example("ansible-pair");
    let vars = &spec.source().provision[0].vars;
    assert_eq!(vars["tools"], serde_json::json!(["curl", "jq"]));
    assert_eq!(vars["cache_port"], serde_json::json!(6379));
    let vagrantfile = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(
        vagrantfile.contains(r#"-e '{"greeting":"hello from ansible","tools":["curl","jq"],"cache_port":6379}'"#),
        "{vagrantfile}"
    );
}

#[test]
fn cloud_vm_runs_windows_over_winrm_with_its_name() {
    let spec = example("windows-hello");
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
    assert!(tf.contains("host     = aws_eip.isoloom_controller.public_ip"), "{tf}");
}

#[test]
fn linux_machines_leave_windows_out_of_etc_hosts() {
    // A Linux member joining an AD domain must not have the Windows DC in /etc/hosts by its
    // short name: that shadows AD DNS and breaks the Kerberos SPN lookup on realm join.
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 192.168.56.0/24 }\nmachines:\n  dc01:\n    os: windows-server@2019\n    networks: { lab: 10 }\n    ports: { ldap: 389 }\n    app: { steps: [{ run: p.ps1 }] }\n  lx01:\n    os: ubuntu@24.04\n    networks: { lab: 12 }\n    ports: { ssh: 22 }\n    depends_on: [dc01]\n    app: { steps: [{ run: p.sh }] }\n",
    );
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let start = vf.find("config.vm.define \"lx01\"").unwrap();
    let end = vf[start + 20..].find("config.vm.define").map_or(vf.len(), |i| start + 20 + i);
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
    // The OS alone: its official image, pinned, kept running idle.
    let spec = lab(
        "version: 1\nname: idle\nnetworks:\n  lan: { cidr: 10.10.1.0/24 }\nmachines:\n  box:\n    os: alpine\n    networks: { lan: 10 }\n  web:\n    networks: { lan: 11 }\n    ports: { web: 80 }\n    app: { image: \"nginx:1.27-alpine\" }\n",
    );
    let image = isoloom_spec::os::container_image(spec.source().machines["box"].os.unwrap().name()).unwrap();
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(
        compose.contains(&format!(
            "  box:\n    image: {image}\n    platform: linux/amd64\n    entrypoint:\n    - sleep\n    - infinity\n"
        )),
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
    let mut src = source("ansible-pair");
    let spec = Lab::new(src.clone());
    let vagrantfile = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    let ctl = &vagrantfile[vagrantfile.find("config.vm.define \"isoloom-controller\"").unwrap()..];
    assert!(
        ctl.contains("m.vm.box = RbConfig::CONFIG[\"host_cpu\"] =~ /arm|aarch64/ ? \"bento/debian-12\" : \"generic/alpine319\""),
        "{ctl}"
    );
    assert!(ctl.contains("o.vm.box = \"generic/alpine319\""), "libvirt has Alpine on both architectures");
    assert!(ctl.contains("v.memory = 768"));
    assert!(ctl.contains("command -v apk"));
    // Without `controller:`: 1 CPU, 512 MB, on every target with a controller.
    src.controller = None;
    let spec = Lab::new(src.clone());
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
    let other = parse(
        "version: 1\nname: t\nnetworks: {}\nmachines: {}\ncontroller: { resources: { cpus: 2 } }\noverrides: { vagrant: { controller: { box: bento/debian-12, box_version: \"202407.22.0\" } } }\n",
    )
    .unwrap();
    src.controller = other.controller;
    src.overrides = other.overrides;
    let spec = Lab::new(src);
    let vagrantfile = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(vagrantfile.contains("m.vm.box = \"bento/debian-12\"\n    m.vm.box_version = \"202407.22.0\""));
    assert!(vagrantfile.contains("v.cpus = 2"));
}

#[test]
fn the_controller_halts_after_provisioning_unless_kept_running() {
    let mut src = source("ansible-pair");
    let vagrantfile = contents(&generate(&Lab::new(src.clone()), Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    let ctl = &vagrantfile[vagrantfile.find("config.vm.define \"isoloom-controller\"").unwrap()..];
    // After the playbooks, before the on-demand checks; marked so a check run waits it out.
    let (ansible, halt) = (ctl.find("name: \"ansible\"").unwrap(), ctl.find("name: \"halt\"").unwrap());
    assert!(ansible < halt, "{ctl}");
    assert!(ctl.contains("touch /run/isoloom-halting; (sleep 5; poweroff)"));
    src.controller.get_or_insert_default().keep_running = true;
    let vagrantfile = contents(&generate(&Lab::new(src), Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(!vagrantfile.contains("name: \"halt\""));
}

#[test]
fn the_resolved_snapshot_tells_infra_from_targets() {
    let spec = example("ansible-pair");
    let resolved = isoloom_targets::resolved::resolve(&spec);
    assert_eq!(resolved["machines"]["web"]["role"], "target");
    assert_eq!(resolved["controller"]["role"], "infra");
    assert_eq!(resolved["controller"]["keep_running"], false);
}

#[test]
fn the_provision_script_can_be_limited_to_machines() {
    let spec = example("ansible-pair");
    let all = isoloom_targets::provision_script(&spec.spec(), &[]);
    // (The retry of unreachable hosts limits itself, from the retry file: only the runs count.)
    let runs = |s: &str| s.lines().filter(|l| l.starts_with("run_play ")).map(String::from).collect::<Vec<_>>();
    assert!(runs(&all).iter().all(|l| !l.contains("--limit")), "{all}");
    let some = isoloom_targets::provision_script(&spec.spec(), &["web".into(), "cache".into()]);
    assert!(runs(&some).iter().all(|l| l.contains(" --limit 'web,cache' ")), "{some}");
}

/// The controller's playbook runs keep Ansible's retry files where `run_play` reads them, so a
/// play whose only failures are unreachable hosts runs again on those hosts (`--limit @…`).
#[test]
fn the_controller_retries_unreachable_hosts_from_the_retry_file() {
    let spec = example("ansible-pair");
    for (target, path) in [
        (Target::Vagrant, ".isoloom/vagrant/Vagrantfile"),
        (Target::Proxmox, ".isoloom/proxmox/main.tf"),
        (Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf"),
    ] {
        let text = contents(&generate(&spec, target).unwrap(), path);
        assert!(
            text.contains("export ANSIBLE_RETRY_FILES_ENABLED=True ANSIBLE_RETRY_FILES_SAVE_PATH=/tmp/isoloom-retry"),
            "{path}"
        );
        assert!(text.contains("--limit @$_isoloom_retry"), "{path}");
        assert!(text.contains("run_play ansible-playbook -i /etc/isoloom/inventory.ini"), "{path}");
    }
}

#[test]
fn host_vars_go_on_each_machines_inventory_line_with_their_types() {
    let spec = example("ansible-pair");
    let vagrantfile = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(vagrantfile.contains("cache ansible_host=10.63.0.20 maxmemory='\"64mb\"'"), "{vagrantfile}");
    assert!(vagrantfile.contains("web ansible_host=10.63.0.10\n"), "{vagrantfile}");
    // Python literals, single-quoted: Ansible reads these back with their types (checked with
    // ansible-inventory 2.x).
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: p.sh }] }\nprovision:\n  - ansible: site.yml\n    host_vars:\n      a: { code: \"10\", quote: \"it's\", n: 3, on: true, none: null, l: [x, 1], m: { k: v } }\n",
    );
    let vagrantfile = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(
        vagrantfile.contains(r#"a ansible_host=10.9.0.10 code='"10"' quote='"it'"'"'s"' n='3' on='True' none='None' l='["x", 1]' m='{"k": "v"}'"#),
        "{vagrantfile}"
    );
}

#[test]
fn host_vars_name_machines_and_variables() {
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  a:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: p.sh }] }\nprovision:\n  - ansible: site.yml\n    host_vars:\n      ghost: { x: 1 }\n      a: { \"bad name\": 1, 2go: 1, ok_1: 1 }\n",
    );
    let problems = problems(&spec);
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(
        problems
            .iter()
            .any(|p| p.contains("provision[0].host_vars.ghost") && p.contains("no machine named `ghost`"))
    );
    assert!(problems.iter().any(|p| p.contains("host_vars.a.bad name")));
    assert!(problems.iter().any(|p| p.contains("host_vars.a.2go")));
}

/// An init job on a machine nothing depends on is a leaf job: `up --wait` would fail on its exit,
/// so the start plan waits for the rest and runs it after (#42).
#[test]
fn leaf_init_jobs_run_after_the_wait() {
    let spec = lab(r#"
version: 1
name: leaf
networks: { lan: { cidr: 10.70.0.0/24 } }
machines:
  db:
    networks: { lan: 20 }
    ports: { db: 5432 }
    app: { image: "postgres:17" }
    setup: [seed.sh]
  web:
    networks: { lan: 10 }
    ports: { web: 80 }
    depends_on: [db]
    app: { image: "nginx:1.27-alpine" }
    setup: [setup.sh, warm.sh]
"#);
    // db's job is waited for by web; web's two jobs by nothing.
    assert_eq!(isoloom_targets::leaf_jobs(&spec.for_target(Target::Docker)), ["web-init-1", "web-init-2"]);
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    let plan = isoloom_targets::start_plan(compose).unwrap();
    assert_eq!(plan.jobs, ["web-init-1", "web-init-2"]);
    assert!(plan.wait.contains(&"web".to_string()) && plan.wait.contains(&"db-init-1".to_string()));
    assert!(!plan.wait.iter().any(|w| w.starts_with("isoloom-check")), "profile services aren't started");
    let cmd = isoloom_targets::start_commands("docker compose", &plan.jobs, Some(900));
    assert_eq!(
        cmd,
        "docker compose up -d --build --wait --wait-timeout 900 $(docker compose config --services | grep -vx -e web-init-1 -e web-init-2) \
         && docker compose up --no-deps --exit-code-from web-init-1 web-init-1 \
         && docker compose up --no-deps --exit-code-from web-init-2 web-init-2"
    );
    // Nothing left over: the one command it always was.
    assert_eq!(
        isoloom_targets::start_commands("docker compose", &[], None),
        "docker compose up -d --build --wait"
    );
    let hello = example("hello-stack");
    let compose = &generate(&hello, Target::Docker).unwrap()[0].contents;
    assert_eq!(isoloom_targets::start_plan(compose).unwrap(), isoloom_targets::StartPlan::default());
}

/// The check runners run with --no-deps: `compose run` would run completed init jobs again,
/// re-seeding the environment on every test (#47).
#[test]
fn checks_on_the_docker_vm_never_rerun_init_jobs() {
    let spec = example("hello-stack");
    let files = generate(&spec, Target::DockerVm).unwrap();
    let vf = files.iter().find(|f| f.path.ends_with("Vagrantfile")).unwrap();
    assert!(vf.contents.contains("--profile check run --rm --no-deps"));
}

#[test]
fn a_dormant_service_is_declared_but_never_waited_for() {
    // 4444 is the shell an exploit opens: `shell` has nothing else, `web` serves 80 too.
    let source = parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  \
         shell: { os: debian@12, networks: { lab: 5 }, ports: { shell: { port: 4444, dormant: true, publish: 4444 } }, app: { image: x, steps: [{ run: p.sh }] } }\n  \
         web: { os: debian@12, networks: { lab: 10 }, ports: { web: 80, shell: { port: 4444, dormant: true } }, app: { image: x, steps: [{ run: p.sh }] } }\n  \
         app: { os: debian@12, networks: { lab: 20 }, depends_on: [web], app: { image: x, steps: [{ run: p.sh }] } }\n",
    )
    .unwrap();
    let spec = Lab::new(source.clone());
    assert_eq!(spec.problems(), vec![]);

    // Docker: the healthcheck probes 80 only; a machine with only dormant services has none,
    // so whatever waits for it waits for it to start.
    let compose: serde_yaml_ng::Value = serde_yaml_ng::from_str(&contents(&generate(&spec, Target::Docker).unwrap(), ".isoloom/docker/compose.yml")).unwrap();
    let svc = |n: &str| compose["services"][n].clone();
    let probe = serde_yaml_ng::to_string(&svc("web")["healthcheck"]).unwrap();
    assert!(probe.contains("127.0.0.1 80") && !probe.contains("4444"), "{probe}");
    assert!(svc("shell").get("healthcheck").is_none());
    assert!(
        serde_yaml_ng::to_string(&svc("shell")).unwrap().contains(":4444"),
        "still published: {:?}",
        svc("shell")
    );
    assert_eq!(svc("app")["depends_on"]["web"]["condition"].as_str(), Some("service_healthy"));
    let check = serde_yaml_ng::to_string(&svc("isoloom-check-app")["depends_on"]).unwrap();
    assert!(check.contains("web:") && !check.contains("shell:"), "{check}");

    // Kubernetes: the port is declared and in the Service, but not in the readiness probe or
    // the wait of the machines depending on it.
    let env = contents(&generate(&spec, Target::Kubernetes).unwrap(), ".isoloom/kubernetes/environment.yaml");
    assert!(env.contains("containerPort: 4444") && env.contains("name: p4444"), "{env}");
    assert!(env.contains("until nc -z web 80; do sleep 2; done"), "{env}");
    assert!(!env.contains("127.0.0.1 4444"), "{env}");

    // VMs: app waits for web on 80 only.
    let vf = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(vf.contains("</dev/tcp/web/80") && !vf.contains("/dev/tcp/web/4444"), "{vf}");

    // No derived check expects it to answer.
    let plan = isoloom_plan::checks::plan(&spec.spec());
    assert!(plan.iter().any(|c| c.name == "web:80 from app"));
    assert!(
        !plan.iter().any(|c| c.name.contains(":4444")),
        "{:?}",
        plan.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    // Depending on a machine whose services are all dormant waits for nothing: refused.
    let mut bad = source;
    bad.machines.get_mut("app").unwrap().depends_on = vec!["shell".into()];
    assert_eq!(
        problems(&Lab::new(bad)),
        ["machines.app.depends_on[0]: `shell`'s ports are all dormant (they answer later), so there's nothing to wait for"]
    );
}

/// dns serves 53 over UDP (published, `dns`) and TCP (`zone`); syslog serves UDP only; app
/// waits for dns.
fn udp_source() -> isoloom_spec::Spec {
    parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n  far: { cidr: 10.9.1.0/24 }\nreach: [{ from: far, to: lab, ports: [53] }]\nmachines:\n  \
         dns: { os: debian@12, networks: { lab: 5 }, ports: { dns: { port: 53, protocol: udp, publish: 10053 }, zone: 53 }, app: { image: x, steps: [{ run: p.sh }] } }\n  \
         syslog: { os: debian@12, networks: { lab: 6 }, ports: { syslog: { port: 514, protocol: udp, publish: 1514 } }, app: { image: x, steps: [{ run: p.sh }] } }\n  \
         app: { os: debian@12, networks: { far: 20 }, depends_on: [dns], app: { image: x, steps: [{ run: p.sh }] } }\n",
    )
    .unwrap()
}

fn udp_spec() -> Lab {
    let spec = Lab::new(udp_source());
    assert_eq!(spec.problems(), vec![]);
    spec
}

#[test]
fn a_udp_service_is_published_as_udp_on_docker() {
    let spec = udp_spec();
    // Docker: published as `<port>/udp`, labelled udp, and the healthcheck probes TCP 53 only;
    // a machine with only UDP services has none.
    let compose: serde_yaml_ng::Value = serde_yaml_ng::from_str(&contents(&generate(&spec, Target::Docker).unwrap(), ".isoloom/docker/compose.yml")).unwrap();
    let svc = |n: &str| compose["services"][n].clone();
    let ports = serde_yaml_ng::to_string(&svc("dns")["ports"]).unwrap();
    assert!(ports.contains("+10053}:53/udp"), "{ports}");
    assert_eq!(svc("dns")["labels"]["isoloom.port.dns"].as_str(), Some("udp:53"));
    assert_eq!(svc("dns")["labels"]["isoloom.port.zone"].as_str(), Some("tcp:53"));
    let probe = serde_yaml_ng::to_string(&svc("dns")["healthcheck"]).unwrap();
    assert!(probe.contains("127.0.0.1 53"), "{probe}");
    assert!(svc("syslog").get("healthcheck").is_none());
    assert!(serde_yaml_ng::to_string(&svc("syslog")["ports"]).unwrap().contains(":514/udp"));
}

#[test]
fn a_udp_and_a_tcp_port_on_one_number_stay_apart_by_name() {
    // A name is one port's (a TCP and a UDP port share a number, never a name): `dns-udp` and
    // `dns`, on Docker and on the clouds.
    let named = lab("version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  \
         dns: { os: debian@12, networks: { lab: 5 }, ports: { dns-udp: { port: 53, protocol: udp, publish: 10053 }, dns: { port: 53, publish: 1053 } }, app: { image: x, steps: [{ run: p.sh }] } }\n");
    assert_eq!(named.problems(), vec![]);
    let c: serde_yaml_ng::Value = serde_yaml_ng::from_str(&contents(&generate(&named, Target::Docker).unwrap(), ".isoloom/docker/compose.yml")).unwrap();
    assert_eq!(c["services"]["dns"]["labels"]["isoloom.port.dns-udp"].as_str(), Some("udp:53"));
    assert_eq!(c["services"]["dns"]["labels"]["isoloom.port.dns"].as_str(), Some("tcp:53"));
    let aws = contents(&generate(&named, Target::CloudVm).unwrap(), ".isoloom/cloud-vm/aws/main.tf");
    assert!(aws.contains("\"dns/dns-udp\"") && aws.contains("\"dns/dns\""), "{aws}");
    // The same name twice is no map: the YAML says so.
    let twice = parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  \
         dns: { networks: { lab: 5 }, ports: { dns: { port: 53, protocol: udp }, dns: 53 }, app: { image: x } }\n",
    );
    assert!(twice.unwrap_err().to_string().contains("dns"));
}

#[test]
fn a_udp_service_is_published_and_opened_as_udp_on_kubernetes_and_the_vms() {
    let spec = udp_spec();
    // Kubernetes: UDP container ports and Service ports, named apart from the TCP one; the
    // NetworkPolicy for `reach` already opens both protocols.
    let env = contents(&generate(&spec, Target::Kubernetes).unwrap(), ".isoloom/kubernetes/environment.yaml");
    assert!(
        env.contains("- containerPort: 53\n          protocol: UDP\n        - containerPort: 53\n"),
        "{env}"
    );
    assert!(env.contains("name: p53-udp") && env.contains("name: p53\n"), "{env}");
    assert!(env.contains("name: p10053-udp"), "{env}");
    assert!(!env.contains("127.0.0.1 514"), "{env}");

    // Vagrant: forwarded as UDP; app waits for dns on TCP 53.
    let vf = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(
        vf.contains("guest: 53, host: 10053, host_ip: \"127.0.0.1\", protocol: \"udp\" # not on libvirt"),
        "{vf}"
    );
    assert!(!vf.contains("/dev/tcp/syslog/514"), "{vf}");

    // Proxmox: the router forwards the published port as UDP.
    let tf = contents(&generate(&spec, Target::Proxmox).unwrap(), ".isoloom/proxmox/main.tf");
    assert!(tf.contains("udp dport 1514 dnat ip to 10.9.0.6:514"), "{tf}");
    assert!(tf.contains("meta l4proto { tcp, udp } th dport { 53 } accept"), "{tf}");

    // The clouds: published as UDP.
    let aws = contents(&generate(&spec, Target::CloudVm).unwrap(), ".isoloom/cloud-vm/aws/main.tf");
    assert!(aws.contains("from_port   = 1514\n    to_port     = 1514\n    protocol    = \"udp\""), "{aws}");
}

#[test]
fn a_udp_service_is_never_waited_for_nor_checked() {
    let spec = udp_spec();
    // No derived check probes UDP (nothing tells an answer from a drop); TCP 53 is checked.
    let plan = isoloom_plan::checks::plan(&spec.spec());
    let names: Vec<&String> = plan.iter().map(|c| &c.name).collect();
    assert!(names.iter().any(|n| *n == "dns:53 from app"), "{names:?}");
    assert!(!names.iter().any(|n| n.contains(":514")), "{names:?}");

    // The snapshot says so: each port with its protocol.
    let snap = isoloom_targets::resolved::resolve(&spec);
    assert_eq!(snap["machines"]["dns"]["ports"]["dns"]["protocol"], "udp");
    assert_eq!(snap["machines"]["dns"]["ports"]["zone"]["protocol"], "tcp");

    // Depending on a machine with only UDP services waits for nothing: refused.
    let mut bad = udp_source();
    bad.machines.get_mut("app").unwrap().depends_on = vec!["syslog".into()];
    assert_eq!(
        problems(&Lab::new(bad)),
        ["machines.app.depends_on[0]: `syslog`'s ports are all UDP (no handshake to wait for), so there's nothing to wait for"]
    );
}

#[test]
fn a_udp_service_is_validated() {
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 5 }, app: { image: x }, ports: ";
    let problems = |svcs: &str| -> Vec<String> { problems(&lab(&format!("{base}{svcs} }}\n"))) };
    // One port over both protocols is two services; the same one twice isn't.
    assert_eq!(
        problems("{ dns: { port: 53, protocol: udp }, zone: { port: 53, protocol: tcp } }"),
        Vec::<String>::new()
    );
    assert_eq!(
        problems("{ dns: { port: 53, protocol: udp }, again: { port: 53, protocol: udp } }"),
        ["machines.a.ports.again.port: port 53 (UDP) is listed twice"]
    );
    // A host port may be published once per protocol.
    assert_eq!(
        problems("{ dns: { port: 53, protocol: udp, publish: 53 }, zone: { port: 53, publish: 53 } }"),
        Vec::<String>::new()
    );
    // HTTP over UDP can't be written: one `protocol` says either.
    let err = parse(&format!("{base}{{ dns: {{ port: 53, protocol: sctp }} }} }}\n")).unwrap_err().to_string();
    assert!(err.contains("sctp"), "{err}");
}

/// The block of one provider inside one VM's define, from the Vagrantfile.
fn provider_block<'a>(vf: &'a str, vm: &str, provider: &str) -> &'a str {
    let start = vf.find(&format!("config.vm.define \"{vm}\"")).unwrap();
    let at = start + vf[start..].find(&format!("m.vm.provider \"{provider}\"")).unwrap();
    &vf[at..at + vf[at..].find("\n    end\n").unwrap()]
}

#[test]
fn qemu_emulates_another_cpu_and_links_a_pair_of_vms() {
    // GOAD-Mini's shape: one Windows DC and the controller running the lab's playbooks.
    let spec = lab(
        "version: 1\nname: mini\nnetworks:\n  lab: { cidr: 192.168.56.0/24 }\nmachines:\n  dc01:\n    os: windows-server@2019\n    networks: { lab: 10 }\n    ports: { ldap: 389 }\n    app: { steps: [{ run: p.ps1 }] }\nprovision:\n  - ansible: ansible/main.yml\n",
    );
    assert_eq!(isoloom_targets::refusal_on(&spec, Target::Vagrant, Some("qemu")), None);
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    assert!(vf.contains("HOST_ARCH = RbConfig::CONFIG[\"host_cpu\"]"));
    // QEMU boots machines in parallel unless told not to: the controller would run its play
    // before the DC is up, and the connecting side of the link could start before the listener.
    assert!(vf.contains("ENV[\"VAGRANT_NO_PARALLEL\"] = \"1\""));

    let dc = provider_block(vf, "dc01", "qemu");
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

    let ctl = provider_block(vf, "isoloom-controller", "qemu");
    assert!(ctl.contains("o.vm.box = \"cloud-image/debian-12\""), "{ctl}");
    assert!(!ctl.contains("v.arch"), "Isoloom's own VMs run on the host's CPU: {ctl}");
    // One listens, the other connects, on the same port; the DC is defined (and up) first.
    let port = |b: &str, side: &str| b.split(&format!("v.socket_opts = \"{side}=127.0.0.1:")).nth(1).map(|r| r[..5].to_string());
    assert!(port(dc, "listen").is_some() && port(dc, "listen") == port(ctl, "connect"), "{dc}\n{ctl}");

    // Other providers are untouched.
    assert!(!provider_block(vf, "dc01", "virtualbox").contains("socket"));
}

/// The block `config.vm.define "<name>"` of a Vagrantfile, up to its `end`.
fn define_block<'a>(vf: &'a str, name: &str) -> &'a str {
    let start = &vf[vf.find(&format!("config.vm.define \"{name}\"")).unwrap()..];
    &start[..start.find("\n  end\n").unwrap()]
}

#[test]
fn vagrant_runs_on_hyperv_with_a_switch_per_network_and_addresses_set_in_the_guest() {
    let spec = example("segmented");
    let v = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    // The first adapter's switch from ISOLOOM_HYPERV_SWITCH, the helper module once.
    assert_eq!(v.matches("HYPERV_SWITCH = ENV.fetch(\"ISOLOOM_HYPERV_SWITCH\", \"Default Switch\")").count(), 1);
    let web = define_block(&v, "web");
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
    hyperv_router_and_macs(&v, spec.source().networks.len());
}

/// On Hyper-V the router is on every network (`networks` of them), with an adapter on each;
/// every adapter of the Vagrantfile `v` has its own MAC.
fn hyperv_router_and_macs(v: &str, networks: usize) {
    // The router is on every network, with an adapter on each; every adapter has its own MAC.
    let router = define_block(v, "isoloom-router");
    assert_eq!(router.matches("\", \"0215").count(), networks, "{router}");
    let macs: Vec<&str> = v.match_indices("\"0215").map(|(i, _)| &v[i..i + 14]).collect();
    let mut unique = macs.clone();
    unique.sort_unstable();
    unique.dedup();
    assert!(macs.len() > networks && macs.len() == unique.len(), "{macs:?}");
}

#[test]
fn vagrant_on_hyperv_takes_windows_and_privileged_machines() {
    // Windows: the box's Hyper-V build from `overrides.vagrant` (`boxes.hyperv`), a static
    // address through WMI.
    let win = example("windows-hello");
    let v = contents(&generate(&win, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
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
    let hosts = example("existing-hosts");
    let v = contents(&generate(&hosts, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(!define_block(&v, "cache").contains("enable_virtualization_extensions"));
    let nested = lab(
        "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  hv: { os: debian@12, networks: { lab: 10 }, hardware: { nested_virtualization: true }, app: { steps: [{ run: a.sh }] } }\n",
    );
    let v = contents(&generate(&nested, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(define_block(&v, "hv").contains("v.enable_virtualization_extensions = true"), "{v}");
}

#[test]
fn an_os_without_a_hyperv_box_is_refused_on_hyperv_until_the_spec_gives_one() {
    let yaml = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  box:\n    os: debian@13\n    networks: { lab: 10 }\n    app: { steps: [{ run: setup.sh }] }\n";
    let spec = lab(yaml);
    // Still generated for every other provider; the Hyper-V block says what is missing.
    let v = contents(&generate(&spec, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
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
    ));
    assert_eq!(isoloom_targets::refusal_on(&own, Target::Vagrant, Some("hyperv")), None);
    let v = contents(&generate(&own, Target::Vagrant).unwrap(), ".isoloom/vagrant/Vagrantfile");
    assert!(v.contains("o.vm.box = \"my/debian13\"\n      o.vm.box_version = \">= 0\""), "{v}");
}
