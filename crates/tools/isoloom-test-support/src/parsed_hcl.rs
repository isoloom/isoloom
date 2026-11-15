//! Rendered HCL (Terraform), parsed: its blocks, attributes and literal values.

/// A rendered HCL file (Terraform), parsed.
///
/// # Panics
/// When it isn't HCL.
#[must_use]
pub fn hcl_body(text: &str) -> hcl::Body {
    hcl::parse(text).unwrap_or_else(|e| panic!("not HCL: {e}\n{text}"))
}

/// The blocks of `body` of type `kind` with labels `labels` (`resource`, `["aws_instance", "web"]`).
#[must_use]
pub fn hcl_blocks<'a>(body: &'a hcl::Body, kind: &str, labels: &[&str]) -> Vec<&'a hcl::Block> {
    body.blocks()
        .filter(|b| b.identifier() == kind && b.labels().iter().map(hcl::BlockLabel::as_str).zip(labels).all(|(got, want)| got == *want))
        .collect()
}

/// Every literal value (number, string, boolean) anywhere in `body`, its nested blocks, lists,
/// objects, function arguments and conditionals included, as text: what a test asks of a block
/// ("does it open port 18080?") without matching digits inside a CIDR, a size or a name.
#[must_use]
pub fn hcl_literals(body: &hcl::Body) -> Vec<String> {
    let mut out = Vec::new();
    for structure in body {
        match structure {
            hcl::Structure::Attribute(a) => literals_of(a.expr(), &mut out),
            hcl::Structure::Block(b) => out.extend(hcl_literals(b.body())),
        }
    }
    out
}

/// The literal values of every attribute `key` anywhere in `body`, its nested blocks included
/// (`volume_size` in an instance's `root_block_device`): an attribute read by name, never by a
/// line that holds `key =`.
#[must_use]
pub fn hcl_values(body: &hcl::Body, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for structure in body {
        match structure {
            hcl::Structure::Attribute(a) if a.key() == key => literals_of(a.expr(), &mut out),
            hcl::Structure::Attribute(_) => {}
            hcl::Structure::Block(b) => out.extend(hcl_values(b.body(), key)),
        }
    }
    out
}

/// The expression of attribute `key` set directly in `body` (not in its nested blocks).
#[must_use]
pub fn hcl_attribute<'a>(body: &'a hcl::Body, key: &str) -> Option<&'a hcl::Expression> {
    body.attributes().find(|a| a.key() == key).map(hcl::Attribute::expr)
}

/// Attribute `key` set directly in `body`, as text: a string's value, anything else (a
/// reference, a number, a call) as HCL writes it (`aws_network_interface.fw_dmz.id`, `true`).
///
/// # Panics
/// When `body` doesn't set it.
#[must_use]
pub fn hcl_text(body: &hcl::Body, key: &str) -> String {
    match hcl_attribute(body, key) {
        Some(hcl::Expression::String(s)) => s.clone(),
        Some(expr) => hcl::format::to_string(expr).unwrap_or_else(|e| panic!("`{key}` doesn't format: {e}")),
        None => panic!("no attribute `{key}` in {body:?}"),
    }
}

/// The blocks named `kind` directly in `body` (an instance's `network_interface`).
#[must_use]
pub fn hcl_nested<'a>(body: &'a hcl::Body, kind: &str) -> Vec<&'a hcl::Block> {
    body.blocks().filter(|b| b.identifier() == kind).collect()
}

/// The names of the `resource "<kind>" "<name>"` blocks of `tf` (`data` too, with `data`), in
/// order.
#[must_use]
pub fn hcl_names(body: &hcl::Body, block: &str, kind: &str) -> Vec<String> {
    body.blocks()
        .filter(|b| b.identifier() == block && b.labels().first().is_some_and(|l| l.as_str() == kind))
        .filter_map(|b| b.labels().get(1).map(|l| l.as_str().to_owned()))
        .collect()
}

/// The body of the one `resource "<kind>" "<name>"` of a parsed module.
///
/// # Panics
/// When the module doesn't have exactly one such resource.
#[must_use]
pub fn hcl_one<'b>(body: &'b hcl::Body, kind: &str, name: &str) -> &'b hcl::Body {
    match hcl_blocks(body, "resource", &[kind, name]).as_slice() {
        [one] => one.body(),
        found => panic!("{} resource \"{kind}\" \"{name}\"", found.len()),
    }
}

/// The body of the one `resource "<kind>" "<name>"` of `tf`, parsed.
///
/// # Panics
/// When `tf` isn't HCL or doesn't have exactly one such resource.
#[must_use]
pub fn hcl_resource(tf: &str, kind: &str, name: &str) -> hcl::Body {
    let body = hcl_body(tf);
    let found = hcl_blocks(&body, "resource", &[kind, name]);
    match found.as_slice() {
        [one] => one.body().clone(),
        _ => panic!("{} `resource \"{kind}\" \"{name}\"` in:\n{tf}", found.len()),
    }
}

fn literals_of(expr: &hcl::Expression, out: &mut Vec<String>) {
    use hcl::Expression as E;
    match expr {
        E::Number(n) => out.push(n.to_string()),
        E::String(s) => out.push(s.clone()),
        E::Bool(b) => out.push(b.to_string()),
        E::TemplateExpr(template) => out.push(template.to_string()),
        E::Array(items) => items.iter().for_each(|e| literals_of(e, out)),
        E::Object(object) => object.values().for_each(|e| literals_of(e, out)),
        E::Parenthesis(inner) => literals_of(inner, out),
        E::FuncCall(call) => call.args.iter().for_each(|e| literals_of(e, out)),
        E::Conditional(c) => {
            literals_of(&c.true_expr, out);
            literals_of(&c.false_expr, out);
        }
        _ => {}
    }
}
