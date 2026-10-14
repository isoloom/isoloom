//! Why an import fails: the source can't be read as its format, or it holds nothing to draft.
//! Everything an import can still draft around becomes a note instead.

/// An import that produced no draft.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The Compose text isn't YAML.
    #[error("not a Compose file: {0}")]
    ComposeYaml(#[from] serde_yaml_ng::Error),
    /// The Compose file's top level isn't a mapping.
    #[error("not a Compose file: expected a mapping")]
    ComposeNotMapping,
    /// The Compose file has no `services:` (or an empty one).
    #[error("the Compose file has no services")]
    NoServices,
    /// A network has no address left: `at` is the field (`networks.back`), `reason` says which.
    #[error("{at}: {reason}")]
    Addressing {
        /// Where in the source, e.g. `networks.back`.
        at: String,
        /// What ran out.
        reason: String,
    },
    /// A network has no free address left for a machine joining it.
    #[error("network `{network}` has no free address left for `{machine}`")]
    NetworkFull {
        /// The Compose network's name.
        network: String,
        /// The machine that couldn't join.
        machine: String,
    },
    /// The recorded Vagrantfile defines no machine.
    #[error("the Vagrantfile defines no machine")]
    NoVagrantMachine,
    /// A `.tf` file isn't valid HCL.
    #[error("{file}: not valid Terraform: {source}")]
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
