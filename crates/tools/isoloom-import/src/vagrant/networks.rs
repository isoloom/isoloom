//! A Vagrantfile's networks, imported: a private network becomes an environment network with the
//! machine's address on it, a public one a bridge to the computer's LAN.

use std::net::Ipv4Addr;

use isoloom_spec::net::Cidr;
use serde_json::Value;

use super::recording::{Call, text};
use super::{Bridge, Import, LAN_PLACEHOLDER, Machine, Net};
use crate::draft::NoteKind;
use crate::naming::label;

impl Import<'_> {
    /// A public network: a `bridged` network, at the machine's fixed address in its block, else
    /// by DHCP from the LAN (the block a placeholder then, said).
    pub(super) fn public_network(&mut self, at: &str, m: &mut Machine, c: &Call) {
        let at = format!("{at}.vm.network");
        let interface = match c.opt("bridge") {
            Some(Value::Array(list)) => {
                let first = text(list.first());
                if list.len() > 1 {
                    self.notes.add(
                        &at,
                        NoteKind::Changed,
                        format!(
                            "several interfaces to bridge to: the first, `{}`, is kept",
                            first.as_deref().unwrap_or_default()
                        ),
                    );
                }
                first
            }
            other => text(other),
        };
        let given = text(c.opt("ip"));
        let fixed = given.as_deref().and_then(|ip| ip.parse::<Ipv4Addr>().ok());
        if let Some(given) = given.as_deref().filter(|_| fixed.is_none()) {
            self.notes.add(
                &at,
                NoteKind::Changed,
                format!("`{given}` isn't an IPv4 address: drafted as an address by DHCP"),
            );
        }
        let dhcp = fixed.is_none();
        let bridge = Bridge {
            interface: interface.clone(),
            dhcp,
            default_route: dhcp && c.opt("use_dhcp_assigned_default_route").and_then(Value::as_bool) == Some(true),
        };
        let (cidr, octet) = if let Some(ip) = fixed {
            self.note_unreadable_netmask(&at, c);
            (block(ip, text(c.opt("netmask")).as_deref()).0, ip.octets()[3])
        } else {
            self.notes.add(
                &at,
                NoteKind::Changed,
                format!("by DHCP from the LAN, whose block the Vagrantfile doesn't say: `cidr` is {LAN_PLACEHOLDER}, set it to the LAN's"),
            );
            (LAN_PLACEHOLDER, 0)
        };
        let key = format!("bridged {}", if dhcp { interface.unwrap_or_default() } else { cidr.to_string() });
        self.nets.entry(key.clone()).or_insert_with(|| Net {
            cidr,
            name: "lan".to_owned(),
            bridged: Some(bridge),
        });
        m.networks.push((key, octet));
    }

    /// A `netmask` that is neither an address nor a length, noted: the block falls back to a /24.
    fn note_unreadable_netmask(&mut self, at: &str, c: &Call) {
        if let Some(mask) = text(c.opt("netmask")).filter(|m| netmask_length(m).is_none()) {
            self.notes.add(
                at,
                NoteKind::Changed,
                format!("netmask `{mask}` is neither an address nor a length: the block is a /24"),
            );
        }
    }

    /// A private network with a fixed address: the machine joins its block (said when it isn't
    /// the address's own: one wider than /24 or narrower than /29). An address that isn't IPv4 is
    /// left out, and said.
    pub(super) fn private_network(&mut self, at: &str, m: &mut Machine, c: &Call) {
        if text(c.opt("type")).as_deref() == Some("dhcp") || c.opt("ip").is_none() {
            self.notes.add(
                format!("{at}.vm.network"),
                NoteKind::Changed,
                "a DHCP private network: give the machine a fixed address",
            );
            return;
        }
        let given = text(c.opt("ip")).unwrap_or_default();
        let Ok(ip) = given.parse::<Ipv4Addr>() else {
            self.notes.add(
                format!("{at}.vm.network"),
                NoteKind::Changed,
                format!("`{given}` isn't an IPv4 address: the machine joins no network for it; give it an IPv4 one"),
            );
            return;
        };
        self.note_unreadable_netmask(&format!("{at}.vm.network"), c);
        let (cidr, asked) = block(ip, text(c.opt("netmask")).as_deref());
        if let Some(len) = asked {
            self.notes.add(
                format!("{at}.vm.network"),
                NoteKind::Changed,
                format!("{ip}/{len} isn't a /24 to /29; re-addressed to {cidr}"),
            );
        }
        let key = cidr.to_string();
        self.nets.entry(key.clone()).or_insert_with(|| Net {
            cidr,
            name: text(c.opt("virtualbox__intnet")).map(|n| label(&n)).unwrap_or_default(),
            bridged: None,
        });
        m.networks.push((key, ip.octets()[3]));
    }
}

/// The block of a private network's address: its netmask's (an address or a length), /24 when
/// it gives none, and never wider than /24 or narrower than /29 (the spec's sizes); with the
/// length asked when it was outside those.
pub(super) fn block(ip: Ipv4Addr, netmask: Option<&str>) -> (Cidr, Option<u32>) {
    let asked = netmask.and_then(netmask_length).unwrap_or(24);
    let clamped = asked.clamp(24, 29);
    // The clamp keeps it within 24..=29, so it fits a `u8`.
    let len = u8::try_from(clamped).unwrap_or(24);
    (Cidr::enclosing(ip, len), (asked != clamped).then_some(asked))
}

/// A netmask's length: from an address (`255.255.255.0`) or written as one (`24`).
fn netmask_length(mask: &str) -> Option<u32> {
    mask.parse::<Ipv4Addr>().ok().map(|m| u32::from(m).count_ones()).or_else(|| mask.parse().ok())
}
