//! The environment's instance in a cloud: its random id and name (`local.name`, so two
//! instances of one spec never collide), its project root, and the tags or labels every
//! resource carries (what a reaper reads).

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, BlockBuilder, Expression, Template, binary, call, conditional, list, number, object, reference, string,
};

/// The labels on every resource: the environment, who manages it, its instance and its expiry
/// (what a reaper reads).
pub(crate) fn tags(env: &Environment) -> Expression {
    object([
        ("isoloom-environment", string(env.name.as_str())),
        ("managed-by", string("isoloom")),
        ("isoloom-instance", reference("local.name")),
        ("isoloom-expires-at", reference("var.expires_at")),
    ])
}

/// The environment's random id (`terraform_data.id`) and the locals: its `name`, the project's
/// `root`, then `extra`.
pub(crate) fn id_and_locals(env: &Environment, extra: Vec<(&str, Expression)>) -> Vec<Block> {
    let id = call("substr", [call("replace", [call("uuid", []), string("-"), string("")]), number(0), number(8)]);
    let name = Template::text(format!("isoloom-{}-", env.name))
        .with_expr(reference("terraform_data.id.output"))
        .into_expression();
    let root = call(
        "abspath",
        [Template::new().with_expr(reference("path.module")).with_text("/../../..").into_expression()],
    );
    let locals = extra.into_iter().fold(
        Block::builder("locals").add_attribute(("name", name)).add_attribute(("root", root)),
        BlockBuilder::add_attribute,
    );
    vec![
        Block::builder("resource")
            .add_label("terraform_data")
            .add_label("id")
            .add_attribute(("input", id))
            .add_block(
                Block::builder("lifecycle")
                    .add_attribute(("ignore_changes", list([reference("input")])))
                    .build(),
            )
            .build(),
        locals.build(),
    ]
}

/// The tags of a cloud whose tags are a list of words: `isoloom`, the environment, its
/// instance's name, and `isoloom-expires-<time>` when it has an end.
pub(crate) fn tag_list(env: &Environment) -> Expression {
    let expires = Template::text("isoloom-expires-").with_expr(reference("var.expires_at")).into_expression();
    let none = binary(reference("var.expires_at"), BinaryOperator::Eq, string(""));
    call(
        "concat",
        [
            list([string("isoloom"), string(env.name.as_str()), reference("local.name")]),
            conditional(none, list(Vec::new()), list([expires])),
        ],
    )
}

/// `"${local.name}<suffix>"`: a resource's name in the environment.
pub(crate) fn name_with(suffix: &str) -> Expression {
    Template::new().with_expr(reference("local.name")).with_text(suffix).into_expression()
}
