//! Why an import fails: the source can't be read as its format, or it holds nothing to draft.
//! Everything an import can still draft around becomes a note instead.

/// An import that produced no draft.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The Compose text isn't YAML.
    #[error("not a Compose file")]
    ComposeYaml(#[source] serde_yaml_ng::Error),
    /// The Compose file's top level isn't a mapping.
    #[error("not a Compose file: its top level isn't a mapping")]
    ComposeNotMapping,
    /// The Compose file has no `services:` (or an empty one).
    #[error("the Compose file has no services")]
    NoServices,
    /// A network has no block left to address it: `at` is the field (`networks.back`).
    #[error("{at}: no {base}.<n>.0/24 block left to address it")]
    NoBlockLeft {
        /// Where in the source, e.g. `networks.back`.
        at: String,
        /// The first two octets of the blocks the import hands out (`10.88`).
        base: &'static str,
    },
    /// A network has no free address left for a machine joining it.
    #[error("network `{network}` has no free address left for `{machine}`")]
    NetworkFull {
        /// The Compose network's name.
        network: String,
        /// The machine that couldn't join.
        machine: String,
    },
    /// What the Vagrantfile set, as the recorder printed it, isn't a recording.
    #[error("the Vagrantfile's recording can't be read")]
    Recording(#[source] serde_json::Error),
    /// The recorded Vagrantfile defines no machine.
    #[error("the Vagrantfile defines no machine")]
    NoVagrantMachine,
    /// A `.tf` file isn't valid HCL.
    #[error("{file}: not valid Terraform")]
    Hcl {
        /// The file's name.
        file: String,
        /// The parser's error.
        #[source]
        source: hcl::Error,
    },
    /// The Terraform files declare no VM resource Isoloom knows.
    #[error("no VM resource found (aws_instance, azurerm_*_virtual_machine, google_compute_instance, proxmox_virtual_environment_vm)")]
    NoTerraformVm,
}

/// What an import returns: a value, or why there is no draft.
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    #[test]
    fn a_source_error_is_the_cause_not_part_of_the_message() -> std::result::Result<(), String> {
        let yaml = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("a: [").err().ok_or("`a: [` parses")?;
        let e = Error::ComposeYaml(yaml);
        assert_eq!(e.to_string(), "not a Compose file");
        assert!(e.source().is_some_and(|s| !s.to_string().is_empty()));
        Ok(())
    }

    #[test]
    fn a_network_out_of_blocks_names_the_field_and_the_range() {
        let e = Error::NoBlockLeft {
            at: "networks.back".to_owned(),
            base: "10.88",
        };
        assert_eq!(e.to_string(), "networks.back: no 10.88.<n>.0/24 block left to address it");
    }
}
