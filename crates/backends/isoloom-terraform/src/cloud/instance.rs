//! The environment's instance in a cloud: its random id and name (`local.name`, so two
//! instances of one spec never collide), its project root, the tags or labels every resource
//! carries (what a reaper reads), and the `lifecycle` of a VM set up once from its user data.

use crate::hcl_builder::{BinaryOperator, Block, BlockBuilder, Expression, Template, binary, call, conditional, list, number, object, reference, string};
use isoloom_plan::Environment;

/// The tags on every resource: the spec's own (`tags:`), then the environment, who manages it,
/// its instance and its expiry (what a reaper reads).
pub(crate) fn tags(env: &Environment) -> Expression {
    let mut tags = named(env);
    tags.push(("isoloom-expires-at", reference("var.expires_at")));
    with_own(isoloom_render::tags::written(env.spec()), tags)
}

/// The same as labels (Google Cloud's): the spec's own made labels, then Isoloom's.
pub(crate) fn labels(env: &Environment) -> Expression {
    let mut tags = named(env);
    tags.push(("isoloom-expires-at", reference("var.expires_at")));
    with_own(isoloom_render::tags::labels(env.spec()), tags)
}

/// `own` (the spec's), then Isoloom's `tags`, as one object.
fn with_own(own: Vec<(String, String)>, tags: Vec<(&'static str, Expression)>) -> Expression {
    let own = own.into_iter().map(|(k, v)| (k, string(v)));
    object(own.chain(tags.into_iter().map(|(k, v)| (k.to_owned(), v))).collect::<Vec<_>>())
}

/// The same for a cloud whose tag values can't be counted on to take an empty one (Oracle
/// Cloud's free-form tags): the expiry only when the environment has one.
pub(crate) fn tags_when_set(env: &Environment) -> Expression {
    let none = binary(reference("var.expires_at"), BinaryOperator::Eq, string(""));
    let expiry = object([("isoloom-expires-at", reference("var.expires_at"))]);
    call(
        "merge",
        [
            with_own(isoloom_render::tags::written(env.spec()), named(env)),
            conditional(none, object::<&str>([]), expiry),
        ],
    )
}

/// The labels naming the environment, who manages it, and its instance.
fn named(env: &Environment) -> Vec<(&'static str, Expression)> {
    vec![
        ("isoloom-environment", string(env.name.as_str())),
        ("managed-by", string("isoloom")),
        ("isoloom-instance", reference("local.name")),
    ]
}

/// The environment's random id (`terraform_data.id`) and the locals: its `name`, the project's
/// `root` when the module reads it (only when Terraform copies the project, `with_root`; the
/// runner-delivered modules never read it, and a declared-but-unused local is a lint failure),
/// then `extra`.
pub(crate) fn id_and_locals(env: &Environment, extra: Vec<(&str, Expression)>, with_root: bool) -> Vec<Block> {
    let id = call("substr", [call("replace", [call("uuid", []), string("-"), string("")]), number(0), number(8)]);
    let name = Template::text(format!("isoloom-{}-", env.name))
        .with_expr(reference("terraform_data.id.output"))
        .into_expression();
    let mut locals = Block::builder("locals").add_attribute(("name", name));
    if with_root {
        let root = call(
            "abspath",
            [Template::new().with_expr(reference("path.module")).with_text("/../../..").into_expression()],
        );
        locals = locals.add_attribute(("root", root));
    }
    let locals = extra.into_iter().fold(locals, BlockBuilder::add_attribute);
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
            list(
                [string("isoloom"), string(env.name.as_str()), reference("local.name")]
                    .into_iter()
                    .chain(isoloom_render::tags::words(env.spec()).into_iter().map(string)),
            ),
            conditional(none, list(Vec::new()), list([expires])),
        ],
    )
}

/// `"${local.name}<suffix>"`: a resource's name in the environment.
pub(crate) fn name_with(suffix: &str) -> Expression {
    Template::new().with_expr(reference("local.name")).with_text(suffix).into_expression()
}

/// The `lifecycle` of a machine whose user data (`attribute`, as `ignore_changes` names it) runs
/// once, at its first boot: its set-up, or a Docker host's expiry. A later apply with other
/// values in it (the launch's inputs, `var.expires_at` among them, which a run never keeps) leaves
/// the machine as it is, where the cloud would replace it (Azure's `custom_data`, GCP's
/// `metadata_startup_script`, DigitalOcean's `user_data`, Oracle Cloud's `metadata["user_data"]`,
/// Linode's `metadata[0].user_data`: each forces a new VM in the provider version the modules
/// require) or stop and start it (AWS's `user_data_base64`).
pub(crate) fn set_up_once(attribute: Expression) -> BlockBuilder {
    Block::builder("lifecycle").add_attribute(("ignore_changes", list([attribute])))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;

    #[test]
    fn tags_that_cannot_be_empty_carry_the_expiry_only_when_there_is_one() {
        let env = crate::testing::planned("version: 1\nname: lab\nmachines:\n  web: { os: debian@12 }\n", Target::CloudVm);
        let written = tags_when_set(&env).to_string();
        assert!(
            written.starts_with("merge({ isoloom-environment = \"lab\", managed-by = \"isoloom\""),
            "{written}"
        );
        assert!(
            written.ends_with("var.expires_at == \"\" ? {} : { isoloom-expires-at = var.expires_at })"),
            "{written}"
        );
    }
}
