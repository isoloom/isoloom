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

/// The entries of the object attribute `key` set directly in `body` (`tags = { … }`), or of every
/// object written in it (`merge({ … }, var.x == "" ? {} : { … })`), in order: each key as text,
/// each value a string's text or as HCL writes anything else (`local.name`). Empty when `body`
/// doesn't set `key`.
#[must_use]
pub fn hcl_map(body: &hcl::Body, key: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(expr) = hcl_attribute(body, key) {
        entries_of(expr, &mut out);
    }
    out
}

fn entries_of(expr: &hcl::Expression, out: &mut Vec<(String, String)>) {
    use hcl::Expression as E;
    let text = |e: &hcl::Expression| match e {
        E::String(s) => s.clone(),
        other => hcl::format::to_string(other).unwrap_or_default(),
    };
    match expr {
        E::Object(object) => {
            for (k, v) in object {
                let key = match k {
                    hcl::ObjectKey::Identifier(id) => id.to_string(),
                    hcl::ObjectKey::Expression(e) => text(e),
                    other => format!("{other:?}"),
                };
                out.push((key, text(v)));
            }
        }
        E::FuncCall(call) => call.args.iter().for_each(|e| entries_of(e, out)),
        E::Conditional(c) => {
            entries_of(&c.true_expr, out);
            entries_of(&c.false_expr, out);
        }
        E::Parenthesis(inner) => entries_of(inner, out),
        _ => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    const TF: &str = r#"
resource "aws_instance" "web" {
  instance_type = "t3.micro"
  subnet_id     = aws_subnet.lan.id
  root_block_device { volume_size = 20 }
  network_interface { device_index = 0 }
  network_interface { device_index = 1 }
  tags = { Name = "web" }
}
resource "aws_instance" "db" { instance_type = "t3.small" }
resource "aws_vpc" "lab" {}
data "aws_ami" "debian" {}
variable "region" { default = "eu-west-3" }
"#;

    #[test]
    fn a_module_is_read_by_block_labels_and_attribute() {
        let body = hcl_body(TF);
        assert_eq!(hcl_names(&body, "resource", "aws_instance"), ["web", "db"]);
        assert_eq!(hcl_names(&body, "data", "aws_ami"), ["debian"]);
        assert_eq!(hcl_blocks(&body, "resource", &["aws_instance"]).len(), 2, "a label prefix");
        assert_eq!(hcl_blocks(&body, "variable", &["region"]).len(), 1);
        let web = hcl_one(&body, "aws_instance", "web");
        assert_eq!(hcl_text(web, "instance_type"), "t3.micro", "a string's value");
        assert_eq!(hcl_text(web, "subnet_id"), "aws_subnet.lan.id", "anything else as HCL writes it");
        assert_eq!(hcl_attribute(web, "volume_size"), None, "directly in the body only");
        assert_eq!(hcl_values(web, "volume_size"), ["20"], "nested blocks too, by name");
        let interfaces: Vec<Vec<String>> = hcl_nested(web, "network_interface")
            .iter()
            .map(|b| hcl_values(b.body(), "device_index"))
            .collect();
        assert_eq!(interfaces, [["0"], ["1"]]);
        assert_eq!(hcl_resource(TF, "aws_instance", "db"), hcl_one(&body, "aws_instance", "db").clone());
    }

    #[test]
    #[should_panic(expected = "no attribute `ami`")]
    fn an_attribute_that_isnt_set_fails_the_test_rather_than_read_empty() {
        let body = hcl_body(TF);
        let _text = hcl_text(hcl_one(&body, "aws_instance", "web"), "ami");
    }

    #[test]
    #[should_panic(expected = "0 resource \"aws_instance\" \"cache\"")]
    fn a_resource_that_isnt_there_fails_the_test() {
        let _body = hcl_one(&hcl_body(TF), "aws_instance", "cache");
    }

    #[test]
    #[should_panic(expected = "2 `resource \"aws_vpc\" \"lab\"`")]
    fn a_resource_written_twice_fails_the_test() {
        let _body = hcl_resource("resource \"aws_vpc\" \"lab\" {}\nresource \"aws_vpc\" \"lab\" {}\n", "aws_vpc", "lab");
    }

    #[test]
    #[should_panic(expected = "not HCL")]
    fn text_that_isnt_hcl_fails_the_test() {
        let _body = hcl_body("resource \"a\" {");
    }

    #[test]
    fn every_literal_is_read_from_lists_objects_calls_and_conditionals_never_a_reference() {
        let body = hcl_body(
            "resource \"x\" \"y\" {\n  ports = [22, 80]\n  open  = true\n  cidr  = cidrsubnet(\"10.0.0.0/16\", 8, 1)\n  name  = var.x ? \"a\" : \"b\"\n  ref   = aws_vpc.lab.id\n  rule { note = \"in a block\" }\n}\n",
        );
        assert_eq!(hcl_literals(&body), ["22", "80", "true", "10.0.0.0/16", "8", "1", "a", "b", "in a block"]);
    }

    #[test]
    fn an_hcl_map_is_read_by_entry_from_every_object_written_in_it() {
        let body = hcl_body("locals {\n  tags = merge({ \"Cost.Center\" = \"R&D\", team = local.team }, var.x == \"\" ? {} : { at = var.x })\n}\n");
        let locals = hcl_blocks(&body, "locals", &[]);
        let tags = locals.first().map(|l| hcl_map(l.body(), "tags")).unwrap_or_default();
        let entries = [("Cost.Center", "R&D"), ("team", "local.team"), ("at", "var.x")].map(|(k, v)| (k.to_owned(), v.to_owned()));
        assert_eq!(tags, entries);
        assert_eq!(hcl_map(&body, "nothing"), Vec::new());
    }
}
