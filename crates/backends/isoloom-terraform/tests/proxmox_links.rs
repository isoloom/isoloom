//! The Proxmox module's links, firewall, pool, arm64 VMs and LXC containers (not run on a real
//! host): what the module writes for each.

pub mod testing;

use isoloom_spec::model::Target;
use isoloom_terraform::ProxmoxBackend;
use testing::{Outcome, proxmox};

/// Why Proxmox refuses `yaml`, if it does.
fn refusal(yaml: &str) -> Outcome<Option<String>> {
    testing::refusal(&ProxmoxBackend, yaml)
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
    ports: { web: { port: 80, publish: 8080 } }
    os: debian@12
    app: { steps: [{ run: s.sh }] }
  admin: { networks: { office.vlan10: 50, office.vlan20: 50 }, os: debian@12, app: { steps: [{ run: s.sh }] } }
";

#[test]
fn a_lan_is_one_vlan_aware_vnet_and_its_vlans_are_tags() -> Outcome {
    let tf = proxmox(TWO)?;
    assert!(tf.contains("resource \"proxmox_sdn_vnet\" \"office\""), "{tf}");
    assert!(tf.contains("vlan_aware = true"), "{tf}");
    assert!(!tf.contains("resource \"proxmox_sdn_vnet\" \"office_vlan10\""), "{tf}");
    assert!(tf.contains("vlan_id  = 10") || tf.contains("vlan_id = 10"), "{tf}");
    assert!(tf.contains("vlan_id  = 20") || tf.contains("vlan_id = 20"), "{tf}");
    Ok(())
}

#[test]
fn mtu_and_rate_limit_go_on_the_devices() -> Outcome {
    let tf = proxmox(TWO)?;
    // The zone carries the largest; each device its network's.
    let zone_mtu = |l: &str| l.split_whitespace().collect::<Vec<_>>() == ["mtu", "=", "9000"];
    assert!(tf.lines().any(zone_mtu), "{tf}");
    assert!(tf.contains("= 1400"), "{tf}");
    assert!(tf.contains("rate_limit = 1.25"), "{tf}");
    Ok(())
}

#[test]
fn the_firewall_enforces_reach_and_the_published_ports() -> Outcome {
    let tf = proxmox(TWO)?;
    assert!(tf.contains("resource \"proxmox_virtual_environment_firewall_ipset\" \"servers\""), "{tf}");
    assert!(tf.contains("resource \"proxmox_virtual_environment_firewall_options\" \"web\""), "{tf}");
    assert!(tf.contains("input_policy  = \"DROP\""), "{tf}");
    assert!(tf.contains("dport   = \"80,443\""), "{tf}");
    assert!(tf.contains("comment = \"published\""), "{tf}");
    assert!(tf.contains("firewall"), "{tf}");
    assert!(tf.contains("resource \"proxmox_virtual_environment_pool\" \"env\""), "{tf}");
    assert!(tf.contains("= proxmox_virtual_environment_pool.env.pool_id"), "{tf}");
    Ok(())
}

#[test]
fn an_arm64_machine_is_an_emulated_aarch64_vm() -> Outcome {
    let tf = proxmox(
        "version: 1\nname: arm\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w: { networks: { lab: 10 }, hardware: { arch: arm64 }, os: debian@12, app: { steps: [{ run: s.sh }] } }\n",
    )?;
    // Pinned to a dated release (no floating `latest`), with the publisher's SHA-512 checksum.
    assert!(tf.contains("debian-12-genericcloud-arm64-20261015-2632.qcow2"), "{tf}");
    assert!(
        tf.contains("e69fe6ec73d73af4de0523b661078774d87c270e72ba99f5e262a54d2de8fff065b5dde145b1671e4c304be333c25e21e20a997742a4b389b7a36c09272131f4"),
        "{tf}"
    );
    assert!(tf.contains("checksum_algorithm"), "{tf}");
    assert!(tf.contains("\"sha512\""), "{tf}");
    assert!(!tf.contains("/latest/"), "arm64 image must be pinned, not `latest`: {tf}");
    assert!(tf.contains("architecture = \"aarch64\""), "{tf}");
    assert!(tf.contains("bios"), "{tf}");
    assert!(tf.contains("\"serial0\""), "{tf}");
    let why = refusal(
        "version: 1\nname: arm\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w: { networks: { lab: 10 }, hardware: { arch: arm64 }, os: rocky@9, app: { steps: [{ run: s.sh }] } }\n",
    )?.expect("refused");
    assert!(why.contains("no arm64 Proxmox image for `rocky-9`"), "{why}");
    Ok(())
}

#[test]
fn a_base_image_is_pinned_to_a_dated_release_with_its_checksum() -> Outcome {
    // A whole-number amd64 image (Debian 12) and an RPM one (Rocky 9): each downloaded from a
    // dated URL, never a floating `latest`/`current`, and verified by the publisher's checksum.
    let tf = proxmox(
        "version: 1\nname: pin\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  d: { networks: { lab: 10 }, os: debian@12, app: { steps: [{ run: s.sh }] } }\n  r: { networks: { lab: 11 }, os: rocky@9, app: { steps: [{ run: s.sh }] } }\n",
    )?;
    assert!(tf.contains("debian-12-genericcloud-amd64-20261015-2632.qcow2"), "{tf}");
    assert!(
        tf.contains("56c0bd55b6623f2739f377e983d2872e7b208d0cb9c3ba4e93d3f72929026da03637c598b3bd472b29998c31c8a4c17d8e6ae1c556852a8b83a66a09bf5c2b37"),
        "{tf}"
    );
    assert!(tf.contains("Rocky-9-GenericCloud-Base-9.8-20260525.0.x86_64.qcow2"), "{tf}");
    assert!(tf.contains("92c206cc6f790c61583247eefe87890f8828420662c17cacf247cec78ab4eec8"), "{tf}");
    assert!(tf.contains("\"sha512\"") && tf.contains("\"sha256\""), "both algorithms wired: {tf}");
    assert!(!tf.contains("/latest/") && !tf.contains(".latest."), "no floating image URL: {tf}");
    assert!(!tf.contains("/current/"), "no floating image URL: {tf}");
    Ok(())
}

const LXC: &str = "version: 1
name: lxc
networks:
  lab: { cidr: 10.9.0.0/24 }
machines:
  web: { networks: { lab: 10 }, ports: { web: 80 }, environment: { MODE: lab }, app: { image: 'nginx:1.27' } }
  box: { networks: { lab: 11 }, depends_on: [web], os: debian@12, app: { steps: [{ run: s.sh }] } }
overrides: { proxmox: { machines: { web: { lxc: true } } } }
";

#[test]
fn an_lxc_machine_is_a_container_from_its_oci_image() -> Outcome {
    let tf = proxmox(LXC)?;
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
    Ok(())
}

#[test]
fn lxc_is_opt_in_and_refuses_what_a_container_cannot_carry() -> Outcome {
    let vm_only = isoloom_plan::Lab::parse(&LXC.replace("overrides: { proxmox: { machines: { web: { lxc: true } } } }\n", "")).map_err(|p| format!("{p:?}"))?;
    assert!(!vm_only.possible().contains(&Target::Proxmox));
    let volumes = refusal(&LXC.replace("environment: { MODE: lab }", "volumes: { data: /data }"))?.expect("refused");
    assert!(volumes.contains("`volumes`"), "{volumes}");
    let disks = refusal(&LXC.replace("environment: { MODE: lab }", "disks: { data: { size_gb: 2, mount: /data } }"))?.expect("refused");
    assert!(disks.contains("`disks`"), "{disks}");
    Ok(())
}
