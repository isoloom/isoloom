//! Azure: what its `cloud-vm` module ([`vm`]) and its `cloud-docker` module ([`docker`])
//! share: the subscription and the region, the provider, and the environment's instance with
//! the tags every resource carries.

pub(crate) mod docker;
pub(crate) mod vm;

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, null, reference, string};

use crate::cloud::instance;
use crate::module::variable::Variable;

/// The `subscription_id` and `region` variables: the operator's default subscription, Sweden
/// by default.
pub(crate) fn variables() -> [Block; 2] {
    [
        Variable::new("subscription_id", "string")
            .default(null())
            .describe("Default: ARM_SUBSCRIPTION_ID from the environment")
            .block(),
        Variable::new("region", "string").default(string("swedencentral")).block(),
    ]
}

/// The provider, the environment's random id, the names and the tags every resource carries
/// (`local.tags`).
pub(crate) fn base(env: &Environment) -> Vec<Block> {
    let mut blocks = vec![
        Block::builder("provider")
            .add_label("azurerm")
            .add_block(Block::builder("features").build())
            .add_attribute(("subscription_id", reference("var.subscription_id")))
            .build(),
    ];
    blocks.extend(instance::id_and_locals(env, vec![("tags", instance::tags(env))]));
    blocks
}
