//! `.isoloomignore`: the project's paths the machines don't get when the project is copied into
//! them (Vagrant's `project` step, Proxmox's cloud-init, the cloud modules' archive, `isoloom
//! run external`). Gitignore-style, at the project's root:
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
//! same rules, for the copies Isoloom makes itself.

use std::path::Path;

/// The file's name, at the project's root.
pub const FILE: &str = ".isoloomignore";

/// One pattern, as matched.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Char(char),
    /// `*`: any run of characters but `/`.
    Star,
    /// `?`: one character but `/`.
    Any,
    /// `[...]`: one character in (or, negated, not in) the ranges.
    Set {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
    /// `**/`: nothing, or anything ending with `/`.
    Folders,
    /// `**` elsewhere: anything.
    Everything,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    tokens: Vec<Token>,
    folders_only: bool,
    keep: bool,
}

/// The rules of an `.isoloomignore`, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules(Vec<Rule>);

impl Rules {
    /// The rules of a file's text.
    pub fn parse(text: &str) -> Rules {
        let mut rules = Vec::new();
        for line in text.split('\n') {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (keep, pat) = match line.strip_prefix('!') {
                Some(p) => (true, p),
                None => (false, line),
            };
            let s = pat.strip_suffix('/').unwrap_or(pat);
            let s = s.strip_suffix("/**").unwrap_or(s);
            let glob = s.strip_suffix('/').unwrap_or(s);
            if glob.is_empty() {
                continue;
            }
            let anchored = if glob.contains('/') {
                glob.strip_prefix('/').unwrap_or(glob).to_string()
            } else {
                format!("**/{glob}")
            };
            rules.push(Rule {
                tokens: tokens(&anchored),
                folders_only: glob != pat,
                keep,
            });
        }
        Rules(rules)
    }

    /// The rules of the project's `.isoloomignore` (none when there's no such file).
    pub fn read(project: &Path) -> Rules {
        std::fs::read_to_string(project.join(FILE)).map(|t| Rules::parse(&t)).unwrap_or_default()
    }

    /// Whether the rules leave a path out (relative to the project, with `/`; a folder's path
    /// ends with `/`).
    pub fn ignored(&self, path: &str) -> bool {
        let chars: Vec<char> = path.chars().collect();
        let mut out = false;
        for r in &self.0 {
            let hit = ends(&r.tokens, &chars, 0).into_iter().any(|e| {
                if r.folders_only {
                    chars.get(e) == Some(&'/')
                } else {
                    e == chars.len() || chars[e] == '/'
                }
            });
            if hit {
                out = !r.keep;
            }
        }
        out
    }

    /// Whether a `!` pattern may bring back something in an ignored folder.
    pub fn brings_back(&self) -> bool {
        self.0.iter().any(|r| r.keep)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn tokens(glob: &str) -> Vec<Token> {
    let c: Vec<char> = glob.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        match c[i] {
            '*' if c.get(i + 1) == Some(&'*') => {
                if c.get(i + 2) == Some(&'/') {
                    out.push(Token::Folders);
                    i += 3;
                } else {
                    out.push(Token::Everything);
                    i += 2;
                }
                continue;
            }
            '*' => out.push(Token::Star),
            '?' => out.push(Token::Any),
            '[' => {
                // A set up to the next `]` (one right after `[` or `[!` is a member); without
                // one, a plain `[`.
                let mut j = i + 1;
                let negated = c.get(j) == Some(&'!');
                if negated {
                    j += 1;
                }
                let start = j;
                if c.get(j) == Some(&']') {
                    j += 1;
                }
                while j < c.len() && c[j] != ']' {
                    j += 1;
                }
                if j < c.len() {
                    let members = &c[start..j];
                    let mut ranges = Vec::new();
                    let mut k = 0;
                    while k < members.len() {
                        if k + 2 < members.len() && members[k + 1] == '-' {
                            ranges.push((members[k], members[k + 2]));
                            k += 3;
                        } else {
                            ranges.push((members[k], members[k]));
                            k += 1;
                        }
                    }
                    out.push(Token::Set { negated, ranges });
                    i = j + 1;
                    continue;
                }
                out.push(Token::Char('['));
            }
            ch => out.push(Token::Char(ch)),
        }
        i += 1;
    }
    out
}

/// Every position where `tokens` can stop matching `s` from `at`, anchored at `at`.
fn ends(tokens: &[Token], s: &[char], at: usize) -> Vec<usize> {
    let Some((first, rest)) = tokens.split_first() else {
        return vec![at];
    };
    let mut out = Vec::new();
    let mut then = |p: usize| {
        for e in ends(rest, s, p) {
            if !out.contains(&e) {
                out.push(e);
            }
        }
    };
    match first {
        Token::Char(ch) => {
            if s.get(at) == Some(ch) {
                then(at + 1)
            }
        }
        Token::Any => {
            if s.get(at).is_some_and(|c| *c != '/') {
                then(at + 1)
            }
        }
        Token::Set { negated, ranges } => {
            if let Some(c) = s.get(at)
                && ranges.iter().any(|(a, b)| a <= c && c <= b) != *negated
            {
                then(at + 1)
            }
        }
        Token::Star => {
            let mut p = at;
            loop {
                then(p);
                if p < s.len() && s[p] != '/' {
                    p += 1;
                } else {
                    break;
                }
            }
        }
        Token::Folders => {
            then(at);
            for (p, c) in s.iter().enumerate().skip(at) {
                if *c == '/' {
                    then(p + 1);
                }
            }
        }
        Token::Everything => {
            for p in at..=s.len() {
                then(p);
            }
        }
    }
    out
}

/// The project's entries a copy takes, parents first: folders (ending with `/`), files and
/// symbolic links (never followed), relative to `root` with `/`. Version control and the tools'
/// state (`.git`, `.vagrant`, `.terraform`) stay out at any depth, and so does what the rules
/// leave out.
pub fn entries(root: &Path, rules: &Rules) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    walk(root, "", rules, &mut out)?;
    Ok(out)
}

fn walk(root: &Path, dir: &str, rules: &Rules, out: &mut Vec<String>) -> std::io::Result<()> {
    let mut names: Vec<String> = std::fs::read_dir(root.join(dir))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        if [".git", ".vagrant", ".terraform"].contains(&name.as_str()) {
            continue;
        }
        let rel = if dir.is_empty() { name } else { format!("{dir}/{name}") };
        let Ok(meta) = std::fs::symlink_metadata(root.join(&rel)) else { continue };
        if meta.is_dir() {
            let path = format!("{rel}/");
            let ignored = rules.ignored(&path);
            if !ignored {
                out.push(path);
            }
            if !ignored || rules.brings_back() {
                walk(root, &rel, rules, out)?;
            }
        } else if !rules.ignored(&rel) {
            out.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept<'a>(rules: &str, paths: &[&'a str]) -> Vec<&'a str> {
        let r = Rules::parse(rules);
        paths.iter().copied().filter(|p| !r.ignored(p)).collect()
    }

    #[test]
    fn gitignore_style_patterns() {
        let paths = [
            "a.txt",
            "logs/x.log",
            "logs/keep.txt",
            "src/app.py",
            "src/deeper/z.py",
            "build/top.o",
            "src/build/out.o",
            "x.tmp",
            "sub/x.tmp",
            ".env",
            "sub/.env",
            "vendor/big/file.bin",
            "vendor/small.txt",
            "data.json",
            "datazjson",
            "docs/sub/deep.md",
            "w/a+b.txt",
            "w/x1",
            "w/x9",
        ];
        let rules =
            "# not copied\n\nlogs/\n!logs/keep.txt\nbuild/\n/x.tmp\nsub/*.tmp\nvendor/big\n.env\ndocs/**\nsrc/*.py\ndata.js?n\nw/a+b.txt   \r\nw/x[!0-4]\n";
        assert_eq!(
            kept(rules, &paths),
            ["a.txt", "logs/keep.txt", "src/deeper/z.py", "vendor/small.txt", "datazjson", "w/x1"]
        );
    }

    #[test]
    fn folders_only_and_double_stars() {
        // `build/` is a folder only: a file named `build` stays.
        assert_eq!(kept("build/", &["build", "build/", "a/build/x"]), ["build"]);
        assert_eq!(kept("a/**/z", &["a/z", "a/b/z", "a/b/c/z", "b/a/z"]), ["b/a/z"]);
        assert_eq!(kept("**/x", &["x", "a/x", "a/x/y", "ax"]), ["ax"]);
        assert_eq!(kept("a/**", &["a", "a/", "a/b"]), ["a"]);
        assert_eq!(kept("*", &["a", "b/c"]), Vec::<&str>::new());
        assert!(Rules::parse("\n# only comments\n/\n").is_empty());
    }
}
