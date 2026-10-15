//! Sizes from the sources (`1.5` CPUs, `1.5g` of memory): decimal text read exactly, then scaled
//! and rounded into a whole count, with no float on the way.

use std::str::FromStr;

/// A non-negative decimal number as written: `units` x 10^-`scale` (`1.5` is 15 and 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Decimal {
    units: u128,
    scale: u32,
}

/// Which way a scaled size that isn't whole goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rounding {
    /// To the whole number below.
    Down,
    /// To the whole number above.
    Up,
}

/// Text that isn't a non-negative decimal number (`1`, `1.5`, `.5`, `+2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotDecimal;

impl FromStr for Decimal {
    type Err = NotDecimal;

    fn from_str(text: &str) -> Result<Decimal, NotDecimal> {
        let text = text.strip_prefix('+').unwrap_or(text);
        let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
        let digits = || whole.chars().chain(fraction.chars());
        if digits().next().is_none() || !digits().all(|c| c.is_ascii_digit()) {
            return Err(NotDecimal);
        }
        let units = digits().try_fold(0_u128, |n, c| n.checked_mul(10)?.checked_add(u128::from(c.to_digit(10)?)));
        let scale = u32::try_from(fraction.len()).map_err(|_| NotDecimal)?;
        // A number too long to hold exactly (39 digits and more) isn't a size.
        Ok(Decimal {
            units: units.ok_or(NotDecimal)?,
            scale,
        })
    }
}

impl Decimal {
    /// The number times `times`, divided by `per`: as the exact fraction (numerator,
    /// denominator), when it fits.
    fn fraction(self, times: u128, per: u128) -> Option<(u128, u128)> {
        Some((self.units.checked_mul(times)?, per.checked_mul(10_u128.checked_pow(self.scale)?)?))
    }

    /// The number times `times`, divided by `per`, rounded: a whole count when it fits a `u32`.
    pub(crate) fn scaled(self, times: u128, per: u128, rounding: Rounding) -> Option<u32> {
        let (top, bottom) = self.fraction(times, per)?;
        let whole = top.checked_div(bottom)?;
        let whole = match rounding {
            Rounding::Up if top % bottom != 0 => whole + 1,
            Rounding::Up | Rounding::Down => whole,
        };
        u32::try_from(whole).ok()
    }

    /// Whether the number times `times`, divided by `per`, is less than one.
    pub(crate) fn below_one(self, times: u128, per: u128) -> bool {
        self.fraction(times, per).is_none_or(|(top, bottom)| top < bottom)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn decimal(text: &str) -> Result<Decimal, String> {
        text.parse().map_err(|_| format!("`{text}` isn't a decimal"))
    }

    #[test]
    fn a_decimal_is_read_exactly() -> Result<(), String> {
        assert_eq!(decimal("1.5")?, Decimal { units: 15, scale: 1 });
        assert_eq!(decimal(".5")?, Decimal { units: 5, scale: 1 });
        assert_eq!(decimal("+2.")?, Decimal { units: 2, scale: 0 });
        for bad in ["", ".", "-1", "1e3", "1.2.3", " 1", "inf"] {
            assert!(bad.parse::<Decimal>().is_err(), "{bad}");
        }
        Ok(())
    }

    #[test]
    fn a_size_is_rounded_the_way_asked() -> Result<(), String> {
        assert_eq!(decimal("1.5")?.scaled(1, 1, Rounding::Up), Some(2));
        assert_eq!(decimal("1.5")?.scaled(1, 1, Rounding::Down), Some(1));
        assert_eq!(decimal("1.5")?.scaled(1024, 1, Rounding::Down), Some(1536));
        assert_eq!(decimal("512")?.scaled(1, 1024, Rounding::Up), Some(1));
        assert!(decimal("512")?.below_one(1, 1024));
        assert!(!decimal("1024")?.below_one(1, 1024));
        assert_eq!(decimal("99999999999")?.scaled(1, 1, Rounding::Up), None, "more than a u32");
        Ok(())
    }

    proptest! {
        #[test]
        fn a_decimal_scales_as_its_whole_and_fraction_say(whole in 0_u32..100_000, fraction in 0_u32..1000, times in 1_u128..2048) {
            let d = decimal(&format!("{whole}.{fraction:03}")).map_err(TestCaseError::fail)?;
            let exact = (u128::from(whole) * 1000 + u128::from(fraction)) * times;
            let down = exact / 1000;
            let up = down + u128::from(exact % 1000 != 0);
            prop_assert_eq!(d.scaled(times, 1, Rounding::Down).map(u128::from), u32::try_from(down).ok().map(u128::from));
            prop_assert_eq!(d.scaled(times, 1, Rounding::Up).map(u128::from), u32::try_from(up).ok().map(u128::from));
            prop_assert_eq!(d.below_one(times, 1), exact < 1000);
        }

        #[test]
        fn text_that_isnt_a_plain_decimal_is_refused(text in "[0-9]*[^0-9.+][0-9.]*") {
            prop_assert!(text.parse::<Decimal>().is_err(), "{text}");
        }
    }
}
