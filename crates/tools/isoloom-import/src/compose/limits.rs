//! Reading a service's limits: memory sizes (`512m`, `1.5g`, bytes) and CPUs, in the spec's
//! whole units.

use serde_yaml_ng::Value;

use super::text::str_of;
use crate::quantity::{Decimal, Rounding};

/// Bytes in a megabyte.
const MB: u128 = 1024 * 1024;

/// Memory like `512m`, `1g`, `1.5G`, `268435456` (bytes) in MB, rounded up; none under 1 MB.
pub(super) fn memory_mb(text: &str) -> Option<u32> {
    let text = text.trim().to_ascii_lowercase();
    let text = text.strip_suffix('b').unwrap_or(&text);
    // The size times `times` per `per` is in MB.
    let (number, times, per) = if let Some(n) = text.strip_suffix('k') {
        (n, 1, 1024)
    } else if let Some(n) = text.strip_suffix('m') {
        (n, 1, 1)
    } else if let Some(n) = text.strip_suffix('g') {
        (n, 1024, 1)
    } else {
        (text, 1, MB)
    };
    let size: Decimal = number.parse().ok()?;
    if size.below_one(times, per) {
        return None;
    }
    size.scaled(times, per, Rounding::Up)
}

/// CPUs as a whole number, at least 1 (`"1.5"` -> 2).
pub(super) fn cpus(v: &Value) -> Option<u32> {
    let cpus: Decimal = str_of(v)?.parse().ok()?;
    cpus.scaled(1, 1, Rounding::Up).map(|c| c.max(1))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn memory_is_read_in_every_unit() {
        assert_eq!(memory_mb("512m"), Some(512));
        assert_eq!(memory_mb("256M"), Some(256));
        assert_eq!(memory_mb("1.5G"), Some(1536));
        assert_eq!(memory_mb("1gb"), Some(1024));
        assert_eq!(memory_mb("268435456"), Some(256));
        assert_eq!(memory_mb("1536k"), Some(2), "rounded up");
        assert_eq!(memory_mb("512k"), None, "under 1 MB");
        assert_eq!(memory_mb("lots"), None);
    }

    #[test]
    fn cpus_are_rounded_up_to_one_at_least() {
        assert_eq!(cpus(&Value::String("1.5".to_owned())), Some(2));
        assert_eq!(cpus(&Value::String("0.25".to_owned())), Some(1));
        assert_eq!(cpus(&Value::Number(4.into())), Some(4));
    }

    proptest! {
        #[test]
        fn megabytes_read_as_themselves_in_every_unit(mb in 1_u32..4_000_000) {
            prop_assert_eq!(memory_mb(&format!("{mb}m")), Some(mb));
            prop_assert_eq!(memory_mb(&format!("{}k", u64::from(mb) * 1024)), Some(mb));
            prop_assert_eq!(memory_mb(&format!("{}", u64::from(mb) * 1024 * 1024)), Some(mb));
        }
    }
}
