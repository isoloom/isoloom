//! What a Vagrantfile set when it ran against a stand-in `Vagrant` module (the CLI's
//! `vagrant_record.rb`): every setting (`x.y = v`), every nested object (`x.vm`), and every call
//! with its arguments and the object its block configured. Loops and variables are already
//! evaluated by Ruby, so a recording is plain data.

use std::str::FromStr;

use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

use crate::{Error, Result};

/// One configuration object of a recorded Vagrantfile: the top `config`, a machine's, or the one
/// a call's block configured.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Recording {
    /// `x.y = v`: each setting's value, as JSON.
    settings: serde_json::Map<String, Value>,
    /// `x.vm`: the nested objects, by name.
    children: IndexMap<String, Recording>,
    /// `x.vm.network "private_network", ip: ...`: the calls, in order.
    calls: Vec<Call>,
}

/// One recorded call: its method's name, its arguments, and the object its block configured.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Call {
    name: String,
    args: Vec<Value>,
    block: Option<Recording>,
}

impl FromStr for Recording {
    type Err = Error;

    /// The recording in the recorder's JSON output.
    ///
    /// # Errors
    ///
    /// [`Error::Recording`] when the text isn't one.
    fn from_str(json: &str) -> Result<Recording> {
        serde_json::from_str(json).map_err(Error::Recording)
    }
}

impl TryFrom<Value> for Recording {
    type Error = Error;

    /// The recording in already parsed JSON.
    ///
    /// # Errors
    ///
    /// [`Error::Recording`] when the JSON isn't one.
    fn try_from(json: Value) -> Result<Recording> {
        serde_json::from_value(json).map_err(Error::Recording)
    }
}

impl Recording {
    /// The nested object `name` (`vm`), when the Vagrantfile reached it.
    pub(super) fn child(&self, name: &str) -> Option<&Recording> {
        self.children.get(name)
    }

    /// The setting `key`, when it is set to something.
    pub(super) fn setting(&self, key: &str) -> Option<&Value> {
        self.settings.get(key).filter(|v| !v.is_null())
    }

    /// The calls made on this object, in order.
    pub(super) fn calls(&self) -> impl Iterator<Item = &Call> {
        self.calls.iter()
    }
}

impl Call {
    /// The method's name: `network`, `provider`, `define`...
    pub(super) fn name(&self) -> &str {
        &self.name
    }

    /// The first argument, as text (`private_network`, a machine's name).
    pub(super) fn first(&self) -> String {
        text(self.args.first()).unwrap_or_default()
    }

    /// The object the call's block configured, when it had one.
    pub(super) fn block(&self) -> Option<&Recording> {
        self.block.as_ref()
    }

    /// The option `key`, given as an argument (`ip: ...`) or set in the block.
    pub(super) fn opt(&self, key: &str) -> Option<&Value> {
        self.args
            .get(1)
            .and_then(|opts| opts.get(key))
            .filter(|v| !v.is_null())
            .or_else(|| self.block()?.setting(key))
    }
}

/// A value as text: a string, or a number as written.
pub(super) fn text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// A value as a whole number: a number, or text holding one.
pub(super) fn number(v: Option<&Value>) -> Option<u32> {
    match v? {
        Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_option_is_read_from_its_arguments_then_its_block() -> Result<()> {
        let r: Recording = r#"{"calls":[{"name":"provision","args":["shell",{"path":"a.sh"}],"block":{"settings":{"reboot":true}}}]}"#.parse()?;
        let call = r.calls().next().ok_or(Error::NoVagrantMachine)?;
        assert_eq!((call.name(), call.first().as_str()), ("provision", "shell"));
        assert_eq!(text(call.opt("path")).as_deref(), Some("a.sh"));
        assert_eq!(call.opt("reboot"), Some(&Value::Bool(true)));
        assert_eq!(call.opt("inline"), None);
        Ok(())
    }

    #[test]
    fn json_that_isnt_a_recording_is_refused() {
        assert!(matches!("{\"calls\": 3}".parse::<Recording>(), Err(Error::Recording(_))));
        assert!(matches!("\"text\"".parse::<Recording>(), Err(Error::Recording(_))));
    }

    #[test]
    fn a_number_is_read_from_a_number_or_text() {
        assert_eq!(number(Some(&Value::from(2048))), Some(2048));
        assert_eq!(number(Some(&Value::from(" 4 "))), Some(4));
        assert_eq!(number(Some(&Value::from(-1))), None);
    }
}
