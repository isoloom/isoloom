//! A machine's secrets as files: which secret, at what path, owned by whom, with what mode.
//! The plan never holds a value: `isoloom run` reads each from its store and hands it to the
//! target's own mechanism (Compose secrets, a Kubernetes Secret).

use std::fmt;

use isoloom_spec::model::{MachineSecret, SecretSource, Spec};

/// Where a value file's contents come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSource {
    /// A secret, read from its store at each start.
    Secret(String),
    /// An input, given at each start (`files: [{ path, input }]`).
    Input(String),
}

/// One secret file of a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretFile {
    /// The secret, as the top-level `secrets` names it (an input's file is the secret
    /// `input-<name>` lowering makes of it).
    pub name: String,
    /// Where the value comes from.
    pub source: ValueSource,
    /// The file's absolute path in the machine.
    pub path: String,
    /// Its owner's user id, when the spec sets one (root otherwise).
    pub uid: Option<u32>,
    /// Its group id, when the spec sets one (root otherwise).
    pub gid: Option<u32>,
    /// Its mode, when the spec sets one ([`DEFAULT_MODE`] in a container, [`VM_DEFAULT_MODE`] on
    /// a Linux VM otherwise).
    pub mode: Option<u32>,
}

/// The mode a container's secret file has when the spec sets none: readable by every user of
/// the container, as Compose and Kubernetes both default to. A container usually runs one
/// process as one user, so this is what its image expects.
pub const DEFAULT_MODE: u32 = 0o444;

/// The mode a Linux VM's secret file has when the spec sets none: readable by its owner only
/// (root, or the spec's `uid`). A VM has other users (its login account, its services), and a
/// secret readable by all of them would be one any of them could leak; the process that needs
/// it runs as root or as the `uid` the spec gives the file. A Windows VM's file has no mode: it
/// is readable by SYSTEM and Administrators only.
pub const VM_DEFAULT_MODE: u32 = 0o400;

impl SecretFile {
    /// The file a machine's `secrets` entry places.
    #[must_use]
    pub fn of(spec: &Spec, s: &MachineSecret) -> SecretFile {
        let m = s.mount();
        let source = match spec.secrets.get(&m.source) {
            Some(SecretSource::Input { input }) => ValueSource::Input(input.clone()),
            _ => ValueSource::Secret(m.source.clone()),
        };
        SecretFile {
            source,
            name: m.source,
            path: s.path(),
            uid: m.uid,
            gid: m.gid,
            mode: m.mode,
        }
    }
}

/// Why a target with no machine to deliver to refuses machine `machine`'s secrets.
pub(crate) fn no_delivery(f: &mut fmt::Formatter<'_>, machine: &str) -> fmt::Result {
    write!(
        f,
        "machine `{machine}` gets `secrets`, and this target has no machine `isoloom run` can hand them to"
    )
}

/// Why a target that places a secret's file as root's refuses one with its own owner.
pub(crate) fn no_owner(f: &mut fmt::Formatter<'_>, machine: &str) -> fmt::Result {
    write!(
        f,
        "a secret of machine `{machine}` sets `uid` or `gid`, and this target places a secret's file as root's"
    )
}
