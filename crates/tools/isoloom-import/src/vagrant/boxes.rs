//! What a machine's box says: its OS, whether Isoloom would pick that box itself, and the
//! memory a provider block gives it.

use serde_json::Value;

use super::recording::text;
use crate::quantity::{Decimal, Rounding};

/// The OS a box runs, and whether the box is the one Isoloom would pick for it (otherwise the
/// draft keeps it as an override).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BoxOs {
    /// The OS, as the spec names it (`debian@12`).
    pub(super) os: &'static str,
    /// Whether Isoloom's own box for that OS is this one.
    pub(super) builtin: bool,
}

/// The OS of a box by its name.
pub(super) fn os_of(name: &str) -> BoxOs {
    let l = name.to_ascii_lowercase();
    let (os, builtin) = match l.as_str() {
        "bento/debian-12" | "generic/debian12" => ("debian@12", true),
        "bento/ubuntu-24.04" => ("ubuntu@24.04", true),
        "kalilinux/rolling" => ("kali", true),
        "stefanscherer/windows_2019" => ("windows-server@2019", true),
        _ if l.contains("windows") && l.contains("2022") => ("windows-server@2022", false),
        _ if l.contains("windows") && (l.contains("11") || l.contains("win11")) => ("windows@11", false),
        _ if l.contains("windows") || l.contains("win2019") => ("windows-server@2019", false),
        _ if l.contains("kali") => ("kali", false),
        _ if l.contains("ubuntu") => ("ubuntu@24.04", false),
        _ => ("debian@12", false),
    };
    BoxOs { os, builtin }
}

/// Memory like `4096`, `"4G"`, `"2048M"` in MB (a bare number is MB; gigabytes rounded down).
pub(super) fn memory_mb(v: Option<&Value>) -> Option<u32> {
    let size = text(v)?.trim().to_ascii_uppercase();
    if let Some(gb) = size.strip_suffix('G') {
        gb.parse::<Decimal>().ok()?.scaled(1024, 1, Rounding::Down)
    } else {
        size.strip_suffix('M').unwrap_or(&size).parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_box_is_isoloom_own_and_another_is_read_by_its_name() {
        assert_eq!(
            os_of("bento/debian-12"),
            BoxOs {
                os: "debian@12",
                builtin: true
            }
        );
        assert_eq!(
            os_of("gusztavvargadr/windows-server-2022"),
            BoxOs {
                os: "windows-server@2022",
                builtin: false
            }
        );
        assert_eq!(
            os_of("ubuntu/jammy64"),
            BoxOs {
                os: "ubuntu@24.04",
                builtin: false
            }
        );
        assert_eq!(
            os_of("someone/thing"),
            BoxOs {
                os: "debian@12",
                builtin: false
            }
        );
    }

    #[test]
    fn memory_is_read_in_megabytes_or_gigabytes() {
        assert_eq!(memory_mb(Some(&Value::from(4096))), Some(4096));
        assert_eq!(memory_mb(Some(&Value::from("4G"))), Some(4096));
        assert_eq!(memory_mb(Some(&Value::from("1.5g"))), Some(1536));
        assert_eq!(memory_mb(Some(&Value::from("2048M"))), Some(2048));
        assert_eq!(memory_mb(Some(&Value::from("lots"))), None);
    }
}
