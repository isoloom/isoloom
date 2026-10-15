//! Values set at run time: params (not secret) and slots (inputs, secrets, connections), each
//! with where it goes. A slot holds a name, a kind, a source and destinations, never a value.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::identity::{HostOs, Provider};
use crate::list::{NonEmpty, StepSelection};
use crate::names::{EnvName, Identifier, Name, SnakeName, StepId};
use crate::path::RelPath;

/// Who sets a param.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SetBy {
    /// The runner decides it (a relay's proxy command, the player's public IP).
    Runner,
    /// The app driving the runner passes it.
    Embedder,
}

/// The shape of a Terraform variable a value goes into.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TfVarShape {
    /// `TF_VAR_<var>`.
    #[default]
    String,
    /// One key of a JSON-object variable (`inputs`), the runner writing the whole object.
    MapEntry,
}

/// The format of a file in the run's state folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StateFileFormat {
    /// A Terraform variable file in JSON.
    TfvarsJson,
    /// `NAME=value` lines.
    Dotenv,
}

/// How a value reaches a guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// Plain SSH with the runner's key.
    Ssh,
    /// `vagrant ssh`.
    VagrantSsh,
    /// WinRM.
    Winrm,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Where a value goes. Never a command-line argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "via", rename_all = "snake_case", deny_unknown_fields)]
pub enum Delivery {
    /// An environment variable of some steps' processes.
    Env {
        /// The variable.
        var: EnvName,
        /// The steps that get it.
        steps: StepSelection,
    },
    /// A Terraform variable.
    TfVar {
        /// The variable.
        var: SnakeName,
        /// `string` (default): `TF_VAR_<var>`. `map_entry`: one key of a JSON-object variable.
        #[serde(default, skip_serializing_if = "is_default")]
        shape: TfVarShape,
    },
    /// A file in the run's state folder, written owner-only and kept until `down` succeeded (a
    /// destroy needs the same values), named by `${run.state_dir}` in the steps that read it.
    StateFile {
        /// Relative to the run's state folder.
        path: RelPath,
        /// The file's format.
        format: StateFileFormat,
        /// The value's key in the file, when not the slot's name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// Isoloom's guest delivery (`/var/lib/isoloom/secrets`, then `.delivered`).
    Guest {
        /// The machine that receives it.
        machine: Name,
        /// How it gets there.
        transport: Transport,
        /// Delivered while this step runs (the guest waits for it), retried until it lands.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        during_step: Option<StepId>,
    },
}

/// A non-secret value set at run time that the generated files read (ISOLOOM_PUBLISH_ADDRESS,
/// ISOLOOM_HYPERV_SWITCH, a Terraform variable such as `region`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Param {
    /// Its name (`${params.<name>}`).
    pub name: EnvName,
    /// What it does, for the embedder.
    pub description: String,
    /// Its value when nobody sets one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// The only values it takes (empty: any).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// The run can't start without it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    /// Only on these hosts (empty: every host).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hosts: BTreeSet<HostOs>,
    /// Only with these providers (empty: every provider).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub providers: BTreeSet<Provider>,
    /// Who sets it.
    pub set_by: SetBy,
    /// Where it goes.
    pub deliver: NonEmpty<Delivery>,
}

/// What a slot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SlotKind {
    /// A launch-time value of the lab (`inputs:`: a flag, a launch token).
    Input,
    /// The spec's `secrets:`, and the project files Compose takes from its environment.
    Secret,
    /// What reaches the server or cloud account (a password, a token, keys).
    Connection,
    /// Made once per run by the runner and kept in its state.
    Generated,
}

/// One of Isoloom's secret stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Store {
    /// An environment variable of the runner.
    Env,
    /// A file (`reference`: absolute, `~/…`, or relative to the project folder).
    File,
    /// A HashiCorp Vault key/value secret (`<path>#<field>`).
    Vault,
    /// A value in a SOPS-encrypted file (`<file>#<key>`).
    Sops,
}

/// Where the runner reads a slot's value when the embedder passes none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecretSource {
    /// The store.
    pub store: Store,
    /// What the store is asked for, as the spec writes it.
    pub reference: String,
}

/// A secret or input slot: a name, sources and destinations, never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecretSlot {
    /// Its name: how the embedder passes its value.
    pub name: Identifier,
    /// What it holds.
    pub kind: SlotKind,
    /// The run can't start without it.
    pub required: bool,
    /// Masked in every event and log line. A flag, a token and a password always are.
    pub sensitive: bool,
    /// The machines (and their init jobs) that receive it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub machines: Vec<Name>,
    /// Only with these providers (empty: every provider).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub providers: BTreeSet<Provider>,
    /// Where the runner reads it when the embedder passes none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SecretSource>,
    /// What the runner delivers when it has no value (neither the embedder nor `source` gives
    /// one): a slot that isn't `required` still reaches its steps, so a tool that refuses an
    /// unset variable (Compose, for a secret read from the environment) gets this instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Where it goes.
    pub deliver: NonEmpty<Delivery>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delivery_is_tagged_by_its_way() {
        let env: Delivery = serde_json::from_str(r#"{"via": "env", "var": "CTF_FLAG_MAIN", "steps": ["up"]}"#).unwrap();
        assert!(matches!(env, Delivery::Env { .. }));
        let tf: Delivery = serde_json::from_str(r#"{"via": "tf_var", "var": "inputs", "shape": "map_entry"}"#).unwrap();
        assert!(matches!(
            tf,
            Delivery::TfVar {
                shape: TfVarShape::MapEntry,
                ..
            }
        ));
        assert_eq!(
            serde_json::to_string(&Delivery::TfVar {
                var: SnakeName::parse("region").unwrap(),
                shape: TfVarShape::String
            })
            .unwrap(),
            r#"{"via":"tf_var","var":"region"}"#
        );
        assert!(serde_json::from_str::<Delivery>(r#"{"via": "argv", "var": "X"}"#).is_err());
        assert!(serde_json::from_str::<Delivery>(r#"{"via": "env", "var": "X", "steps": ["*"], "value": "secret"}"#).is_err());
    }
}
