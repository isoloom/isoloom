//! Every reference in every module the Terraform backends write names a block the module
//! declares: a resource's address is spelled where it is declared and again where it is read,
//! and the two can drift (a name sanitized one way and read another, a resource renamed in one
//! place), which `terraform plan` only finds on the user's machine.

use std::collections::BTreeSet;
use std::path::Path;

use crate::testing::{Outcome, planned};
use hcl::template::{Directive, Element, Template};
use hcl::{Block, BlockLabel, Body, Expression, ObjectKey, Structure, TraversalOperator};
use isoloom_render::backend::Backend;
use isoloom_terraform::{CloudVmBackend, EsxiBackend, ProxmoxBackend};

/// A resource or data source's address: whether it is data, its type and its name.
type Address = (bool, String, String);

/// The blocks a module declares, and the resource and data addresses its expressions read.
#[derive(Default)]
struct Module {
    declared: BTreeSet<Address>,
    read: BTreeSet<Address>,
}

/// The text of a block label.
fn label(l: &BlockLabel) -> &str {
    match l {
        BlockLabel::Identifier(i) => i.as_str(),
        BlockLabel::String(s) => s,
    }
}

impl Module {
    fn body(&mut self, body: &Body, scope: &BTreeSet<String>) {
        for structure in body {
            match structure {
                Structure::Attribute(a) => self.expr(&a.expr, scope),
                Structure::Block(b) => self.block(b, scope),
            }
        }
    }

    fn block(&mut self, b: &Block, scope: &BTreeSet<String>) {
        let labels: Vec<&str> = b.labels.iter().map(label).collect();
        match (b.identifier.as_str(), labels.as_slice()) {
            ("resource", [kind, name]) => {
                self.declared.insert((false, (*kind).to_owned(), (*name).to_owned()));
            }
            ("data", [kind, name]) => {
                self.declared.insert((true, (*kind).to_owned(), (*name).to_owned()));
            }
            _ => {}
        }
        // A `dynamic "x"` block reads its element as `x.value` inside its content.
        let mut inner = scope.clone();
        if b.identifier.as_str() == "dynamic"
            && let [name] = labels.as_slice()
        {
            inner.insert((*name).to_owned());
        }
        self.body(&b.body, &inner);
    }

    fn template(&mut self, t: &Template, scope: &BTreeSet<String>) {
        for element in t.elements() {
            match element {
                Element::Literal(_) => {}
                Element::Interpolation(i) => self.expr(&i.expr, scope),
                Element::Directive(d) => match d.as_ref() {
                    Directive::If(d) => {
                        self.expr(&d.cond_expr, scope);
                        self.template(&d.true_template, scope);
                        if let Some(f) = &d.false_template {
                            self.template(f, scope);
                        }
                    }
                    Directive::For(d) => {
                        self.expr(&d.collection_expr, scope);
                        let mut inner = scope.clone();
                        inner.extend(d.key_var.iter().chain([&d.value_var]).map(|v| v.as_str().to_owned()));
                        self.template(&d.template, &inner);
                    }
                },
            }
        }
    }

    fn expr(&mut self, e: &Expression, scope: &BTreeSet<String>) {
        match e {
            Expression::Array(items) => items.iter().for_each(|i| self.expr(i, scope)),
            Expression::Object(o) => {
                for (k, v) in o {
                    if let ObjectKey::Expression(k) = k {
                        self.expr(k, scope);
                    }
                    self.expr(v, scope);
                }
            }
            Expression::TemplateExpr(t) => {
                if let Ok(t) = Template::from_expr(t) {
                    self.template(&t, scope);
                }
            }
            Expression::Variable(v) => self.root(v.as_str(), &[], scope),
            Expression::Traversal(t) => {
                for op in &t.operators {
                    if let TraversalOperator::Index(i) = op {
                        self.expr(i, scope);
                    }
                }
                match &t.expr {
                    Expression::Variable(v) => self.root(v.as_str(), &t.operators, scope),
                    other => self.expr(other, scope),
                }
            }
            Expression::FuncCall(f) => f.args.iter().for_each(|a| self.expr(a, scope)),
            Expression::Parenthesis(p) => self.expr(p, scope),
            Expression::Conditional(c) => {
                self.expr(&c.cond_expr, scope);
                self.expr(&c.true_expr, scope);
                self.expr(&c.false_expr, scope);
            }
            Expression::Operation(o) => match o.as_ref() {
                hcl::Operation::Unary(u) => self.expr(&u.expr, scope),
                hcl::Operation::Binary(b) => {
                    self.expr(&b.lhs_expr, scope);
                    self.expr(&b.rhs_expr, scope);
                }
            },
            Expression::ForExpr(f) => {
                self.expr(&f.collection_expr, scope);
                let mut inner = scope.clone();
                inner.extend(f.key_var.iter().chain([&f.value_var]).map(|v| v.as_str().to_owned()));
                for part in [f.key_expr.as_ref(), Some(&f.value_expr), f.cond_expr.as_ref()].into_iter().flatten() {
                    self.expr(part, &inner);
                }
            }
            _ => {}
        }
    }

    /// A traversal from `root`: a resource (`aws_subnet.lan.id`, a type has an underscore) or a
    /// data source (`data.aws_ami.ubuntu.id`) is read; variables, locals, modules, `each`,
    /// `count`, `self`, and names a `for` or `dynamic` brings into scope are not addresses.
    fn root(&mut self, root: &str, ops: &[TraversalOperator], scope: &BTreeSet<String>) {
        let attr = |i: usize| match ops.get(i) {
            Some(TraversalOperator::GetAttr(a)) => Some(a.as_str().to_owned()),
            _ => None,
        };
        if root == "data" {
            if let (Some(kind), Some(name)) = (attr(0), attr(1)) {
                self.read.insert((true, kind, name));
            }
        } else if root.contains('_')
            && !scope.contains(root)
            && let Some(name) = attr(0)
        {
            self.read.insert((false, root.to_owned(), name));
        }
    }
}

/// What every `.tf` file `backend` writes for example `name` reads without declaring, per file,
/// and how many addresses it read; nothing when the backend refuses the example.
fn dangling(backend: &dyn Backend, name: &str) -> Outcome<(Vec<String>, usize)> {
    let lab = crate::testing::example(name)?;
    let Ok(env) = planned(&lab, backend.target()) else {
        return Ok((Vec::new(), 0));
    };
    let Ok(files) = backend.render(&env) else { return Ok((Vec::new(), 0)) };
    let mut out = Vec::new();
    let mut reads = 0;
    for f in files
        .iter()
        .filter(|f| Path::new(&f.path).extension().is_some_and(|x| x.eq_ignore_ascii_case("tf")))
    {
        let body: Body = hcl::parse(&f.contents).map_err(|e| format!("{}: {e}", f.path))?;
        let mut module = Module::default();
        module.body(&body, &BTreeSet::new());
        reads += module.read.len();
        for (data, kind, ref_name) in module.read.difference(&module.declared) {
            let prefix = if *data { "data." } else { "" };
            out.push(format!("{name} {}: {prefix}{kind}.{ref_name}", f.path));
        }
    }
    Ok((out, reads))
}

#[test]
fn every_reference_names_a_block_of_its_module() -> Outcome {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
    let mut names: Vec<String> = std::fs::read_dir(&examples)?
        .filter_map(Result::ok)
        .filter(|e| e.path().join("isoloom.yml").is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();
    let backends: [&dyn Backend; 3] = [&CloudVmBackend, &ProxmoxBackend, &EsxiBackend];
    let mut found = Vec::new();
    let mut rendered = 0;
    let mut reads = 0;
    for name in &names {
        for backend in backends {
            let lab = crate::testing::example(name)?;
            if planned(&lab, backend.target()).is_ok_and(|env| backend.render(&env).is_ok()) {
                rendered += 1;
            }
            let (dangling, read) = dangling(backend, name)?;
            found.extend(dangling);
            reads += read;
        }
    }
    assert!(found.is_empty(), "references to no block of their module:\n{}", found.join("\n"));
    // Most examples render on at least one of these backends: a handful means the loop read nothing.
    assert!(rendered > 30, "only {rendered} modules rendered");
    assert!(reads > 1000, "only {reads} resource and data references read");
    Ok(())
}
