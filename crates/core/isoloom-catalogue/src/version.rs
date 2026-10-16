//! Release versions and how a machine's `service: <entry>@<version>` picks one (design §4.3): a
//! version names the release it equals, else the newest release it is a prefix of, on dot
//! boundaries (`postgres@16` is the newest 16.x, never 160); no version means the entry's
//! default. The release picked is recorded in `resolved.json`, so it moves only with an Isoloom
//! upgrade.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::Error;

/// A version: parts of letters and digits joined by dots (`1.30.6`, `16`, `2.4`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version(String);

/// The version rule, as a JSON Schema pattern.
const PATTERN: &str = "^[0-9A-Za-z]+(\\.[0-9A-Za-z]+)*$";

impl Version {
    /// The version as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Its parts, in order (`1`, `30`, `6`).
    pub fn parts(&self) -> impl Iterator<Item = &str> {
        self.0.split('.')
    }

    /// Whether `prefix` is this version or a prefix of it on dot boundaries (`16` of `16.10`,
    /// not of `160`).
    #[must_use]
    pub fn starts_with(&self, prefix: &Version) -> bool {
        let mut mine = self.parts();
        prefix.parts().all(|p| mine.next() == Some(p))
    }
}

impl FromStr for Version {
    type Err = Error;

    /// A version, when it is dot-separated parts of letters and digits.
    ///
    /// # Errors
    ///
    /// [`Error::Version`]: an empty part, or a character other than letters, digits and dots.
    fn from_str(text: &str) -> Result<Version, Error> {
        let ok = text.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()));
        if ok {
            Ok(Version(text.to_owned()))
        } else {
            Err(Error::Version(text.to_owned()))
        }
    }
}

impl Ord for Version {
    /// Part by part: numbers as numbers, other parts as text, a number before text; a version
    /// that goes on after another's last part is the newer (`1.30.6` after `1.30`).
    fn cmp(&self, other: &Version) -> Ordering {
        let mut a = self.parts();
        let mut b = other.parts();
        loop {
            match (a.next(), b.next()) {
                (None, None) => return Ordering::Equal,
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
                (Some(x), Some(y)) => match part_order(x, y) {
                    Ordering::Equal => {}
                    order => return order,
                },
            }
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Two parts in order: numbers by value (then by their digits, so `01` and `1` differ), text by
/// text, a number before text.
fn part_order(x: &str, y: &str) -> Ordering {
    let number = |p: &str| p.chars().all(|c| c.is_ascii_digit()).then(|| p.trim_start_matches('0').len());
    match (number(x), number(y)) {
        (Some(lx), Some(ly)) => lx
            .cmp(&ly)
            .then_with(|| x.trim_start_matches('0').cmp(y.trim_start_matches('0')))
            .then_with(|| x.cmp(y)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => x.cmp(y),
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Version, D::Error> {
        // A YAML author may write `16` or `8.4` unquoted: a number, read as its text.
        let text = match serde_json::Value::deserialize(d)? {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            _ => return Err(serde::de::Error::custom("a version is text: `1.30.6`")),
        };
        text.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Version {
    fn schema_name() -> Cow<'static, str> {
        "Version".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "pattern": PATTERN, "description": "A version: parts of letters and digits joined by dots (`1.30.6`)." })
    }
}

/// The release `asked` picks among `versions`: the one it equals, else the newest it is a
/// prefix of; `None` when it picks none.
#[must_use]
pub fn pick<'a>(versions: impl IntoIterator<Item = &'a Version>, asked: &Version) -> Option<&'a Version> {
    let matching: Vec<&Version> = versions.into_iter().filter(|v| v.starts_with(asked)).collect();
    matching.iter().copied().find(|v| *v == asked).or_else(|| matching.into_iter().max())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn v(text: &str) -> Version {
        text.parse().unwrap()
    }

    #[test]
    fn a_prefix_picks_the_newest_release_it_starts_on_dot_boundaries() {
        let releases = [v("16.9"), v("16.10"), v("17.6"), v("160.1")];
        assert_eq!(pick(&releases, &v("16")), Some(&v("16.10")));
        assert_eq!(pick(&releases, &v("16.9")), Some(&v("16.9")));
        assert_eq!(pick(&releases, &v("17")), Some(&v("17.6")));
        assert_eq!(pick(&releases, &v("1")), None, "never 16 nor 160 for `1`");
        assert_eq!(pick(&releases, &v("18")), None);
    }

    #[test]
    fn a_release_equal_to_the_version_asked_wins_over_longer_ones() {
        let releases = [v("1.30"), v("1.30.6")];
        assert_eq!(pick(&releases, &v("1.30")), Some(&v("1.30")));
        assert_eq!(pick(&releases, &v("1")), Some(&v("1.30.6")));
    }

    #[test]
    fn parts_compare_as_numbers_then_text() {
        assert!(v("1.30.10") > v("1.30.9"));
        assert!(v("1.30.6") > v("1.30"));
        assert!(v("2") > v("1.99"));
        assert!(v("8.0") < v("8.0rc1"), "a number part sorts before a text part");
    }

    #[test]
    fn a_version_is_parts_of_letters_and_digits() {
        assert!("1.30.6".parse::<Version>().is_ok() && "16".parse::<Version>().is_ok());
        for wrong in ["", "1..2", ".1", "1.", "1.2-3", "1 2"] {
            assert!(wrong.parse::<Version>().is_err(), "{wrong}");
        }
        assert_eq!(serde_yaml_ng::from_str::<Version>("8.4").unwrap(), v("8.4"));
        assert_eq!(serde_yaml_ng::from_str::<Version>("16").unwrap(), v("16"));
    }

    fn version() -> impl Strategy<Value = Version> {
        proptest::collection::vec("[0-9]{1,3}|[a-z]{1,3}", 1..4).prop_map(|parts| v(&parts.join(".")))
    }

    proptest! {
        /// A written version reads back as itself, and anything else is refused.
        #[test]
        fn the_parser_keeps_what_it_accepts(text in "[0-9a-z.+-]{0,10}") {
            match text.parse::<Version>() {
                Ok(parsed) => prop_assert_eq!(parsed.as_str(), text.as_str()),
                Err(_) => prop_assert!(text.split('.').any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_alphanumeric()))),
            }
        }

        /// What a version picks starts with it, and no release it starts is newer.
        #[test]
        fn a_pick_is_the_newest_match(releases in proptest::collection::vec(version(), 0..8), asked in version()) {
            match pick(&releases, &asked) {
                Some(picked) => {
                    prop_assert!(picked.starts_with(&asked));
                    if picked != &asked {
                        prop_assert!(releases.iter().filter(|r| r.starts_with(&asked)).all(|r| r <= picked));
                    }
                }
                None => prop_assert!(releases.iter().all(|r| !r.starts_with(&asked))),
            }
        }

        /// The order is total and agrees with equality.
        #[test]
        fn the_order_is_consistent(a in version(), b in version()) {
            prop_assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
            prop_assert_eq!(a.cmp(&b) == Ordering::Equal, a == b);
        }
    }
}
