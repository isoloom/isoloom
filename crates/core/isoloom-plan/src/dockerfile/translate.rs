//! A Dockerfile run on one VM, instruction by instruction: each `RUN`, `COPY`, `ADD`, `WORKDIR`
//! and `VOLUME` becomes a shell step run once, as root (as the `USER` in effect through `su`),
//! with the `ENV` and `ARG` values in effect exported and the `WORKDIR` its folder. `FROM` starts
//! from the service matrix's entry, a distro's bare OS, an earlier stage of the same file (its
//! steps already ran on this VM), or a fetched Dockerfile translated first, the same way. The
//! stages the last one needs run in file order on the same VM, so `COPY --from=<stage>` copies
//! within it; every stage must therefore be one OS. What the last stage leaves (environment,
//! folder, user, entrypoint and command) is the image the VM's service runs.

use std::collections::BTreeMap;

use super::Untranslatable;
use super::instruction::{self, Command, Copy, Instruction, Line, Source};
use super::matrix::{self, NoEntry};
use super::sources::Sources;

/// The PATH an image has unless it sets its own.
const DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// How many fetched Dockerfiles deep a `FROM` chain may go.
const MAX_DEPTH: usize = 8;

/// The arguments Docker gives every build (`ARG TARGETPLATFORM` and the like): a VM is amd64 Linux.
const AUTOMATIC: [(&str, &str); 8] = [
    ("TARGETPLATFORM", "linux/amd64"),
    ("TARGETOS", "linux"),
    ("TARGETARCH", "amd64"),
    ("TARGETVARIANT", ""),
    ("BUILDPLATFORM", "linux/amd64"),
    ("BUILDOS", "linux"),
    ("BUILDARCH", "amd64"),
    ("BUILDVARIANT", ""),
];

/// Name/value pairs, in order (an environment, build arguments).
pub(super) type Vars = Vec<(String, String)>;

/// What a stage (or a base image) leaves for what follows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Image {
    /// The VM's OS.
    pub os: String,
    /// The environment, in order.
    pub env: Vars,
    /// The working folder.
    pub workdir: String,
    /// The user (`None`: root).
    pub user: Option<String>,
    /// The entrypoint.
    pub entrypoint: Option<Command>,
    /// The command.
    pub cmd: Option<Command>,
    /// The shell `RUN`'s shell form runs in (`None`: `/bin/sh -c`).
    pub shell: Option<Vec<String>>,
}

impl Image {
    fn of(base: matrix::Base) -> Image {
        // Every image sets PATH (Docker's default unless it says otherwise): `ENV PATH=x:$PATH`
        // extends it.
        let mut env = vec![("PATH".to_owned(), DEFAULT_PATH.to_owned())];
        for (k, v) in base.env {
            env.retain(|(name, _)| *name != k);
            env.push((k, v));
        }
        Image {
            os: base.os,
            env,
            workdir: base.workdir.unwrap_or_else(|| "/".to_owned()),
            user: base.user,
            entrypoint: base.entrypoint,
            cmd: base.cmd,
            shell: None,
        }
    }

    fn set_env(&mut self, key: &str, value: String) {
        match self.env.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,
            None => self.env.push((key.to_owned(), value)),
        }
    }
}

/// A Dockerfile to translate: its name in problems, its text, its build context (a project
/// folder) and the build arguments given.
#[derive(Debug, Clone, Copy)]
pub(super) struct Dockerfile<'f> {
    /// Its project path, or a fetched base's image and origin.
    pub file: &'f str,
    /// Its text.
    pub text: &'f str,
    /// The build context.
    pub context: &'f str,
    /// The build arguments given.
    pub given: &'f [(String, String)],
}

/// A Dockerfile being translated: what every instruction of it reads.
struct File<'f> {
    df: Dockerfile<'f>,
    /// Its stages: each one's lines (the first's with the global ARGs) and name.
    stages: Vec<(Vec<Line>, Option<String>)>,
    /// The global ARGs' values.
    global: Vars,
    /// How many fetched Dockerfiles deep it is.
    depth: usize,
}

impl File<'_> {
    fn at(&self, line: &Line, reason: String, fix: &str) -> Untranslatable {
        Untranslatable::at(self.df.file, line.number, &line.keyword, reason, fix)
    }

    /// A given build argument's value.
    fn given(&self, name: &str) -> Option<String> {
        self.df.given.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }
}

/// A stage under way: its image so far, its ARGs and whether it set its own command.
struct Stage {
    img: Image,
    args: Vars,
    cmd_set: bool,
}

impl Stage {
    /// The values instructions other than `RUN` substitute: the ARGs, then the environment.
    fn vars(&self) -> Vars {
        self.args.iter().chain(self.img.env.iter()).cloned().collect()
    }
}

/// The translation under way: the shell steps so far and the VM's OS once a stage set it.
pub(super) struct Run<'a> {
    sources: &'a Sources,
    /// The machine's own `os`, which the matrix installs on when it can.
    wanted: Option<&'a str>,
    /// The shell steps, in order.
    pub steps: Vec<String>,
    /// The VM's OS, and the image that set it.
    os: Option<(String, String)>,
}

impl<'a> Run<'a> {
    pub(super) fn new(sources: &'a Sources, wanted: Option<&'a str>) -> Run<'a> {
        Run {
            sources,
            wanted,
            steps: Vec::new(),
            os: None,
        }
    }

    /// Dockerfile `df`, `depth` fetched Dockerfiles deep: what its last stage leaves.
    pub(super) fn file(&mut self, df: Dockerfile<'_>, depth: usize) -> Result<Image, Untranslatable> {
        let lines = instruction::read(df.text).map_err(|u| Untranslatable::at(df.file, u.line, "", u.reason, "fix the Dockerfile"))?;
        let stages = split_stages(&lines);
        let Some(last) = stages.len().checked_sub(1) else {
            return Err(Untranslatable::at(
                df.file,
                1,
                "FROM",
                "the Dockerfile has no `FROM`".to_owned(),
                "start it `FROM` an image",
            ));
        };
        let mut global: Vars = AUTOMATIC.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        for line in stages.first().map(|(l, _)| l.as_slice()).unwrap_or_default() {
            if let Instruction::Arg { name, default } = &line.instruction {
                let given = df.given.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
                global.push((name.clone(), given.or_else(|| default.clone()).unwrap_or_default()));
            }
        }
        let f = File { df, stages, global, depth };
        let needed = needed(&f.stages, last);
        let mut done: BTreeMap<usize, Image> = BTreeMap::new();
        for i in needed {
            let image = self.stage(&f, i, &done)?;
            done.insert(i, image);
        }
        done.remove(&last)
            .ok_or_else(|| Untranslatable::at(df.file, 1, "FROM", "the last stage didn't run".to_owned(), "report this to Isoloom"))
    }

    /// Stage `i` of `f` run, the stages before it `done`.
    fn stage(&mut self, f: &File<'_>, i: usize, done: &BTreeMap<usize, Image>) -> Result<Image, Untranslatable> {
        let lines = f.stages.get(i).map(|(l, _)| l.as_slice()).unwrap_or_default();
        // Stage 0's lines start with the global ARGs; its FROM is the first FROM.
        let from_at = lines.iter().position(|l| matches!(l.instruction, Instruction::From { .. })).unwrap_or_default();
        let mut st = Stage {
            img: self.from(f, i, lines.get(from_at), done)?,
            args: Vec::new(),
            cmd_set: false,
        };
        for line in lines.iter().skip(from_at + 1) {
            self.instruction(f, i, &mut st, line)?;
        }
        Ok(st.img)
    }

    /// The image stage `i` starts `FROM` (at `line`): an earlier stage's, or a base's.
    fn from(&mut self, f: &File<'_>, i: usize, line: Option<&Line>, done: &BTreeMap<usize, Image>) -> Result<Image, Untranslatable> {
        let Some(line) = line else {
            return Err(Untranslatable::at(
                f.df.file,
                1,
                "FROM",
                "a stage has no `FROM`".to_owned(),
                "start each stage `FROM` an image",
            ));
        };
        let Instruction::From { image, .. } = &line.instruction else {
            return Err(f.at(line, "a stage has no `FROM`".to_owned(), "start each stage `FROM` an image"));
        };
        // A reference may quote its parts (`debian:"${RELEASE}-slim"`): the image is without them.
        let image_ref = substitute(image, &f.global).replace(['"', '\''], "");
        let img = match stage_index(&f.stages, &image_ref).filter(|j| *j < i) {
            Some(j) => done
                .get(&j)
                .cloned()
                .ok_or_else(|| f.at(line, format!("stage `{image_ref}` didn't run"), "report this to Isoloom"))?,
            None => self.base(f, line, &image_ref)?,
        };
        match &self.os {
            None => self.os = Some((img.os.clone(), image_ref)),
            Some((os, first)) if *os != img.os => {
                return Err(f.at(
                    line,
                    format!("`{image_ref}` is {}, but `{first}` made the VM {os}: a VM is one OS", img.os),
                    "build every stage FROM images of one OS, or give the machine `os` and `app.steps`",
                ));
            }
            Some(_) => {}
        }
        Ok(img)
    }

    /// One instruction of stage `i`.
    fn instruction(&mut self, f: &File<'_>, i: usize, st: &mut Stage, line: &Line) -> Result<(), Untranslatable> {
        match &line.instruction {
            Instruction::From { .. } | Instruction::Expose | Instruction::Metadata(_) => {}
            Instruction::Arg { name, default } => {
                let value = f
                    .given(name)
                    .or_else(|| default.as_ref().map(|d| substitute(d, &st.vars())))
                    .or_else(|| f.global.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
                    .unwrap_or_default();
                st.args.retain(|(k, _)| k != name);
                st.args.push((name.clone(), value));
            }
            Instruction::Env(pairs) => {
                for (k, v) in pairs {
                    let value = substitute(v, &st.vars());
                    st.img.set_env(k, value);
                }
            }
            Instruction::Workdir(dir) => {
                st.img.workdir = absolute(&st.img.workdir, &substitute(dir, &st.vars()));
                self.push(st, &format!("mkdir -p {}", quote(&st.img.workdir)), true);
            }
            Instruction::User(user) => {
                let user = substitute(user, &st.vars());
                st.img.user = (!["root", "0", "0:0"].contains(&user.as_str())).then_some(user);
            }
            Instruction::Run { flags: true, .. } => {
                return Err(f.at(line, "`RUN --mount`/`--network` needs BuildKit's build sandbox".to_owned(), "a plain `RUN`"));
            }
            Instruction::Run { command, .. } => self.run(st, command),
            Instruction::Copy(c) | Instruction::Add(c) => {
                let add = matches!(line.instruction, Instruction::Add(_));
                let body = copy(f, i, c, add, st).map_err(|(reason, fix)| f.at(line, reason, fix))?;
                self.push(st, &body, true);
            }
            Instruction::Volume(paths) => {
                let dirs: Vec<String> = paths.iter().map(|p| quote(&absolute(&st.img.workdir, &substitute(p, &st.vars())))).collect();
                self.push(st, &format!("mkdir -p {}", dirs.join(" ")), true);
            }
            Instruction::Shell(shell) => st.img.shell = Some(shell.clone()),
            Instruction::Cmd(c) => {
                st.img.cmd = Some(c.clone());
                st.cmd_set = true;
            }
            Instruction::Entrypoint(e) => {
                st.img.entrypoint = Some(e.clone());
                // An entrypoint set here drops the command the base image had.
                if !st.cmd_set {
                    st.img.cmd = None;
                }
            }
            Instruction::Other(keyword) => {
                return Err(f.at(
                    line,
                    format!("`{keyword}` has no VM form"),
                    "leave it out, or give the machine `os` and `app.steps`",
                ));
            }
        }
        Ok(())
    }

    /// A `RUN`: its shell form in the image's shell, its exec form as it is.
    fn run(&mut self, st: &Stage, command: &Command) {
        let body = match (command, &st.img.shell) {
            (Command::Shell(text), Some(shell)) => format!("{} {}", argv(shell), quote(text.trim())),
            (Command::Shell(text), None) => text.trim().to_owned(),
            (Command::Exec(a), _) => argv(a),
        };
        self.push(st, &body, false);
    }

    /// The image `image_ref` (a `FROM` at `line` of `f`) as a VM: the matrix's entry, else a
    /// fetched Dockerfile translated first.
    fn base(&mut self, f: &File<'_>, line: &Line, image_ref: &str) -> Result<Image, Untranslatable> {
        match matrix::of(image_ref, self.wanted) {
            Ok(base) => {
                // The base's install runs strict: a failed download or package stops the VM's set-up.
                self.steps.extend(base.prelude.iter().map(|p| format!("set -eu\n{p}")));
                Ok(Image::of(base))
            }
            Err(NoEntry::Refused { reason, fix }) => Err(f.at(line, reason, &fix)),
            Err(NoEntry::Unknown(_)) => match self.sources.get(image_ref) {
                Some(_) if f.depth >= MAX_DEPTH => Err(f.at(
                    line,
                    format!("`{image_ref}` is more than {MAX_DEPTH} fetched images deep"),
                    "give the machine `os` and `app.steps`",
                )),
                Some(fetched) => {
                    let named = format!("{image_ref} ({})", fetched.origin);
                    let (text, context, args) = (fetched.dockerfile.clone(), fetched.context.clone(), fetched.args.clone());
                    let df = Dockerfile {
                        file: &named,
                        text: &text,
                        context: &context,
                        given: &args,
                    };
                    self.file(df, f.depth + 1)
                }
                None => Err(f.at(
                    line,
                    format!("`{image_ref}` is in neither the service matrix nor the fetched bases"),
                    "`isoloom bases fetch` vendors its published Dockerfile; or give the machine `os` and `app.steps`",
                )),
            },
        }
    }

    /// Shell step `body`, run with the stage's environment and arguments in its folder, as its
    /// user unless `as_root` (Docker makes folders and copies files as root).
    fn push(&mut self, st: &Stage, body: &str, as_root: bool) {
        let exports = st
            .args
            .iter()
            .chain(st.img.env.iter())
            .map(|(k, v)| format!("export {k}={}\n", quote(v)))
            .collect::<Vec<_>>()
            .concat();
        let script = format!("set -e\n{exports}mkdir -p {0} && cd {0}\n{body}\n", quote(&st.img.workdir));
        let step = match (&st.img.user, as_root) {
            (Some(user), false) => format!(
                "set -e\nu={}\nsu -s /bin/sh \"$(getent passwd \"$u\" | cut -d: -f1)\" -c {}\n",
                quote(user_name(user)),
                quote(&script)
            ),
            _ => script,
        };
        self.steps.push(step);
    }
}

/// A `COPY`/`ADD`'s shell (stage `i` of `f`), or why it has none and the fix.
fn copy(f: &File<'_>, i: usize, c: &Copy, add: bool, st: &Stage) -> Result<String, (String, &'static str)> {
    let vars = st.vars();
    let dest = substitute(&c.dest, &vars);
    let into = dest.ends_with('/') || dest == "." || c.sources.len() > 1;
    let dest = absolute(&st.img.workdir, &dest);
    let root = match &c.source {
        Source::Context => Some(format!("/opt/isoloom/{}", f.df.context.trim_end_matches('/'))),
        Source::From(stage) => match stage_index(&f.stages, stage).or_else(|| stage.parse().ok()) {
            Some(j) if j < i => None,
            _ => {
                return Err((
                    format!("`--from={stage}` copies out of another image, which the VM doesn't have"),
                    "fetch or install what it copies in a `RUN`, or give the machine `os` and `app.steps`",
                ));
            }
        },
    };
    let mut lines = vec![format!("dest={}", quote(&dest))];
    for src in &c.sources {
        lines.push(copy_one(&substitute(src, &vars), root.as_deref(), add, into)?);
    }
    if let Some(owner) = &c.chown {
        lines.push(format!("chown -R {} \"$dest\"", quote(&substitute(owner, &vars))));
    }
    if let Some(mode) = &c.chmod {
        lines.push(format!("chmod -R {} \"$dest\"", quote(mode)));
    }
    Ok(format!("{COPY_HELPERS}{}", lines.join("\n")))
}

/// One source of a `COPY`/`ADD` into `$dest`: from the project copy under `root`, or (`None`)
/// from an earlier stage on this VM; `into`: `$dest` is a folder.
fn copy_one(src: &str, root: Option<&str>, add: bool, into: bool) -> Result<String, (String, &'static str)> {
    if add && (src.starts_with("http://") || src.starts_with("https://")) {
        let name = src.rsplit('/').next().unwrap_or("download");
        let to = if into { format!("\"$dest\"/{}", quote(name)) } else { "\"$dest\"".to_owned() };
        return Ok(format!("isoloom_fetch {} {to}", quote(src)));
    }
    let rel = src.trim_start_matches("./");
    let path = match root {
        Some(root) => format!("{root}/{rel}"),
        None if rel == "/" || rel.is_empty() => {
            return Err((
                "it copies a stage's whole filesystem: a VM can't be two systems".to_owned(),
                "copy the folders it needs, or give the machine `os` and `app.steps`",
            ));
        }
        None => absolute("/", rel),
    };
    let archive = add && [".tar", ".tgz", ".tar.gz", ".tar.xz", ".tar.bz2", ".tbz2"].iter().any(|e| rel.ends_with(e));
    let verb = match (archive, into) {
        (true, _) => "isoloom_untar",
        (false, true) => "isoloom_copy_into",
        (false, false) => "isoloom_copy_to",
    };
    Ok(format!("for s in {}; do {verb} \"$s\" \"$dest\"; done", glob_quote(path.trim_end_matches('/'))))
}

/// `COPY`'s semantics as shell functions: a folder's contents into the destination, a file to
/// it (or into it, when it is a folder), a missing source an error.
const COPY_HELPERS: &str = r#"isoloom_copy_into() { [ -e "$1" ] || { echo "missing: $1" >&2; exit 1; }; mkdir -p "$2"; if [ -d "$1" ]; then cp -a "$1"/. "$2"/; else cp -a "$1" "$2"/; fi; }
isoloom_copy_to() { [ -e "$1" ] || { echo "missing: $1" >&2; exit 1; }; if [ -d "$1" ]; then mkdir -p "$2"; cp -a "$1"/. "$2"/; else mkdir -p "$(dirname "$2")"; cp -a "$1" "$2"; fi; }
isoloom_untar() { mkdir -p "$2"; tar -xf "$1" -C "$2"; }
isoloom_fetch() { command -v curl >/dev/null || { apt-get update && apt-get install -y curl; } || apk add curl || dnf install -y curl; mkdir -p "$(dirname "$2")"; curl -fsSL "$1" -o "$2"; }
"#;

/// The stages of `lines`: each its lines (the first's with the global ARGs before its FROM) and
/// its name.
fn split_stages(lines: &[Line]) -> Vec<(Vec<Line>, Option<String>)> {
    let mut out: Vec<(Vec<Line>, Option<String>)> = Vec::new();
    let mut pending: Vec<Line> = Vec::new();
    for line in lines {
        if let Instruction::From { stage, .. } = &line.instruction {
            if out.is_empty() {
                pending.push(line.clone());
                out.push((std::mem::take(&mut pending), stage.clone()));
            } else {
                out.push((vec![line.clone()], stage.clone()));
            }
            continue;
        }
        match out.last_mut() {
            Some((stage, _)) => stage.push(line.clone()),
            None => pending.push(line.clone()),
        }
    }
    out
}

/// The index of the stage named `name` (or numbered).
fn stage_index(stages: &[(Vec<Line>, Option<String>)], name: &str) -> Option<usize> {
    let name = name.to_ascii_lowercase();
    stages.iter().position(|(_, n)| n.as_deref() == Some(name.as_str()))
}

/// The stages `last` needs: its `FROM` chain and the stages it copies from, transitively.
fn needed(stages: &[(Vec<Line>, Option<String>)], last: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut todo = vec![last];
    while let Some(i) = todo.pop() {
        if out.contains(&i) {
            continue;
        }
        out.push(i);
        let Some((lines, _)) = stages.get(i) else { continue };
        for line in lines {
            let named = match &line.instruction {
                Instruction::From { image, .. } => Some(image.clone()),
                Instruction::Copy(Copy { source: Source::From(s), .. }) | Instruction::Add(Copy { source: Source::From(s), .. }) => Some(s.clone()),
                _ => None,
            };
            if let Some(j) = named.and_then(|n| stage_index(stages, &n).or_else(|| n.parse().ok())) {
                todo.push(j);
            }
        }
    }
    out.sort_unstable();
    out
}

/// `text` with `$NAME`, `${NAME}`, `${NAME:-default}` and `${NAME:+alt}` filled in from `vars`
/// (unset: empty), `\$` kept literal: Docker's substitution in instructions other than `RUN`.
pub(super) fn substitute(text: &str, vars: &[(String, String)]) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek().copied()) {
            ('\\', Some('$')) => {
                out.push('$');
                chars.next();
            }
            ('$', Some('{')) => {
                chars.next();
                let inner: String = chars.by_ref().take_while(|ch| *ch != '}').collect();
                out.push_str(&braced(&inner, vars));
            }
            ('$', Some(n)) if n.is_ascii_alphabetic() || n == '_' => {
                let mut name = String::new();
                while let Some(ch) = chars.peek().copied().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_') {
                    name.push(ch);
                    chars.next();
                }
                out.push_str(&value(&name, vars).unwrap_or_default());
            }
            (c, _) => out.push(c),
        }
    }
    out
}

/// The value of `name` in `vars` (the last one set).
fn value(name: &str, vars: &[(String, String)]) -> Option<String> {
    vars.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}

/// `${inner}` filled in: `NAME`, `NAME:-default` or `NAME:+alt`.
fn braced(inner: &str, vars: &[(String, String)]) -> String {
    let (name, op, word) = match inner.find(":-").or_else(|| inner.find(":+")) {
        Some(at) => (
            inner.get(..at).unwrap_or_default(),
            inner.get(at..at + 2).unwrap_or_default(),
            inner.get(at + 2..).unwrap_or_default(),
        ),
        None => (inner, "", ""),
    };
    let set = value(name, vars).filter(|v| !v.is_empty());
    match (op, set) {
        (":-", None) | (":+", Some(_)) => word.to_owned(),
        (":+", None) => String::new(),
        (_, v) => v.unwrap_or_default(),
    }
}

/// `path` against folder `base` (`.` is `base`), absolute.
fn absolute(base: &str, path: &str) -> String {
    let joined = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("{}/{path}", base.trim_end_matches('/'))
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    format!("/{}", parts.join("/"))
}

/// The name part of `user[:group]`.
fn user_name(user: &str) -> &str {
    user.split(':').next().unwrap_or(user)
}

/// `s` single-quoted for the shell.
pub(super) fn quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-:=@%+,".contains(c)) {
        return s.to_owned();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A path quoted except its glob characters, which the shell expands.
fn glob_quote(path: &str) -> String {
    if !path.contains(['*', '?', '[']) {
        return quote(path);
    }
    let mut out = String::new();
    let mut plain = String::new();
    for c in path.chars() {
        if "*?[]".contains(c) {
            if !plain.is_empty() {
                out.push_str(&quote(&std::mem::take(&mut plain)));
            }
            out.push(c);
        } else {
            plain.push(c);
        }
    }
    if !plain.is_empty() {
        out.push_str(&quote(&plain));
    }
    out
}

/// `argv` as one shell command line.
pub(super) fn argv(a: &[String]) -> String {
    a.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    #[test]
    fn substitution_follows_dockers_forms() {
        let v = vars(&[("A", "1"), ("EMPTY", "")]);
        assert_eq!(substitute("$A-${A}-${B:-d}-${A:+x}-${B:+x}-\\$A", &v), "1-1-d-x--$A");
        assert_eq!(substitute("${EMPTY:-d}", &v), "d");
    }

    #[test]
    fn paths_resolve_against_the_workdir() {
        assert_eq!(absolute("/var/www/html", "."), "/var/www/html");
        assert_eq!(absolute("/app", "config/x.php"), "/app/config/x.php");
        assert_eq!(absolute("/app", "/etc/../srv/"), "/srv");
    }

    #[test]
    fn only_the_last_stage_and_what_it_needs_run() {
        let lines = instruction::read("FROM debian:12 AS a\nRUN a\nFROM debian:12 AS unused\nRUN b\nFROM a AS c\nCOPY --from=0 /x /y\n").unwrap();
        let stages = split_stages(&lines);
        assert_eq!(needed(&stages, 2), [0, 2]);
    }

    #[test]
    fn globs_stay_globs() {
        assert_eq!(glob_quote("/opt/isoloom/app/*.py"), "/opt/isoloom/app/*.py");
        assert_eq!(glob_quote("/opt/isoloom/my app/*.py"), "'/opt/isoloom/my app/'*.py");
    }
}
