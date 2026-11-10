//! What the ESXi host is told about a VM, and what Vagrant's ESXi provider and the `esxi` target
//! say of its console: the rules they share.

/// Why an ESXi VM has no graphical console in the run manifest, the one text Vagrant's ESXi
/// provider and the `esxi` target give.
pub const NO_CONSOLE: &str = "an ESXi VM's console goes through ESXi's own API (a WebMKS ticket), which Isoloom doesn't use";

/// The guest OS type ESXi gives a VM of Windows `os` (VMware's names: Windows Server 2022 is the
/// one after 2019, 2025 the one after 2022), so it offers the right virtual hardware and tools;
/// `None` for any other OS, which keeps the type its box or OVA was built with. The one table
/// Vagrant's ESXi provider and the `esxi` target read.
#[must_use]
pub fn guest_os(os: &str) -> Option<&'static str> {
    Some(match os {
        "windows-10" => "windows9-64",
        "windows-11" => "windows11-64",
        "windows-server-2008r2" => "windows7srv-64",
        "windows-server-2012r2" => "windows8srv-64",
        "windows-server-2016" => "windows9srv-64",
        "windows-server-2019" => "windows2019srv-64",
        "windows-server-2022" => "windows2019srvnext-64",
        "windows-server-2025" => "windows2022srvnext-64",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::KNOWN_OS;

    use super::guest_os;

    #[test]
    fn every_windows_os_has_an_esxi_guest_os_and_any_other_keeps_its_own() {
        for os in KNOWN_OS.iter().filter(|o| o.starts_with("windows")) {
            assert!(guest_os(os).is_some(), "{os}");
        }
        assert_eq!(guest_os("windows-server-2019"), Some("windows2019srv-64"));
        assert_eq!(guest_os("windows-server-2022"), Some("windows2019srvnext-64"));
        assert_eq!(guest_os("debian-12"), None);
    }
}
