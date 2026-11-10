//! A bandwidth as `tc` writes a rate (`10mbit`, `512kbit`, `2mbps`): a number then one of eight
//! units, read by one grammar for a machine's `rate_limit` and a network's `tc.rate`.

use std::fmt;

/// The units of a rate, as `tc` reads them, each with the bytes per second one of it is: `bit`
/// units count bits, `bps` units count bytes; k, m and g are powers of 1000.
const UNITS: [(&str, f64); 8] = [
    ("bit", 0.125),
    ("kbit", 125.0),
    ("mbit", 125_000.0),
    ("gbit", 125_000_000.0),
    ("bps", 1.0),
    ("kbps", 1e3),
    ("mbps", 1e6),
    ("gbps", 1e9),
];

/// A rate as `tc` writes it (`10mbit`), parsed once: it holds only a number at or above zero and
/// one of `tc`'s units. Its text form ([`fmt::Display`]) is the text it was read from.
#[derive(Debug, Clone, PartialEq)]
pub struct Rate {
    /// The text it was read from.
    text: String,
    /// Its number.
    value: f64,
    /// The bytes per second one of its unit is.
    unit_bytes: f64,
}

/// Why a text isn't a [`Rate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RateError {
    /// Below zero (`-5mbit`), which netem refuses when it runs.
    #[error("a rate can't be negative")]
    Negative,
    /// Not a number then a unit `tc` reads.
    #[error("isn't a rate like 10mbit or 512kbit")]
    Malformed,
}

impl Rate {
    /// Reads a rate: a number (`10`, `1.5`), then `bit`, `kbit`, `mbit`, `gbit`, `bps`, `kbps`,
    /// `mbps` or `gbps`.
    ///
    /// # Errors
    ///
    /// [`RateError::Negative`] for a rate below zero, [`RateError::Malformed`] for any other
    /// text that isn't one.
    ///
    /// # Example
    ///
    /// ```
    /// use isoloom_spec::net::{Rate, RateError};
    ///
    /// assert_eq!(Rate::parse("10mbit").map(|r| r.megabytes_per_second()), Ok(1.25));
    /// assert_eq!(Rate::parse("-10mbit"), Err(RateError::Negative));
    /// assert_eq!(Rate::parse("10mb"), Err(RateError::Malformed));
    /// ```
    pub fn parse(text: &str) -> Result<Rate, RateError> {
        if text.trim_start().starts_with('-') {
            return Err(RateError::Negative);
        }
        let digits = text.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        let unit = text.strip_prefix(digits).unwrap_or_default();
        let value: f64 = digits.parse().map_err(|_| RateError::Malformed)?;
        let (_, unit_bytes) = UNITS.into_iter().find(|(u, _)| *u == unit).ok_or(RateError::Malformed)?;
        Ok(Rate {
            text: text.to_owned(),
            value,
            unit_bytes,
        })
    }

    /// The rate in megabytes per second, as Proxmox's network devices take a rate limit.
    #[must_use]
    pub fn megabytes_per_second(&self) -> f64 {
        // As `tc` counts it: a bit unit's bits are an eighth of a byte each.
        self.value * self.unit_bytes / 1e6
    }
}

impl fmt::Display for Rate {
    /// The rate as written (`10mbit`): what `tc` reads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{Rate, RateError, UNITS};

    fn mbs(text: &str) -> Option<f64> {
        Rate::parse(text).ok().map(|r| r.megabytes_per_second())
    }

    proptest! {
        #[test]
        fn every_unit_counts_as_tc_counts_it(value in 0_u32..100_000, unit in 0..UNITS.len()) {
            let (name, bytes) = UNITS[unit];
            let got = mbs(&format!("{value}{name}")).unwrap();
            let want = f64::from(value) * bytes / 1e6;
            prop_assert!((got - want).abs() <= want.abs() * 1e-12, "{value}{name}: {got} vs {want}");
        }

        #[test]
        fn a_number_without_a_known_unit_is_no_rate(value in 0_u32..1000, unit in "[a-z]{0,6}") {
            prop_assume!(!UNITS.iter().any(|(u, _)| *u == unit));
            prop_assert_eq!(Rate::parse(&format!("{value}{unit}")), Err(RateError::Malformed));
        }

        #[test]
        fn any_text_reads_without_panicking_and_writes_back_as_read(text in "\\PC{0,16}") {
            if let Ok(rate) = Rate::parse(&text) {
                prop_assert_eq!(rate.to_string(), text);
            }
        }
    }

    #[test]
    fn rates_become_megabytes_per_second() {
        assert_eq!(mbs("10mbit"), Some(1.25));
        assert_eq!(mbs("2mbps"), Some(2.0));
        assert_eq!(mbs("512kbit"), Some(0.064));
        assert_eq!(mbs("1.5mbit"), Some(0.1875));
    }

    #[test]
    fn a_rate_below_zero_or_without_its_number_or_unit_is_refused() {
        assert_eq!(Rate::parse("-2mbit"), Err(RateError::Negative));
        assert_eq!(Rate::parse(" -2mbit"), Err(RateError::Negative));
        for text in ["", "ten", "mbit", "10", "10 mbit", "10Mbit", "fast"] {
            assert_eq!(Rate::parse(text), Err(RateError::Malformed), "{text}");
        }
    }
}
