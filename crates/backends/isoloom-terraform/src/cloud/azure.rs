//! Azure: what its `cloud-vm` module ([`vm`]) and its `cloud-docker` module ([`docker`])
//! share: the subscription and the region, and the provider with the tags every resource
//! carries.

pub(crate) mod docker;
pub(crate) mod vm;

use crate::hcl_builder::{Block, Expression, Module, null, reference};
use isoloom_plan::Environment;

use super::{Base, operator};
use crate::cloud::instance;
use crate::module::Usage;
use crate::module::variable::Variable;

/// What Azure's modules share: who may reach the machines and their key, the subscription and
/// the region, the provider with the tags, and the one user every VM is given.
pub(crate) const BASE: Base = Base {
    cloud: crate::cloud::Cloud::Azure,
    usage: |folder| Usage::with_runner_key(folder, operator::ALLOWED),
    variables: |region| variables(region).into(),
    provider,
    user: |_| vm::USER,
};

/// The `subscription_id` and `region` variables: the operator's default subscription, `region`
/// by default.
pub(crate) fn variables(region: &str) -> [Variable; 2] {
    [
        Variable::new("subscription_id", "string")
            .default(null())
            .describe("Default: ARM_SUBSCRIPTION_ID from the environment"),
        crate::cloud::region_variable(region),
    ]
}

/// The provider, and the tags every resource carries (`local.tags`).
pub(crate) fn provider(env: &Environment, m: &mut Module) -> Vec<(&'static str, Expression)> {
    m.block(
        Block::builder("provider")
            .add_label("azurerm")
            .add_block(Block::builder("features").build())
            .add_attribute(("subscription_id", reference("var.subscription_id")))
            .build(),
    );
    vec![("tags", instance::tags(env))]
}
