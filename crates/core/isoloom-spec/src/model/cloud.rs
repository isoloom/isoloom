//! Cloud services an environment deploys beside its machines, and how their outputs fill in.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An environment of cloud services: a Terraform module applied into the user's account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CloudServices {
    /// The cloud the module deploys into (its credentials come from the environment, as
    /// Terraform expects).
    pub provider: CloudProvider,
    /// A Terraform root module in the project (a folder holding `.tf` files), used as is.
    pub terraform: String,
    /// The module's variables: any value (a string, number, boolean, list or map). A string
    /// holding `{{ inputs.NAME }}`, at any depth (`["{{ inputs.PLAYER_IP }}/32"]`), takes the
    /// launch-time input `NAME` (declared in `inputs`), e.g. the user's IP for an allow-list:
    /// required at `run`, kept for `down`.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    #[schemars(with = "IndexMap<String, serde_json::Value>")]
    pub vars: IndexMap<String, serde_json::Value>,
    /// What the environment exposes, by name: a module output each. Checks and the message
    /// use them as `{{ cloud.outputs.<name> }}`.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub outputs: IndexMap<String, String>,
    /// About what the environment costs per hour while it exists, in US dollars: the author's
    /// estimate, for whoever runs it. `isoloom run` says it before applying, and the resolved
    /// snapshot carries it (`cloud.hourly_usd`) for anything that shows costs or keeps to a
    /// budget. Isoloom doesn't bill or stop anything by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hourly_usd: Option<f64>,
    /// Steps run on this machine before Terraform applies the module (a key pair the module
    /// reads, a build): scripts in the project, run in order from the module's copy, with the
    /// spec's inputs and `env` in their environment. They run at every `run`: make them
    /// idempotent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
    /// Environment variables for Terraform and the `before` steps (`AWS_REGION: us-east-1`).
    /// Not for credentials: those come from the user's own environment.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,
    /// How many times to apply again when an apply fails (a module that only converges on a
    /// second apply), 0 to 5. 0 by default.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub apply_retries: u8,
    /// What the module needs on this machine besides Terraform: a command name (`jq`, `gsed`)
    /// that must be on the PATH, or a shell command that must succeed
    /// (`python3 -c 'import boto3'`). `isoloom doctor` and `isoloom run` check them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

/// Whether a count is zero (left out when serialized).
#[allow(clippy::trivially_copy_pass_by_ref)] // serde's `skip_serializing_if` passes a reference.
fn is_zero(n: &u8) -> bool {
    *n == 0
}

/// A cloud an environment of cloud services deploys into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CloudProvider {
    /// Amazon Web Services.
    Aws,
    /// Microsoft Azure.
    Azure,
    /// Google Cloud.
    Gcp,
}

impl CloudServices {
    /// The launch-time input a variable takes, when its value is exactly `{{ inputs.NAME }}`.
    /// A value can also hold inputs inside its strings, at any depth: see
    /// [`inputs_in`](Self::inputs_in).
    pub fn input_of(value: &serde_json::Value) -> Option<&str> {
        let s = value.as_str()?.trim();
        s.strip_prefix("{{ inputs.")?.strip_suffix(" }}").map(str::trim)
    }

    /// The launch-time inputs a variable's value takes: every `{{ inputs.NAME }}` in its strings,
    /// at any depth, in order (repeats kept).
    pub fn inputs_in(value: &serde_json::Value) -> Vec<String> {
        let mut found = Vec::new();
        Self::fill_inputs(value, &mut |name| {
            found.push(name.to_string());
            Some(String::new())
        });
        found
    }

    /// The value with each `{{ inputs.NAME }}` in its strings, at any depth, replaced by
    /// `input(NAME)` (left as written when it gives nothing).
    pub fn fill_inputs(value: &serde_json::Value, input: &mut dyn FnMut(&str) -> Option<String>) -> serde_json::Value {
        use serde_json::Value;
        match value {
            Value::String(s) => Value::String(fill_string(s, input)),
            Value::Array(a) => Value::Array(a.iter().map(|v| Self::fill_inputs(v, input)).collect()),
            Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), Self::fill_inputs(v, input))).collect()),
            other => other.clone(),
        }
    }
}

/// `s` with each `{{ inputs.NAME }}` replaced by `input(NAME)`; any other `{{ ... }}`, and an
/// input `input` gives nothing for, left as written.
fn fill_string(s: &str, input: &mut dyn FnMut(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        let Some(len) = rest[start..].find("}}") else { break };
        let whole = &rest[start..start + len + 2];
        out.push_str(&rest[..start]);
        let inner = whole[2..whole.len() - 2].trim();
        match inner.strip_prefix("inputs.").map(str::trim).and_then(&mut *input) {
            Some(v) => out.push_str(&v),
            None => out.push_str(whole),
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

impl CloudProvider {
    /// The name the spec writes it with (`aws`, `azure`, `gcp`).
    pub fn id(self) -> &'static str {
        match self {
            CloudProvider::Aws => "aws",
            CloudProvider::Azure => "azure",
            CloudProvider::Gcp => "gcp",
        }
    }
}
