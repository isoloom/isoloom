//! The auto-stop (`auto_stop_minutes`): a cloud VM shuts itself down after that long, so a
//! forgotten environment stops costing.

use isoloom_render::hcl_builder::{BinaryOperator, Expression, Template, binary, conditional, number, reference, string};

/// `var.auto_stop_minutes > 0 ? <yes> : <no>`.
pub(crate) fn if_set(yes: Expression, no: Expression) -> Expression {
    conditional(binary(reference("var.auto_stop_minutes"), BinaryOperator::Greater, number(0)), yes, no)
}

/// The auto-stop as a first-boot script: `#!/bin/sh` then `shutdown -h +<minutes>`.
pub(crate) fn script() -> Expression {
    Template::text("#!/bin/sh\nshutdown -h +")
        .with_expr(reference("var.auto_stop_minutes"))
        .with_text("\n")
        .into_expression()
}

/// The auto-stop scheduled again after a restart (an `after_reboot` hook).
pub(crate) fn rescheduled() -> Template {
    Template::new().with_expr(if_set(
        Template::text("sudo shutdown -h +")
            .with_expr(reference("var.auto_stop_minutes"))
            .into_expression(),
        string("true"),
    ))
}
