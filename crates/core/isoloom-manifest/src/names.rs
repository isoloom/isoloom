//! The names, ids and versions of a manifest: each a newtype that only holds a value matching its
//! pattern, so a manifest that parsed has no malformed name anywhere. The JSON Schema carries the
//! same pattern.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{Error, Result};

/// A newtype over `String` that holds only values `$check` accepts; `$pattern` is the same rule
/// as a JSON Schema pattern, `$rule` as a reader would fix the value.
///
/// The default form derives a lexicographic [`Ord`], right for names. Prefix the invocation with
/// `custom_ord` for a type whose order isn't its text order (a [`Version`] sorts by its numbers,
/// not its characters); the caller then writes the [`Ord`]/[`PartialOrd`] impls.
macro_rules! pattern_type {
    ($(#[$doc:meta])* $ty:ident, $what:literal, $pattern:literal, $rule:literal, $check:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $ty(String);

        pattern_type!(@shared $ty, $what, $pattern, $rule, $check);
    };
    (custom_ord $(#[$doc:meta])* $ty:ident, $what:literal, $pattern:literal, $rule:literal, $check:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $ty(String);

        pattern_type!(@shared $ty, $what, $pattern, $rule, $check);
    };
    (@shared $ty:ident, $what:literal, $pattern:literal, $rule:literal, $check:expr) => {
        impl $ty {
            /// The value, when it matches the pattern.
            ///
            /// # Errors
            ///
            /// [`Error::Pattern`], naming the value and the rule it breaks.
            pub fn parse(value: &str) -> Result<$ty> {
                $ty::try_from(value.to_owned())
            }

            /// The value as written.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $ty {
            type Error = Error;

            fn try_from(value: String) -> Result<$ty> {
                let check: fn(&str) -> bool = $check;
                if check(&value) {
                    Ok($ty(value))
                } else {
                    Err(Error::Pattern { what: $what, value, rule: $rule })
                }
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
                $ty::try_from(String::deserialize(d)?).map_err(serde::de::Error::custom)
            }
        }

        impl FromStr for $ty {
            type Err = Error;

            fn from_str(value: &str) -> Result<$ty> {
                $ty::parse(value)
            }
        }

        impl From<$ty> for String {
            fn from(value: $ty) -> String {
                value.0
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl JsonSchema for $ty {
            fn schema_name() -> Cow<'static, str> {
                stringify!($ty).into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "string", "pattern": $pattern, "description": $rule })
            }
        }
    };
}

pattern_type!(
    /// An environment's name: kebab-case (`flag-portal`, `shop-2`), as the spec's `name:`.
    EnvironmentName,
    "environment name",
    "^[a-z0-9]+(-[a-z0-9]+)*$",
    "lowercase letters and digits in words joined by single dashes (`flag-portal`)",
    |v| v.split('-').all(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
);

pattern_type!(
    /// The name of a machine, a network, a volume or a check runner.
    Name,
    "name",
    "^[A-Za-z0-9][A-Za-z0-9_.-]*$",
    "a letter or digit, then letters, digits, `_`, `.` or `-` (`web`, `isoloom-check-db`)",
    |v| first_then(v, |c| c.is_ascii_alphanumeric(), |c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
);

pattern_type!(
    /// A port's name, unique on its machine (`portal`, `dns-udp`), as the spec's `ports:` key. It
    /// has no dot, so `${published.<machine>.<port>}` reads one way only.
    PortName,
    "port name",
    "^[a-z][a-z0-9-]{0,30}$",
    "a lowercase letter, then up to 30 lowercase letters, digits or `-` (`portal`, `dns-udp`)",
    |v| v.len() <= 31 && first_then(v, |c| c.is_ascii_lowercase(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
);

pattern_type!(
    /// An environment variable's name, and a param's.
    EnvName,
    "environment variable name",
    "^[A-Za-z_][A-Za-z0-9_]*$",
    "a letter or `_`, then letters, digits or `_` (`CTF_FLAG_MAIN`)",
    |v| first_then(v, |c| c.is_ascii_alphabetic() || c == '_', |c| c.is_ascii_alphanumeric() || c == '_')
);

pattern_type!(
    /// A slot's name (an input as the spec names it, `CTF_FLAG_MAIN`; a secret, `db-password`),
    /// and a Terraform output's.
    Identifier,
    "identifier",
    "^[A-Za-z_][A-Za-z0-9_-]*$",
    "a letter or `_`, then letters, digits, `_` or `-` (`CTF_FLAG_MAIN`, `db-password`)",
    |v| first_then(v, |c| c.is_ascii_alphabetic() || c == '_', |c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
);

pattern_type!(
    /// A step's id, unique within its list (`up`, `job-web-init-1`).
    StepId,
    "step id",
    "^[a-z][a-z0-9-]*$",
    "a lowercase letter, then lowercase letters, digits or `-` (`pin-ports`)",
    |v| first_then(v, |c| c.is_ascii_lowercase(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
);

pattern_type!(
    /// A `snake_case` name: an output's, a Terraform variable's, an action argument's.
    SnakeName,
    "snake_case name",
    "^[a-z][a-z0-9_]*$",
    "a lowercase letter, then lowercase letters, digits or `_` (`web_portal`)",
    |v| first_then(v, |c| c.is_ascii_lowercase(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
);

pattern_type!(
    /// A native tool's or a Vagrant plugin's name (`docker-compose`, `vagrant-libvirt`).
    ToolName,
    "tool name",
    "^[a-z][a-z0-9_.-]*$",
    "a lowercase letter, then lowercase letters, digits, `_`, `.` or `-` (`docker-compose`)",
    |v| first_then(v, |c| c.is_ascii_lowercase(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-'))
);

pattern_type!(
    /// An operating system's id (`debian-12`, `windows-server-2019`, `alpine-3.19`).
    OsName,
    "operating system id",
    "^[a-z0-9][a-z0-9.-]*$",
    "a lowercase letter or digit, then lowercase letters, digits, `.` or `-` (`debian-12`)",
    |v| first_then(v, |c| c.is_ascii_lowercase() || c.is_ascii_digit(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-'))
);

pattern_type!(
    custom_ord
    /// A version: `major.minor.patch`, with an optional pre-release (`0.10.1`, `2.17.0`). Its
    /// order is semantic, not textual: it sorts by the numbers (`2.9.0 < 2.14.0`), and a
    /// pre-release sorts before the release it leads to (`1.0.0-rc.1 < 1.0.0`).
    Version,
    "version",
    "^[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?$",
    "three numbers joined by dots, optionally `-` and a pre-release (`2.17.0`, `1.0.0-rc.1`)",
    is_version
);

impl Ord for Version {
    /// Semver precedence: the three numbers compared as numbers, then a pre-release before the
    /// release (semver §11). A value only ever holds a string that matched the version pattern, so
    /// the three numeric parts are present and all digits.
    fn cmp(&self, other: &Version) -> std::cmp::Ordering {
        version_cmp(&self.0, &other.0)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pattern_type!(
    /// A container image pinned by its digest (`hashicorp/terraform:1.9.8@sha256:…`).
    PinnedImage,
    "pinned image",
    "^[^@\\s]+@sha256:[0-9a-f]{64}$",
    "an image reference, then `@sha256:` and the 64 lowercase hex digits of its digest",
    is_pinned
);

/// Whether `v` is a first character `first` accepts followed by characters `rest` accepts.
fn first_then(v: &str, first: fn(char) -> bool, rest: fn(char) -> bool) -> bool {
    let mut chars = v.chars();
    chars.next().is_some_and(first) && chars.all(rest)
}

fn is_version(v: &str) -> bool {
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() == 3
        && numbers.iter().all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        && pre.is_none_or(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-')))
}

/// Two valid version strings in semver precedence: numbers as numbers, then pre-release before
/// release. Both are strings [`is_version`] accepts, so each has three all-digit core parts.
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a_core, a_pre) = a.split_once('-').map_or((a, None), |(c, p)| (c, Some(p)));
    let (b_core, b_pre) = b.split_once('-').map_or((b, None), |(c, p)| (c, Some(p)));
    let core = a_core
        .split('.')
        .zip(b_core.split('.'))
        .map(|(x, y)| numeric_cmp(x, y))
        .find(|o| *o != Ordering::Equal)
        .unwrap_or(Ordering::Equal);
    core.then_with(|| pre_release_cmp(a_pre, b_pre))
}

/// A pre-release sorts before the release (`None`), and two pre-releases by their dot-identifiers.
fn pre_release_cmp(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => identifiers_cmp(x, y),
    }
}

/// Dot-separated pre-release identifiers (semver §11.4): field by field, a shorter run losing
/// when its fields tie.
fn identifiers_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut xs = a.split('.');
    let mut ys = b.split('.');
    loop {
        match (xs.next(), ys.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match identifier_cmp(x, y) {
                Ordering::Equal => {}
                order => return order,
            },
        }
    }
}

/// One pre-release identifier: numbers as numbers, text as text (ASCII), a number before text.
fn identifier_cmp(x: &str, y: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    match (digits(x), digits(y)) {
        (true, true) => numeric_cmp(x, y),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => x.cmp(y),
    }
}

/// Two all-digit strings by their value, without parsing (so any length is safe): the shorter run
/// of significant digits is smaller, then the digits themselves settle equal lengths.
fn numeric_cmp(x: &str, y: &str) -> std::cmp::Ordering {
    let xs = x.trim_start_matches('0');
    let ys = y.trim_start_matches('0');
    xs.len().cmp(&ys.len()).then_with(|| xs.cmp(ys))
}

fn is_pinned(v: &str) -> bool {
    v.split_once("@sha256:").is_some_and(|(image, digest)| {
        !image.is_empty()
            && !image.chars().any(|c| c == '@' || c.is_whitespace())
            && digest.len() == 64
            && digest.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn names_follow_their_patterns() {
        assert!(EnvironmentName::parse("flag-portal").is_ok());
        assert!(EnvironmentName::parse("Flag").is_err());
        assert!(EnvironmentName::parse("a--b").is_err());
        assert!(Name::parse("isoloom-check-db").is_ok());
        assert!(Name::parse("-web").is_err());
        assert!(EnvName::parse("CTF_FLAG_MAIN").is_ok());
        assert!(EnvName::parse("ctf_flag").is_ok());
        assert!(EnvName::parse("1FLAG").is_err());
        assert!(Identifier::parse("db-password").is_ok());
        assert!(StepId::parse("job-web-init-1").is_ok());
        assert!(StepId::parse("Up").is_err());
        assert!(SnakeName::parse("web_portal").is_ok());
        assert!(SnakeName::parse("web-portal").is_err());
        assert!(OsName::parse("alpine-3.19").is_ok());
    }

    #[test]
    fn a_port_name_is_short_lowercase_and_has_no_dot() {
        let longest = format!("p{}", "0".repeat(30));
        for ok in ["portal", "dns-udp", "a", longest.as_str()] {
            assert!(PortName::parse(ok).is_ok(), "{ok}");
        }
        let too_long = format!("p{}", "0".repeat(31));
        for bad in ["", "Portal", "1st", "-a", "a.b", "a_b", too_long.as_str()] {
            assert!(PortName::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn versions_are_three_numbers_and_a_pre_release() {
        for ok in ["0.10.1", "2.17.0", "1.0.0-rc.1"] {
            assert!(Version::parse(ok).is_ok(), "{ok}");
        }
        for bad in ["1.0", "1.0.0-", "v1.0.0", "1..0"] {
            assert!(Version::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_version_sorts_by_its_numbers_not_its_characters() {
        let v = |s: &str| Version::parse(s).unwrap();
        // The lexicographic trap: as strings `2.9.0 > 2.14.0`, as versions it is less.
        assert!(v("2.9.0") < v("2.14.0"));
        assert!(v("1.2.0") < v("1.10.0"));
        assert!(v("2.14.1") > v("2.14.0"));
        assert!(v("10.0.0") > v("9.0.0"));
        assert_eq!(
            v("2.14.0").cmp(&v("02.014.00")),
            std::cmp::Ordering::Equal,
            "leading zeros don't change the value"
        );
    }

    #[test]
    fn a_pre_release_sorts_before_the_release_and_by_its_identifiers() {
        let v = |s: &str| Version::parse(s).unwrap();
        // SemVer §11: a pre-release is older than the release it leads to.
        assert!(v("1.0.0-rc.1") < v("1.0.0"));
        assert!(v("1.0.0-alpha") < v("1.0.0-alpha.1"), "fewer fields lose when they tie");
        assert!(v("1.0.0-alpha.1") < v("1.0.0-alpha.2"), "numeric fields compare as numbers");
        assert!(v("1.0.0-alpha.9") < v("1.0.0-alpha.11"));
        assert!(v("1.0.0-1") < v("1.0.0-alpha"), "a numeric field is older than a text one");
        assert!(v("1.0.0-alpha") < v("1.0.0-beta"));
    }

    fn version_text() -> impl Strategy<Value = String> {
        let core = "[0-9]{1,4}\\.[0-9]{1,4}\\.[0-9]{1,4}";
        prop_oneof![
            core,
            (core, "[0-9A-Za-z]{1,3}(\\.[0-9A-Za-z]{1,3}){0,2}").prop_map(|(c, pre)| format!("{c}-{pre}")),
        ]
    }

    proptest! {
        /// Whatever `version_text` emits parses, and ordering is total and matches equality.
        #[test]
        fn the_version_order_is_total_and_agrees_with_equality(a in version_text(), b in version_text()) {
            let (a, b) = (Version::parse(&a).unwrap(), Version::parse(&b).unwrap());
            prop_assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
            prop_assert_eq!(a.cmp(&b) == std::cmp::Ordering::Equal, a == b);
        }

        /// Bumping any one of the three numbers makes a strictly newer version.
        #[test]
        fn a_higher_number_is_a_newer_version(major in 0u16..500, minor in 0u16..500, patch in 0u16..500) {
            let v = |s: String| Version::parse(&s).unwrap();
            let lower = v(format!("{major}.{minor}.{patch}"));
            let bumped_major = v(format!("{}.{minor}.{patch}", major + 1));
            let bumped_minor = v(format!("{major}.{}.{patch}", minor + 1));
            let bumped_patch = v(format!("{major}.{minor}.{}", patch + 1));
            prop_assert!(lower < bumped_major);
            prop_assert!(lower < bumped_minor);
            prop_assert!(lower < bumped_patch);
        }
    }

    #[test]
    fn a_pinned_image_names_its_digest() {
        let digest = "a".repeat(64);
        assert!(PinnedImage::parse(&format!("hashicorp/terraform:1.9.8@sha256:{digest}")).is_ok());
        assert!(PinnedImage::parse("hashicorp/terraform:1.9.8").is_err());
        assert!(PinnedImage::parse(&format!("@sha256:{digest}")).is_err());
        assert!(PinnedImage::parse(&format!("hashicorp/terraform 1.9@sha256:{digest}")).is_err());
        assert!(PinnedImage::parse(&format!("a@b@sha256:{digest}")).is_err());
        assert!(PinnedImage::parse(&format!("a@sha256:{}", "A".repeat(64))).is_err());
        assert!(PinnedImage::parse(&format!("a@sha256:{}", "a".repeat(63))).is_err());
    }

    #[test]
    fn a_name_converts_from_and_to_its_text() {
        let name: Name = "web".parse().unwrap();
        assert_eq!((name.as_str(), name.to_string()), ("web", "web".to_owned()));
        assert_eq!(Name::try_from("db".to_owned()).map(String::from), Ok("db".to_owned()));
        assert_eq!(serde_json::to_string(&name).unwrap(), r#""web""#);
        assert!(
            serde_json::from_str::<Name>(r#""-web""#)
                .unwrap_err()
                .to_string()
                .contains("isn't a valid name")
        );
    }

    #[test]
    fn a_refusal_names_the_value_and_the_rule() {
        let err = StepId::parse("Up").unwrap_err();
        assert_eq!(err.code(), "pattern");
        assert!(err.to_string().starts_with("`Up` isn't a valid step id: a lowercase letter"), "{err}");
    }
}
