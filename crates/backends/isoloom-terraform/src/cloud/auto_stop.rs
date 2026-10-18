//! A machine's end. On the clouds, the run's (`expires_at`): a Windows machine's scheduled task
//! shuts it down then, so a forgotten environment stops costing even when the runner is gone (a
//! Linux machine's own set-up schedules it itself, at every boot:
//! `crate::machine::user_data::expiry_shutdown`). The Docker host on a Proxmox server keeps its
//! `auto_stop_minutes`.

use isoloom_render::hcl_builder::{BinaryOperator, Expression, Template, binary, conditional, reference, string};

/// `var.expires_at != "" ? <yes> : <no>`: whether the run has an end.
pub(crate) fn if_expires(yes: Expression, no: Expression) -> Expression {
    conditional(binary(reference("var.expires_at"), BinaryOperator::NotEq, string("")), yes, no)
}

/// The PowerShell registering the shutdown at the run's end (`var.expires_at`, Unix seconds).
pub(crate) fn windows_task() -> Template {
    Template::text("Register-ScheduledTask -TaskName isoloom-expiry -Action (New-ScheduledTaskAction -Execute shutdown.exe -Argument '/s /t 0') -Trigger (New-ScheduledTaskTrigger -Once -At ([DateTimeOffset]::FromUnixTimeSeconds(")
        .with_expr(reference("var.expires_at"))
        .with_text(").LocalDateTime)) -User SYSTEM -RunLevel Highest -Force | Out-Null")
}
