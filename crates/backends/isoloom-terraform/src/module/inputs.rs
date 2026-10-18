//! The spec's launch-time inputs (`inputs:`): the `inputs` variable a module takes them in, and
//! the inputs file a machine reads them from.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Expression, Template, call, list, object, reference, string};
use isoloom_render::steps;

use super::variable::Variable;

/// The `inputs` variable, when the spec has `inputs:`: the values they are given at launch.
pub(crate) fn variable(env: &Environment) -> Option<Block> {
    variable_of(env).map(Variable::block)
}

/// [`variable`], before it is written.
pub(crate) fn variable_of(env: &Environment) -> Option<Variable> {
    let variable = Variable::new("inputs", "string")
        .typed(call("map", [reference("string")]))
        .default(object::<&str>([]))
        .sensitive()
        .describe("Values given at launch");
    (!env.spec().inputs.is_empty()).then_some(variable)
}

/// A machine's inputs file (`KEY='value'` lines, read with `sh`'s `.`), from `var.inputs`.
/// Each value is single-quoted (its quotes written `'\''`), so `sh` takes it as it is: no `$`,
/// backtick or `$(...)` expands.
pub(crate) fn file(inputs: &[String]) -> Expression {
    let lines = inputs.iter().map(|i| {
        let value = call("lookup", [reference("var.inputs"), string(i.as_str()), string("")]);
        Template::text(format!("{i}='"))
            .with_expr(call("replace", [value, string("'"), string("'\\''")]))
            .with_text("'")
            .into_expression()
    });
    call("join", [string("\n"), list(lines)])
}

/// The inputs file of `names` written owner-only at [`steps::KEPT_INPUTS_FILE`] by a machine's
/// own set-up, from `var.inputs`, base64 on its way so no value can end the script it travels in
/// (its `umask` in a subshell: the ready file the set-up writes next stays readable).
pub(crate) fn written(names: &[String]) -> Template {
    Template::text("mkdir -p /var/lib/isoloom && (umask 077 && echo ")
        .with_expr(call("base64encode", [file(names)]))
        .with_text(format!(" | base64 -d > {})", steps::KEPT_INPUTS_FILE))
}
