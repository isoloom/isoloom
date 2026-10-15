//! A VM's further addresses on a network (`lan: [10, 11]`), written once for every VM target:
//! each on the interface that holds the machine's primary address there, since the hypervisor
//! or the cloud configures only that one. A cloud that filters by address has been told of them
//! by its own module (a secondary private address, an alias range).

use std::fmt::Write as _;

use isoloom_plan::more_addresses::MoreAddresses;

use crate::shell::quote;

/// Shell that puts each further address on the interface holding the primary one. Idempotent
/// (`replace`): a transport runs it again at every boot and whenever a link comes up.
pub fn linux(addresses: &[MoreAddresses]) -> String {
    let mut sh = String::new();
    for a in addresses {
        let find = quote(&format!(" inet {}/", a.primary));
        let _ = writeln!(sh, "dev=$(ip -o -4 addr show | grep -F {find} | awk '{{print $2; exit}}')");
        for more in &a.more {
            let _ = writeln!(sh, "ip addr replace {more}/{} dev \"$dev\"", a.prefix);
        }
    }
    sh
}

/// The same in PowerShell for a Windows VM: the interface found by the primary address, each
/// further one added once (Windows keeps them across restarts) and never used as a source, so
/// the machine's own connections still leave from its primary address.
pub fn windows(addresses: &[MoreAddresses]) -> String {
    addresses
        .iter()
        .flat_map(|a| {
            a.more.iter().map(move |more| {
                format!(
                    "if (-not (Get-NetIPAddress -IPAddress '{more}' -ErrorAction SilentlyContinue)) {{ New-NetIPAddress -InterfaceIndex (Get-NetIPAddress -IPAddress '{}').InterfaceIndex -IPAddress '{more}' -PrefixLength {} -SkipAsSource $true | Out-Null }}",
                    a.primary, a.prefix
                )
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web() -> Vec<MoreAddresses> {
        vec![MoreAddresses {
            primary: "10.0.0.10".parse().unwrap(),
            prefix: 24,
            more: vec!["10.0.0.11".parse().unwrap(), "10.0.0.12".parse().unwrap()],
        }]
    }

    #[test]
    fn each_address_lands_on_the_interface_of_the_primary_one() {
        let sh = linux(&web());
        assert!(sh.starts_with("dev=$(ip -o -4 addr show | grep -F ' inet 10.0.0.10/'"), "{sh}");
        assert!(
            sh.contains("ip addr replace 10.0.0.11/24 dev \"$dev\"\nip addr replace 10.0.0.12/24 dev \"$dev\"\n"),
            "{sh}"
        );
        let ps = windows(&web());
        assert_eq!(ps.lines().count(), 2, "{ps}");
        assert!(
            ps.contains("(Get-NetIPAddress -IPAddress '10.0.0.10').InterfaceIndex -IPAddress '10.0.0.11' -PrefixLength 24 -SkipAsSource $true"),
            "{ps}"
        );
    }
}
