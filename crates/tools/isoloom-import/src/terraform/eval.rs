//! Static evaluation of the configuration: `variable` defaults and `locals` into a context, then
//! every resource expanded over its `for_each` or `count` into instances.

use hcl::eval::{Context, Evaluate};
use hcl::{Block, Body, Expression, Value};
use serde_json::{Value as Json, json};

use super::instance::Instance;
use crate::draft::{NoteKind, Notes};

/// An expression as JSON when it's a value, else its text (an unresolved reference).
fn expr_json(e: &Expression) -> Json {
    match e {
        Expression::Null => Json::Null,
        Expression::Bool(b) => json!(b),
        Expression::Number(n) => serde_json::to_value(n).unwrap_or(Json::Null),
        Expression::String(s) => json!(s),
        Expression::Array(a) => Json::Array(a.iter().map(expr_json).collect()),
        Expression::Object(o) => Json::Object(o.iter().map(|(k, v)| (k.to_string().trim_matches('"').to_owned(), expr_json(v))).collect()),
        other => json!(hcl::format::to_string(other).unwrap_or_default()),
    }
}

/// A body's attributes and nested blocks as JSON (blocks of the same name collected in a list).
fn body_json(body: &Body) -> Json {
    let mut out = serde_json::Map::new();
    for a in body.attributes() {
        out.insert(a.key().to_owned(), expr_json(a.expr()));
    }
    for b in body.blocks() {
        let entry = out.entry(b.identifier().to_owned()).or_insert_with(|| Json::Array(Vec::new()));
        if let Json::Array(list) = entry {
            list.push(body_json(b.body()));
        }
    }
    Json::Object(out)
}

/// One instance of an expanded resource: its key, and the `each` or `count` variable it sees.
type Expansion = (Option<String>, Option<(&'static str, Value)>);

/// The blocks of one kind (`variable`, `locals`, `resource`) across every file.
fn blocks<'a>(bodies: &'a [Body], identifier: &'a str) -> impl Iterator<Item = &'a Block> {
    bodies.iter().flat_map(move |body| body.blocks().filter(move |b| b.identifier() == identifier))
}

/// What resources are evaluated with: `var` (the variables' defaults), `local` and `path.module`.
fn context(bodies: &[Body]) -> Context<'static> {
    // Variables (their defaults) and locals, in order.
    let mut vars = hcl::Map::new();
    for b in blocks(bodies, "variable") {
        if let (Some(name), Some(d)) = (b.labels().first(), b.body().attributes().find(|a| a.key() == "default"))
            && let Ok(v) = d.expr().evaluate(&Context::new())
        {
            vars.insert(name.as_str().to_owned(), v);
        }
    }
    let mut ctx = Context::new();
    ctx.declare_var("var", Value::Object(vars));
    let mut locals = hcl::Map::new();
    for a in blocks(bodies, "locals").flat_map(|b| b.body().attributes()) {
        let mut c = ctx.clone();
        c.declare_var("local", Value::Object(locals.clone()));
        if let Ok(v) = a.expr().evaluate(&c) {
            locals.insert(a.key().to_owned(), v);
        }
    }
    ctx.declare_var("local", Value::Object(locals));
    ctx.declare_var("path", Value::Object([("module".to_owned(), Value::from("."))].into_iter().collect()));
    ctx
}

/// The `for_each` instance with this key and value.
fn each(key: String, value: Value) -> Expansion {
    let each: hcl::Map<String, Value> = [("key".to_owned(), Value::from(key.clone())), ("value".to_owned(), value)]
        .into_iter()
        .collect();
    (Some(key), Some(("each", Value::Object(each))))
}

/// The instances a resource block makes: one, or one per `for_each` entry or `count`. None when
/// that can't be read without running Terraform, with a note at `at`.
fn expansions(b: &Block, ctx: &Context, at: &str, notes: &mut Notes) -> Option<Vec<Expansion>> {
    let attr = |k: &str| b.body().attributes().find(|a| a.key() == k).map(|a| a.expr().clone());
    if let Some(fe) = attr("for_each") {
        match fe.evaluate(ctx) {
            Ok(Value::Object(m)) => Some(m.into_iter().map(|(k, v)| each(k, v)).collect()),
            Ok(Value::Array(a)) => Some(
                a.into_iter()
                    .map(|v| {
                        let k = match &v {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        each(k, v)
                    })
                    .collect(),
            ),
            _ => {
                let text = "its `for_each` couldn't be read without running Terraform: add these machines by hand";
                notes.add(at, NoteKind::Changed, text);
                None
            }
        }
    } else if let Some(c) = attr("count") {
        if let Ok(Value::Number(n)) = c.evaluate(ctx) {
            Some(
                (0..n.as_u64().unwrap_or(0))
                    .map(|i| {
                        let count: hcl::Map<String, Value> = [("index".to_owned(), Value::from(i))].into_iter().collect();
                        (Some(i.to_string()), Some(("count", Value::Object(count))))
                    })
                    .collect(),
            )
        } else {
            notes.add(at, NoteKind::Changed, "its `count` couldn't be read without running Terraform");
            None
        }
    } else {
        Some(vec![(None, None)])
    }
}

/// Evaluates every resource of the configuration, expanding `for_each` and `count`.
pub(super) fn instances(bodies: &[Body], notes: &mut Notes) -> Vec<Instance> {
    let ctx = context(bodies);
    let mut out = Vec::new();
    for b in blocks(bodies, "resource") {
        let (Some(kind), Some(name)) = (b.labels().first(), b.labels().get(1)) else {
            continue;
        };
        let (kind, name) = (kind.as_str().to_owned(), name.as_str().to_owned());
        let Some(expansions) = expansions(b, &ctx, &format!("resource {kind}.{name}"), notes) else {
            continue;
        };
        for (key, extra) in expansions {
            let mut c = ctx.clone();
            if let Some((n, v)) = extra {
                c.declare_var(n, v);
            }
            let mut body = b.body().clone();
            // An expression that can't be evaluated without running Terraform (another
            // resource's attribute) stays as its text, which `resolve` follows: expected.
            body.evaluate_in_place(&c).unwrap_or(());
            out.push(Instance {
                kind: kind.clone(),
                name: name.clone(),
                key,
                attrs: body_json(&body),
            });
        }
    }
    out
}

/// The instance a reference text points at (`aws_subnet.lab.id`, `aws_network_interface.nic["dc01"].id`).
pub(super) fn resolve<'a>(all: &'a [Instance], reference: &str) -> Option<&'a Instance> {
    let r = reference.trim().trim_start_matches("${").trim_end_matches('}');
    all.iter().find(|i| {
        let a = i.address();
        r == a || r.starts_with(&format!("{a}.")) || r.replace("\\\"", "\"").starts_with(&format!("{a}."))
    })
}
