//! The variables a Vagrantfile reads from the host's environment, each with the default it falls
//! back to, written once: the Vagrantfile reads them from here (`ENV.fetch`), and the run
//! manifest's params advertise the same names and defaults (`crate::manifest`), so the runner
//! never sees one default and the Vagrantfile use another.

use isoloom_manifest::contract;

use crate::ruby::Value;

/// A variable a Vagrantfile reads, and its default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Setting {
    /// The variable's name.
    pub(crate) name: &'static str,
    /// What the Vagrantfile reads when it is unset, if anything.
    pub(crate) default: Default,
}

/// A setting's default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Default {
    /// None: unset, the Vagrantfile reads it empty, or leaves the setting out.
    None,
    /// Text.
    Text(&'static str),
    /// A port number.
    Port(u16),
}

impl Setting {
    /// Its default as the run manifest's param gives it.
    pub(crate) fn default(self) -> Option<String> {
        match self.default {
            Default::None => None,
            Default::Text(text) => Some(text.to_owned()),
            Default::Port(port) => Some(port.to_string()),
        }
    }

    /// The Ruby reading it: `ENV.fetch("<name>", "<default>")`, the default empty when it has
    /// none.
    pub(crate) fn fetch(self) -> String {
        format!("ENV.fetch({}, {})", Value::str(self.name), Value::str(&self.default().unwrap_or_default()))
    }

    /// The Ruby reading it as it is, `nil` when unset: `ENV["<name>"]`.
    pub(crate) fn raw(self) -> String {
        format!("ENV[{}]", Value::str(self.name))
    }
}

/// The ESXi host the VMs go to.
pub(crate) const ESXI_HOSTNAME: Setting = Setting {
    name: contract::ESXI_HOSTNAME,
    default: Default::None,
};

/// The ESXi host's SSH port.
pub(crate) const ESXI_HOSTPORT: Setting = Setting {
    name: contract::ESXI_HOSTPORT,
    default: Default::Port(contract::ESXI_DEFAULT_SSH_PORT),
};

/// The ESXi user.
pub(crate) const ESXI_USERNAME: Setting = Setting {
    name: "ESXI_USERNAME",
    default: Default::Text("root"),
};

/// The datastore the VMs' disks go on (unset: ESXi's first).
pub(crate) const ESXI_DATASTORE: Setting = Setting {
    name: "ESXI_DATASTORE",
    default: Default::None,
};

/// The ESXi port groups, comma-separated: the management network, then one per network.
pub(crate) const ESXI_VIRTUAL_NETWORK: Setting = Setting {
    name: "ESXI_VIRTUAL_NETWORK",
    default: Default::Text("VM Network"),
};

/// The Hyper-V switch each VM's first adapter goes on.
pub(crate) const HYPERV_SWITCH: Setting = Setting {
    name: "ISOLOOM_HYPERV_SWITCH",
    default: Default::Text("Default Switch"),
};

/// The command Vagrant's SSH to the guests goes through, when set.
pub(crate) const SSH_PROXY_COMMAND: Setting = Setting {
    name: contract::SSH_PROXY_COMMAND,
    default: Default::None,
};

/// The host ports the runner chose for this run, as a JSON object (unset: none).
pub(crate) const HOSTPORTS: Setting = Setting {
    name: contract::HOSTPORTS,
    default: Default::Text("{}"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_reads_with_its_default_or_as_it_is() {
        assert_eq!(ESXI_HOSTPORT.fetch(), "ENV.fetch(\"ESXI_HOSTPORT\", \"22\")");
        assert_eq!(ESXI_HOSTPORT.default().as_deref(), Some("22"));
        assert_eq!(ESXI_HOSTNAME.fetch(), "ENV.fetch(\"ESXI_HOSTNAME\", \"\")");
        assert_eq!(ESXI_HOSTNAME.default(), None);
        assert_eq!(ESXI_DATASTORE.raw(), "ENV[\"ESXI_DATASTORE\"]");
    }
}
