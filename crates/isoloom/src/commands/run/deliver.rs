//! The tool that brings a VM target up, with the secrets delivered to its machines: while it
//! runs (their set-up waits for them), after it (Proxmox's cloud-init runs once Terraform is
//! done), or before it (an existing host). See `secrets::vms`.

use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use isoloom_runtime::events::{ErrorCode, Scope};
use isoloom_spec::Target;

use super::Ask;
use crate::failure::Failure;
use crate::prepare::Prepared;
use crate::secrets::Values;
use crate::secrets::vms::{self, Missed, Place, Recipient};
use crate::tools::process;
use crate::{Res, output};

/// How long delivery goes on once the tool is done: a machine already set up (a run again)
/// gets its secrets afresh.
const AFTER: Duration = Duration::from_secs(120);

/// Where the target's files are, for `ask`.
fn place<'a>(p: &Prepared, ask: &Ask<'a>) -> Place<'a> {
    Place {
        out: p.dir.join(isoloom_spec::instance::output_dir(ask.instance)),
        cloud: ask.cloud,
        instance: ask.instance,
    }
}

/// An existing host's secrets, before its provisioning reads them.
pub fn before_external(p: &Prepared, ask: &Ask, values: &Values) -> Res<()> {
    let place = place(p, ask);
    let recipients = vms::recipients(p, &place);
    if recipients.is_empty() {
        return Ok(());
    }
    reachable(&recipients)?;
    let at = Scope::step(Some(p.target.id()), "secrets");
    output::announce(&at, None, "Delivering the secrets");
    let missed = vms::deliver(p, &place, values, recipients, &vms::for_at_most(AFTER));
    check(&missed)?;
    output::done(&at, None);
    Ok(())
}

/// Runs the tool (`cmd`, the step `at`), with the secrets delivered to every machine waiting
/// for them. Without secrets (none read for `down`), only the tool.
pub fn run_tool(p: &Prepared, ask: &Ask, values: &Values, cmd: &mut Command, at: &Scope) -> Res<ExitStatus> {
    let place = place(p, ask);
    let recipients = if values.is_empty() || p.target == Target::External {
        Vec::new()
    } else {
        vms::recipients(p, &place)
    };
    if recipients.is_empty() {
        return process::run_step(cmd, at);
    }
    reachable(&recipients)?;
    vms::forget_announcements(p, &place);
    let (status, missed) = if p.target == Target::Proxmox {
        (process::run_step(cmd, at)?, recipients.into_iter().map(not_tried).collect())
    } else {
        alongside(&Delivery { p, place: &place, values }, recipients, cmd, at)?
    };
    if !status.success() {
        if !missed.is_empty() {
            eprintln!("note: the secrets never reached {}", vms::report(&missed));
        }
        return Ok(status);
    }
    let limit = if p.target == Target::Proxmox {
        Duration::from_secs(u64::from(isoloom_render::secrets::DELIVERY_TIMEOUT))
    } else {
        AFTER
    };
    let left: Vec<Recipient> = missed.into_iter().map(|m| m.recipient).collect();
    check(&vms::deliver(p, &place, values, left, &vms::for_at_most(limit)))?;
    Ok(status)
}

/// A recipient not tried yet.
fn not_tried(recipient: Recipient) -> Missed {
    Missed {
        recipient,
        why: "not tried".into(),
    }
}

/// What delivering needs: the project, where its files are, the values.
struct Delivery<'a> {
    p: &'a Prepared,
    place: &'a Place<'a>,
    values: &'a Values,
}

/// The tool, with delivery running beside it until it is done.
fn alongside(d: &Delivery, recipients: Vec<Recipient>, cmd: &mut Command, at: &Scope) -> Res<(ExitStatus, Vec<Missed>)> {
    let Delivery { p, place, values } = *d;
    let stop = AtomicBool::new(false);
    let (status, missed) = std::thread::scope(|s| {
        let go_on = || !stop.load(Ordering::Relaxed);
        let delivery = s.spawn(move || vms::deliver(p, place, values, recipients, &go_on));
        let status = process::run_step(cmd, at);
        stop.store(true, Ordering::Relaxed);
        (status, delivery.join())
    });
    let missed = missed.map_err(|_| Failure::new(ErrorCode::Io, "delivering the secrets stopped unexpectedly"))?;
    Ok((status?, missed))
}

/// An error before the tool starts when a recipient can't be reached from this host.
fn reachable(recipients: &[Recipient]) -> Res<()> {
    vms::reachable(recipients).map_err(|why| Failure::new(ErrorCode::ToolMissing, why).into())
}

/// An error naming every recipient left without its secrets.
fn check(missed: &[Missed]) -> Res<()> {
    if missed.is_empty() {
        return Ok(());
    }
    let message = format!("couldn't deliver the secrets:\n  {}", vms::report(missed));
    Err(Failure::new(ErrorCode::ToolFailed, message).into())
}
