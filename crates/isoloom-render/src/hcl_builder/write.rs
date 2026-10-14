//! Writing a module: `hcl-rs` structures in the layout `terraform fmt` leaves unchanged.
//!
//! `terraform fmt` indents by bracket depth and aligns the `=` of consecutive attributes whose
//! value fits on their line (a value that opens a bracket it doesn't close starts no alignment
//! run and ends the current one). Leaf expressions are written by `hcl-rs` in its compact form;
//! lists, objects and calls are laid out here.

use hcl::expr::{Expression, FuncCall, Object, ObjectKey};
use hcl::format::{Format, Formatter};
use hcl::{Block, BlockLabel, Body, Structure};

use super::escape::{quoted_inside, strings_escaped};

/// A list, an object or a call longer than this, on one line, is written across lines.
pub const WIDTH: usize = 100;

/// A Terraform file: its preamble (comment lines), then top-level blocks, each with the comment
/// above it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Module {
    preamble: String,
    sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq)]
enum Section {
    Block { comment: Option<String>, block: Block },
    Fragment(String),
}

impl Module {
    /// A module starting with `preamble`, written as it is (comment lines, each ending with a
    /// newline).
    pub fn new(preamble: impl Into<String>) -> Module {
        Module {
            preamble: preamble.into(),
            sections: Vec::new(),
        }
    }

    /// Adds a top-level block.
    pub fn block(&mut self, block: Block) {
        self.sections.push(Section::Block { comment: None, block });
    }

    /// Adds a top-level block with a comment above it (each line of `comment` becomes a `#` line).
    pub fn commented(&mut self, comment: &str, block: Block) {
        self.sections.push(Section::Block {
            comment: Some(comment.to_string()),
            block,
        });
    }

    /// Adds blocks.
    pub fn blocks(&mut self, blocks: impl IntoIterator<Item = Block>) {
        for b in blocks {
            self.block(b);
        }
    }

    /// Adds a fixed fragment of HCL, written as it is: for the static parts shared with modules
    /// not built here yet (the `.isoloomignore` lists).
    pub fn fragment(&mut self, hcl: &str) {
        self.sections.push(Section::Fragment(hcl.trim_matches('\n').to_string()));
    }

    /// The module as text.
    pub fn render(&self) -> Result<String, hcl::Error> {
        let mut out = self.preamble.clone();
        for (i, s) in self.sections.iter().enumerate() {
            if i > 0 || !out.is_empty() {
                out.push('\n');
            }
            match s {
                Section::Block { comment, block: b } => {
                    for line in comment.iter().flat_map(|c| c.lines()) {
                        out.push_str(if line.is_empty() { "#" } else { "# " });
                        out.push_str(line);
                        out.push('\n');
                    }
                    block(&mut out, b, 0)?;
                }
                Section::Fragment(f) => {
                    out.push_str(f);
                    out.push('\n');
                }
            }
        }
        Ok(out)
    }
}

/// Two spaces per level.
fn pad(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn block(out: &mut String, b: &Block, indent: usize) -> Result<(), hcl::Error> {
    pad(out, indent);
    out.push_str(b.identifier());
    for l in b.labels() {
        out.push(' ');
        match l {
            BlockLabel::Identifier(i) => out.push_str(i.as_str()),
            BlockLabel::String(s) => out.push_str(&format!("\"{}\"", quoted_inside(s))),
        }
    }
    if b.body().0.is_empty() {
        out.push_str(" {}\n");
        return Ok(());
    }
    out.push_str(" {\n");
    body(out, b.body(), indent + 1)?;
    pad(out, indent);
    out.push_str("}\n");
    Ok(())
}

fn body(out: &mut String, body: &Body, indent: usize) -> Result<(), hcl::Error> {
    let mut run: Vec<(String, String)> = Vec::new();
    for s in body {
        match s {
            Structure::Attribute(a) => {
                let value = value(&strings_escaped(&a.expr), indent)?;
                run.push((a.key.as_str().to_string(), value));
            }
            Structure::Block(b) => {
                entries(out, &std::mem::take(&mut run), indent);
                block(out, b, indent)?;
            }
        }
    }
    entries(out, &run, indent);
    Ok(())
}

/// `key = value` lines, the `=` of each run of one-line values aligned.
fn entries(out: &mut String, entries: &[(String, String)], indent: usize) {
    let mut start = 0;
    while start < entries.len() {
        let end = entries[start..]
            .iter()
            .position(|(_, v)| v.contains('\n'))
            .map_or(entries.len(), |p| start + p + 1);
        let run = &entries[start..end];
        let width = run.iter().filter(|(_, v)| !v.contains('\n')).map(|(k, _)| k.chars().count()).max().unwrap_or(0);
        for (k, v) in run {
            pad(out, indent);
            out.push_str(k);
            if !v.contains('\n') {
                out.push_str(&" ".repeat(width - k.chars().count()));
            }
            out.push_str(" = ");
            out.push_str(v);
            out.push('\n');
        }
        start = end;
    }
}

/// An expression as written at nesting `indent` (the level of the line it starts on).
fn value(e: &Expression, indent: usize) -> Result<String, hcl::Error> {
    let flat = compact(e)?;
    if flat.chars().count() <= WIDTH && !flat.contains('\n') {
        return Ok(flat);
    }
    match e {
        Expression::Array(items) if !items.is_empty() => array(items, indent),
        Expression::Object(o) if !o.is_empty() => object(o, indent),
        Expression::FuncCall(c) if !c.args.is_empty() => func_call(c, indent),
        _ => Ok(flat),
    }
}

/// One item a line.
fn array(items: &[Expression], indent: usize) -> Result<String, hcl::Error> {
    let mut out = String::from("[\n");
    for (i, item) in items.iter().enumerate() {
        pad(&mut out, indent + 1);
        out.push_str(&value(item, indent + 1)?);
        if i + 1 < items.len() {
            out.push(',');
        }
        out.push('\n');
    }
    pad(&mut out, indent);
    out.push(']');
    Ok(out)
}

/// One item a line, aligned as a block's attributes are.
fn object(o: &Object<ObjectKey, Expression>, indent: usize) -> Result<String, hcl::Error> {
    let mut items = Vec::new();
    for (k, v) in o {
        let key = match k {
            ObjectKey::Identifier(i) => i.as_str().to_string(),
            ObjectKey::Expression(e) => compact(e)?,
            // Non-exhaustive upstream: any other key as `hcl-rs` writes it.
            other => other.to_string(),
        };
        items.push((key, value(v, indent + 1)?));
    }
    let mut out = String::from("{\n");
    entries(&mut out, &items, indent + 1);
    pad(&mut out, indent);
    out.push('}');
    Ok(out)
}

/// The arguments on the call's line, the last one laid out from there (`f(a, [` ... `])`).
fn func_call(c: &FuncCall, indent: usize) -> Result<String, hcl::Error> {
    let mut out = format!("{}(", c.name);
    let (last, first) = c.args.split_last().map_or((None, &[][..]), |(l, f)| (Some(l), f));
    for a in first {
        out.push_str(&compact(a)?);
        out.push_str(", ");
    }
    if let Some(l) = last {
        out.push_str(&value(l, indent)?);
    }
    out.push_str(if c.expand_final { "...)" } else { ")" });
    Ok(out)
}

/// `hcl-rs`'s one-line form of an expression.
fn compact(e: &Expression) -> Result<String, hcl::Error> {
    let mut f = Formatter::builder().compact(true).build_vec();
    e.format_string(&mut f)
}

#[cfg(test)]
mod tests {
    use super::Module;
    use crate::hcl_builder::{Block, Template, call, list, number, object, reference, string};

    fn rendered(b: Block) -> String {
        let mut m = Module::new("");
        m.block(b);
        m.render().unwrap()
    }

    #[test]
    fn one_line_attributes_are_aligned_and_blocks_nest() {
        let b = Block::builder("resource")
            .add_label("aws_vpc")
            .add_label("env")
            .add_attribute(("cidr_block", string("10.0.0.0/24")))
            .add_attribute(("tags", object([("Name", reference("local.name"))])))
            .add_block(
                Block::builder("lifecycle")
                    .add_attribute(("ignore_changes", list([reference("input")])))
                    .build(),
            )
            .add_block(Block::builder("serial_device").build())
            .build();
        assert_eq!(
            rendered(b),
            "resource \"aws_vpc\" \"env\" {\n  cidr_block = \"10.0.0.0/24\"\n  tags       = { Name = local.name }\n  lifecycle {\n    ignore_changes = [input]\n  }\n  serial_device {}\n}\n"
        );
    }

    #[test]
    fn long_lists_go_one_item_a_line_and_break_the_alignment() {
        let long: Vec<_> = (0..12).map(|i| string(format!("command number {i}"))).collect();
        let b = Block::builder("x")
            .add_attribute(("a", number(1)))
            .add_attribute(("inline", list(long)))
            .add_attribute(("on_failure", reference("continue")))
            .build();
        let text = rendered(b);
        assert!(text.starts_with("x {\n  a = 1\n  inline = [\n    \"command number 0\",\n"), "{text}");
        assert!(text.ends_with("    \"command number 11\"\n  ]\n  on_failure = continue\n}\n"), "{text}");
    }

    #[test]
    fn a_call_hugs_its_long_last_argument() {
        let items: Vec<_> = (0..8).map(|i| (format!("key{i}"), string(format!("a long enough value {i}")))).collect();
        let b = Block::builder("x").add_attribute(("data", call("yamlencode", [object(items)]))).build();
        let text = rendered(b);
        assert!(text.starts_with("x {\n  data = yamlencode({\n    key0 = \"a long enough value 0\"\n"), "{text}");
        assert!(text.ends_with("\n  })\n}\n"), "{text}");
    }

    #[test]
    fn a_key_that_is_a_keyword_is_quoted() {
        let b = Block::builder("x").add_attribute(("m", object([("for", number(1)), ("web", number(2))]))).build();
        assert_eq!(rendered(b), "x {\n  m = { \"for\" = 1, web = 2 }\n}\n");
    }

    #[test]
    fn comments_and_blank_lines_between_blocks() {
        let mut m = Module::new("# head\n");
        m.commented("one\ntwo", Block::builder("a").build());
        m.block(
            Block::builder("b")
                .add_attribute(("t", Template::text("x").with_expr(reference("var.y"))))
                .build(),
        );
        assert_eq!(m.render().unwrap(), "# head\n\n# one\n# two\na {}\n\nb {\n  t = \"x${var.y}\"\n}\n");
    }
}
