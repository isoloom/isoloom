//! A Linux VM's addresses on the environment's networks, kept where NetworkManager runs.
//!
//! Vagrant gives a VM its static address on each private network (`vm.network
//! "private_network", ip:`), but on a guest whose network is NetworkManager's (Kali, Fedora,
//! desktop Ubuntu), NetworkManager also takes the card, as a connection of its own that asks DHCP
//! for an address. The environment's networks have no DHCP, so the attempt times out (45 s) and
//! NetworkManager drops the static address: the VM is left with none on the network, and nothing
//! there reaches it (seen on libvirt with Kali: `eth1` empty a minute after `vagrant up`).
//!
//! So each Linux VM's provisioner, at every `vagrant up` (Vagrant sets the addresses again at
//! each), hands each card holding one of its addresses back to Vagrant's configuration:
//! NetworkManager is told to leave the card alone now (`nmcli device set <card> managed no`) and
//! at every later start (`unmanaged-devices` in a configuration file of its own), and an address
//! NetworkManager already dropped is put back. A guest without NetworkManager is left alone.

use std::net::Ipv4Addr;

use crate::ruby::{Provision, Ruby, Value};

/// The provisioner's name: what `vagrant up --provision-with` takes.
pub(crate) const NAME: &str = "addresses";

/// The script, given each address as `<ip>/<prefix>`: for each, the card holding it, or, when
/// NetworkManager already dropped it, the card NetworkManager is still asking DHCP on that has no
/// address; that card unmanaged and given the address back; then the cards written down for
/// NetworkManager's next starts.
pub(crate) const SCRIPT: &str = r#"set -eu
command -v nmcli >/dev/null 2>&1 && nmcli -t general status >/dev/null 2>&1 || exit 0
cards=""
for address in "$@"; do
  ip=${address%/*}
  card=$(ip -o -4 addr show | awk -v ip="$ip" '{ split($4, a, "/"); if (a[1] == ip) { print $2; exit } }')
  if [ -z "$card" ]; then
    # Dropped already: the card NetworkManager is configuring that has no IPv4 address.
    for c in $(nmcli -t -f DEVICE,STATE device | awk -F: '$2 ~ /^connecting/ { print $1 }'); do
      [ -n "$(ip -o -4 addr show dev "$c")" ] || { card=$c; break; }
    done
  fi
  [ -n "$card" ] || { echo "addresses: no card for $address" >&2; exit 1; }
  nmcli device set "$card" managed no
  ip -o -4 addr show dev "$card" | grep -q " $ip/" || ip addr add "$address" dev "$card"
  ip link set "$card" up
  case ";$cards;" in *";interface-name:$card;"*) ;; *) cards="$cards${cards:+;}interface-name:$card" ;; esac
done
mkdir -p /etc/NetworkManager/conf.d
printf '[keyfile]\nunmanaged-devices=%s\n' "$cards" >/etc/NetworkManager/conf.d/90-isoloom-addresses.conf
"#;

/// `m.vm.provision "addresses"`, run at every `vagrant up`, for a Linux VM at `addresses` (each
/// with its network's prefix length). Nothing for a VM with no address on the environment's
/// networks.
pub(crate) fn provisioner(r: &mut Ruby, addresses: &[(Ipv4Addr, u8)]) {
    if addresses.is_empty() {
        return;
    }
    let args = addresses.iter().map(|(ip, prefix)| Value::str(&format!("{ip}/{prefix}")));
    r.provision(
        Provision::named("m", NAME, "shell")
            .kw("run", Value::str("always"))
            .kw("inline", Value::str(SCRIPT))
            .kw("args", Value::array(args)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linux_vm_keeps_its_addresses_from_networkmanager_at_every_up() {
        let mut r = Ruby::new();
        provisioner(&mut r, &[(Ipv4Addr::new(10, 80, 0, 249), 24), (Ipv4Addr::new(10, 81, 0, 9), 28)]);
        let out = r.finish();
        assert!(out.contains("m.vm.provision \"addresses\", type: \"shell\""), "{out}");
        assert!(out.contains("run: \"always\""), "{out}");
        assert!(out.contains("args: [\"10.80.0.249/24\", \"10.81.0.9/28\"]"), "{out}");
        let mut none = Ruby::new();
        provisioner(&mut none, &[]);
        assert!(none.finish().trim().is_empty(), "no address, no provisioner");
    }

    #[cfg(unix)]
    #[test]
    fn the_script_parses_and_leaves_a_guest_without_networkmanager_alone() {
        let parsed = std::process::Command::new("sh").args(["-n", "-c", SCRIPT]).status().unwrap();
        assert!(parsed.success(), "not valid sh");
        // No nmcli on PATH: nothing is touched, and it succeeds.
        let empty = tempfile::Builder::new().prefix("isoloom-no-nmcli-").tempdir().unwrap();
        let out = std::process::Command::new("/bin/sh")
            .args(["-c", SCRIPT, "addresses", "10.80.0.249/24"])
            .env("PATH", empty.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }
}
