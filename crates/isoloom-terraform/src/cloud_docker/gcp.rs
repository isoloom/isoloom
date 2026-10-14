//! Google Cloud: a project of its own unless one is given, a network and subnet, a firewall, and
//! a Debian instance.

use isoloom_plan::Environment;
use isoloom_render::gcp_project_name;
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, BlockBuilder, Expression, Module, Template, attr, binary, boolean, conditional, index, list, null, number, object, reference, string,
};
use isoloom_spec::model::Protocol;

use super::host::{self, Reach, Sizing, port_strings, published};
use crate::vm::module::{Variable, terraform};

/// The module, from the plan.
pub fn module(env: &Environment) -> Module {
    let mut m = Module::new(host::preamble(super::DIR, "gcp", "-var allowed_cidr=<your IP>/32 "));
    m.block(terraform(&[("google", "hashicorp/google", "~> 8.0")]));
    m.blocks(variables(env));
    m.blocks(host::variables(env, true));
    m.block(
        Block::builder("provider")
            .add_label("google")
            .add_attribute(("region", reference("var.region")))
            .build(),
    );
    m.commented(
        "A project of its own when none is given: everything goes when the environment is destroyed.",
        project(env),
    );
    m.block(compute_service());
    m.block(host::id());
    let project = conditional(
        project_given(true),
        reference("var.project"),
        attr(index(reference("google_project_service.compute"), 0), "project"),
    );
    m.block(host::locals(env, vec![("project", project)]));
    m.blocks(network(env));
    m.block(instance(env));
    let reach = Reach {
        host: nat_ip(),
        user: "isoloom",
        replace_on: reference("google_compute_instance.env.id"),
        auto_stop: true,
    };
    host::environment(env, &reach, &mut m);
    m
}

/// `google_compute_instance.env.network_interface[0].access_config[0].nat_ip`.
fn nat_ip() -> Expression {
    let nic = index(reference("google_compute_instance.env.network_interface"), 0);
    attr(index(attr(nic, "access_config"), 0), "nat_ip")
}

/// `var.project != ""` when `given`, else `var.project == ""`.
fn project_given(given: bool) -> Expression {
    let op = if given { BinaryOperator::NotEq } else { BinaryOperator::Eq };
    binary(reference("var.project"), op, string(""))
}

fn variables(env: &Environment) -> Vec<Block> {
    let machine_type = Sizing::of(env).pick(["e2-medium", "e2-standard-2", "e2-standard-4"]);
    vec![
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
        Variable::new("machine_type", "string").default(string(machine_type)).block(),
    ]
}

/// `count = var.project == "" ? 1 : 0`: made only without a project.
fn only_without_project(b: BlockBuilder) -> BlockBuilder {
    b.add_attribute(("count", conditional(project_given(false), number(1), number(0))))
}

fn project(env: &Environment) -> Block {
    let org = conditional(binary(reference("var.org_id"), BinaryOperator::Eq, string("")), null(), reference("var.org_id"));
    only_without_project(Block::builder("resource").add_label("google_project").add_label("env"))
        .add_attribute(("name", string(gcp_project_name(&env.name))))
        .add_attribute((
            "project_id",
            Template::text("isoloom-").with_expr(reference("terraform_data.id.output")).into_expression(),
        ))
        .add_attribute(("billing_account", reference("var.billing_account")))
        .add_attribute(("org_id", org))
        .add_attribute(("deletion_policy", string("DELETE")))
        .add_attribute(("auto_create_network", boolean(false)))
        .build()
}

fn compute_service() -> Block {
    only_without_project(Block::builder("resource").add_label("google_project_service").add_label("compute"))
        .add_attribute(("project", attr(index(reference("google_project.env"), 0), "project_id")))
        .add_attribute(("service", string("compute.googleapis.com")))
        .add_attribute(("disable_on_destroy", boolean(false)))
        .build()
}

/// `resource "<kind>" "env"` in the project, named after the environment.
fn resource(kind: &str) -> BlockBuilder {
    Block::builder("resource")
        .add_label(kind)
        .add_label("env")
        .add_attribute(("project", reference("local.project")))
        .add_attribute(("name", reference("local.name")))
}

/// One `allow` of the firewall.
fn allow(protocol: &str, ports: &[u16]) -> Block {
    Block::builder("allow")
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("ports", port_strings(ports)))
        .build()
}

/// The network, its subnet, and the firewall: SSH and the published ports from `allowed_cidr`.
fn network(env: &Environment) -> Vec<Block> {
    let mut tcp = vec![22];
    tcp.extend(published(env, Protocol::Tcp));
    let udp = published(env, Protocol::Udp);
    let mut firewall = resource("google_compute_firewall")
        .add_attribute(("network", reference("google_compute_network.env.id")))
        .add_attribute(("source_ranges", list([reference("var.allowed_cidr")])))
        .add_block(allow("tcp", &tcp));
    if !udp.is_empty() {
        firewall = firewall.add_block(allow("udp", &udp));
    }
    vec![
        resource("google_compute_network")
            .add_attribute(("auto_create_subnetworks", boolean(false)))
            .build(),
        resource("google_compute_subnetwork")
            .add_attribute(("network", reference("google_compute_network.env.id")))
            .add_attribute(("ip_cidr_range", string("10.42.1.0/24")))
            .add_attribute(("region", reference("var.region")))
            .build(),
        firewall.build(),
    ]
}

/// Debian 12 in the region's first zone, the `isoloom` user with the operator's key.
fn instance(env: &Environment) -> Block {
    let ssh_keys = Template::text("isoloom:").with_expr(reference("var.ssh_public_key"));
    resource("google_compute_instance")
        .add_attribute(("machine_type", reference("var.machine_type")))
        .add_attribute(("zone", Template::new().with_expr(reference("var.region")).with_text("-a").into_expression()))
        .add_attribute(("labels", host::tags(env)))
        .add_block(
            Block::builder("boot_disk")
                .add_block(
                    Block::builder("initialize_params")
                        .add_attribute(("image", string("debian-cloud/debian-12")))
                        .add_attribute(("size", number(30)))
                        .build(),
                )
                .build(),
        )
        .add_block(
            Block::builder("network_interface")
                .add_attribute(("subnetwork", reference("google_compute_subnetwork.env.id")))
                .add_block(Block::builder("access_config").build())
                .build(),
        )
        .add_attribute(("metadata", object([("ssh-keys", ssh_keys.into_expression())])))
        .build()
}
