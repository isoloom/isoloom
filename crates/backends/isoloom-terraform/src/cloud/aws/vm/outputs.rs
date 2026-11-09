//! The module's outputs (the ones every cloud module gives, `crate::cloud::vm::outputs`), as
//! AWS reaches its machines.

use crate::hcl_builder::Module;
use isoloom_plan::Environment;

use super::machines::{controller_ip, public_ip};
use crate::cloud::vm::outputs::{self, Reach};

/// The outputs every cloud module gives.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let reach = Reach {
        public_ip: &|n| public_ip(env, n),
        user: crate::cloud::aws::BASE.user,
        controller: Some(controller_ip()),
    };
    outputs::write(env, &reach, m);
}
