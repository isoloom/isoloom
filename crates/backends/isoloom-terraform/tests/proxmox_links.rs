//! The Proxmox module's links, firewall, pool, arm64 VMs and LXC containers (not run on a real
//! host): what the module writes for each.

pub mod testing;

use isoloom_spec::model::Target;
use isoloom_terraform::ProxmoxBackend;
use isoloom_test_support::{hcl_body, hcl_literals, hcl_names, hcl_nested, hcl_one, hcl_text};
use testing::{Outcome, proxmox};

/// The one `resource "<kind>" "<name>"` of `tf`.
fn resource<'b>(tf: &'b hcl::Body, kind: &str, name: &str) -> &'b hcl::Body {
    hcl_one(tf, kind, name)
}

/// Attribute `key` of every `kind` block directly in `body`, in order (an absent one: empty).
fn each(body: &hcl::Body, kind: &str, key: &str) -> Vec<String> {
    hcl_nested(body, kind)
        .iter()
        .map(|b| {
            if b.body().attributes().any(|a| a.key() == key) {
                hcl_text(b.body(), key)
            } else {
                String::new()
            }
        })
        .collect()
}

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
    let tf = hcl_body(&proxmox(TWO)?);
    assert_eq!(hcl_names(&tf, "resource", "proxmox_sdn_vnet"), ["servers", "office"]);
    assert_eq!(hcl_text(resource(&tf, "proxmox_sdn_vnet", "office"), "vlan_aware"), "true");
    // The router is on each VLAN of the LAN through its tag on the one VNet.
    let router = resource(&tf, "proxmox_virtual_environment_vm", "isoloom_router");
    let bridges = each(router, "network_device", "bridge");
    let tags = each(router, "network_device", "vlan_id");
    let office: Vec<&String> = bridges
        .iter()
        .zip(&tags)
        .filter(|(b, _)| *b == "proxmox_sdn_vnet.office.id")
        .map(|(_, t)| t)
        .collect();
    assert_eq!(office, ["10", "20"], "{bridges:?} {tags:?}");
    Ok(())
}

#[test]
fn mtu_and_rate_limit_go_on_the_devices() -> Outcome {
    let tf = hcl_body(&proxmox(TWO)?);
    // The zone carries the largest; each device its network's.
    assert_eq!(hcl_text(resource(&tf, "proxmox_sdn_zone_simple", "env"), "mtu"), "9000");
    let router = resource(&tf, "proxmox_virtual_environment_vm", "isoloom_router");
    assert_eq!(each(router, "network_device", "mtu"), ["", "1400", "1400", "9000"]);
    let web = resource(&tf, "proxmox_virtual_environment_vm", "web");
    assert_eq!(each(web, "network_device", "rate_limit"), ["1.25"]);
    assert_eq!(each(web, "network_device", "mtu"), ["9000"]);
    Ok(())
}

#[test]
fn the_firewall_enforces_reach_and_the_published_ports() -> Outcome {
    let tf = hcl_body(&proxmox(TWO)?);
    assert!(hcl_names(&tf, "resource", "proxmox_virtual_environment_firewall_ipset").contains(&"servers".to_owned()));
    assert_eq!(
        hcl_text(resource(&tf, "proxmox_virtual_environment_firewall_options", "web"), "input_policy"),
        "DROP"
    );
    let rules = resource(&tf, "proxmox_virtual_environment_firewall_rules", "web");
    let ports: Vec<(String, String)> = each(rules, "rule", "comment").into_iter().zip(each(rules, "rule", "dport")).collect();
    assert!(ports.contains(&("reach office-vlan10 to servers".to_owned(), "80,443".to_owned())), "{ports:?}");
    assert!(ports.contains(&("published".to_owned(), "80".to_owned())), "{ports:?}");
    let web = resource(&tf, "proxmox_virtual_environment_vm", "web");
    assert_eq!(each(web, "network_device", "firewall"), ["true"]);
    // Everything the module makes is in the environment's pool.
    assert_eq!(hcl_names(&tf, "resource", "proxmox_virtual_environment_pool"), ["env"]);
    assert_eq!(hcl_text(web, "pool_id"), "proxmox_virtual_environment_pool.env.pool_id");
    Ok(())
}

#[test]
fn an_arm64_machine_is_an_emulated_aarch64_vm() -> Outcome {
    let tf = proxmox(
        "version: 1\nname: arm\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w: { networks: { lab: 10 }, hardware: { arch: arm64 }, os: debian@12, app: { steps: [{ run: s.sh }] } }\n",
    )?;
    let tf = hcl_body(&tf);
    // Pinned to a dated release (no floating `latest`), with the publisher's SHA-512 checksum.
    // The router and controller run the host's own Debian; the machine its arm64 build.
    let file = resource(&tf, "proxmox_download_file", "debian_12_arm64");
    assert!(hcl_text(file, "url").ends_with("/debian-12-genericcloud-arm64-20261015-2632.qcow2"), "{file:?}");
    assert!(
        !hcl_text(file, "url").contains("/latest/"),
        "arm64 image must be pinned, not `latest`: {file:?}"
    );
    assert_eq!(hcl_text(file, "checksum_algorithm"), "sha512");
    assert_eq!(
        hcl_text(file, "checksum"),
        "e69fe6ec73d73af4de0523b661078774d87c270e72ba99f5e262a54d2de8fff065b5dde145b1671e4c304be333c25e21e20a997742a4b389b7a36c09272131f4"
    );
    let vm = resource(&tf, "proxmox_virtual_environment_vm", "w");
    assert!(vm.attributes().any(|a| a.key() == "bios"), "{vm:?}");
    let literals = hcl_literals(vm);
    assert!(
        literals.contains(&"aarch64".to_owned()) && literals.contains(&"serial0".to_owned()),
        "{literals:?}"
    );
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
    let tf = hcl_body(&tf);
    let files: Vec<(String, String, String)> = hcl_names(&tf, "resource", "proxmox_download_file")
        .iter()
        .map(|n| {
            let f = resource(&tf, "proxmox_download_file", n);
            (hcl_text(f, "url"), hcl_text(f, "checksum_algorithm"), hcl_text(f, "checksum"))
        })
        .collect();
    let find = |image: &str| {
        files
            .iter()
            .find(|(url, ..)| url.ends_with(image))
            .ok_or_else(|| format!("no {image}: {files:?}"))
    };
    let debian = find("/debian-12-genericcloud-amd64-20261015-2632.qcow2")?;
    assert_eq!(debian.1, "sha512");
    assert_eq!(
        debian.2,
        "56c0bd55b6623f2739f377e983d2872e7b208d0cb9c3ba4e93d3f72929026da03637c598b3bd472b29998c31c8a4c17d8e6ae1c556852a8b83a66a09bf5c2b37"
    );
    let rocky = find("/Rocky-9-GenericCloud-Base-9.8-20260525.0.x86_64.qcow2")?;
    assert_eq!(
        (rocky.1.as_str(), rocky.2.as_str()),
        ("sha256", "92c206cc6f790c61583247eefe87890f8828420662c17cacf247cec78ab4eec8")
    );
    for (url, ..) in &files {
        assert!(
            !url.contains("/latest/") && !url.contains(".latest.") && !url.contains("/current/"),
            "no floating image URL: {url}"
        );
    }
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
fn an_lxc_container_is_the_default_container_size() -> Outcome {
    let tf = proxmox(LXC)?;
    let web = isoloom_test_support::hcl_resource(&tf, "proxmox_virtual_environment_container", "web");
    // A container's default size: 1 CPU, 512 MB, 5 GB, each in its block.
    for (block, key, value) in [("cpu", "cores", "1"), ("memory", "dedicated", "512"), ("disk", "size", "5")] {
        let blocks = isoloom_test_support::hcl_blocks(&web, block, &[]);
        let values: Vec<String> = blocks.iter().flat_map(|b| isoloom_test_support::hcl_values(b.body(), key)).collect();
        assert_eq!(values, [value], "{block}.{key}: {web:?}");
    }
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
