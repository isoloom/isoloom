//! What is wrong with a spec, as an author reads it: a stable code, the field path, the line,
//! the reason and the fix.
//!
//! ```text
//! error[removed-field] machines.web.docker  (isoloom.yml:12:5)
//!   a machine says what it runs, never how a platform runs it, so `docker:` is gone
//!   fix: `app: { build: build/web }` for your Dockerfile, or `app: { image: <ref> }`
//! ```

use std::fmt;

use serde::Serialize;

/// The kind of problem: a stable, kebab-case code that tools and editors can key on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Code {
    /// A key the spec doesn't have (a typo, or a field in the wrong place).
    UnknownField,
    /// A key of the earlier format: never read, its replacement named in the fix.
    RemovedField,
    /// A field that must be there.
    MissingField,
    /// A value of the wrong type or outside what the field accepts.
    InvalidValue,
    /// Fields that can't go together.
    Conflict,
    /// A name that names nothing declared (a machine, a network, an input, a port).
    UnknownReference,
    /// A sensitive value where it would show (a secret or a sensitive input as an environment
    /// variable).
    SensitiveValue,
    /// A target can't run the machine as written (not a spec error: another target may).
    Refused,
}

impl Code {
    /// The code as written in a problem: `removed-field`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Code::UnknownField => "unknown-field",
            Code::RemovedField => "removed-field",
            Code::MissingField => "missing-field",
            Code::InvalidValue => "invalid-value",
            Code::Conflict => "conflict",
            Code::UnknownReference => "unknown-reference",
            Code::SensitiveValue => "sensitive-value",
            Code::Refused => "refused",
        }
    }
}

/// Where a field sits in the file: its line and column, both from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Span {
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1.
    pub column: usize,
}

/// A field's path in the spec: `machines.web.ports.site.port`, `checks[2].http`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize)]
#[serde(transparent)]
pub struct FieldPath(String);

impl FieldPath {
    /// The path as written.
    #[must_use]
    pub fn new(path: impl Into<String>) -> FieldPath {
        FieldPath(path.into())
    }

    /// The path of field `key` under this one.
    #[must_use]
    pub fn key(&self, key: &str) -> FieldPath {
        if self.0.is_empty() {
            FieldPath(key.to_owned())
        } else {
            FieldPath(format!("{}.{key}", self.0))
        }
    }

    /// The path of item `i` of the list at this one.
    #[must_use]
    pub fn index(&self, i: usize) -> FieldPath {
        FieldPath(format!("{}[{i}]", self.0))
    }

    /// The path as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The enclosing field's path (`machines.web` for `machines.web.app`), when there is one.
    #[must_use]
    pub fn parent(&self) -> Option<FieldPath> {
        let cut = self.0.rfind(['.', '['])?;
        self.0.get(..cut).map(|parent| FieldPath(parent.to_owned()))
    }

    /// Whether this path is `other` or a field inside it.
    #[must_use]
    pub fn within(&self, other: &FieldPath) -> bool {
        self.0 == other.0 || self.0.strip_prefix(other.0.as_str()).is_some_and(|rest| rest.starts_with(['.', '[']))
    }
}

impl fmt::Display for FieldPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One thing wrong with a spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    /// What kind of problem it is.
    pub code: Code,
    /// The field it is about.
    pub path: FieldPath,
    /// Where that field is in the file, when it is written there (a field a group gives a
    /// machine has the machine's own place, or none).
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Why it is wrong.
    pub reason: String,
    /// What to write instead, when there is one answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

impl Problem {
    /// A problem with no fix to offer and no place yet (see [`crate::syntax::Spans::place`]).
    #[must_use]
    pub fn new(code: Code, path: FieldPath, reason: impl Into<String>) -> Problem {
        Problem {
            code,
            path,
            span: None,
            reason: reason.into(),
            fix: None,
        }
    }

    /// The problem with its fix.
    #[must_use]
    pub fn fix(mut self, fix: impl Into<String>) -> Problem {
        self.fix = Some(fix.into());
        self
    }

    /// The three lines an author reads, naming `file` (`isoloom.yml`) on the first.
    #[must_use]
    pub fn render(&self, file: &str) -> String {
        let head = format!("error[{}] {}", self.code.id(), self.path);
        lines(&head, self.span, file, &self.reason, self.fix.as_deref())
    }

    /// The problem on one line, its field then its reason (`machines.web.app.build: a Dockerfile
    /// builds a container only`): what a listing of targets shows.
    #[must_use]
    pub fn line(&self) -> String {
        format!("{}: {}", self.path, self.reason)
    }

    /// The top-level section the problem is in: `machines` for `machines.web.app`.
    #[must_use]
    pub fn section(&self) -> &str {
        self.path.as_str().split(['.', '[']).next().unwrap_or_default()
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render("isoloom.yml"))
    }
}

/// The problems of a spec, in file order.
pub type Problems = Vec<Problem>;

/// The known name closest to `word`, when it is close (a typo).
pub(crate) fn closest<'a>(word: &str, known: &[&'a str]) -> Option<&'a str> {
    known
        .iter()
        .map(|k| (distance(word, k), *k))
        .filter(|(d, k)| *d <= 2.max(k.len() / 4))
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

/// The edit distance between two words.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    // One row of the table at a time: `row[j]` is the distance between what of `a` was read and
    // the first `j` letters of `b`.
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut next = Vec::with_capacity(row.len());
        next.push(i + 1);
        for ((cb, diagonal), above) in b.iter().zip(&row).zip(row.iter().skip(1)) {
            let left = next.last().copied().unwrap_or_default();
            next.push((diagonal + usize::from(ca != *cb)).min(above + 1).min(left + 1));
        }
        row = next;
    }
    row.last().copied().unwrap_or_default()
}

/// The three lines an author reads: `head` and the field's place in `file`, the reason, then
/// the fix when there is one.
pub(crate) fn lines(head: &str, span: Option<Span>, file: &str, reason: &str, fix: Option<&str>) -> String {
    let place = span.map(|s| format!("  ({file}:{}:{})", s.line, s.column)).unwrap_or_default();
    let mut out = format!("{head}{place}\n  {reason}");
    if let Some(fix) = fix {
        out.push_str("\n  fix: ");
        out.push_str(fix);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_problem_reads_on_one_line_as_its_path_and_reason() {
        let p = Problem::new(Code::Refused, FieldPath::new("machines.web.app.build"), "a Dockerfile builds a container only");
        assert_eq!(p.line(), "machines.web.app.build: a Dockerfile builds a container only");
        assert_eq!(p.to_string(), p.render("isoloom.yml"));
        assert_eq!(p.to_string(), "error[refused] machines.web.app.build\n  a Dockerfile builds a container only");
    }

    #[test]
    fn a_problem_reads_as_three_lines() {
        let mut p = Problem::new(Code::RemovedField, FieldPath::new("machines.web.docker"), "`docker:` is gone").fix("`app: { image: <ref> }`");
        assert_eq!(
            p.render("isoloom.yml"),
            "error[removed-field] machines.web.docker\n  `docker:` is gone\n  fix: `app: { image: <ref> }`"
        );
        p.span = Some(Span { line: 12, column: 5 });
        assert!(
            p.render("isoloom.yml")
                .starts_with("error[removed-field] machines.web.docker  (isoloom.yml:12:5)\n")
        );
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["code"], "removed-field");
        assert_eq!((json["line"].as_u64(), json["column"].as_u64()), (Some(12), Some(5)));
        assert_eq!(p.section(), "machines");
    }

    #[test]
    fn a_typo_finds_the_name_it_meant() {
        // A long name allows more edits: a quarter of its length.
        assert_eq!(closest("nested-virtualisatio", &["nested_virtualization"]), Some("nested_virtualization"));
        assert_eq!((distance("a", "b"), distance("on", "no"), distance("flaw", "lawn")), (1, 2, 2));
        assert_eq!(closest("netwrks", &["networks", "ports"]), Some("networks"));
        assert_eq!(closest("dc1", &["dc01", "ws01"]), Some("dc01"));
        assert_eq!(closest("zzz", &["networks"]), None);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!((distance("", "abc"), distance("abc", "")), (3, 3));
    }

    #[test]
    fn paths_nest_and_contain() {
        let web = FieldPath::new("machines").key("web");
        let file = web.key("files").index(0);
        assert_eq!(file.as_str(), "machines.web.files[0]");
        assert!(file.within(&web) && !web.within(&file) && !FieldPath::new("machines.webs").within(&web));
        assert_eq!(file.parent(), Some(web.key("files")));
    }
}
