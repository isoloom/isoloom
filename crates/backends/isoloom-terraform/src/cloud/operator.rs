//! What the operator gives every cloud module, after the cloud's own variables: who may reach
//! the machines, the operator's SSH key, the auto-stop, the expiry, and the launch-time inputs.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, number, string};

use crate::module::inputs;
use crate::module::variable::Variable;

/// Who may reach the machines, as a module's usage tells the operator to give it.
pub(crate) const ALLOWED: &str = "-var allowed_cidr=<your IP>/32 ";

/// The variables every cloud module has (after its own): who may reach the machines, the
/// operator's SSH key and the auto-stop, the expiry its resources are tagged with, then the
/// inputs when the spec has some.
pub(crate) fn variables(env: &Environment) -> Vec<Block> {
    let mut v = vec![
        Variable::new("allowed_cidr", "string")
            .describe("Who may reach the machines (SSH and the published ports), e.g. your IP/32")
            .block(),
    ];
    v.extend(access());
    v.push(
        Variable::new("expires_at", "string")
            .default(string(""))
            .describe("When the environment should end, in Unix seconds (empty: no end), as a tag on every resource so a reaper can find what to destroy")
            .block(),
    );
    v.extend(inputs::variable(env));
    v
}

/// The operator's SSH key, and the auto-stop.
fn access() -> Vec<Block> {
    vec![
        Variable::new("ssh_public_key", "string").block(),
        Variable::new("ssh_private_key_file", "string")
            .describe("The private key of ssh_public_key: Terraform sets the machines up over SSH")
            .block(),
        Variable::new("auto_stop_minutes", "number")
            .default(number(0))
            .describe("Shut the machines down after this many minutes (0: never)")
            .block(),
    ]
}
