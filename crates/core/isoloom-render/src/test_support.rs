//! What tests of the crates above this one build their cases from, behind the `test-support`
//! feature (a dev-dependency's): never part of the API a backend renders with.

use crate::run_manifest::guest::{GuestCommand, GuestCompose};

/// A guest with nothing in it but `status`: no machines, slots, steps or checks.
#[must_use]
pub fn empty_guest(status: GuestCommand) -> GuestCompose {
    GuestCompose {
        machines: Vec::new(),
        networks: Vec::new(),
        slots: Vec::new(),
        params: Vec::new(),
        start: Vec::new(),
        ready: Vec::new(),
        check_setup: Vec::new(),
        check_teardown: Vec::new(),
        runners: Vec::new(),
        status,
    }
}
