//! What an instance learns about its own VNICs at boot, from Oracle Cloud's instance metadata
//! service (IMDS v2): a secondary VNIC is attached only once the instance exists, so Terraform
//! can't hand its MAC address (or an address Oracle Cloud picked) to the instance's own set-up.

use std::net::Ipv4Addr;

/// Shell printing what `pick` (a Python expression over `v`, one VNIC of IMDS's list) gives for
/// the VNIC whose private address is `ip`, once it is attached (waited for up to 15 minutes).
fn lookup(ip: Ipv4Addr, pick: &str) -> String {
    format!(
        "$(i=0; until v=$(curl -sf -H 'Authorization: Bearer Oracle' http://169.254.169.254/opc/v2/vnics/ | python3 -c 'import json, sys; print(next((str({pick}) for v in json.load(sys.stdin) if v.get(\"privateIp\") == \"{ip}\"), \"\"))' 2>/dev/null) && [ -n \"$v\" ]; do i=$((i+5)); [ $i -lt 900 ] || break; sleep 5; done; echo \"$v\")"
    )
}

/// Shell printing the MAC address of the VNIC at `ip`, lowercase as `ip link` writes it.
pub(super) fn mac(ip: Ipv4Addr) -> String {
    lookup(ip, "v.get(\"macAddr\", \"\").lower()")
}

/// Shell printing the first IPv6 address Oracle Cloud gave the VNIC at `ip`.
pub(super) fn ipv6(ip: Ipv4Addr) -> String {
    lookup(ip, "(v.get(\"ipv6Addresses\") or [\"\"])[0]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vnic_is_found_by_its_private_address_and_waited_for() {
        let m = mac(Ipv4Addr::new(10, 0, 2, 5));
        assert!(m.contains("v.get(\"privateIp\") == \"10.0.2.5\""), "{m}");
        assert!(m.contains("v.get(\"macAddr\", \"\").lower()"), "{m}");
        assert!(m.starts_with("$(i=0; until v=$(curl -sf -H 'Authorization: Bearer Oracle'"), "{m}");
        assert!(ipv6(Ipv4Addr::new(10, 0, 2, 5)).contains("ipv6Addresses"));
    }
}
