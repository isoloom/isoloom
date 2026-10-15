use isoloom_spec::model::Target;
use isoloom_spec::syntax;

use super::*;
use crate::testing;

/// Every override kind: Docker's block and existing networks and volumes on the Docker
/// targets, Kubernetes', Proxmox's and ESXi's own, images per machine, by clone and by group.
const LAB: &str = "version: 1
name: t
networks: { lan: { cidr: 10.9.0.0/24 }, corp: { cidr: 10.8.0.0/24 } }
volumes: { data: { owner: a } }
groups: { webs: { members: [w*] }, everything: { members: [webs, db] } }
machines:
  a: { networks: { lan: 10 }, os: debian@12, volumes: { data: /data } }
  w: { networks: { lan: 20 }, os: debian@12, count: 2 }
  db: { networks: { lan: 30 }, os: debian@12 }
overrides:
  docker: { networks: { lan: { cidr: 172.30.0.0/24 }, corp: { external: corp-net } }, volumes: { data: { external: data-vol } } }
  kubernetes: { volumes: { data: { external: data-pvc } } }
  proxmox:
    networks: { corp: { external: vmbr9 } }
    volumes: { data: { virtiofs: { mapping: shared } } }
    machines: { everything: { image: local:vztmpl/base }, db: { image: local:vztmpl/db, lxc: true } }
  esxi: { networks: { corp: { external: VM Network } } }
  vagrant: { machines: { w: { box: example/web, box_version: 1.2.3 } } }
  aws: { machines: { a: { image: ami-1 } } }
  azure: { machines: { a: { image: az-1 } } }
  gcp: { machines: { a: { image: gcp-1 } } }
  digitalocean: { machines: { a: { image: do-1 } } }
  linode: { machines: { a: { image: li-1 } } }
  oci: { machines: { a: { image: oci-1 } } }
  external: { machines: { webs: { ssh: { address: 192.0.2.7, user: ops } } } }
";

fn source() -> syntax::Spec {
    testing::lab(LAB).source().clone()
}

#[test]
fn an_existing_network_is_the_target_being_lowereds_own() {
    let s = source();
    let external = |scope| networks(&s, scope).get("corp").and_then(|n| n.external.clone());
    for t in [Target::Docker, Target::Hosted, Target::DockerVm, Target::CloudDocker] {
        assert_eq!(external(Scope::Target(t)).as_deref(), Some("corp-net"), "{t:?}");
    }
    assert_eq!(external(Scope::Target(Target::Proxmox)).as_deref(), Some("vmbr9"));
    assert_eq!(external(Scope::Target(Target::Esxi)).as_deref(), Some("VM Network"));
    for scope in [Scope::Lab, Scope::Target(Target::Vagrant), Scope::Target(Target::Kubernetes)] {
        assert_eq!(external(scope), None, "{scope:?}");
    }
    let lan = networks(&s, Scope::Lab).get("lan").and_then(|n| n.docker.clone()).map(|d| d.cidr);
    assert_eq!(lan.as_deref(), Some("172.30.0.0/24"), "Docker's block, whatever the target");
}

#[test]
fn an_existing_volume_is_the_target_being_lowereds_own_and_virtiofs_proxmoxs() {
    let s = source();
    let external = |scope| volumes(&s, scope).get("data").and_then(|v| v.external.clone());
    assert_eq!(external(Scope::Target(Target::Docker)).as_deref(), Some("data-vol"));
    assert_eq!(external(Scope::Target(Target::Kubernetes)).as_deref(), Some("data-pvc"));
    assert_eq!(external(Scope::Target(Target::Proxmox)), None);
    assert_eq!(external(Scope::Lab), None);
    assert!(volumes(&s, Scope::Lab).get("data").is_some_and(|v| v.virtiofs.is_some()));
}

#[test]
fn a_machines_entry_is_its_own_else_its_counted_machines_else_its_groups() {
    let s = source();
    let proxmox = s.overrides.proxmox.as_ref().expect("proxmox overrides");
    let image = |name: &str| for_machine(&s, &proxmox.machines, name).and_then(|m| m.image.clone());
    assert_eq!(image("db").as_deref(), Some("local:vztmpl/db"), "its own entry first");
    assert_eq!(image("w-01").as_deref(), Some("local:vztmpl/base"), "a clone, through the groups");
    assert_eq!(image("a"), None);
    let vagrant = s.overrides.vagrant.as_ref().expect("vagrant overrides");
    assert!(for_machine(&s, &vagrant.machines, "w-02").is_some(), "a clone takes its counted machine's");
    assert!(lxc(&s, "db", Scope::Target(Target::Proxmox)) && !lxc(&s, "db", Scope::Lab) && !lxc(&s, "a", Scope::Target(Target::Proxmox)));
    assert_eq!(external(&s, "w-01").map(|e| e.address), Some("192.0.2.7".to_owned()));
    assert_eq!(external(&s, "a"), None);
}

#[test]
fn a_group_holds_the_members_of_the_groups_it_names_but_stops_at_a_loop() {
    let s = testing::lab(
        "version: 1\nname: t\ngroups: { x: { members: [y] }, y: { members: [x, m] } }\nmachines:\n  m: { os: debian@12 }\n  n: { os: debian@12 }\n",
    )
    .source()
    .clone();
    assert!(member(&s, "x", "m", 0) && member(&s, "y", "m", 0));
    assert!(!member(&s, "x", "n", 0) && !member(&s, "x", "m", 8) && !member(&s, "nowhere", "m", 0));
    assert!(member(&s, "y", "m", 7) && !member(&s, "y", "m", 8), "groups nest eight deep at most");
}

#[test]
fn each_cloud_gives_the_machine_its_image() {
    let image = vm_image(&source(), "a").expect("images");
    assert_eq!(
        [image.aws, image.azure, image.gcp, image.digitalocean, image.linode, image.oci].map(Option::unwrap_or_default),
        ["ami-1", "az-1", "gcp-1", "do-1", "li-1", "oci-1"]
    );
    let w = vm_image(&source(), "w-01").expect("a box");
    assert_eq!(
        (w.vagrant.as_deref(), w.vagrant_version.as_deref(), w.proxmox.as_deref()),
        (Some("example/web"), Some("1.2.3"), Some("local:vztmpl/base"))
    );
    assert_eq!(
        vm_image(&testing::lab("version: 1\nname: t\nmachines:\n  m: { os: debian@12 }\n").source().clone(), "m"),
        None
    );
}

#[test]
fn a_clone_takes_the_entry_of_its_own_counted_machine_only() {
    let s = testing::lab("version: 1\nname: t\nmachines:\n  w: { os: debian@12, count: 2 }\n  v: { os: debian@12, count: 2 }\noverrides: { vagrant: { machines: { w: { box: example/w } } } }\n")
        .source()
        .clone();
    let vagrant = s.overrides.vagrant.as_ref().expect("vagrant overrides");
    assert!(for_machine(&s, &vagrant.machines, "w-02").is_some());
    assert!(for_machine(&s, &vagrant.machines, "v-01").is_none());
}
