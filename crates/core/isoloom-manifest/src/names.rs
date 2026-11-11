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
        pattern_type!(@ordered None; $(#[$doc])* $ty, $what, $pattern, $rule, $check);
    };
    // A name the pattern can't bound: its longest, as the schema's `maxLength` (`$check` holds
    // the same bound).
    (max_len $max:expr; $(#[$doc:meta])* $ty:ident, $what:literal, $pattern:literal, $rule:literal, $check:expr) => {
        pattern_type!(@ordered Some($max); $(#[$doc])* $ty, $what, $pattern, $rule, $check);
    };
    (@ordered $max:expr; $(#[$doc:meta])* $ty:ident, $what:literal, $pattern:literal, $rule:literal, $check:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $ty(String);

        pattern_type!(@shared $ty, $what, $pattern, $rule, $check, $max);
    };
    (custom_ord $(#[$doc:meta])* $ty:ident, $what:literal, $pattern:literal, $rule:literal, $check:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $ty(String);

        pattern_type!(@shared $ty, $what, $pattern, $rule, $check, None);
    };
    (@shared $ty:ident, $what:literal, $pattern:literal, $rule:literal, $check:expr, $max:expr) => {
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
                let mut schema = json_schema!({ "type": "string", "pattern": $pattern, "description": $rule });
                let max: Option<usize> = $max;
                if let Some(max) = max {
                    schema.insert("maxLength".into(), max.into());
                }
                schema
            }
        }
    };
}

/// The longest kebab-case name ([`is_kebab`]): an environment's name is a DNS label and a
/// Kubernetes namespace.
pub const KEBAB_MAX: usize = 63;

/// Kebab case, as an environment's name (`my-env`, `shop-2`): lowercase letters and digits in
/// words joined by single dashes, at most [`KEBAB_MAX`] characters. The one rule the spec's
/// kebab-case names follow too.
#[must_use]
pub fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= KEBAB_MAX
        && s.split('-')
            .all(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

pattern_type!(
    max_len KEBAB_MAX;
    /// An environment's name: kebab-case (`my-env`, `shop-2`), as the spec's `name:` ([`is_kebab`]).
    EnvironmentName,
    "environment name",
    "^[a-z0-9]+(-[a-z0-9]+)*$",
    "lowercase letters and digits in words joined by single dashes (`my-env`), at most 63 characters",
    is_kebab
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
    "a letter or `_`, then letters, digits or `_` (`APP_TOKEN`)",
    |v| first_then(v, |c| c.is_ascii_alphabetic() || c == '_', |c| c.is_ascii_alphanumeric() || c == '_')
);

pattern_type!(
    /// A slot's name (an input as the spec names it, `APP_TOKEN`; a secret, `db-password`),
    /// and a Terraform output's.
    Identifier,
    "identifier",
    "^[A-Za-z_][A-Za-z0-9_-]*$",
    "a letter or `_`, then letters, digits, `_` or `-` (`APP_TOKEN`, `db-password`)",
    |v| first_then(v, |c| c.is_ascii_alphabetic() || c == '_', |c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
);

pattern_type!(
    /// A step's id, unique within its list (`up`, `job-isoloom-init-web-1`).
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

impl SnakeName {
    /// A name of the spec (`web-admin`, `Api.Url`) in snake case: lowercase, each character but
    /// a letter or a digit as `_` (`web_admin`, `api_url`).
    ///
    /// # Errors
    ///
    /// [`Error::Pattern`] when that isn't a snake-case name (a label starting with a digit).
    pub fn from_label(label: &str) -> Result<SnakeName> {
        let snake: String = label
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect();
        SnakeName::parse(&snake)
    }
}

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
    /// A Vagrant box's name: `<name>` or `<org>/<name>` (`bento/debian-12`), each part letters,
    /// digits, `.`, `_` and `-`, starting with a letter or a digit (so it is never read as an
    /// option).
    BoxName,
    "box name",
    "^[A-Za-z0-9][A-Za-z0-9._-]*(/[A-Za-z0-9][A-Za-z0-9._-]*)?$",
    "`<name>` or `<org>/<name>`, each a letter or digit, then letters, digits, `.`, `_` or `-` (`bento/debian-12`)",
    |v| {
        let part = |p: &str| first_then(p, |c| c.is_ascii_alphanumeric(), |c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        match v.split_once('/') {
            Some((org, name)) => part(org) && part(name),
            None => part(v),
        }
    }
);

pattern_type!(
    /// A Vagrant box's version, or a constraint on it as Vagrant reads one (`202510.26.0`,
    /// `>= 1.2, < 2`): never starting with `-` (so it is never read as an option).
    BoxVersion,
    "box version",
    "^[0-9A-Za-z<>=~!][0-9A-Za-z<>=~!., _-]*$",
    "a letter, digit or `<>=~!`, then letters, digits, spaces or `.,<>=~!_-` (`202510.26.0`, `>= 1.2`)",
    |v| first_then(v, |c| c.is_ascii_alphanumeric() || "<>=~!".contains(c), |c| c.is_ascii_alphanumeric() || "<>=~!., _-".contains(c))
);

impl BoxVersion {
    /// Whether it names one version (digits and dots: `202510.26.0`), not a constraint.
    #[must_use]
    pub fn is_exact(&self) -> bool {
        self.0.split('.').all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    }
}

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

pattern_type!(
    /// What a Terraform provider prints when one resource instance of the state is already gone,
    /// that instance's attributes in the state written `{<attribute>}` (`no such IPSet '{id}'`):
    /// filled with an instance's attributes, it names that instance and no other.
    GoneText,
    "gone error",
    "^[^{}]*(\\{[a-z][a-z0-9_]*\\}[^{}]*)+$",
    "text naming at least one of the instance's attributes as `{name}` (`no such IPSet '{id}'`), with no other `{` or `}`",
    |v| pieces(v).is_some_and(|p| p.iter().any(|piece| matches!(piece, Piece::Attribute(_))))
);

impl GoneText {
    /// The attributes it names, in order.
    pub fn attributes(&self) -> impl Iterator<Item = &str> {
        pieces(&self.0).unwrap_or_default().into_iter().filter_map(|p| match p {
            Piece::Attribute(name) => Some(name),
            Piece::Text(_) => None,
        })
    }

    /// The text for one instance, each `{<attribute>}` its `value`; none when an attribute it
    /// names has no value, or an empty one (the text would no longer name that instance).
    pub fn fill(&self, value: impl Fn(&str) -> Option<String>) -> Option<String> {
        let mut out = String::new();
        for piece in pieces(&self.0)? {
            match piece {
                Piece::Text(text) => out.push_str(text),
                Piece::Attribute(name) => out.push_str(&value(name).filter(|v| !v.is_empty())?),
            }
        }
        Some(out)
    }
}

/// A piece of a [`GoneText`].
enum Piece<'a> {
    /// Text as it is.
    Text(&'a str),
    /// `{<attribute>}`: the attribute's name.
    Attribute(&'a str),
}

/// `v` in pieces; none when a `{` or a `}` isn't part of a `{<snake_case name>}`.
fn pieces(v: &str) -> Option<Vec<Piece<'_>>> {
    let snake = |name: &str| first_then(name, |c| c.is_ascii_lowercase(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    let mut out = Vec::new();
    let mut rest = v;
    while let Some((before, after)) = rest.split_once('{') {
        let (name, next) = after.split_once('}')?;
        if before.contains('}') || !snake(name) {
            return None;
        }
        if !before.is_empty() {
            out.push(Piece::Text(before));
        }
        out.push(Piece::Attribute(name));
        rest = next;
    }
    if rest.contains('}') {
        return None;
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest));
    }
    Some(out)
}

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

    /// An environment's name is kebab case of at most 63 characters wherever it is read: the
    /// spec's validation, the manifest's parser and its schema (whose pattern can't bound it).
    #[test]
    fn an_environment_name_has_one_length_everywhere() {
        let longest = "a".repeat(KEBAB_MAX);
        assert!(is_kebab(&longest) && EnvironmentName::parse(&longest).is_ok());
        let longer = format!("{longest}b");
        assert!(!is_kebab(&longer) && EnvironmentName::parse(&longer).is_err());
        let schema = schemars::schema_for!(EnvironmentName);
        assert_eq!(schema.get("maxLength").and_then(serde_json::Value::as_u64), Some(63));
    }

    #[test]
    fn names_follow_their_patterns() {
        assert!(EnvironmentName::parse("web-portal").is_ok());
        assert!(EnvironmentName::parse("Portal").is_err());
        assert!(EnvironmentName::parse("a--b").is_err());
        assert!(Name::parse("isoloom-check-db").is_ok());
        assert!(Name::parse("-web").is_err());
        assert!(EnvName::parse("DEMO_SECRET").is_ok());
        assert!(EnvName::parse("demo_secret").is_ok());
        assert!(EnvName::parse("1TOKEN").is_err());
        assert!(Identifier::parse("db-password").is_ok());
        assert!(StepId::parse("job-isoloom-init-web-1").is_ok());
        assert!(StepId::parse("Up").is_err());
        assert!(SnakeName::parse("web_portal").is_ok());
        assert!(SnakeName::parse("web-portal").is_err());
        assert!(OsName::parse("alpine-3.19").is_ok());
    }

    #[test]
    fn a_label_in_snake_case_is_lowercase_with_underscores() {
        assert_eq!(
            SnakeName::from_label("Web-Admin.ui").map(|s| s.as_str().to_owned()).ok().as_deref(),
            Some("web_admin_ui")
        );
        assert!(SnakeName::from_label("9lives").is_err());
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
    fn a_gone_error_names_its_instance_by_its_attributes() {
        let text = GoneText::parse("Configuration file 'nodes/{node_name}/qemu-server/{id}.conf' does not exist").unwrap();
        assert_eq!(text.attributes().collect::<Vec<_>>(), ["node_name", "id"]);
        let attribute = |name: &str| match name {
            "id" => Some("101".to_owned()),
            "node_name" => Some("pve".to_owned()),
            _ => None,
        };
        assert_eq!(
            text.fill(attribute).as_deref(),
            Some("Configuration file 'nodes/pve/qemu-server/101.conf' does not exist")
        );
        assert_eq!(text.fill(|name| (name == "id").then(|| "101".to_owned())), None, "an attribute it lacks");
        assert_eq!(text.fill(|_| Some(String::new())), None, "an empty one names no instance");
        let ipset = GoneText::parse("no such IPSet '{id}'").unwrap();
        assert_eq!(ipset.fill(|_| Some("iso1n0".to_owned())).as_deref(), Some("no such IPSet 'iso1n0'"));
        for bad in ["no such IPSet", "", "{Id}", "{}", "a {id", "a id}", "a }{id}", "{id}{", "{{id}}", "{node-name}"] {
            assert_eq!(GoneText::parse(bad).map_err(|e| e.code()), Err("pattern"), "{bad}");
        }
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
