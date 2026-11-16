use isoloom_spec::model::Target;

use super::Controller;
use crate::testing;

const NEEDS_ONE: &str = "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\nprovision: [{ ansible: site.yml }]\n";

fn controller(extra: &str) -> Option<Controller> {
    testing::environment(&format!("{NEEDS_ONE}{extra}"), Target::Vagrant).controller
}

#[test]
fn each_input_carries_the_sensitivity_the_spec_gives_it() {
    let env = testing::environment(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\ninputs: { URL: { sensitive: false }, TOKEN: {} }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\n",
        Target::Vagrant,
    );
    let inputs: Vec<(&str, bool)> = env.inputs.iter().map(|i| (i.name.as_str(), i.sensitive)).collect();
    assert_eq!(inputs, [("URL", false), ("TOKEN", true)]);
    assert_eq!(env.input_names(), ["URL", "TOKEN"]);
}

#[test]
fn the_controller_has_one_cpu_and_512_mb_unless_the_spec_sizes_it() {
    assert_eq!(
        controller(""),
        Some(Controller {
            cpus: 1,
            memory_mb: 512,
            disk_gb: None,
            keep_running: false
        })
    );
    assert_eq!(
        controller("controller: { resources: { cpus: 2, memory_mb: 768, disk_gb: 10 }, keep_running: true }\n"),
        Some(Controller {
            cpus: 2,
            memory_mb: 768,
            disk_gb: Some(10),
            keep_running: true
        })
    );
}

#[test]
fn a_probe_reaches_a_machine_at_its_address_else_by_name() {
    let env = testing::environment(
        "version: 1\nname: t\nnetworks:\n  v4: { cidr: 10.9.0.0/24 }\n  v6: { cidr: 10.8.0.0/24, ipv6: { cidr: \"fd00:8::/64\", only: true } }\n  auto: { cidr: 10.7.0.0/24, ipv6: { cidr: \"fd00:7::/64\", only: true, addresses: auto } }\nmachines:\n  web: { networks: { v4: 10, v6: 10, auto: 10 }, app: { image: nginx } }\n",
        Target::Kubernetes,
    );
    assert_eq!(env.probe_address("web", "v4"), "10.9.0.10");
    assert_eq!(env.probe_address("web", "v6"), "fd00:8::10");
    assert_eq!(env.probe_address("web", "auto"), "web");
    assert_eq!(env.probe_address("ghost", "v4"), "ghost");
}

/// `web` has both forms, `img` only an image, `vm` only a VM (a reboot), `ct` is an LXC
/// container on Proxmox, `user` is supplied; `data` is shared by `vm` and `both`.
const SHAPES: &str = "version: 1
name: t
networks: { a: { cidr: 10.1.0.0/24, ipv6: { cidr: \"fd00:1::/64\" } }, b: { cidr: 10.2.0.0/24, ipv6: { cidr: \"fd00:2::/64\" } } }
reach: [{ from: a, to: b }]
volumes: { data: { owner: vm } }
machines:
  both: { networks: { a: 10 }, os: debian@12, app: { image: nginx, steps: [{ run: a.sh }] }, volumes: { data: /d, own: /o } }
  img: { networks: { a: 11 }, app: { image: nginx } }
  vm: { networks: { b: 12 }, os: debian@12, app: { steps: [{ run: a.sh }, reboot] }, volumes: { data: /d } }
  ct: { networks: { a: 13 }, app: { image: alpine } }
  win: { networks: { a: 14 }, os: windows-server@2022 }
  user: { access: true, networks: { a: 5 } }
overrides:
  proxmox: { machines: { ct: { lxc: true } } }
  vagrant: { machines: { win: { winrm: ssl } } }
";

fn shapes(target: Target) -> Vec<(String, super::MachineShape)> {
    testing::environment(SHAPES, target)
        .machines
        .iter()
        .map(|(n, m)| (n.clone(), m.shape))
        .collect()
}

fn shape_of(target: Target, machine: &str) -> Option<super::MachineShape> {
    shapes(target).into_iter().find(|(n, _)| n == machine).map(|(_, s)| s)
}

#[test]
fn each_target_produces_a_machine_in_the_form_it_runs() {
    use super::MachineShape::{Container, External, Supplied, Vm};
    for t in [Target::Docker, Target::Hosted, Target::CloudDocker, Target::Kubernetes] {
        assert_eq!(shape_of(t, "both"), Some(Container), "{t:?}");
        assert_eq!(shape_of(t, "user"), Some(Supplied), "{t:?}");
    }
    assert_eq!(shape_of(Target::Proxmox, "ct"), Some(Container), "an LXC container");
    assert_eq!(shape_of(Target::Proxmox, "both"), Some(Vm));
    assert_eq!(
        shape_of(Target::Proxmox, "img"),
        Some(Supplied),
        "no VM and no LXC: Proxmox refuses it, nothing is built"
    );
    assert_eq!(shape_of(Target::Vagrant, "both"), Some(Vm));
    assert_eq!(shape_of(Target::External, "both"), Some(External));
}

#[test]
fn every_vm_comes_with_its_os_and_nothing_else_is_one() {
    let vms = ["vm: debian-12", "both: debian-12", "win: windows-server-2022"];
    for (t, want) in [
        (Target::Proxmox, vms.as_slice()),
        (Target::Vagrant, vms.as_slice()),
        (Target::Docker, [].as_slice()),
    ] {
        let env = testing::environment(SHAPES, t);
        let found: Vec<String> = env.vms().map(|vm| format!("{}: {}", vm.name, vm.os)).collect();
        assert_eq!(found, want, "{t:?}");
        let mut shaped: Vec<&str> = env
            .machines
            .iter()
            .filter(|(_, m)| m.shape == super::MachineShape::Vm)
            .map(|(n, _)| n.as_str())
            .collect();
        let mut named: Vec<&str> = env.vms().map(|vm| vm.name).collect();
        shaped.sort_unstable();
        named.sort_unstable();
        assert_eq!(named, shaped, "{t:?}: the VMs are the machines shaped as one");
    }
    let env = testing::environment(SHAPES, Target::Proxmox);
    assert_eq!(env.vm("win").map(|vm| vm.os), Some("windows-server-2022"));
    assert!(env.vm("ct").is_none(), "a container is no VM");
}

#[test]
fn a_vm_has_set_up_steps_and_a_container_none() {
    let steps = |t: Target, m: &str| testing::environment(SHAPES, t).machines.get(m).map(|p| p.lifecycle.clone()).unwrap_or_default();
    let vm = steps(Target::Vagrant, "vm");
    assert_eq!((vm.first(), vm.last()), (Some(&crate::Step::Prelude), Some(&crate::Step::Ready)), "{vm:?}");
    assert_eq!(steps(Target::Docker, "both"), []);
}

#[test]
fn a_cloud_routes_ipv6_itself_and_the_other_vm_targets_through_the_router() {
    let routes6 = |t: Target| {
        testing::environment(SHAPES, t)
            .machines
            .get("both")
            .map(|m| m.routes6.len())
            .unwrap_or_default()
    };
    assert_eq!(routes6(Target::Vagrant), 1);
    assert_eq!(routes6(Target::CloudVm), 0);
    let cloud = testing::environment(SHAPES, Target::CloudVm);
    let ipv6_routes = cloud.machines.get("both").and_then(|m| {
        m.lifecycle.iter().find_map(|s| match s {
            crate::Step::Ipv6 { routes, .. } => Some(routes.len()),
            _ => None,
        })
    });
    assert_eq!(ipv6_routes, Some(0));
}

#[test]
fn a_shared_volume_names_its_owner_and_users_and_an_own_one_is_the_machines() {
    let env = testing::environment(SHAPES, Target::Vagrant);
    let both = env.machines.get("both").map(|m| m.volumes.clone()).unwrap_or_default();
    let kinds: Vec<(&str, &super::VolumeKind)> = both.iter().map(|v| (v.name.as_str(), &v.kind)).collect();
    assert_eq!(
        kinds,
        [
            (
                "data",
                &super::VolumeKind::Shared {
                    owner: "vm".into(),
                    users: vec!["both".into(), "vm".into()]
                }
            ),
            ("own", &super::VolumeKind::Own)
        ]
    );
}

#[test]
fn a_windows_vm_answers_winrm_its_way() {
    let env = testing::environment(SHAPES, Target::Vagrant);
    let win = env.machines.get("win").expect("planned");
    assert!(win.is_windows() && !win.winssh);
    assert_eq!(win.winrm, Some(isoloom_spec::model::Winrm::Ssl));
    let vm = env.machines.get("vm").expect("planned");
    assert!(!vm.is_windows() && vm.winrm.is_none());
    assert!(super::on_docker(Target::Docker) && super::on_docker(Target::Hosted) && !super::on_docker(Target::CloudDocker));
}

#[test]
fn only_playbooks_need_a_controller() {
    let lab = testing::lab(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\nchecks: [checks/a.sh]\n",
    );
    let spec = lab.for_target(Target::Vagrant);
    assert!(!Controller::needed(&spec) && lab.plan(&spec, Target::Vagrant, None).expect("the spec plans").controller.is_none());
    let playbook = testing::lab(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\nchecks: [checks/a.yml]\n",
    );
    assert!(Controller::needed(&playbook.for_target(Target::Vagrant)));
    assert_eq!(Controller::of(&spec).memory_mb, Controller::MEMORY_MB);
}

#[test]
fn a_configuration_that_doesnt_resolve_is_an_error_not_a_vm_without_one() {
    let lab = testing::lab(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  win: { networks: { lan: 10 }, os: windows-server@2022, app: { steps: [{ run: iis.ps1 }, { copy: { from: a, to: 'C:\\a', owner: admin } }] } }\n",
    );
    assert!(lab.refusal(Target::Vagrant).is_some(), "lowering refuses the step first");
    let unresolved = lab
        .plan(&lab.for_target(Target::Vagrant), Target::Vagrant, None)
        .expect_err("the recipe doesn't resolve");
    let super::PlanError::UnresolvedRecipe { machine, unportable } = &unresolved else {
        panic!("not the recipe: {unresolved:?}")
    };
    assert_eq!((machine.as_str(), unportable.len()), ("win", 1), "{unresolved:?}");
    assert!(
        unresolved
            .to_string()
            .starts_with("machine `win`: its configuration doesn't resolve on its OS: "),
        "{unresolved}"
    );
}

#[test]
fn a_rate_limit_is_planned_as_a_rate_and_one_that_isnt_is_an_error() {
    let plan = |rate: &str| {
        let lab = testing::lab(&format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: 10 }}, os: debian@12, rate_limit: {rate} }}\n"
        ));
        lab.plan(&lab.for_target(Target::Proxmox), Target::Proxmox, None)
    };
    let env = plan("512kbit").expect("a rate plans");
    let rate = env.machines["web"].rate_limit.as_ref().expect("its rate limit");
    assert_eq!((rate.to_string(), rate.megabytes_per_second()), ("512kbit".to_owned(), 0.064));
    assert_eq!(
        plan("fast").expect_err("validation refuses it, and the plan doesn't drop it"),
        super::PlanError::RateLimit {
            machine: "web".into(),
            rate: "fast".into(),
            error: isoloom_spec::net::RateError::Malformed
        }
    );
}
