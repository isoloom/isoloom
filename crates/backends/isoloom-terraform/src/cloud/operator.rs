//! What the operator gives every cloud module, after the cloud's own variables: who may reach
//! the machines, the runner's SSH key, the expiry and the launch-time inputs. How a module is
//! started is its cloud's (`Base::usage`).

use isoloom_render::hcl_builder::{Block, Module, string};

use crate::module::Usage;
use crate::module::variable::{self, Variable};

/// Who may reach the machines, as a module's usage tells the operator to give it.
pub(crate) const ALLOWED: &str = "-var allowed_cidr=<your IP>/32 ";

/// The usage of a cloud whose provider reads its token from the environment: `auth` says which
/// (`# Auth:` line), then `apply` takes who may reach the machines and their key.
pub(crate) const fn with_token<'a>(folder: &'a str, auth: &'a str) -> Usage<'a> {
    Usage {
        before: auth,
        ..Usage::with_runner_key(folder, ALLOWED)
    }
}

/// A provider configured by the environment alone (its token): `provider "<name>" {}`.
pub(crate) fn bare_provider(m: &mut Module, name: &str) {
    m.block(Block::builder("provider").add_label(name).build());
}

/// Who may reach the machines, the runner's key and the run's expiry: the operator variables a
/// cloud module always has, the launch-time inputs apart (`cloud-docker` has no input variable;
/// its containers get the values over SSH).
pub(crate) fn host_reaching() -> Vec<Variable> {
    vec![
        Variable::new("allowed_cidr", "string").describe("Who may reach the machines (SSH and the published ports), e.g. your IP/32"),
        variable::ssh_public_key(true),
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
