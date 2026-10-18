//! What the operator gives every cloud module, after the cloud's own variables: who may reach
//! the machines, the runner's SSH key, the expiry and the launch-time inputs; and how a cloud
//! module is started (through the Isothrum runner, which sets most of them).

use isoloom_render::hcl_builder::string;

use crate::module::Usage;
use crate::module::variable::Variable;

/// Who may reach the machines, as a module's usage tells the operator to give it.
pub(crate) const ALLOWED: &str = "-var allowed_cidr=<your IP>/32 ";

/// What a cloud module's first lines say: `apply` takes who may reach the machines and the key
/// they let in; then the machines wait for the project, which the Isothrum runner delivers.
pub(crate) fn usage(folder: &str) -> Usage<'_> {
    Usage::with_runner_key(folder, ALLOWED)
}

/// Who may reach the machines, the runner's key and the run's expiry: the operator variables a
/// cloud module always has, the launch-time inputs apart (`cloud-docker` has no input variable;
/// its containers get the values over SSH).
pub(crate) fn host_reaching() -> Vec<Variable> {
    vec![
        Variable::new("allowed_cidr", "string").describe("Who may reach the machines (SSH and the published ports), e.g. your IP/32"),
        Variable::new("ssh_public_key", "string")
            .describe("The key the machines let in: the Isothrum runner's (it delivers the project and runs the checks over SSH)"),
        expires_at(),
    ]
}

/// `expires_at`: when the run ends (the runner computes it), as a tag on every resource and the
/// machines' own shutdown time.
fn expires_at() -> Variable {
    Variable::new("expires_at", "string")
        .default(string(""))
        .describe("When the environment should end, in Unix seconds (empty: no end): a tag on every resource, so a reaper finds what to destroy, and the time the machines shut themselves down")
}
