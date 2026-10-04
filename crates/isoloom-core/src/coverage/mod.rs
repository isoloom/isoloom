//! Coverage, both ways. From each output format's side ([`formats`]): every feature of the
//! format (every key of the Compose schema, every Vagrant setting) and whether an Isoloom
//! spec can produce it. From the spec's side ([`spec`]): every field of the spec and what
//! each output does with it. The source of truth for `isoloom coverage`, docs/COVERAGE.md
//! and the website; the tests keep both tables in step with the formats and the generators.

pub mod compose;
pub mod spec;
pub mod terraform;
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
    /// Not classified yet (the coverage tests reject it).
    Unclassified,
}

impl Support {
    pub const KINDS: [&'static str; 9] = [
        "emitted",
        "partial",
        "equivalent",
        "in image",
        "planned",
        "open",
        "by design",
        "tooling",
        "unclassified",
    ];

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
            Support::Unclassified => "unclassified",
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
            Support::Unclassified => "?",
        }
    }

    /// How Isoloom produces it, or why not (with the reason's kind first).
    pub fn note(self) -> String {
        let prefix = match self {
            Support::Emitted { .. } | Support::Partial { .. } => "",
            Support::Equivalent { .. } => "Another way: ",
            Support::InImage { .. } => "In the image: ",
            Support::Planned { .. } => "Not yet: ",
            Support::Open { .. } => "Not yet, undecided: ",
            Support::ByDesign { .. } => "By design: ",
            Support::Tooling { .. } => "Not needed: ",
            Support::Unclassified => "",
        };
        format!("{prefix}{}", self.reason())
    }

    /// The reason alone, without its kind.
    pub fn reason(self) -> String {
        match self {
            Support::Emitted { from } => origin(from),
            Support::Partial { from, gap } => format!("{}; not: {gap}", origin(from)),
            Support::Equivalent { via } => format!("via {via}"),
            Support::InImage { note } | Support::Planned { note } | Support::Open { note } | Support::Tooling { note } => note.to_string(),
            Support::ByDesign { why } => why.to_string(),
            Support::Unclassified => "not classified yet".to_string(),
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
    pub rows: Vec<(String, Support)>,
}

impl Format {
    pub fn count(&self, kind: &str) -> usize {
        self.rows.iter().filter(|(_, s)| s.kind() == kind).count()
    }

    /// "118 features: 14 yes, 7 partly, 97 no".
    pub fn summary(&self) -> String {
        let count = |v: &str| self.rows.iter().filter(|(_, s)| s.implemented() == v).count();
        format!(
            "{} features: {} yes, {} partly, {} no",
            self.rows.len(),
            count("Yes"),
            count("Partly"),
            count("No")
        )
    }
}

/// The formats Isoloom generates, with their features.
pub fn formats() -> Vec<Format> {
    let mut all = vec![compose::format()];
    all.extend(vagrant::formats());
    all.extend(terraform::formats());
    all
}

/// The three answers per feature, plus the format's own tooling (left out of the counts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    /// A spec gets it today: written by Isoloom, the same effect another way, or in the image.
    CanExpress,
    /// The format doesn't have it yet (planned, or still undecided).
    Missing,
    /// Deliberately not expressible.
    Never,
    /// The format's tooling, not how the environment behaves.
    Tooling,
}

impl Bucket {
    pub const ALL: [Bucket; 4] = [Bucket::CanExpress, Bucket::Missing, Bucket::Never, Bucket::Tooling];

    pub fn title(self) -> &'static str {
        match self {
            Bucket::CanExpress => "Can express today",
            Bucket::Missing => "Missing",
            Bucket::Never => "Never (by design)",
            Bucket::Tooling => "Not counted: tooling",
        }
    }
}

impl Support {
    pub fn bucket(self) -> Bucket {
        match self {
            Support::Emitted { .. } | Support::Partial { .. } | Support::Equivalent { .. } | Support::InImage { .. } => Bucket::CanExpress,
            Support::Planned { .. } | Support::Open { .. } | Support::Unclassified => Bucket::Missing,
            Support::ByDesign { .. } => Bucket::Never,
            Support::Tooling { .. } => Bucket::Tooling,
        }
    }
}

impl Format {
    pub fn in_bucket(&self, b: Bucket) -> usize {
        self.rows.iter().filter(|(_, s)| s.bucket() == b).count()
    }

    /// Features that change how the environment behaves (tooling left out).
    pub fn behavior(&self) -> usize {
        self.rows.len() - self.in_bucket(Bucket::Tooling)
    }
}

impl Support {
    /// The reference table's "Implemented" column: does Isoloom produce it?
    pub fn implemented(self) -> &'static str {
        match self {
            Support::Emitted { .. } => "Yes",
            Support::Partial { .. } => "Partly",
            _ => "No",
        }
    }
}

/// A feature's section in the reference table, and its name within it.
fn section(key: &str) -> (&'static str, &str) {
    if let Some(rest) = key.strip_prefix("resource ") {
        return ("Resource types", rest);
    }
    if let Some(rest) = key.strip_prefix("provider ") {
        return ("Settings", rest.split_once(": ").map(|(_, s)| s).unwrap_or(rest));
    }
    for (prefix, title) in [
        ("services.*.", "Services"),
        ("networks.*.", "Networks"),
        ("volumes.*.", "Volumes"),
        ("config.vm.network ", "Network types"),
        ("config.vm.provision ", "Provisioners"),
        ("config.vm.", "Machine settings (config.vm)"),
        ("config.", "Other settings"),
    ] {
        if let Some(rest) = key.strip_prefix(prefix) {
            return (title, if title == "Other settings" { key } else { rest });
        }
    }
    ("Top level", key)
}

/// The whole coverage page as Markdown: a summary, then a reference table per format, every
/// feature in the format's own order, with whether Isoloom produces it.
pub fn markdown() -> String {
    let formats = formats();
    let mut md = String::from(
        "# Coverage\n\nEverything each format can do, and whether Isoloom produces it from a spec. The lists come from\nthe formats themselves: every key of the Compose schema, every setting of Vagrant's and each\nprovider plugin's configuration. Tests fail when a list has an unclassified entry, or when it\ndisagrees with what Isoloom really generates. For Terraform, each provider's resource types\n(the arguments of each are detailed as its generator is built).\n\n",
    );
    md.push_str("| Format | Features | Yes | Partly | No |\n| --- | ---: | ---: | ---: | ---: |\n");
    for f in &formats {
        let count = |v: &str| f.rows.iter().filter(|(_, s)| s.implemented() == v).count();
        let _ = writeln!(
            md,
            "| [{}](#{}) | {} | {} | {} | {} |",
            f.name,
            anchor(f.name),
            f.rows.len(),
            count("Yes"),
            count("Partly"),
            count("No")
        );
    }
    md.push_str("\nEvery **No** says why: another way to get the same effect, in the image, not yet, by design, or\nnot needed (the format's own tooling).\n");
    for f in &formats {
        let _ = write!(md, "\n## {}\n\nFrom {}.\n", f.name, f.source);
        let mut current = "";
        for (key, s) in &f.rows {
            let (title, name) = section(key);
            if title != current {
                let _ = write!(md, "\n### {title}\n\n| Key | Implemented | Notes |\n| --- | --- | --- |\n");
                current = title;
            }
            let _ = writeln!(md, "| `{name}` | {} | {} |", s.implemented(), capitalize(&s.note()));
        }
    }
    md
}

/// A heading's anchor, as the site generates them ("Vagrant: VirtualBox" -> "vagrant-virtualbox").
fn anchor(title: &str) -> String {
    let mut out = String::new();
    for c in title.to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if c == ' ' || c == '-' {
            out.push('-');
        }
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
