//! The module's outputs (the ones every cloud module gives, `crate::vm::cloud::outputs`), as
//! AWS reaches its machines.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::Module;

use super::image_user;
use super::machines::{controller_ip, public_ip};
use crate::vm::cloud::{Reach, outputs};
use crate::vm::{is_windows, os};

/// The outputs every cloud module gives.
pub fn blocks(env: &Environment, m: &mut Module) {
    let reach = Reach {
        public_ip: &|n| public_ip(env, n),
        user: &|mp| user(mp).to_string(),
        controller: Some((controller_ip(), "admin")),
    };
    outputs(env, &reach, m);
}

/// A machine's SSH user (`isoloom` on Windows).
fn user(mp: &MachinePlan) -> &'static str {
    if is_windows(mp) { "isoloom" } else { image_user(os(mp).unwrap_or_default()) }
}
