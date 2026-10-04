//! Coverage, both ways. From each output format's side ([`formats`]): every feature of the
//! format (every key of the Compose schema, every Vagrant setting) and whether an Isoloom
//! spec can produce it. From the spec's side ([`spec`]): every field of the spec and what
//! each output does with it. The source of truth for `isoloom coverage`, docs/COVERAGE.md
//! and the website; the tests keep both tables in step with the formats and the generators.

pub mod compose;
pub mod spec;
pub mod vagrant;

use std::fmt::Write;

pub use spec::{Output, Row, Status, paths, score, table};

/// Whether an Isoloom spec can produce a feature of an output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// Isoloom writes it, from this spec field.
    Emitted { from: &'static str },
    /// Isoloom writes it, but not every use of it can be expressed.
    Partial { from: &'static str, gap: &'static str },
    /// Not written, but the same effect comes another way.
    Equivalent { via: &'static str },
    /// Belongs in the machine itself: its image or its provisioning.
    InImage { note: &'static str },
    /// A concept the format should gain.
    Planned { note: &'static str },
    /// Not expressible; whether the format should expose it is still open.
    Open { note: &'static str },
    /// Deliberately not expressible.
    ByDesign { why: &'static str },
    /// Tooling of the format, not the environment's behavior.
    Tooling { note: &'static str },
}

impl Support {
    pub const KINDS: [&'static str; 8] = ["emitted", "partial", "equivalent", "in image", "planned", "open", "by design", "tooling"];

    pub fn kind(self) -> &'static str {
        match self {
            Support::Emitted { .. } => "emitted",
            Support::Partial { .. } => "partial",
            Support::Equivalent { .. } => "equivalent",
            Support::InImage { .. } => "in image",
            Support::Planned { .. } => "planned",
            Support::Open { .. } => "open",
            Support::ByDesign { .. } => "by design",
            Support::Tooling { .. } => "tooling",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Support::Emitted { .. } => "✓",
            Support::Partial { .. } => "◐",
            Support::Equivalent { .. } => "≈",
            Support::InImage { .. } => "image",
            Support::Planned { .. } => "planned",
            Support::Open { .. } => "open",
            Support::ByDesign { .. } => "—",
            Support::Tooling { .. } => "tooling",
        }
    }

    pub fn note(self) -> String {
        match self {
            Support::Emitted { from } => origin(from),
            Support::Partial { from, gap } => format!("{}; not: {gap}", origin(from)),
            Support::Equivalent { via } => format!("via {via}"),
            Support::InImage { note } | Support::Planned { note } | Support::Open { note } | Support::Tooling { note } => note.to_string(),
            Support::ByDesign { why } => why.to_string(),
        }
    }

    /// Whether a spec can get this feature today (written, or the same effect another way).
    pub fn expressible(self) -> bool {
        matches!(
            self,
            Support::Emitted { .. } | Support::Partial { .. } | Support::Equivalent { .. } | Support::InImage { .. }
        )
    }

    /// Whether Isoloom writes the key itself.
    pub fn written(self) -> bool {
        matches!(self, Support::Emitted { .. } | Support::Partial { .. })
    }
}

/// "from `machines.*.networks` (fixed addresses)"; a parenthesized origin is Isoloom's own
/// choice, shown as is.
fn origin(from: &str) -> String {
    if let Some(inner) = from.strip_prefix('(').and_then(|f| f.strip_suffix(')')) {
        return inner.to_string();
    }
    match from.split_once(" (") {
        Some((path, rest)) if path.contains('.') || !path.contains(' ') => format!("from `{path}` ({rest}"),
        _ if from.contains(' ') && !from.contains('.') => format!("from {from}"),
        _ => format!("from `{from}`"),
    }
}

/// One output format's features and their support.
pub struct Format {
    pub name: &'static str,
    pub file: &'static str,
    /// Where the feature list comes from.
    pub source: &'static str,
    pub rows: Vec<(&'static str, Support)>,
}

impl Format {
    pub fn count(&self, kind: &str) -> usize {
        self.rows.iter().filter(|(_, s)| s.kind() == kind).count()
    }

    /// "61 of 96 behavior features" (tooling left out).
    pub fn summary(&self) -> String {
        let behavior = self.rows.iter().filter(|(_, s)| !matches!(s, Support::Tooling { .. })).count();
        let expressible = self.rows.iter().filter(|(_, s)| s.expressible()).count();
        format!(
            "{expressible} of {behavior} behavior features expressible ({} features in all)",
            self.rows.len()
        )
    }
}

/// The formats Isoloom generates, with their features.
pub fn formats() -> Vec<Format> {
    vec![compose::format(), vagrant::format()]
}

/// The whole coverage page as Markdown.
pub fn markdown() -> String {
    let mut md = String::from(
        "# Coverage\n\nEvery feature of each output format, and whether an Isoloom spec can produce it. Generated by\n`isoloom coverage --markdown`; the tests fail when the Compose schema has a key with no row, when\nIsoloom writes a key not marked as written, or when a key marked as written appears in no example.\n\n",
    );
    md.push_str("✓ emitted · ◐ partial · ≈ equivalent (same effect another way) · image: belongs in the image or provisioning · planned: the format should gain it · open: undecided · — by design: deliberately not expressible · tooling: the format's tooling, not behavior\n\n");
    md.push_str("| Format | ");
    md.push_str(&Support::KINDS.join(" | "));
    md.push_str(" |\n| --- |");
    for _ in Support::KINDS {
        md.push_str(" ---: |");
    }
    md.push('\n');
    let formats = formats();
    for f in &formats {
        let _ = write!(md, "| {} |", f.name);
        for k in Support::KINDS {
            let _ = write!(md, " {} |", f.count(k));
        }
        md.push('\n');
    }
    md.push_str("\nTerraform (Proxmox, cloud VMs) gets its table with its generator.\n");
    for f in &formats {
        let _ = write!(md, "\n## {}\n\n`{}`: {}. Features from {}.\n\n", f.name, f.file, f.summary(), f.source);
        md.push_str("| Feature | | Notes |\n| --- | :---: | --- |\n");
        for (key, s) in &f.rows {
            let _ = writeln!(md, "| `{key}` | {} | {} |", s.symbol(), s.note());
        }
    }
    md.push('\n');
    md.push_str(&spec::section());
    md
}
