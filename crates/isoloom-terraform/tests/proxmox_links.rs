//! The Proxmox module's links, firewall, pool, arm64 VMs and LXC containers (not run on a real
//! host): what the module writes for each.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::proxmox::ProxmoxBackend;

fn render(yaml: &str) -> String {
    let spec = isoloom_spec::parse(yaml).unwrap();
    assert_eq!(isoloom_spec::validate(&spec), vec![]);
    let e = Environment::build(&spec, Target::Proxmox, None);
    match ProxmoxBackend.render(&e) {
        Ok(files) => files[0].contents.clone(),
        Err(e) => panic!("{e}"),
    }
}

fn refusal(yaml: &str) -> String {
    let spec = isoloom_spec::parse(yaml).unwrap();
    let e = Environment::build(&spec, Target::Proxmox, None);
    ProxmoxBackend.refusal(&e).expect("refused")
}

const TWO: &str = "version: 1
name: links
networks:
  office:
    cidr: 10.70.0.0/16
    mtu: 1400
    vlans:
      10: { cidr: 10.70.10.0/24 }
      20: { cidr: 10.70.20.0/24 }
  servers: { cidr: 10.70.99.0/24, mtu: 9000 }
reach: [{ from: office.vlan10, to: servers, ports: [80, 443] }]
machines:
  web:
    networks: { servers: 10 }
    rate_limit: 10mbit
    services: [{ port: 80, publish: 8080 }]
    vm: { os: debian-12, provision: [s.sh] }
  admin: { networks: { office.vlan10: 50, office.vlan20: 50 }, vm: { os: debian-12, provision: [s.sh] } }
";

#[test]
fn a_lan_is_one_vlan_aware_vnet_and_its_vlans_are_tags() {
    let tf = render(TWO);
    assert!(tf.contains("resource \"proxmox_sdn_vnet\" \"office\""), "{tf}");
    assert!(tf.contains("vlan_aware = true"), "{tf}");
    assert!(!tf.contains("resource \"proxmox_sdn_vnet\" \"office_vlan10\""), "{tf}");
    assert!(tf.contains("vlan_id  = 10") || tf.contains("vlan_id = 10"), "{tf}");
    assert!(tf.contains("vlan_id  = 20") || tf.contains("vlan_id = 20"), "{tf}");
}

#[test]
fn mtu_and_rate_limit_go_on_the_devices() {
    let tf = render(TWO);
    // The zone carries the largest; each device its network's.
    assert!(tf.contains("mtu   = 9000") || tf.contains("mtu = 9000"), "{tf}");
    assert!(tf.contains("= 1400"), "{tf}");
    assert!(tf.contains("rate_limit = 1.25"), "{tf}");
}

#[test]
fn the_firewall_enforces_reach_and_the_published_ports() {
    let tf = render(TWO);
    assert!(tf.contains("resource \"proxmox_virtual_environment_firewall_ipset\" \"servers\""), "{tf}");
    assert!(tf.contains("resource \"proxmox_virtual_environment_firewall_options\" \"web\""), "{tf}");
    assert!(tf.contains("input_policy  = \"DROP\""), "{tf}");
    assert!(tf.contains("dport   = \"80,443\""), "{tf}");
    assert!(tf.contains("comment = \"published\""), "{tf}");
    assert!(tf.contains("firewall"), "{tf}");
    assert!(tf.contains("resource \"proxmox_virtual_environment_pool\" \"env\""), "{tf}");
    assert!(tf.contains("= proxmox_virtual_environment_pool.env.pool_id"), "{tf}");
}

#[test]
fn an_arm64_machine_is_an_emulated_aarch64_vm() {
    let tf = render(
        "version: 1\nname: arm\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w: { networks: { lab: 10 }, arch: arm64, vm: { os: debian-12, provision: [s.sh] } }\n",
    );
    assert!(tf.contains("debian-12-genericcloud-arm64.qcow2"), "{tf}");
    assert!(tf.contains("architecture = \"aarch64\""), "{tf}");
    assert!(tf.contains("bios"), "{tf}");
    assert!(tf.contains("\"serial0\""), "{tf}");
    let why = refusal(
        "version: 1\nname: arm\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w: { networks: { lab: 10 }, arch: arm64, vm: { os: rocky-9, provision: [s.sh] } }\n",
    );
    assert!(why.contains("no arm64 Proxmox image for `rocky-9`"), "{why}");
}

const LXC: &str = "version: 1
name: lxc
networks:
  lab: { cidr: 10.9.0.0/24 }
machines:
  web: { networks: { lab: 10 }, services: [{ port: 80 }], environment: { MODE: lab }, docker: { image: 'nginx:1.27', lxc: true } }
  box: { networks: { lab: 11 }, depends_on: [web], vm: { os: debian-12, provision: [s.sh] } }
";

#[test]
fn an_lxc_machine_is_a_container_from_its_oci_image() {
    let tf = render(LXC);
    assert!(tf.contains("resource \"proxmox_oci_image\" \"web\""), "{tf}");
    assert!(
        tf.contains("reference           = \"docker.io/library/nginx:1.27\"") || tf.contains("\"docker.io/library/nginx:1.27\""),
        "{tf}"
    );
    assert!(tf.contains("resource \"proxmox_virtual_environment_container\" \"web\""), "{tf}");
    assert!(tf.contains("container_id = proxmox_virtual_environment_container.web.vm_id"), "{tf}");
    assert!(
        tf.contains("proxmox_virtual_environment_container.web,") || tf.contains("proxmox_virtual_environment_container.web]"),
        "{tf}"
    );
    // The router answers the environment's names for it.
    assert!(tf.contains("dnsmasq"), "{tf}");
    assert!(tf.contains("10.9.0.10 web"), "{tf}");
}

#[test]
fn lxc_is_opt_in_and_refuses_what_a_container_cannot_carry() {
    let spec = isoloom_spec::parse(&LXC.replace(", lxc: true", "")).unwrap();
    assert!(!isoloom_spec::effective(&spec).contains(&Target::Proxmox));
    let why = refusal(&LXC.replace("environment: { MODE: lab }", "volumes: { data: /data }"));
    assert!(why.contains("`volumes`"), "{why}");
    let why = refusal(&LXC.replace("environment: { MODE: lab }", "disks: { data: { size_gb: 2, mount: /data } }"));
    assert!(why.contains("`disks`"), "{why}");
}
