//! The module's outputs (the ones every cloud module gives, `crate::cloud::vm::outputs`), as
//! AWS reaches its machines.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::Module;

use super::image_user;
use super::machines::{controller_ip, public_ip};
use crate::cloud::vm::outputs::{self, Reach};
use crate::machine::controller;

/// The outputs every cloud module gives.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    // The same user the driver gives the run manifest's guests: one source for both.
    let reach = Reach {
        public_ip: &|n| public_ip(env, n),
        user: &|mp| image_user(mp.os.as_deref().unwrap_or_default()).to_owned(),
        controller: Some((controller_ip(), image_user(controller::OS))),
    };
    outputs::write(env, &reach, m);
}
