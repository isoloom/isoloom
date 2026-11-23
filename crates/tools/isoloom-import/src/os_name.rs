//! The OS an image's name says (a Vagrant box, a cloud image or its filter): the family it
//! names and the version it says, as the spec writes it. Every importer reads it the same way.

/// The words a name says a version with, and the OS the spec names that version by.
type Version = (&'static [&'static str], &'static str);

/// The words naming a family, its versions, and the OS for a name that says no version.
type Family = (&'static [&'static str], &'static [Version], &'static str);

/// What an image's name says of each OS family: the words naming the family, then each version's
/// words (its number, written with or without its dot, and its codename) with the OS the spec
/// names it by, and the OS for a name that says no version. The first family named wins:
/// Windows before the others (`windows-10-ubuntu-wsl` is Windows).
const FAMILIES: &[Family] = &[
    (
        &["windows", "win10", "win11", "win20", "datacenter"],
        &[
            (&["2008r2", "2008-r2"], "windows-server@2008r2"),
            (&["2012r2", "2012-r2"], "windows-server@2012r2"),
            (&["2016"], "windows-server@2016"),
            (&["2019"], "windows-server@2019"),
            (&["2022"], "windows-server@2022"),
            (&["2025"], "windows-server@2025"),
            (&["win11", "windows11", "windows-11", "windows_11"], "windows@11"),
            (&["win10", "windows10", "windows-10", "windows_10"], "windows@10"),
        ],
        "windows-server@2019",
    ),
    (&["kali"], &[], "kali"),
    (
        &["ubuntu"],
        &[
            (&["14.04", "14_04", "1404", "trusty"], "ubuntu@14.04"),
            (&["16.04", "16_04", "1604", "xenial"], "ubuntu@16.04"),
            (&["18.04", "18_04", "1804", "bionic"], "ubuntu@18.04"),
            (&["20.04", "20_04", "2004", "focal"], "ubuntu@20.04"),
            (&["22.04", "22_04", "2204", "jammy"], "ubuntu@22.04"),
            (&["24.04", "24_04", "2404", "noble"], "ubuntu@24.04"),
        ],
        "ubuntu@24.04",
    ),
    (
        &["debian"],
        &[
            (&["debian11", "debian-11", "debian_11", "bullseye"], "debian@11"),
            (&["debian12", "debian-12", "debian_12", "bookworm"], "debian@12"),
            (&["debian13", "debian-13", "debian_13", "trixie"], "debian@13"),
        ],
        "debian@12",
    ),
    (&["rocky"], &[], "rocky@9"),
    (&["alma"], &[], "alma@9"),
    (&["alpine"], &[(&["3.19", "319"], "alpine@3.19"), (&["3.24", "324"], "alpine@3.24")], "alpine"),
    (&["fedora"], &[], "fedora"),
    (&["centos"], &[], "centos"),
];

/// The OS `name` says: the family it names and the version it says, else that family's default;
/// Debian when it names none.
pub(crate) fn read(name: &str) -> &'static str {
    let l = name.to_ascii_lowercase();
    FAMILIES
        .iter()
        .find(|(words, ..)| words.iter().any(|w| l.contains(w)))
        .map_or("debian@12", |(_, versions, default)| {
            versions
                .iter()
                .find(|(words, _)| words.iter().any(|w| l.contains(w)))
                .map_or(*default, |(_, os)| *os)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_says_its_family_and_version() {
        for (name, os) in [
            ("ubuntu/jammy64", "ubuntu@22.04"),
            ("ubuntu/focal64", "ubuntu@20.04"),
            ("bento/ubuntu-18.04", "ubuntu@18.04"),
            ("generic/ubuntu2204", "ubuntu@22.04"),
            ("0001-com-ubuntu-server-jammy 22_04-lts-gen2", "ubuntu@22.04"),
            ("ubuntu/noble64", "ubuntu@24.04"),
            ("someone/ubuntu", "ubuntu@24.04"),
            ("debian/bullseye64", "debian@11"),
            ("generic/debian11", "debian@11"),
            ("debian/trixie64", "debian@13"),
            ("debian/bookworm64", "debian@12"),
            ("gusztavvargadr/windows-10", "windows@10"),
            ("gusztavvargadr/windows-11", "windows@11"),
            ("gusztavvargadr/windows-server-2016", "windows-server@2016"),
            ("gusztavvargadr/windows-server-2025", "windows-server@2025"),
            ("jborean93/WindowsServer2012R2", "windows-server@2012r2"),
            ("2016-Datacenter", "windows-server@2016"),
            ("centos/7", "centos"),
            ("generic/rocky9", "rocky@9"),
            ("almalinux/9", "alma@9"),
            ("generic/alpine319", "alpine@3.19"),
            ("generic/fedora42", "fedora"),
            ("kalilinux/rolling", "kali"),
            ("someone/thing", "debian@12"),
            // Cloud images, as Terraform names them.
            ("ami-0windows2022 windows", "windows-server@2022"),
            ("2019-Datacenter", "windows-server@2019"),
            ("ubuntu-2404-lts", "ubuntu@24.04"),
            ("ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*", "ubuntu@22.04"),
            ("debian-11", "debian@11"),
        ] {
            assert_eq!(read(name), os, "{name}");
        }
    }

    #[test]
    fn every_os_a_name_is_read_as_is_one_the_spec_takes() {
        let named = FAMILIES
            .iter()
            .flat_map(|(_, versions, default)| versions.iter().map(|(_, os)| *os).chain([*default]));
        for os in named.chain(["debian@12"]) {
            assert!(isoloom_spec::os::resolve(os).is_ok(), "{os}");
        }
    }
}
