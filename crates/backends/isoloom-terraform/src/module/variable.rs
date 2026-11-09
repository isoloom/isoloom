//! A module's `variable` blocks, built up.

use crate::hcl_builder::{Block, Expression, boolean, reference, string};
use isoloom_render::run_manifest::RUNNER;

/// A `variable` block, built up.
#[derive(Debug, Clone)]
pub(crate) struct Variable {
    name: String,
    /// Its type as written (`string`, `number`, `bool`, `map(string)`).
    kind: String,
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
            kind: ty.to_owned(),
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
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Its description, when it has one.
    pub(crate) fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Whether it is sensitive.
    pub(crate) fn is_sensitive(&self) -> bool {
        self.sensitive
    }

    /// Whether it is a `bool`.
    pub(crate) fn is_bool(&self) -> bool {
        self.kind == "bool"
    }

    /// Its default as text (`"local"` is `local`, `true`, `1`); none when it has none (it is
    /// required) or when the default isn't a plain value.
    pub(crate) fn default_text(&self) -> Option<String> {
        match self.default.as_ref()? {
            Expression::String(s) => Some(s.clone()),
            Expression::Number(n) => Some(n.to_string()),
            Expression::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }

    /// Whether it has a default.
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

/// `ssh_public_key`, with the one description every module gives it, which says who signs in
/// with its private key: the runner (`runner`, a target with a run manifest), else Terraform
/// itself. Required as built: a cloud's machines are reachable only with a key (its key pair or
/// instance metadata takes one), and ESXi's Terraform signs in with its private key
/// (`ssh_private_key_file`) to set the machines up. A Proxmox module defaults it to empty
/// instead: its machines are set up from cloud-init snippets, nobody needs to sign in for them to
/// be made, and an empty key installs none.
pub(crate) fn ssh_public_key(runner: bool) -> Variable {
    let who = if runner {
        format!("as {RUNNER} does to deliver the project and run the checks")
    } else {
        "as Terraform does to set them up (`ssh_private_key_file`), and the checks by hand".to_owned()
    };
    Variable::new("ssh_public_key", "string").describe(&format!(
        "The SSH public key the machines let in: whoever holds its private key signs in, {who}"
    ))
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
