//! A Dockerfile read into typed instructions, each with the line it starts on: continuation lines
//! joined (the `escape` directive honoured), comments dropped, heredocs (`RUN <<EOF`) folded into
//! their instruction, the exec form (`["a", "b"]`) told from the shell form.

use std::fmt;

/// A command as a Dockerfile writes it: a text for the shell, or a program and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// The shell form: `RUN apt-get update`.
    Shell(String),
    /// The exec form: `CMD ["nginx", "-g", "daemon off;"]`.
    Exec(Vec<String>),
}

/// Where `COPY`/`ADD` takes its sources from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The build context.
    Context,
    /// An earlier stage (`--from=build`), or an image (`--from=composer:2`).
    From(String),
}

/// A `COPY` or `ADD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    /// Where the sources are.
    pub source: Source,
    /// The sources, as written (globs and all).
    pub sources: Vec<String>,
    /// The destination, as written.
    pub dest: String,
    /// `--chown=user[:group]`.
    pub chown: Option<String>,
    /// `--chmod=mode`.
    pub chmod: Option<String>,
}

/// One instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    /// `FROM image [AS name]`.
    From {
        /// The image (or an earlier stage's name).
        image: String,
        /// The stage's name.
        stage: Option<String>,
    },
    /// `RUN`. `mounts`: it uses `--mount` (a build cache or secret).
    Run {
        /// What runs.
        command: Command,
        /// It names `--mount` or `--network`.
        flags: bool,
    },
    /// `COPY`.
    Copy(Copy),
    /// `ADD`: a `COPY` that also fetches URLs and unpacks local archives.
    Add(Copy),
    /// `ENV k=v ...` (or the old `ENV k v`).
    Env(Vec<(String, String)>),
    /// `ARG name[=default]`.
    Arg {
        /// The name.
        name: String,
        /// Its default.
        default: Option<String>,
    },
    /// `WORKDIR`.
    Workdir(String),
    /// `USER user[:group]`.
    User(String),
    /// `EXPOSE`.
    Expose,
    /// `CMD`.
    Cmd(Command),
    /// `ENTRYPOINT`.
    Entrypoint(Command),
    /// `VOLUME`: the paths.
    Volume(Vec<String>),
    /// `SHELL`: the shell RUN's shell form runs in.
    Shell(Vec<String>),
    /// What has no effect on what runs: `LABEL`, `MAINTAINER`, `STOPSIGNAL`, `HEALTHCHECK` (the
    /// spec's own `healthcheck` and ports are what a machine is probed by).
    Metadata(&'static str),
    /// An instruction with no VM form (`ONBUILD`), or one this reader doesn't know.
    Other(String),
}

/// An instruction and the line it starts on (1-based).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The line it starts on.
    pub number: usize,
    /// The keyword as written, upper-cased (`RUN`).
    pub keyword: String,
    /// The instruction.
    pub instruction: Instruction,
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {} (`{}`)", self.number, self.keyword)
    }
}

/// A Dockerfile that can't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// The line.
    pub line: usize,
    /// Why.
    pub reason: String,
}

/// Every instruction of `text`, in order.
///
/// # Errors
///
/// [`Unreadable`]: a line that is no instruction, or an instruction missing its arguments.
pub fn read(text: &str) -> Result<Vec<Line>, Unreadable> {
    let lines: Vec<&str> = text.lines().collect();
    let escape = escape_char(&lines);
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(first) = lines.get(i) {
        let start = i;
        i += 1;
        let trimmed = first.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let logical = logical_line(&lines, trimmed, &mut i, escape);
        let unreadable = |reason: String| Unreadable { line: start + 1, reason };
        let (keyword, rest) = split_keyword(&logical).ok_or_else(|| unreadable(format!("`{logical}` is no instruction")))?;
        let rest = match heredoc_end(&keyword, rest) {
            Some(end) => format!("{}\n{}", without_heredoc_marker(rest), heredoc_body(&lines, &mut i, &end)),
            None => rest.to_owned(),
        };
        let instruction = parse(&keyword, &rest).map_err(unreadable)?;
        out.push(Line {
            number: start + 1,
            keyword,
            instruction,
        });
    }
    Ok(out)
}

/// The logical line starting with `first`, its continuation lines (from `i`, which moves past
/// them) joined; comment and blank lines inside a continuation are dropped, as Docker does.
fn logical_line(lines: &[&str], first: &str, i: &mut usize, escape: char) -> String {
    let mut logical = String::new();
    let mut current = first.to_owned();
    while let Some(head) = current.strip_suffix(escape) {
        logical.push_str(head);
        logical.push(' ');
        current = String::new();
        while let Some(next) = lines.get(*i) {
            *i += 1;
            let t = next.trim();
            if !t.is_empty() && !t.starts_with('#') {
                t.clone_into(&mut current);
                break;
            }
        }
    }
    logical.push_str(&current);
    logical
}

/// A heredoc's body: the lines from `i` up to `end` (which `i` moves past).
fn heredoc_body(lines: &[&str], i: &mut usize, end: &str) -> String {
    let mut body = Vec::new();
    while let Some(next) = lines.get(*i) {
        *i += 1;
        if next.trim() == end {
            break;
        }
        body.push(*next);
    }
    body.join("\n")
}

/// The escape character the parser directive sets (`# escape=``), else `\`.
fn escape_char(lines: &[&str]) -> char {
    for line in lines {
        let t = line.trim();
        let Some(directive) = t.strip_prefix('#') else { break };
        let directive = directive.trim().to_ascii_lowercase();
        if let Some(value) = directive.strip_prefix("escape").map(str::trim).and_then(|v| v.strip_prefix('=')) {
            return if value.trim() == "`" { '`' } else { '\\' };
        }
    }
    '\\'
}

/// The keyword (upper-cased) and the rest of a logical line.
fn split_keyword(line: &str) -> Option<(String, &str)> {
    let end = line.find(char::is_whitespace).unwrap_or(line.len());
    let keyword = line.get(..end)?;
    if keyword.is_empty() || !keyword.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some((keyword.to_ascii_uppercase(), line.get(end..).unwrap_or_default().trim()))
}

/// The heredoc's end word, when a `RUN` or `COPY` opens one (`<<EOF`, `<<-'EOF'`).
fn heredoc_end(keyword: &str, rest: &str) -> Option<String> {
    if keyword != "RUN" {
        return None;
    }
    let at = rest.find("<<")?;
    let word = rest.get(at + 2..)?.trim_start_matches('-');
    let word: String = word.trim_matches(['"', '\'']).chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    (!word.is_empty()).then_some(word)
}

/// A heredoc `RUN`'s own line without its `<<EOF` marker (`RUN <<EOF` runs only the body;
/// `RUN bash <<EOF` hands the body to bash).
fn without_heredoc_marker(rest: &str) -> String {
    rest.split_whitespace().filter(|w| !w.starts_with("<<")).collect::<Vec<_>>().join(" ")
}

/// The instruction `keyword` with its arguments `rest`.
fn parse(keyword: &str, rest: &str) -> Result<Instruction, String> {
    let needs = |what: &str| format!("`{keyword}` needs {what}");
    Ok(match keyword {
        "FROM" => from(rest).ok_or_else(|| needs("an image"))?,
        "RUN" => {
            let (flags, command) = flags_and_rest(rest);
            Instruction::Run {
                command: command_of(command),
                flags: flags.iter().any(|f| ["--mount", "--network", "--security"].iter().any(|p| f.starts_with(p))),
            }
        }
        "COPY" => Instruction::Copy(copy(rest).ok_or_else(|| needs("a source and a destination"))?),
        "ADD" => Instruction::Add(copy(rest).ok_or_else(|| needs("a source and a destination"))?),
        "ENV" => Instruction::Env(pairs(rest).ok_or_else(|| needs("`name=value`"))?),
        "ARG" => arg(rest).ok_or_else(|| needs("a name"))?,
        "WORKDIR" => Instruction::Workdir(unquote(rest)),
        "USER" => Instruction::User(rest.trim().to_owned()),
        "EXPOSE" => Instruction::Expose,
        "CMD" => Instruction::Cmd(command_of(rest)),
        "ENTRYPOINT" => Instruction::Entrypoint(command_of(rest)),
        "VOLUME" => Instruction::Volume(json_array(rest).unwrap_or_else(|| rest.split_whitespace().map(str::to_owned).collect())),
        "SHELL" => Instruction::Shell(json_array(rest).ok_or_else(|| needs("a JSON array"))?),
        "LABEL" => Instruction::Metadata("LABEL"),
        "MAINTAINER" => Instruction::Metadata("MAINTAINER"),
        "STOPSIGNAL" => Instruction::Metadata("STOPSIGNAL"),
        "HEALTHCHECK" => Instruction::Metadata("HEALTHCHECK"),
        other => Instruction::Other(other.to_owned()),
    })
}

/// `FROM [--platform=..] image [AS name]`.
fn from(rest: &str) -> Option<Instruction> {
    let words: Vec<&str> = rest.split_whitespace().filter(|w| !w.starts_with("--")).collect();
    let image = (*words.first()?).to_owned();
    let stage = match words.as_slice() {
        [_, as_, name] if as_.eq_ignore_ascii_case("as") => Some((*name).to_ascii_lowercase()),
        _ => None,
    };
    Some(Instruction::From { image, stage })
}

/// `ARG name[=default]`.
fn arg(rest: &str) -> Option<Instruction> {
    let (name, default) = match rest.split_once('=') {
        Some((n, d)) => (n.trim(), Some(unquote(d.trim()))),
        None => (rest.trim(), None),
    };
    (!name.is_empty()).then(|| Instruction::Arg {
        name: name.to_owned(),
        default,
    })
}

/// The leading `--flag[=value]` words of `rest`, and what follows them.
fn flags_and_rest(rest: &str) -> (Vec<&str>, &str) {
    let mut flags = Vec::new();
    let mut tail = rest.trim_start();
    while tail.starts_with("--") {
        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        flags.push(tail.get(..end).unwrap_or_default());
        tail = tail.get(end..).unwrap_or_default().trim_start();
    }
    (flags, tail)
}

/// The exec form when `rest` is a JSON array of strings, else the shell form.
fn command_of(rest: &str) -> Command {
    match json_array(rest) {
        Some(argv) => Command::Exec(argv),
        None => Command::Shell(rest.trim().to_owned()),
    }
}

/// `rest` as a JSON array of strings, when it is one.
fn json_array(rest: &str) -> Option<Vec<String>> {
    let t = rest.trim();
    if !t.starts_with('[') {
        return None;
    }
    serde_json::from_str(t).ok()
}

/// A `COPY`/`ADD`'s flags, sources and destination.
fn copy(rest: &str) -> Option<Copy> {
    let (flags, tail) = flags_and_rest(rest);
    let mut words = json_array(tail).unwrap_or_else(|| tail.split_whitespace().map(str::to_owned).collect());
    let dest = words.pop()?;
    if words.is_empty() {
        return None;
    }
    let flag = |name: &str| flags.iter().find_map(|f| f.strip_prefix(name).map(str::to_owned));
    Some(Copy {
        source: flag("--from=").map_or(Source::Context, |s| Source::From(s.to_ascii_lowercase())),
        sources: words,
        dest,
        chown: flag("--chown="),
        chmod: flag("--chmod="),
    })
}

/// `ENV`'s pairs: `k=v k2="v 2"`, or the old `k v w` (one pair, the rest its value).
fn pairs(rest: &str) -> Option<Vec<(String, String)>> {
    let rest = rest.trim();
    let first = rest.split_whitespace().next()?;
    if !first.contains('=') {
        let (k, v) = rest.split_once(char::is_whitespace)?;
        return Some(vec![(k.to_owned(), v.trim().to_owned())]);
    }
    let mut out = Vec::new();
    for word in shell_words(rest) {
        let (k, v) = word.split_once('=')?;
        out.push((k.to_owned(), v.to_owned()));
    }
    Some(out)
}

/// `text` split as the shell splits words, quotes removed (no expansion).
fn shell_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    let mut started = false;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => word.extend(chars.next()),
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, '\\') => {
                word.extend(chars.next());
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
                started = false;
            }
            (None, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    if started || !word.is_empty() {
        out.push(word);
    }
    out
}

/// `text` without one pair of surrounding quotes.
fn unquote(text: &str) -> String {
    let t = text.trim();
    for q in ['"', '\''] {
        if let Some(inner) = t.strip_prefix(q).and_then(|s| s.strip_suffix(q)) {
            return inner.to_owned();
        }
    }
    t.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> Instruction {
        read(text).unwrap().remove(0).instruction
    }

    #[test]
    fn continuations_join_and_comments_inside_them_drop() {
        let lines = read("FROM debian:12\n\n# a comment\nRUN apt-get update \\\n  # inside\n && apt-get install -y curl\n").unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[1].number, lines[1].keyword.as_str()), (4, "RUN"));
        assert_eq!(
            lines[1].instruction,
            Instruction::Run {
                command: Command::Shell("apt-get update  && apt-get install -y curl".to_owned()),
                flags: false
            }
        );
    }

    #[test]
    fn exec_and_shell_forms_are_told_apart() {
        assert_eq!(one(r#"CMD ["nginx", "-g", "daemon off;"]"#), Instruction::Cmd(Command::Exec(vec!["nginx".into(), "-g".into(), "daemon off;".into()])));
        assert_eq!(one("CMD python app.py"), Instruction::Cmd(Command::Shell("python app.py".into())));
    }

    #[test]
    fn env_reads_both_forms_and_quotes() {
        assert_eq!(one(r#"ENV A=1 B="two words""#), Instruction::Env(vec![("A".into(), "1".into()), ("B".into(), "two words".into())]));
        assert_eq!(one("ENV PATH /usr/local/bin:$PATH"), Instruction::Env(vec![("PATH".into(), "/usr/local/bin:$PATH".into())]));
    }

    #[test]
    fn copy_reads_its_flags() {
        let Instruction::Copy(c) = one("COPY --from=build --chown=app:app /src/out /app/") else { panic!() };
        assert_eq!((c.source, c.sources, c.dest, c.chown), (Source::From("build".into()), vec!["/src/out".to_owned()], "/app/".to_owned(), Some("app:app".to_owned())));
    }

    #[test]
    fn a_heredoc_is_folded_into_its_run() {
        let lines = read("FROM alpine:3.19\nRUN <<EOF\nset -e\necho hi\nEOF\nCMD [\"sh\"]\n").unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].instruction, Instruction::Run { command: Command::Shell("set -e\necho hi".into()), flags: false });
    }

    #[test]
    fn the_escape_directive_changes_the_continuation() {
        let lines = read("# escape=`\nFROM debian:12\nRUN echo a `\n  b\n").unwrap();
        assert_eq!(lines[1].instruction, Instruction::Run { command: Command::Shell("echo a  b".into()), flags: false });
    }

    #[test]
    fn a_line_that_is_no_instruction_is_unreadable() {
        assert_eq!(read("FROM debian:12\n123 go\n").unwrap_err().line, 2);
    }
}
