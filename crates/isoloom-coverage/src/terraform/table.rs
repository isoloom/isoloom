//! A Terraform table built from a provider's own schema: every resource type it offers, and the
//! settings, arguments and nested blocks of what Isoloom writes, each classified by a rule.
//!
//! A rule names resource types or argument paths: `name` exactly, `prefix*` (and, for
//! `prefix_*`, the bare `prefix`), `block.*` for a block and everything inside it, or
//! `*suffix` for any path ending so. The exact name wins, then the longest pattern. Nothing
//! falls back to a default: a type or argument no rule names is unclassified.

use std::collections::BTreeSet;

use super::support::{ID, OLDER_NAME, TAGS, TIMEOUTS, WRITTEN};
use crate::{Format, Support};

/// Rules: each verdict with the names it covers, separated by whitespace.
pub type Rules = &'static [(Support, &'static str)];

/// The arguments every resource may have, whichever the provider.
const COMMON: Rules = &[
    (ID, "id"),
    (TIMEOUTS, "timeouts.*"),
    (
        TAGS,
        "*.tags *.tags_all *.labels *.effective_labels *.terraform_labels *.freeform_tags *.defined_tags *.description *.display_name",
    ),
];

/// One provider's vendored lists (`coverage/terraform/`).
pub struct Provider {
    /// Its local name in a module (`aws`), as in `provider "aws"`.
    pub name: &'static str,
    /// Every resource type, one a line.
    pub types: &'static str,
    /// Its settings, and the arguments and blocks of the types and data sources Isoloom writes.
    pub schema: &'static str,
}

/// A table: where its lists come from, and how each entry is classified.
pub struct Table {
    /// The title (`Terraform: AWS`).
    pub name: &'static str,
    /// The generated modules it describes.
    pub file: &'static str,
    /// The providers and versions the lists come from.
    pub source: &'static str,
    /// The providers.
    pub providers: &'static [Provider],
    /// Stripped from a type's name before the type rules match (`aws_`).
    pub prefix: &'static str,
    /// A prefix the provider also gives older names of its types: such a type whose name without
    /// it also exists is that type's older name.
    pub legacy: Option<&'static str>,
    /// The verdict on the resource types and data sources the generators write.
    pub generated: Support,
    /// The rules for resource types and data sources (names without `prefix`).
    pub types: Rules,
    /// What the generators write: `("types", types and data.T)`, `("provider", settings)`, and per
    /// resource type or data source (`data.T`) its arguments and blocks.
    pub written: &'static [(&'static str, &'static str)],
    /// The rules for the provider's settings.
    pub settings: Rules,
    /// The rules for the arguments of each resource type or data source (`data.T`).
    pub arguments: &'static [(&'static str, Rules)],
    /// The verdict on any argument no rule names, for the utility providers only.
    pub every_argument: Option<Support>,
    /// Whether the page counts the not portable resource types in one line.
    pub collapse: bool,
}

/// Whether `pattern` names `name` (see the module's comment).
fn matches(pattern: &str, name: &str) -> bool {
    if let Some(base) = pattern.strip_suffix(".*") {
        return name == base || name.strip_prefix(base).is_some_and(|r| r.starts_with('.'));
    }
    if let Some(base) = pattern.strip_suffix('*') {
        return name.starts_with(base) || base.strip_suffix('_') == Some(name);
    }
    if let Some(end) = pattern.strip_prefix('*') {
        return name.ends_with(end) || end.strip_prefix('.') == Some(name);
    }
    pattern == name
}

/// The verdict of the best rule naming `name`: an exact name first, then the longest pattern.
pub fn classify(name: &str, rules: &[Rules]) -> Option<Support> {
    let mut best: Option<((bool, usize), Support)> = None;
    for (support, words) in rules.iter().flat_map(|r| r.iter()) {
        for word in words.split_whitespace().filter(|w| matches(w, name)) {
            let rank = (!word.contains('*'), word.len());
            if best.is_none_or(|(b, _)| rank > b) {
                best = Some((rank, *support));
            }
        }
    }
    best.map(|(_, s)| s)
}

/// The entries of a vendored list, without its comment lines.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|l| !l.is_empty() && !l.starts_with('#'))
}

impl Table {
    /// The table: resource types, data sources, settings, then each type's arguments.
    pub fn format(&self) -> Format {
        let written = self.written_keys();
        let mut rows = self.type_rows(&written);
        let mut arguments = Vec::new();
        for p in self.providers {
            // A resource type's own line: its row comes from the list of every type.
            for line in lines(p.schema).filter(|l| l.strip_prefix("resource ").is_none_or(|t| t.contains(' '))) {
                let (key, support) = self.schema_row(p, line, &written);
                if key.starts_with("argument ") {
                    arguments.push((key, support));
                } else {
                    rows.push((key, support));
                }
            }
        }
        // Data sources and settings after the types, then the arguments by type.
        rows.sort_by_key(|(k, _)| ["resource ", "data ", "provider "].iter().position(|p| k.starts_with(p)));
        rows.extend(arguments);
        Format {
            name: self.name,
            file: self.file,
            source: self.source,
            rows,
            collapse_not_portable: self.collapse,
        }
    }

    /// Every key the generators write, as the rows name them.
    fn written_keys(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for (owner, names) in self.written {
            for name in names.split_whitespace() {
                if *owner == "provider" {
                    // Only single-provider tables write settings.
                    out.extend(self.providers.iter().map(|p| format!("provider {}: {name}", p.name)));
                    continue;
                }
                out.insert(match *owner {
                    "types" => match name.strip_prefix("data.") {
                        Some(data) => format!("data {data}"),
                        None => format!("resource {name}"),
                    },
                    _ => format!("argument {owner}.{name}"),
                });
            }
        }
        out
    }

    /// One row per resource type of each provider.
    fn type_rows(&self, written: &BTreeSet<String>) -> Vec<(String, Support)> {
        let mut rows = Vec::new();
        for p in self.providers {
            let all: BTreeSet<&str> = lines(p.types).collect();
            for t in &all {
                let key = format!("resource {t}");
                let support = if written.contains(&key) { self.generated } else { self.type_support(t, &all) };
                rows.push((key, support));
            }
        }
        rows
    }

    /// A resource type's or data source's verdict from the type rules.
    fn type_support(&self, t: &str, all: &BTreeSet<&str>) -> Support {
        if let Some(legacy) = self.legacy
            && let Some(short) = t.strip_prefix(legacy)
        {
            if all.contains(format!("{}{short}", self.prefix).as_str()) {
                return OLDER_NAME;
            }
            return classify(short, &[self.types]).unwrap_or(Support::Unclassified);
        }
        classify(t.strip_prefix(self.prefix).unwrap_or(t), &[self.types]).unwrap_or(Support::Unclassified)
    }

    /// A schema line (`provider PATH`, `resource T`, `data T`, `resource T PATH`, `data T PATH`)
    /// as a row.
    fn schema_row(&self, p: &Provider, line: &str, written: &BTreeSet<String>) -> (String, Support) {
        let parts: Vec<&str> = line.split(' ').collect();
        let key = match parts.as_slice() {
            ["provider", path] => format!("provider {}: {path}", p.name),
            ["data", t] => format!("data {t}"),
            [kind, t] => format!("{kind} {t}"),
            ["data", t, path] => format!("argument data.{t}.{path}"),
            [_, t, path] => format!("argument {t}.{path}"),
            _ => format!("unknown {line}"),
        };
        if written.contains(&key) {
            // A written argument's own rule may say more (a feature not run on a real host).
            let own = match parts.as_slice() {
                [_, t, path] => self.argument(t, path).filter(|s| s.written()),
                _ => None,
            };
            let support = if key.starts_with("data ") { self.generated } else { own.unwrap_or(WRITTEN) };
            return (key, support);
        }
        let support = match parts.as_slice() {
            ["provider", path] => classify(path, &[self.settings]),
            ["data", t] => classify(t.strip_prefix(self.prefix).unwrap_or(t), &[self.types]),
            ["data", t, path] => self.argument(&format!("data.{t}"), path),
            [_, t, path] => self.argument(t, path),
            _ => None,
        };
        (key, support.unwrap_or(Support::Unclassified))
    }

    /// An argument's verdict: its type's rules and the common ones, else the table's default.
    fn argument(&self, owner: &str, path: &str) -> Option<Support> {
        let own = self.arguments.iter().find(|(o, _)| *o == owner).map_or(&[][..], |(_, r)| *r);
        classify(path, &[own, COMMON]).or(self.every_argument)
    }

    /// The rule words that name nothing in the provider's lists: a stale rule, to remove.
    pub fn stale_rules(&self) -> Vec<String> {
        // Type rules name types without the provider's prefix (or its older one).
        let data = self.schema_lines().filter_map(|l| l.strip_prefix("data ")).filter(|t| !t.contains(' '));
        let types: Vec<&str> = self
            .providers
            .iter()
            .flat_map(|p| lines(p.types))
            .chain(data)
            .map(|t| self.legacy.and_then(|l| t.strip_prefix(l)).or(t.strip_prefix(self.prefix)).unwrap_or(t))
            .collect();
        let mut stale = unused("types", self.types, &types);
        let settings: Vec<&str> = self.schema_lines().filter_map(|l| l.strip_prefix("provider ")).collect();
        stale.extend(unused("settings", self.settings, &settings));
        for (owner, rules) in self.arguments {
            let (kind, t) = owner.strip_prefix("data.").map_or(("resource ", *owner), |t| ("data ", t));
            let head = format!("{kind}{t} ");
            let paths: Vec<&str> = self.schema_lines().filter_map(|l| l.strip_prefix(head.as_str())).collect();
            stale.extend(unused(owner, rules, &paths));
        }
        stale
    }

    fn schema_lines(&self) -> impl Iterator<Item = &str> {
        self.providers.iter().flat_map(|p| lines(p.schema))
    }
}

/// The words of `rules` that name none of `names`.
fn unused(owner: &str, rules: Rules, names: &[&str]) -> Vec<String> {
    rules
        .iter()
        .flat_map(|(_, words)| words.split_whitespace())
        .filter(|w| !names.iter().any(|n| matches(w, n)))
        .map(|w| format!("{owner} rule `{w}`"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Support::Planned;

    const A: Support = Planned { note: "a" };
    const B: Support = Planned { note: "b" };

    #[test]
    fn patterns_name_what_they_say() {
        assert!(matches("vpc_*", "vpc_endpoint") && matches("vpc_*", "vpc") && !matches("vpc_*", "vpcx"));
        assert!(matches("disk.*", "disk") && matches("disk.*", "disk.size") && !matches("disk.*", "disks"));
        assert!(matches("*.tags", "tags") && matches("*.tags", "root.tags") && matches("*_endpoint", "ec2_endpoint"));
        assert!(matches("ami", "ami") && !matches("ami", "ami_copy"));
    }

    #[test]
    fn the_exact_name_wins_then_the_longest_pattern() {
        let rules: Rules = &[(A, "ec2_* ec2_transit_gateway"), (B, "ec2_transit_* ec2_fleet")];
        assert_eq!(classify("ec2_transit_gateway", &[rules]), Some(A));
        assert_eq!(classify("ec2_transit_gateway_route", &[rules]), Some(B));
        assert_eq!(classify("ec2_host", &[rules]), Some(A));
        assert_eq!(classify("s3_bucket", &[rules]), None);
    }
}
