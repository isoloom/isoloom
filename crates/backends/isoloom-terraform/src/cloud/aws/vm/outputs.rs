//! The module's outputs (the ones every cloud module gives, `crate::cloud::vm::outputs`), as
//! AWS reaches its machines.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::Module;

use super::image_user;
use super::machines::{controller_ip, public_ip};
use crate::cloud::vm::outputs::{self, Reach};

/// The outputs every cloud module gives.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let reach = Reach {
        public_ip: &|n| public_ip(env, n),
        user: &|mp| user(mp).to_owned(),
        controller: Some((controller_ip(), "admin")),
    };
    outputs::write(env, &reach, m);
}

/// A machine's SSH user (`isoloom` on Windows).
fn user(mp: &MachinePlan) -> &'static str {
    if mp.is_windows() {
        "isoloom"
    } else {
        image_user(mp.os.as_deref().unwrap_or_default())
    }
}
