//! The checks that say an environment works: a script, or a declared probe and what it expects.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One check: a script in the project (a string), or a declared probe (a map).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum Check {
    /// A path in the project: a `.sh` script run with sh from where a user stands (exits 0
    /// when the behavior holds), or an Ansible playbook (`.yml`) run from the controller.
    Script(String),
    /// A probe Isoloom runs itself, from a machine of the environment.
    Declared(Box<Declared>),
}

impl Check {
    /// The project path this check runs, when it is a script or a playbook.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        match self {
            Check::Script(p) => Some(p),
            Check::Declared(d) => d.script.as_deref(),
        }
    }

    /// Whether this check is an Ansible playbook (`.yml` / `.yaml`).
    pub fn is_playbook(&self) -> bool {
        self.path().is_some_and(is_yaml_file)
    }
}

/// Whether `path` names a YAML file (an Ansible playbook): `.yml` or `.yaml`, as written.
pub(crate) fn is_yaml_file(path: &str) -> bool {
    std::path::Path::new(path).extension().is_some_and(|e| e == "yml" || e == "yaml")
}

// A string is a script; a map is a declared check. Written by hand (not `untagged`) so a typo in
// a declared check is reported as such, with its field, instead of "no variant matched".
impl<'de> Deserialize<'de> for Check {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Check;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a script path, or a check with `http`, `tcp`, `exec` or `script`")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Check, E> {
                Ok(Check::Script(v.to_owned()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<Check, A::Error> {
                Declared::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(|d| Check::Declared(Box::new(d)))
            }
        }
        d.deserialize_any(V)
    }
}

/// A check Isoloom runs itself. Exactly one of `http`, `tcp`, `exec` and `script` says what to
/// check; `from` says where from; `expect` what counts as passing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Declared {
    /// Shown in results. Default: what the check does (`http://web/ from user`).
    #[serde(default)]
    pub name: Option<String>,
    /// The machine the check runs from (its networks, its routes, its view). Default: the
    /// access machine when the spec has one, else the environment's networks at once.
    #[serde(default)]
    pub from: Option<String>,
    /// An HTTP(S) request to this URL. `expect`: a status code (default 200), `any` (an answer
    /// of any status), or `blocked` (nothing answers).
    #[serde(default)]
    pub http: Option<String>,
    /// With `http`: the request method (default GET).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// With `http`: request headers.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub headers: IndexMap<String, String>,
    /// With `http`: the request body, sent as is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// With `http`: text the response body must contain. With `tcp`: text the bytes read in
    /// the first 5 seconds of the connection (after `send`) must contain, like a banner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    /// With `http`: follow redirects (up to 5 hops) and compare the final answer's status, for a
    /// page that redirects (a login page, an app's index). Default: the first answer counts. A
    /// POST followed by a 301, 302 or 303 goes on as a GET, as in a browser; other methods are
    /// sent again to each hop.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub follow: bool,
    /// With `tcp` and `contains`: a line written once connected (followed by a newline; end it
    /// with `\r` for CRLF), before reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send: Option<String>,
    /// A TCP connection to `host:port`. `expect`: `open` (default) or `blocked`.
    #[serde(default)]
    pub tcp: Option<String>,
    /// A command run inside the `from` machine (`from` is required). `expect`: text its output
    /// must contain (default: the command succeeds).
    #[serde(default)]
    pub exec: Option<String>,
    /// A script in the project, run with sh from the `from` machine's position; it exits 0 when
    /// the behavior holds.
    #[serde(default)]
    pub script: Option<String>,
    /// What counts as passing, in the check's own terms: a status code, `any` or `blocked`
    /// for `http`; `open` or `blocked` for `tcp`; text the output contains for `exec`. Each
    /// kind's field says its default.
    #[serde(default)]
    pub expect: Option<Expect>,
    /// How long to keep retrying until the check passes, as seconds (`90`) or a duration (`90s`,
    /// `3m`, `1m30s`): default 30 seconds for `http` and `tcp` (except `blocked`, tried once), 0
    /// for `exec` and `script`. Held in seconds.
    #[serde(default, deserialize_with = "duration")]
    #[schemars(with = "Option<WaitSchema>")]
    pub wait: Option<u32>,
}

// What the schema shows and the reader returns.
/// A wait as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
enum WaitSchema {
    /// Seconds.
    Seconds(u32),
    /// A duration: `90s`, `3m`, `1m30s`.
    Duration(String),
}

impl<'de> Deserialize<'de> for WaitSchema {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<WaitSchema, D::Error> {
        let not_a_wait = || serde::de::Error::custom("a wait is a number of seconds or a duration (`90s`, `3m`)");
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()).map(WaitSchema::Seconds).ok_or_else(not_a_wait),
            serde_json::Value::String(s) => Ok(WaitSchema::Duration(s)),
            _ => Err(not_a_wait()),
        }
    }
}

/// A wait written as seconds (`90`) or a duration (`90s`, `3m`, `1m30s`), in seconds.
fn duration<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    match Option::<WaitSchema>::deserialize(d)? {
        None => Ok(None),
        Some(WaitSchema::Seconds(n)) => Ok(Some(n)),
        Some(WaitSchema::Duration(s)) => super::seconds(&s)
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom(format!("`{s}` isn't a duration: write `90s`, `3m` or `1m30s`"))),
    }
}

/// What a declared check expects: an HTTP status code, or a word (`any`, `blocked`, `open`),
/// or for `exec` the text the output must contain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Expect {
    /// An HTTP status code (`200`, `404`).
    Status(u16),
    /// A word (`any`, `blocked`, `open`), or the text an `exec` output must contain.
    Text(String),
}
