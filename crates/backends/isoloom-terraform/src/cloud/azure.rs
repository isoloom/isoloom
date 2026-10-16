//! Azure: what its `cloud-vm` module ([`vm`]) and its `cloud-docker` module ([`docker`])
//! share: the subscription and the region, and the provider with the tags every resource
//! carries.

pub(crate) mod docker;
pub(crate) mod vm;

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Expression, Module, null, reference, string};

use crate::cloud::instance;
use crate::module::variable::Variable;

/// The `subscription_id` and `region` variables: the operator's default subscription, Sweden
/// by default.
pub(crate) fn variables() -> [Variable; 2] {
    [
        Variable::new("subscription_id", "string")
            .default(null())
            .describe("Default: ARM_SUBSCRIPTION_ID from the environment"),
        Variable::new("region", "string").default(string("swedencentral")),
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
