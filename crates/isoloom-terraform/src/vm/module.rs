//! What every VM module has: the `terraform` block, variables (the inputs among them), outputs,
//! and the project's lists (`.isoloomignore`).

use isoloom_render::hcl_builder::{Block, Expression, Template, boolean, call, list, object, reference, string};
use isoloom_render::project;

/// A provider a module requires: its local name, source and version constraint.
pub type Provider = (&'static str, &'static str, &'static str);

/// The `terraform` block: Terraform 1.6 or later, a local state, the providers.
pub fn terraform(providers: &[Provider]) -> Block {
    let required = providers.iter().fold(Block::builder("required_providers"), |b, (name, source, version)| {
        b.add_attribute((*name, object([("source", string(*source)), ("version", string(*version))])))
    });
    Block::builder("terraform")
        .add_attribute(("required_version", string(">= 1.6")))
        .add_block(Block::builder("backend").add_label("local").build())
        .add_block(required.build())
        .build()
}

/// A `variable` block, built up.
#[derive(Debug, Clone)]
pub struct Variable {
    name: String,
    ty: Expression,
    default: Option<Expression>,
    sensitive: bool,
    description: Option<String>,
}

impl Variable {
    /// A variable of type `ty` (`string`, `number`, `bool`).
    pub fn new(name: &str, ty: &str) -> Variable {
        Variable {
            name: name.to_string(),
            ty: reference(ty),
            default: None,
            sensitive: false,
            description: None,
        }
    }

    /// The variable with a default.
    #[must_use]
    pub fn default(mut self, value: Expression) -> Variable {
        self.default = Some(value);
        self
    }

    /// The variable, sensitive.
    #[must_use]
    pub fn sensitive(mut self) -> Variable {
        self.sensitive = true;
        self
    }

    /// The variable with a description.
    #[must_use]
    pub fn describe(mut self, text: &str) -> Variable {
        self.description = Some(text.to_string());
        self
    }

    /// The `variable` block.
    pub fn block(self) -> Block {
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
        b.build()
    }
}

/// The `inputs` variable: the values the spec's `inputs:` are given at launch.
pub fn inputs_variable() -> Block {
    let mut v = Variable::new("inputs", "string")
        .default(object::<&str>([]))
        .sensitive()
        .describe("Values given at launch");
    v.ty = call("map", [reference("string")]);
    v.block()
}

/// A machine's inputs file (`KEY='value'` lines, read with `sh`'s `.`), from `var.inputs`.
/// Each value is single-quoted (its quotes written `'\''`), so `sh` takes it as it is: no `$`,
/// backtick or `$(...)` expands.
pub fn inputs_file(inputs: &[String]) -> Expression {
    let lines = inputs.iter().map(|i| {
        let value = call("lookup", [reference("var.inputs"), string(i.as_str()), string("")]);
        Template::text(format!("{i}='"))
            .with_expr(call("replace", [value, string("'"), string("'\\''")]))
            .with_text("'")
            .into_expression()
    });
    call("join", [string("\n"), list(lines)])
}

/// An `output` block.
pub fn output(name: &str, value: Expression) -> Block {
    Block::builder("output").add_label(name).add_attribute(("value", value)).build()
}

/// The project's lists, read from `.isoloomignore` when Terraform runs (see
/// `isoloom_render::project`): with `archive`, `tar`'s exclusion file too.
pub fn project_lists(archive: bool) -> String {
    project::lists(archive)
}
