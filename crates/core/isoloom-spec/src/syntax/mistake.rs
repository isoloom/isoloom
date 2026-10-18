//! A mistake a typed read finds, with the code, the field and the fix an author reads. Serde
//! carries a read's error as text only, so a mistake travels encoded in that text: its reason
//! first (what a raw error shows), then its code, field and fix after a separator no spec text
//! holds. The parser decodes it into a [`Problem`](super::Problem) at the field it names.

use std::fmt;

use super::problem::{Code, FieldPath, Problem};

/// What separates a mistake's parts in its text: the ASCII unit separator.
const MARK: char = '\u{1f}';

/// A coded mistake, found while reading a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mistake {
    /// Its code.
    code: Code,
    /// The field under the value read where it is, when not the value itself (`url` of a
    /// `file` step).
    field: Option<&'static str>,
    /// Why it is wrong.
    reason: String,
    /// What to write instead.
    fix: Option<String>,
}

impl Mistake {
    /// A mistake of `code` in the value being read.
    pub(crate) fn new(code: Code, reason: impl Into<String>) -> Mistake {
        Mistake {
            code,
            field: None,
            reason: reason.into(),
            fix: None,
        }
    }

    /// The mistake, at field `field` of the value being read.
    #[must_use]
    pub(crate) fn at(mut self, field: &'static str) -> Mistake {
        self.field = Some(field);
        self
    }

    /// The mistake, with its fix.
    #[must_use]
    pub(crate) fn fix(mut self, fix: impl Into<String>) -> Mistake {
        self.fix = Some(fix.into());
        self
    }

    /// The problem `message` holds, at `path` (the value read), when it is an encoded mistake.
    pub(crate) fn decode(message: &str, path: &FieldPath) -> Option<Problem> {
        let mut parts = message.split(MARK);
        let reason = parts.next()?;
        let code = parts.next().and_then(code)?;
        let field = parts.next().unwrap_or_default();
        let fix = parts.next().unwrap_or_default();
        let at = if field.is_empty() { path.clone() } else { path.key(field) };
        let problem = Problem::new(code, at, reason);
        Some(if fix.is_empty() { problem } else { problem.fix(fix) })
    }
}

/// The code `id` names (`missing-field`).
fn code(id: &str) -> Option<Code> {
    [
        Code::UnknownField,
        Code::RemovedField,
        Code::MissingField,
        Code::InvalidValue,
        Code::Conflict,
        Code::UnknownReference,
        Code::SensitiveValue,
        Code::Refused,
    ]
    .into_iter()
    .find(|c| c.id() == id)
}

impl fmt::Display for Mistake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{MARK}{}{MARK}{}{MARK}{}",
            self.reason,
            self.code.id(),
            self.field.unwrap_or_default(),
            self.fix.as_deref().unwrap_or_default()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mistake_reads_back_as_its_problem_at_its_field() {
        let m = Mistake::new(Code::MissingField, "every fetch is pinned").at("url").fix("`sha256: <hex>`");
        let at = FieldPath::new("machines.web.app.steps[2].file");
        let p = Mistake::decode(&m.to_string(), &at).expect("a mistake");
        assert_eq!((p.code, p.path.as_str()), (Code::MissingField, "machines.web.app.steps[2].file.url"));
        assert_eq!((p.reason.as_str(), p.fix.as_deref()), ("every fetch is pinned", Some("`sha256: <hex>`")));
        assert!(m.to_string().starts_with("every fetch is pinned"), "the reason shows first");
    }

    #[test]
    fn a_plain_message_is_no_mistake() {
        assert_eq!(Mistake::decode("invalid type: map, expected a string", &FieldPath::new("a")), None);
        let bare = Mistake::decode(&Mistake::new(Code::Conflict, "one guard").to_string(), &FieldPath::new("a")).expect("a mistake");
        assert_eq!((bare.path.as_str(), bare.fix), ("a", None));
    }
}
