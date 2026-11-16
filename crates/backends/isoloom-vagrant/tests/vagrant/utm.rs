//! UTM's part of a Vagrantfile, run on a stand-in for Vagrant: each network a vmnet network.

use isoloom_test_support::needs;

use crate::testing::{Outcome, VAGRANT_STAND_IN_RB, loadable, plan, printed};

/// After [`VAGRANT_STAND_IN_RB`]: fires every VM's UTM triggers: before `Boot` (the adapters, the `osascript` call recorded instead of run) and
/// after `CheckGuestAdditions` (the guest's addresses, its script recorded). Prints each QEMU
/// argument the adapters set (`arg <vm> <argument>`) and each address the guest sets
/// (`address <vm> <mac> <address>`).
const UTM_TRIGGERS_RB: &str = r#"
require "open3"
$vm = nil
ok = Struct.new(:success?).new(true)
Open3.define_singleton_method(:capture2e) do |*argv, stdin_data: nil|
  # osascript - <vm id> <mark> <argument>...
  argv.drop(4).each { |a| puts "arg #{$vm} #{a}" }
  ["", ok]
end
IsoloomUtm.define_singleton_method(:address) do |_machine, script|
  script.scan(/^set_address (\S+) (\S+)/) { |mac, ip| puts "address #{$vm} #{mac} #{ip}" }
end
$triggers.keys.map(&:first).uniq.each do |vm|
  $vm = vm
  machine = Struct.new(:name, :id).new(vm.to_sym, "id-#{vm}")
  [["Boot", :before], ["CheckGuestAdditions", :after]].each do |action, at|
    code = $triggers[[vm, action, at]] or next
    code.call(nil, machine)
  end
end
"#;

/// What [`UTM_TRIGGERS_RB`] prints for `example`: each VM's adapters, as (VM, network id, MAC),
/// and the addresses its guest sets, as (VM, MAC, address).
/// A VM, a network id or MAC, and a MAC or address: one line of [`UTM_TRIGGERS_RB`].
type Triple = (String, String, String);

fn utm_adapters(example: &str) -> Outcome<(Vec<Triple>, Vec<Triple>)> {
    let scratch = tempfile::Builder::new().prefix("isoloom-utm-nics-").tempdir()?;
    let (vagrantfile, mut ruby) = loadable(example, scratch.path())?;
    ruby.arg("-e").arg([VAGRANT_STAND_IN_RB, UTM_TRIGGERS_RB].concat()).arg(vagrantfile);
    let out = printed(ruby)?;
    // Each VM's netdevs (id → network id), then its cards (netdev, MAC).
    let mut netdevs = std::collections::BTreeMap::new();
    let (mut cards, mut addresses) = (Vec::new(), Vec::new());
    for line in out.lines() {
        let mut words = line.splitn(3, ' ');
        let (kind, vm, rest) = (words.next(), words.next().unwrap_or_default().to_owned(), words.next().unwrap_or_default());
        match kind {
            Some("arg") => {
                let (device, options) = rest.split_once(' ').ok_or("an argument without a value")?;
                let opts: std::collections::BTreeMap<&str, &str> = options.split(',').filter_map(|o| o.split_once('=')).collect();
                match device {
                    "-netdev" => {
                        assert!(options.starts_with("vmnet-host,"), "{vm}: {rest}");
                        netdevs.insert(
                            (vm.clone(), opts.get("id").ok_or("id missing")?.to_string()),
                            opts.get("net-uuid").ok_or("net-uuid missing")?.to_string(),
                        );
                    }
                    "-device" => {
                        let uuid = netdevs
                            .get(&(vm.clone(), opts.get("netdev").ok_or("netdev missing")?.to_string()))
                            .ok_or("a card before its netdev")?;
                        cards.push((vm, uuid.clone(), opts.get("mac").ok_or("mac missing")?.to_string()));
                    }
                    other => return Err(format!("unexpected argument {other}").into()),
                }
            }
            Some("address") => {
                let (mac, ip) = rest.split_once(' ').ok_or("an address without its MAC")?;
                addresses.push((vm, mac.to_owned(), ip.to_owned()));
            }
            _ => {}
        }
    }
    Ok((cards, addresses))
}

/// On UTM each network of the environment is a vmnet network of its own: every VM on it gets a card on the same
/// network id, a network's VMs share none with another network, and each guest sets its address
/// on the card with that MAC. Run on segmented (three networks and the router between them) and
/// supplier-portal-api (three VMs on one network, the controller among them). The Ruby itself
/// runs here when `ruby` is on the PATH (CI has it).
#[test]
#[ignore = "needs ruby: runs the Vagrantfile's Ruby here, as Vagrant's own Ruby would; `cargo xtask test` runs it"]
fn every_vm_of_a_network_gets_a_card_on_the_same_vmnet_network_on_utm() -> Outcome {
    needs(&["ruby"])?;
    for example in ["segmented", "supplier-portal-api"] {
        let env = plan(example)?;
        let (cards, addresses) = utm_adapters(example)?;
        let mut ids: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> = std::collections::BTreeMap::new();
        for vm in env.vms() {
            let label = vm.name;
            for (net, ip) in &vm.plan.addresses {
                // The card the guest sets this network's address on.
                let at = format!("{ip}/{}", env.block(net).prefix());
                let (_, mac, _) = addresses
                    .iter()
                    .find(|(v, _, a)| v == label && *a == at)
                    .ok_or_else(|| format!("{example}: {label} sets no {at} ({net}): {addresses:?}"))?;
                let (_, uuid, _) = cards
                    .iter()
                    .find(|(v, _, m)| v == label && m == mac)
                    .ok_or_else(|| format!("{example}: {label} has no card with {mac} ({net}): {cards:?}"))?;
                ids.entry(net.as_str()).or_default().insert(uuid.as_str());
            }
        }
        assert_eq!(ids.len(), env.networks.len(), "{example}: a network no VM is on: {ids:?}");
        for (net, uuids) in &ids {
            assert_eq!(uuids.len(), 1, "{example}: network {net}'s VMs are on different vmnet networks: {uuids:?}");
        }
        let distinct: std::collections::BTreeSet<_> = ids.values().flatten().collect();
        assert_eq!(distinct.len(), ids.len(), "{example}: two networks share a vmnet network: {ids:?}");
    }
    Ok(())
}
