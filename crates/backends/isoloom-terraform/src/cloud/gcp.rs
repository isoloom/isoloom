//! Google Cloud: what its `cloud-vm` module ([`vm`]) and its `cloud-docker` module
//! ([`docker`]) share: the project (its own unless one is given) and the region, the provider,
//! the Compute API, and an instance's public address.

pub(crate) mod docker;
pub(crate) mod vm;

use isoloom_plan::Environment;
use isoloom_render::gcp_project_name;
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, Expression, Module, Template, attr, binary, boolean, conditional, index, null, number, reference, string,
};

use crate::module::resource;
use crate::module::variable::Variable;

/// The `project`, `billing_account`, `org_id` and `region` variables: an existing project, or a
/// billing account for a project of its own; Paris by default.
pub(crate) fn variables() -> [Block; 4] {
    [
        Variable::new("project", "string")
            .default(string(""))
            .describe("An existing project; or leave empty and give billing_account for a project of its own")
            .block(),
        Variable::new("billing_account", "string")
            .default(string(""))
            .describe("With no project: create one for this environment, billed here, deleted with it")
            .block(),
        Variable::new("org_id", "string").default(string("")).block(),
        Variable::new("region", "string").default(string("europe-west9")).block(),
    ]
}

/// The provider, in the region.
pub(crate) fn provider() -> Block {
    Block::builder("provider")
        .add_label("google")
        .add_attribute(("region", reference("var.region")))
        .build()
}

/// The provider, and the project local: the project given, else one of its own ([`project`]).
pub(crate) fn provider_and_project(env: &Environment, m: &mut Module) -> Vec<(&'static str, Expression)> {
    m.block(provider());
    vec![("project", project(env, m))]
}

/// `var.project == "" ? 1 : 0`: a project of its own when none is given.
pub(crate) fn own_project() -> Expression {
    conditional(binary(reference("var.project"), BinaryOperator::Eq, string("")), number(1), number(0))
}

/// The project of its own when none is given (everything goes when the environment is
/// destroyed) and the Compute API in it; the module's project, as `local.project` takes it.
pub(crate) fn project(env: &Environment, m: &mut Module) -> Expression {
    let org = conditional(binary(reference("var.org_id"), BinaryOperator::Eq, string("")), null(), reference("var.org_id"));
    let id = Template::text("isoloom-").with_expr(reference("terraform_data.id.output"));
    m.commented(
        "A project of its own when none is given: everything goes when the environment is destroyed.",
        resource("google_project", "env")
            .add_attribute(("count", own_project()))
            .add_attribute(("name", string(gcp_project_name(&env.name))))
            .add_attribute(("project_id", id.into_expression()))
            .add_attribute(("billing_account", reference("var.billing_account")))
            .add_attribute(("org_id", org))
            .add_attribute(("deletion_policy", string("DELETE")))
            .add_attribute(("auto_create_network", boolean(false)))
            .build(),
    );
    m.block(
        resource("google_project_service", "compute")
            .add_attribute(("count", own_project()))
            .add_attribute(("project", attr(index(reference("google_project.env"), 0), "project_id")))
            .add_attribute(("service", string("compute.googleapis.com")))
            .add_attribute(("disable_on_destroy", boolean(false)))
            .build(),
    );
    conditional(
        binary(reference("var.project"), BinaryOperator::NotEq, string("")),
        reference("var.project"),
        attr(index(reference("google_project_service.compute"), 0), "project"),
    )
}

/// The public address of instance `id` (its resource name): its first interface's.
pub(crate) fn public_ip(id: &str) -> Expression {
    let nic = index(reference(&format!("google_compute_instance.{id}.network_interface")), 0);
    attr(index(attr(nic, "access_config"), 0), "nat_ip")
}
