//! Coverage: everything each output format can do, and whether Isoloom produces it.
//!
//! The rule for what belongs in the format: **the VM targets can do it** (local VMs, Proxmox,
//! cloud VMs). Containers are the lightweight option, used when they can produce the same
//! machine; a feature they can't produce makes an environment VM-only (like Windows) rather
//! than staying out of the format. Container mechanics (capabilities, cgroups...) aren't
//! machine features: Isoloom sets them itself. 100% coverage means every portable feature is
//! produced.
//!
//! The source of truth for `isoloom coverage`, docs/COVERAGE.md and the website; the tests
//! keep the tables in step with the formats' own definitions and with the generators. The
//! spec's side ([`spec`]: every field of a spec and what each output does with it) keeps the
//! generators' claims honest.

pub mod compose;
pub mod spec;
pub mod terraform;
pub mod vagrant;

use std::fmt::Write;

pub use spec::{Output, Row, Status, paths, score, table};

/// What Isoloom does with a feature of an output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// Isoloom writes it, from this spec field.
    Emitted { from: &'static str },
    /// Isoloom writes it, but not for every use.
    Partial { from: &'static str, gap: &'static str },
    /// Not written, but the same effect comes from another field.
    Equivalent { via: &'static str },
    /// Belongs in the machine itself: its image or its provisioning.
    InImage { note: &'static str },
    /// Every target can do it: the format should gain it.
    Planned { note: &'static str },
    /// Some VM targets can't do it (cloud VMs most often), so it stays out of the format.
    NotPortable { why: &'static str },
    /// The format's own tooling: no effect on how the environment behaves.
    Tooling { note: &'static str },
    /// Not classified yet (the coverage tests reject it).
    Unclassified,
}

impl Support {
    /// For the tests' messages.
    pub fn kind(self) -> &'static str {
        match self {
            Support::Emitted { .. } => "emitted",
            Support::Partial { .. } => "partial",
            Support::Equivalent { .. } => "equivalent",
            Support::InImage { .. } => "in image",
            Support::Planned { .. } => "planned",
            Support::NotPortable { .. } => "not portable",
            Support::Tooling { .. } => "tooling",
            Support::Unclassified => "unclassified",
        }
    }

    /// The "Every target" column: can every kind of target do it?
    pub fn portable(self) -> &'static str {
        match self {
            Support::NotPortable { .. } => "No",
            Support::Tooling { .. } => "n/a",
            Support::Unclassified => "?",
            _ => "Yes",
        }
    }

    /// The "Implemented" column: does Isoloom produce it?
    pub fn implemented(self) -> &'static str {
        match self {
            Support::Emitted { .. } => "Yes",
            Support::Partial { .. } => "Partly",
            Support::Equivalent { .. } => "Another way",
            Support::InImage { .. } => "In the image",
            Support::Planned { .. } => "Not yet",
            Support::NotPortable { .. } | Support::Tooling { .. } | Support::Unclassified => "No",
        }
    }

    /// How Isoloom produces it, or why not.
    pub fn note(self) -> String {
        match self {
            Support::Emitted { from } => origin(from),
            Support::Partial { from, gap } => format!("{}; not yet: {gap}", origin(from)),
            Support::Equivalent { via } => format!("via {via}"),
            Support::InImage { note } | Support::Planned { note } | Support::NotPortable { why: note } | Support::Tooling { note } => note.to_string(),
            Support::Unclassified => "not classified yet".to_string(),
        }
    }

    /// Counts toward 100%: portable (tooling and unportable features don't count).
    pub fn counts(self) -> bool {
        !matches!(self, Support::NotPortable { .. } | Support::Tooling { .. })
    }

    /// Done: produced, or the same effect another way.
    pub fn done(self) -> bool {
        matches!(self, Support::Emitted { .. } | Support::Equivalent { .. } | Support::InImage { .. })
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

/// One output format's features and what Isoloom does with each.
pub struct Format {
    pub name: &'static str,
    pub file: &'static str,
    /// Where the feature list comes from.
    pub source: &'static str,
    pub rows: Vec<(String, Support)>,
    /// On the page, list the features that aren't "not portable" and count the rest in one line
    /// (a cloud's hundreds of managed services).
    pub collapse_not_portable: bool,
}

/// Toward 100%: the portable features, and how many are done, partly done, or to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Score {
    pub portable: usize,
    pub done: usize,
    pub partly: usize,
    pub to_do: usize,
}

impl Score {
    /// Done counts fully, partly done half.
    pub fn percent(self) -> usize {
        if self.portable == 0 {
            return 100;
        }
        (self.done * 200 + self.partly * 100) / (self.portable * 2)
    }
}

impl Format {
    pub fn score(&self) -> Score {
        let portable: Vec<Support> = self.rows.iter().map(|(_, s)| *s).filter(|s| s.counts()).collect();
        Score {
            portable: portable.len(),
            done: portable.iter().filter(|s| s.done()).count(),
            partly: portable.iter().filter(|s| matches!(s, Support::Partial { .. })).count(),
            to_do: portable.iter().filter(|s| matches!(s, Support::Planned { .. } | Support::Unclassified)).count(),
        }
    }

    /// "62% of 61 portable features (34 done, 7 partly, 20 to do)".
    pub fn summary(&self) -> String {
        let s = self.score();
        format!(
            "{}% of {} portable features ({} done, {} partly, {} to do; {} features in all)",
            s.percent(),
            s.portable,
            s.done,
            s.partly,
            s.to_do,
            self.rows.len()
        )
    }
}

/// The formats Isoloom generates (or will), with their features.
pub fn formats() -> Vec<Format> {
    let mut all = vec![compose::format()];
    all.extend(vagrant::formats());
    all.extend(terraform::formats());
    all
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

/// The whole coverage page as Markdown: the rule, the score per format, then a reference
/// table per format, every feature in the format's own order.
pub fn markdown() -> String {
    let formats = formats();
    let mut md = String::from(
        "# Coverage\n\nEverything each format can do, and whether Isoloom produces it from a spec.\n\n**What belongs in the format:** a machine feature the VM targets can do (local VMs, Proxmox,\ncloud VMs). Containers are the lightweight option, used when they can produce the same machine:\na feature they can't makes an environment VM-only (like Windows) instead of staying out.\nContainer mechanics (capabilities, cgroups) aren't machine features: Isoloom sets them itself.\n**100% coverage** means every portable feature is produced.\n\n",
    );
    md.push_str("| Format | Coverage | Portable features | Done | Partly | To do |\n| --- | ---: | ---: | ---: | ---: | ---: |\n");
    for f in &formats {
        let s = f.score();
        let _ = writeln!(
            md,
            "| [{}](#{}) | {}% | {} | {} | {} | {} |",
            f.name,
            anchor(f.name),
            s.percent(),
            s.portable,
            s.done,
            s.partly,
            s.to_do
        );
    }
    md.push_str("\nDone includes features produced another way, or set in the image. The lists come from the\nformats themselves (every key of the Compose schema, every setting of Vagrant and each provider\nplugin, every resource type of each Terraform provider); tests fail on an unclassified entry, or\nwhen a table disagrees with what Isoloom really generates.\n");
    for f in &formats {
        let _ = write!(md, "\n## {}\n\n{}. From {}.\n", f.name, capitalize(&f.summary()), f.source);
        let mut current = "";
        let collapsed = f
            .rows
            .iter()
            .filter(|(_, s)| f.collapse_not_portable && matches!(s, Support::NotPortable { .. }))
            .count();
        for (key, s) in f
            .rows
            .iter()
            .filter(|(_, s)| !(f.collapse_not_portable && matches!(s, Support::NotPortable { .. })))
        {
            let (title, name) = section(key);
            if title != current {
                let _ = write!(md, "\n### {title}\n\n| Key | Every target | Implemented | Notes |\n| --- | --- | --- | --- |\n");
                current = title;
            }
            let _ = writeln!(md, "| `{name}` | {} | {} | {} |", s.portable(), s.implemented(), capitalize(&s.note()));
        }
        if collapsed > 0 {
            let _ = writeln!(
                md,
                "\nAnd {collapsed} other resource types: this cloud's managed services (databases, storage, functions...). Not portable: no other VM target has them. `isoloom coverage --all` lists them."
            );
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
