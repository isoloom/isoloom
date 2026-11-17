//! A module's `variable` blocks, built up.

use crate::hcl_builder::{Block, Expression, boolean, reference, string};

/// A `variable` block, built up.
#[derive(Debug, Clone)]
pub(crate) struct Variable {
    name: String,
    ty: Expression,
    default: Option<Expression>,
    sensitive: bool,
    description: Option<String>,
    validation: Option<(Expression, String)>,
}

impl Variable {
    /// A variable of type `ty` (`string`, `number`, `bool`).
    pub(crate) fn new(name: &str, ty: &str) -> Variable {
        Variable {
            name: name.to_owned(),
            ty: reference(ty),
            default: None,
            sensitive: false,
            description: None,
            validation: None,
        }
    }

    /// The variable with a type that is an expression (`map(string)`).
    #[must_use]
    pub(crate) fn typed(mut self, ty: Expression) -> Variable {
        self.ty = ty;
        self
    }

    /// The variable with a `validation` block: Terraform refuses a value failing `condition`,
    /// with `message`.
    #[must_use]
    pub(crate) fn validated(mut self, condition: Expression, message: &str) -> Variable {
        self.validation = Some((condition, message.to_owned()));
        self
    }

    /// The variable with a default.
    #[must_use]
    pub(crate) fn default(mut self, value: Expression) -> Variable {
        self.default = Some(value);
        self
    }

    /// The variable, sensitive.
    #[must_use]
    pub(crate) fn sensitive(mut self) -> Variable {
        self.sensitive = true;
        self
    }

    /// The variable with a description.
    #[must_use]
    pub(crate) fn describe(mut self, text: &str) -> Variable {
        self.description = Some(text.to_owned());
        self
    }

    /// Its name.
    #[cfg(test)]
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Whether it is sensitive.
    #[cfg(test)]
    pub(crate) fn is_sensitive(&self) -> bool {
        self.sensitive
    }

    /// Its default, as HCL.
    #[cfg(test)]
    pub(crate) fn default_hcl(&self) -> Option<String> {
        self.default.as_ref().map(ToString::to_string)
    }

    /// Whether it has a default.
    #[cfg(test)]
    pub(crate) fn has_default(&self) -> bool {
        self.default.is_some()
    }

    /// The `variable` block: its type, default, sensitivity, description and validation, in
    /// that order.
    pub(crate) fn block(self) -> Block {
        let mut b = Block::builder("variable").add_label(self.name.as_str()).add_attribute(("type", self.ty));
        if let Some(d) = self.default {
            b = b.add_attribute(("default", d));
        }
        if self.sensitive {
            b = b.add_attribute(("sensitive", boolean(true)));
        }
        if let Some(d) = self.description {
            b = b.add_attribute(("description", string(d)));
        }
        if let Some((condition, message)) = self.validation {
            b = b.add_block(
                Block::builder("validation")
                    .add_attribute(("condition", condition))
                    .add_attribute(("error_message", string(message)))
                    .build(),
            );
        }
        b.build()
    }
}

/// `ssh_public_key`, with the one description every module gives it. Required: a cloud's
/// machines are reachable only with a key (its key pair or instance metadata takes one), and
/// Terraform signs in with its private half ([`ssh_private_key_file`]) to set the machines up
/// (ESXi) or to wait for their set-up (`crate::ready`).
pub(crate) fn ssh_public_key() -> Variable {
    Variable::new("ssh_public_key", "string").describe(
        "The SSH public key the machines let in: Terraform signs in with its private half (`ssh_private_key_file`) to set them up or wait for their set-up, and so can you, to run the checks",
    )
}

/// `ssh_private_key_file`: the private half of `ssh_public_key`, which Terraform signs in to the
/// machines with to wait for their set-up (`crate::ready`). A path, `~` allowed.
pub(crate) fn ssh_private_key_file() -> Variable {
    Variable::new("ssh_private_key_file", "string").describe(
        "The private key of ssh_public_key (a path, ~ allowed): Terraform signs in to the machines with it to bring the project in and wait for their set-up",
    )
}

#[cfg(test)]
mod tests {
    use crate::hcl_builder::Module;

    use super::*;

    #[test]
    fn a_variable_writes_its_attributes_in_order() {
        let mut m = Module::new("");
        m.block(Variable::new("x", "string").describe("d").sensitive().default(string("")).block());
        assert_eq!(
            m.render().unwrap(),
            "variable \"x\" {\n  type        = string\n  default     = \"\"\n  sensitive   = true\n  description = \"d\"\n}\n"
        );
    }
}
