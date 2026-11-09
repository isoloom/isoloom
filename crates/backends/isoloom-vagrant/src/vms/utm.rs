//! UTM's two network cards (`vagrant_utm`): a VM gets UTM's shared network (vmnet, the Mac's
//! NAT, fast) and the plugin's emulated one (QEMU's user-mode network, `10.0.2.0/24`, gateway
//! `10.0.2.2`), which Vagrant reaches the VM through: its SSH and forwarded ports. Both take a
//! default route by DHCP with the same metric, and Linux picks the emulated one, so all of the
//! VM's internet traffic goes through QEMU's slow user-mode stack (measured: about 12 times
//! slower than the shared network, an image pull of a gigabyte taking over half an hour).
//!
//! On UTM, a Linux VM's first provisioner, at every `vagrant up`, gives the emulated card's
//! default route a higher metric (200), so the shared network's wins: its route now, and its
//! network configuration (NetworkManager, systemd-networkd or ifupdown) for every later boot and
//! DHCP renewal. Only the default route changes: SSH and the forwarded ports still reach the VM
//! through the emulated card. The card is the one whose default route is through `10.0.2.2`, not
//! a name; a VM with no other default route (no shared network) is left alone. The other
//! providers have no such pair (QEMU's own provider gives a VM the user-mode network only): there
//! the provisioner, defined on every provider so UTM's keeps its place first, does nothing.

use crate::ruby::{Provision, Ruby, Value};

/// The provisioner's name: what `vagrant up --provision-with` takes.
pub(crate) const NAT_ROUTE: &str = "nat-route";

/// The Ruby constant holding [`NAT_ROUTE_SH`], defined once before `Vagrant.configure`.
const CONSTANT: &str = "ISOLOOM_NAT_ROUTE";

/// The script: the emulated card's default route at metric 200, now and in its configuration.
///
/// The configuration is the card's own: a box may configure every card from one shared file
/// (Debian's cloud images: netplan's `10-netplan-all.network`), and a setting there would raise
/// the shared network's route too, a tie again. So systemd-networkd gets a file of its own that
/// matches the emulated card only and sorts before the shared one. Applying it renews the card's
/// lease, so its gateway is unreachable for a moment: the script waits for the route to come back
/// before it touches it. `ISOLOOM_NETWORKD_DIR` (default `/etc/systemd/network`) is for tests.
pub(crate) const NAT_ROUTE_SH: &str = r#"set -eu
gw=10.0.2.2
nat=$(ip -4 route show default via "$gw" | awk '{for (i = 1; i < NF; i++) if ($i == "dev") { print $(i + 1); exit }}')
# No default route through the emulated network, or no other one to prefer: nothing to do.
[ -n "$nat" ] || exit 0
ip -4 route show default | grep -v "via $gw dev $nat " | grep -q . || exit 0
# Waits (up to 30 s) for the card's default route after its configuration was applied again.
route_back() {
  i=0
  until ip -4 route show default via "$gw" dev "$nat" | grep -q .; do
    i=$((i + 1))
    [ "$i" -lt 30 ] || return 1
    sleep 1
  done
}
netd=${ISOLOOM_NETWORKD_DIR:-/etc/systemd/network}
if command -v nmcli >/dev/null 2>&1 && nmcli -t -f RUNNING general 2>/dev/null | grep -q running; then
  con=$(nmcli -g GENERAL.CONNECTION device show "$nat")
  if [ -n "$con" ] && [ "$(nmcli -g ipv4.route-metric connection show "$con")" != 200 ]; then
    nmcli connection modify "$con" ipv4.route-metric 200
    nmcli device reapply "$nat"
  fi
elif command -v networkctl >/dev/null 2>&1 && file=$(networkctl status "$nat" 2>/dev/null | sed -n 's/^ *Network File: //p') && [ -n "$file" ] && [ "$file" != n/a ]; then
  own="$netd/05-isoloom-nat-route.network"
  if [ "$file" != "$own" ]; then
    mkdir -p "$netd"
    printf '[Match]\nName=%s\n\n[Network]\nDHCP=ipv4\n\n[DHCPv4]\nRouteMetric=200\n' "$nat" > "$own"
    networkctl reload
    networkctl reconfigure "$nat"
  fi
elif [ -f /etc/network/interfaces ]; then
  for f in /etc/network/interfaces /etc/network/interfaces.d/*; do
    [ -f "$f" ] && grep -q "^iface $nat inet dhcp" "$f" || continue
    sed -n "/^iface $nat inet dhcp/,/^[^ \t]/p" "$f" | grep -q '^[ \t]*metric ' ||
      sed -i "/^iface $nat inet dhcp/a\\    metric 200" "$f"
  done
fi
if ! route_back; then
  echo "nat-route: the emulated card has no default route any more; the shared network carries the traffic"
  exit 0
fi
# The route as it is now, whatever the configuration does at its next renewal.
ip -4 route show default via "$gw" dev "$nat" | grep -q ' metric 200' ||
  ip -4 route replace default via "$gw" dev "$nat" metric 200
for m in $(ip -4 route show default via "$gw" dev "$nat" | awk '{m = 0; for (i = 1; i < NF; i++) if ($i == "metric") m = $(i + 1); print m}'); do
  [ "$m" = 200 ] || ip -4 route del default via "$gw" dev "$nat" metric "$m"
done
"#;

/// The script as the Ruby constant the UTM blocks' provisioners run (before `Vagrant.configure`).
pub(crate) fn constant(r: &mut Ruby) {
    r.comment("On UTM, a Linux VM's default route through UTM's shared network, not the plugin's slower emulated one (the `nat-route` provisioners).");
    r.text(&format!("{CONSTANT} = <<~'SH'\n{NAT_ROUTE_SH}SH\n"));
}

/// `<var>.vm.provision "nat-route"`, run at every `vagrant up`: on every provider a no-op, so it
/// comes first among the VM's provisioners (`var` is the VM's: `m`, `config`).
pub(crate) fn placeholder(r: &mut Ruby, var: &str) {
    r.provision(named(var).kw("inline", Value::str(":")));
}

/// The UTM provider block's override of [`placeholder`] (`o` being the block's config), in its
/// place: the emulated card's default route made to lose.
pub(crate) fn override_in_block(r: &mut Ruby) {
    r.provision(named("o").kw("preserve_order", true).kw("inline", Value::raw(CONSTANT)));
}

/// The named shell provisioner, run at every `vagrant up`.
fn named(var: &str) -> Provision {
    Provision::new(var, NAT_ROUTE).kw("type", Value::str("shell")).kw("run", Value::str("always"))
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use super::NAT_ROUTE_SH;

    /// A run of the script in a scratch folder: stand-ins on a `PATH` holding nothing else but the
    /// text tools it uses, what they were asked (one call a line), and its networkd folder.
    struct Run {
        dir: std::path::PathBuf,
    }

    impl Run {
        /// A scratch folder named `name`, with a stand-in `ip` whose `route show default [via G]
        /// [dev D]` prints the lines of `routes` holding each filter given, and logs every other
        /// call.
        fn new(name: &str, routes: &str) -> Result<Run, Box<dyn std::error::Error>> {
            let dir = std::env::temp_dir().join(format!("isoloom-nat-route-{}-{name}", std::process::id()));
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
            }
            std::fs::create_dir_all(dir.join("bin"))?;
            std::fs::write(dir.join("routes"), routes)?;
            let run = Run { dir };
            run.stand_in(
                "ip",
                &format!(
                    "if [ \"$3\" = show ]; then\n  shift 4\n  awk -v want=\"$*\" 'BEGIN {{ n = split(want, w, \" \") }} {{ ok = 1; for (i = 1; i < n; i += 2) if (index($0, w[i] \" \" w[i + 1] \" \") == 0) ok = 0; if (ok) print }}' {}\nelse\n  echo \"ip $*\" >> {}\nfi\n",
                    run.dir.join("routes").display(),
                    run.log().display(),
                ),
            )?;
            for tool in ["awk", "grep", "sed", "mkdir"] {
                let found = Command::new("sh").arg("-c").arg(format!("command -v {tool}")).output()?;
                std::os::unix::fs::symlink(String::from_utf8(found.stdout)?.trim(), run.dir.join("bin").join(tool))?;
            }
            Ok(run)
        }

        fn log(&self) -> std::path::PathBuf {
            self.dir.join("log")
        }

        fn networkd(&self) -> std::path::PathBuf {
            self.dir.join("networkd")
        }

        /// A stand-in `name` running the shell `body`.
        fn stand_in(&self, name: &str, body: &str) -> Result<(), Box<dyn std::error::Error>> {
            let path = self.dir.join("bin").join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}"))?;
            Command::new("chmod").arg("+x").arg(&path).status()?;
            Ok(())
        }

        /// The script run to its end (it must succeed): the calls it made, one a line.
        fn calls(&self) -> Result<String, Box<dyn std::error::Error>> {
            let out = Command::new("/bin/sh")
                .arg("-c")
                .arg(NAT_ROUTE_SH)
                .env_clear()
                .env("PATH", self.dir.join("bin"))
                .env("ISOLOOM_NETWORKD_DIR", self.networkd())
                .output()?;
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            Ok(if Path::new(&self.log()).exists() {
                std::fs::read_to_string(self.log())?
            } else {
                String::new()
            })
        }
    }

    /// The calls the script makes with `routes` and no network manager.
    fn changes(name: &str, routes: &str) -> Result<String, Box<dyn std::error::Error>> {
        Run::new(name, routes)?.calls()
    }

    const TWO_DEFAULTS: &str =
        "default via 192.168.64.1 dev enp0s1 proto dhcp src 192.168.64.5 metric 100\ndefault via 10.0.2.2 dev enp0s2 proto dhcp src 10.0.2.15 metric 100\n";

    #[test]
    fn with_two_default_routes_the_emulated_cards_loses() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            changes("two", TWO_DEFAULTS)?,
            "ip -4 route replace default via 10.0.2.2 dev enp0s2 metric 200\nip -4 route del default via 10.0.2.2 dev enp0s2 metric 100\n"
        );
        Ok(())
    }

    /// Debian's cloud images configure every card from netplan's one `10-netplan-all.network`: a
    /// setting there would raise the shared network's route too (a tie again). The emulated card
    /// gets a file of its own, sorting first, matching it alone.
    #[test]
    fn under_networkd_the_emulated_card_gets_a_file_of_its_own() -> Result<(), Box<dyn std::error::Error>> {
        let run = Run::new("networkd", TWO_DEFAULTS)?;
        let log = run.log();
        run.stand_in(
            "networkctl",
            &format!(
                "if [ \"$1\" = status ]; then echo '  Network File: /run/systemd/network/10-netplan-all.network'; else echo \"networkctl $*\" >> {}; fi\n",
                log.display()
            ),
        )?;
        let calls = run.calls()?;
        assert!(calls.starts_with("networkctl reload\nnetworkctl reconfigure enp0s2\n"), "{calls}");
        let own = std::fs::read_to_string(run.networkd().join("05-isoloom-nat-route.network"))?;
        assert_eq!(own, "[Match]\nName=enp0s2\n\n[Network]\nDHCP=ipv4\n\n[DHCPv4]\nRouteMetric=200\n");
        assert_eq!(std::fs::read_dir(run.networkd())?.count(), 1, "nothing is added to the shared file");
        Ok(())
    }

    /// Once its own file applies, a later run changes nothing in the configuration.
    #[test]
    fn a_card_already_on_its_own_file_is_not_reconfigured() -> Result<(), Box<dyn std::error::Error>> {
        let run = Run::new("own", TWO_DEFAULTS)?;
        let own = run.networkd().join("05-isoloom-nat-route.network");
        run.stand_in(
            "networkctl",
            &format!(
                "if [ \"$1\" = status ]; then echo '  Network File: {}'; else echo \"networkctl $*\" >> {}; fi\n",
                own.display(),
                run.log().display()
            ),
        )?;
        assert!(!run.calls()?.contains("networkctl"));
        Ok(())
    }

    #[test]
    fn the_emulated_network_alone_or_none_is_left_alone() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(changes("alone", "default via 10.0.2.2 dev eth0 proto dhcp src 10.0.2.15 metric 100\n")?, "");
        assert_eq!(changes("other", "default via 192.168.1.1 dev eth0 proto dhcp metric 100\n")?, "");
        Ok(())
    }
}
