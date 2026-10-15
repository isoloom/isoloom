//! `.isoloomignore`: the project's paths the machines don't get when the project is copied into
//! them (Vagrant's `project` step, Proxmox's cloud-init, the cloud modules' archive, and the
//! runner's own copies). Gitignore-style, at the project's root:
//!
//! - one pattern per line; blank lines and `#` comments are skipped, spaces around a pattern
//!   too;
//! - `!pattern` brings back what an earlier pattern left out;
//! - a trailing `/` (or `/**`) matches folders only (and so everything in them);
//! - a `/` at the start or in the middle anchors the pattern at the project's root; otherwise it
//!   matches at any depth (`*.log`, `.env`);
//! - `*` is any run of characters but `/`, `?` one character but `/`, `[abc]` / `[a-z]` /
//!   `[!abc]` a set, `**` any number of folders (`a/**/b`, `**/x`);
//! - a pattern matching a folder matches everything in it, and the last pattern matching a path
//!   (or a folder it is in) decides: unlike git, `!` can bring back a file of an ignored folder.
//!
//! The generated files read the file when they run, each translating a pattern into the same
//! regular expression (`^<glob>(/|$)`, or `^<glob>/` for folders only); this matcher follows the
//! same rules, for a copy made outside those tools (the runner's), so it can't disagree with
//! them. It reads text only: walking the project is the caller's.

use std::collections::BTreeSet;

/// The file's name, at the project's root.
pub const FILE: &str = ".isoloomignore";

/// One piece of a pattern, as matched.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// A character as it is.
    Char(char),
    /// `*`: any run of characters but `/`.
    Star,
    /// `?`: one character but `/`.
    Any,
    /// `[...]`: one character in (or, negated, not in) the ranges.
    Set {
        /// `[!...]`.
        negated: bool,
        /// Each member as a range (`a-z`, or `x-x` for one character).
        ranges: Vec<(char, char)>,
    },
    /// `**/`: nothing, or anything ending with `/`.
    Folders,
    /// `**` elsewhere: anything.
    Everything,
}

/// One line of the file, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    tokens: Vec<Token>,
    /// A trailing `/` (or `/**`): it matches folders only.
    folders_only: bool,
    /// `!pattern`: it brings back what it matches.
    keep: bool,
}

/// The rules of an `.isoloomignore`, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules(Vec<Rule>);

impl Rules {
    /// The rules of a file's text (every line is a rule or nothing: none is an error).
    #[must_use]
    pub fn parse(text: &str) -> Rules {
        Rules(text.split('\n').filter_map(rule).collect())
    }

    /// Whether the rules leave a path out (relative to the project, with `/`; a folder's path
    /// ends with `/`): the last rule matching it, or a folder it is in, decides.
    #[must_use]
    pub fn ignored(&self, path: &str) -> bool {
        let chars: Vec<char> = path.chars().collect();
        self.0.iter().fold(false, |out, r| {
            let hit = ends(&r.tokens, &chars, 0).into_iter().any(|e| match chars.get(e) {
                // Stopping on a `/`: a folder that holds the path (or the path itself, a folder).
                Some(c) => *c == '/',
                // The whole path: a file, unless the rule matches folders only.
                None => !r.folders_only,
            });
            if hit { !r.keep } else { out }
        })
    }

    /// Whether a `!` pattern may bring back something in an ignored folder.
    #[must_use]
    pub fn brings_back(&self) -> bool {
        self.0.iter().any(|r| r.keep)
    }

    /// Whether there are no rules at all (no `.isoloomignore`, or an empty one).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One line's rule; `None` for a blank line, a comment or a pattern of nothing (`/`).
fn rule(line: &str) -> Option<Rule> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (keep, pat) = line.strip_prefix('!').map_or((false, line), |p| (true, p));
    let s = pat.strip_suffix('/').unwrap_or(pat);
    let s = s.strip_suffix("/**").unwrap_or(s);
    let glob = s.strip_suffix('/').unwrap_or(s);
    if glob.is_empty() {
        return None;
    }
    // A `/` at the start or in the middle anchors the pattern at the root.
    let anchored = if glob.contains('/') {
        glob.strip_prefix('/').unwrap_or(glob).to_owned()
    } else {
        format!("**/{glob}")
    };
    Some(Rule {
        tokens: tokens(&anchored.chars().collect::<Vec<char>>()),
        folders_only: glob != pat,
        keep,
    })
}

/// A glob's tokens, read left to right: each step takes at least one character.
fn tokens(glob: &[char]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = glob;
    while let Some((token, after)) = token(rest) {
        out.push(token);
        rest = after;
    }
    out
}

/// The first token of `glob` and what follows it; `None` at its end.
fn token(glob: &[char]) -> Option<(Token, &[char])> {
    Some(match glob {
        [] => return None,
        ['*', '*', '/', rest @ ..] => (Token::Folders, rest),
        ['*', '*', rest @ ..] => (Token::Everything, rest),
        ['*', rest @ ..] => (Token::Star, rest),
        ['?', rest @ ..] => (Token::Any, rest),
        ['[', rest @ ..] => set(rest).unwrap_or((Token::Char('['), rest)),
        [c, rest @ ..] => (Token::Char(*c), rest),
    })
}

/// A set after its `[`, up to the next `]` (one right after `[` or `[!` is a member), and what
/// follows it; `None` without a closing `]` (the `[` is then a plain character).
fn set(after: &[char]) -> Option<(Token, &[char])> {
    let (negated, body) = match after {
        ['!', rest @ ..] => (true, rest),
        _ => (false, after),
    };
    // The first member may be a `]`; the set ends at the next one.
    let from = usize::from(body.first() == Some(&']'));
    let close = body.iter().skip(from).position(|c| *c == ']')? + from;
    let (members, rest) = body.split_at_checked(close)?;
    Some((
        Token::Set {
            negated,
            ranges: ranges(members),
        },
        rest.get(1..).unwrap_or_default(),
    ))
}

/// A set's members as ranges: `a-z`, or one character.
fn ranges(members: &[char]) -> Vec<(char, char)> {
    let mut out = Vec::new();
    let mut rest = members;
    loop {
        rest = match rest {
            [a, '-', b, tail @ ..] => {
                out.push((*a, *b));
                tail
            }
            [c, tail @ ..] => {
                out.push((*c, *c));
                tail
            }
            [] => return out,
        };
    }
}

/// Every position where `tokens` can stop matching `s` from `at`, anchored at `at`.
fn ends(tokens: &[Token], s: &[char], at: usize) -> BTreeSet<usize> {
    let Some((first, rest)) = tokens.split_first() else {
        return BTreeSet::from([at]);
    };
    let next = s.get(at);
    let one = |matched: bool| if matched { vec![at + 1] } else { Vec::new() };
    let positions: Vec<usize> = match first {
        Token::Char(ch) => one(next == Some(ch)),
        Token::Any => one(next.is_some_and(|c| *c != '/')),
        Token::Set { negated, ranges } => one(next.is_some_and(|c| ranges.iter().any(|(a, b)| a <= c && c <= b) != *negated)),
        // Any run of characters up to the next `/` (or the end).
        Token::Star => {
            let run = s.get(at..).unwrap_or_default().iter().take_while(|c| **c != '/').count();
            (at..=at + run).collect()
        }
        // Here, or right after any `/` from here on.
        Token::Folders => std::iter::once(at)
            .chain((at..s.len()).filter(|p| s.get(*p) == Some(&'/')).map(|p| p + 1))
            .collect(),
        Token::Everything => (at..=s.len()).collect(),
    };
    positions.into_iter().flat_map(|p| ends(rest, s, p)).collect()
}

#[cfg(test)]
mod tests;
