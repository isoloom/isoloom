//! libvirt's part of a Vagrantfile: its boxes' architecture, and its networks made where another
//! network already holds their block.

use crate::testing::{Outcome, field, keywords, libvirt_networks, vagrantfile};

/// Vagrant 2.4 picks one build of a multi-architecture box by `box_architecture` (an older one
/// takes any: a ppc64le Debian on an amd64 host). libvirt's roboxes boxes are such boxes: a
/// machine's gets its own architecture, Isoloom's own VMs the host's.
#[test]
fn libvirts_own_boxes_are_pinned_to_an_architecture() -> Outcome {
    let file = vagrantfile("ansible-pair")?;
    let libvirt_blocks: Vec<&str> = file.split("provider \"libvirt\" do |v, o|").skip(1).collect();
    assert!(!libvirt_blocks.is_empty(), "no libvirt box of its own");
    for block in libvirt_blocks {
        let body = block.split("\n    end").next().unwrap_or_default();
        assert!(
            body.contains("o.vm.box_architecture = \"amd64\"") || body.contains("o.vm.box_architecture = :auto"),
            "{body}"
        );
    }
    assert!(file.contains("o.vm.box_architecture = :auto"), "the controller takes the host's");
    Ok(())
}

// Every libvirt network a VM joins exists when the VM starts: vagrant-libvirt creates it.
// vagrant-libvirt (`create_networks.rb`, `handle_ip_option`) finds a private network that has an
// `ip:` by its address block first, unless its forward mode is `veryisolated`, and only then by
// its name. When the computer already has a network on the block under another name (another
// copy of the environment, another environment on the same block, one left behind), it takes
// that one, makes nothing, and the VM's interface still names the network the Vagrantfile gave:
// `virDomainCreate` fails with "Network not found". The tests below read each VM's
// `private_network` lines from the rendered Vagrantfile and follow vagrant-libvirt's lookup on a
// computer that already holds every one of the environment's blocks under other names.

/// The network vagrant-libvirt attaches the VM to (`Ok`), or why the VM won't start (`Err`), on a
/// computer whose networks are `host` (name → block).
fn attached(kw: &std::collections::BTreeMap<String, String>, host: &std::collections::BTreeMap<String, String>) -> Result<String, String> {
    let name = field(kw, "libvirt__network_name");
    let veryisolated = kw.get("libvirt__forward_mode").is_some_and(|m| m == "veryisolated");
    if !veryisolated {
        let block = block_of(field(kw, "ip"), field(kw, "netmask"));
        if let Some((other, _)) = host.iter().find(|(other, b)| **b == block && other.as_str() != name) {
            return Err(format!("{block} is already {other}'s: nothing makes `{name}`, which the VM's interface names"));
        }
    }
    // Found by its name, or made under it.
    Ok(name.to_owned())
}

/// The block `ip`/`netmask` is in, as `a.b.c.d/n`.
fn block_of(ip: &str, netmask: &str) -> String {
    // Unparsed, a block reads 0.0.0.0/0, which every crowded computer holds: the test fails.
    let ip: std::net::Ipv4Addr = ip.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
    let mask: std::net::Ipv4Addr = netmask.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
    let bits = u32::from(mask);
    format!("{}/{}", std::net::Ipv4Addr::from(u32::from(ip) & bits), bits.count_ones())
}

/// The computer as a run finds it: every block of `networks` already held by a network of
/// another name.
fn crowded(networks: &[(String, std::collections::BTreeMap<String, String>)]) -> std::collections::BTreeMap<String, String> {
    networks
        .iter()
        .map(|(_, kw)| {
            (
                format!("{}-elsewhere", field(kw, "libvirt__network_name")),
                block_of(field(kw, "ip"), field(kw, "netmask")),
            )
        })
        .collect()
}

/// Every VM of `vagrantfile` starts on a computer that already holds its networks' blocks under
/// other names: each joins the network the Vagrantfile names, made for it.
fn every_network_is_made(vagrantfile: &str) {
    let networks = libvirt_networks(vagrantfile);
    assert!(!networks.is_empty(), "no libvirt network in:\n{vagrantfile}");
    let host = crowded(&networks);
    for (vm, kw) in &networks {
        assert_eq!(attached(kw, &host).as_ref(), Ok(&field(kw, "libvirt__network_name").to_owned()), "{vm}: {kw:?}");
    }
}

#[test]
fn a_multi_machine_labs_network_is_made_even_where_another_network_holds_its_block() -> Outcome {
    // supplier-portal-api and invoice-portal-api are both on 10.20.0.0/24.
    every_network_is_made(&vagrantfile("supplier-portal-api")?);
    every_network_is_made(&vagrantfile("segmented")?);
    Ok(())
}

#[test]
fn a_one_machine_labs_network_is_made_too() -> Outcome {
    every_network_is_made(&vagrantfile("solo-web")?);
    Ok(())
}

#[test]
fn an_ipv6_network_is_made_by_its_name_with_its_block() -> Outcome {
    let vagrantfile = vagrantfile("dual-stack")?;
    every_network_is_made(&vagrantfile);
    for (vm, kw) in libvirt_networks(&vagrantfile) {
        assert!(kw.contains_key("libvirt__ipv6_address"), "{vm}: {kw:?}");
    }
    Ok(())
}

#[test]
fn the_lookup_this_test_follows_fails_on_a_network_found_by_its_block() {
    // The failure the run met: a network found by its block under another name.
    let kw: std::collections::BTreeMap<String, String> = [("libvirt__network_name", "isoloom-a-lab"), ("ip", "10.20.0.31"), ("netmask", "255.255.255.0")]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    let host: std::collections::BTreeMap<String, String> = [("isoloom-b-lab".to_owned(), "10.20.0.0/24".to_owned())].into_iter().collect();
    assert!(attached(&kw, &host).is_err());
    let mut isolated = kw;
    isolated.insert("libvirt__forward_mode".to_owned(), "veryisolated".to_owned());
    assert_eq!(attached(&isolated, &host), Ok("isoloom-a-lab".to_owned()));
}

#[test]
fn keywords_read_strings_with_interpolations_and_bare_values() {
    let kw =
        keywords("m.vm.network \"private_network\", ip: \"10.20.0.32\", virtualbox__intnet: \"isoloom-x-#{ISOLOOM_KEY}-lab\", libvirt__dhcp_enabled: false");
    assert_eq!(field(&kw, "ip"), "10.20.0.32");
    assert_eq!(field(&kw, "virtualbox__intnet"), "isoloom-x-#{ISOLOOM_KEY}-lab");
    assert_eq!(field(&kw, "libvirt__dhcp_enabled"), "false");
}
