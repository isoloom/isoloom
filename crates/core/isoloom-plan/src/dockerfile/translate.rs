//! A Dockerfile run on one VM, instruction by instruction: each `RUN`, `COPY`, `ADD`, `WORKDIR`
//! and `VOLUME` becomes a shell step run once, as root (as the `USER` in effect through `su`),
//! with the `ENV` and `ARG` values in effect exported and the `WORKDIR` its folder. `FROM` starts
//! from the service matrix's entry, a distro's bare OS, an earlier stage of the same file (its
//! steps already ran on this VM), or a fetched Dockerfile translated first, the same way. The
//! stages the last one needs run in file order on the same VM, so `COPY --from=<stage>` copies
//! within it; every stage must therefore be one OS. What the last stage leaves (environment,
//! folder, user, entrypoint and command) is the image the VM's service runs.

use std::collections::BTreeMap;

use isoloom_catalogue::{Catalogue, Version, pick};

use super::Untranslatable;
use super::instruction::{self, Command, Instruction, Line};
use super::matrix::{self, EntryCall, NoEntry};
use super::sources::Sources;

mod copy;
mod stages;
mod words;

use self::copy::{copied_entry, copy};
use self::stages::{needed, split_stages, stage_index};
use self::words::{absolute, user_name};
pub(super) use self::words::{argv, quote, substitute};

/// The PATH an image has unless it sets its own.
const DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// The folder of the commands a translated step finds first (the account shims the first step
/// writes).
pub(super) const SHIMS: &str = "/usr/local/lib/isoloom/shims";

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
    /// The step ([`Op::Entry`]) of the catalogue entry it is, when it is one: the last stage's
    /// entry takes its settings from the image's environment.
    pub entry: Option<usize>,
}

/// One step of the translation, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Op {
    /// A shell step.
    Shell(String),
    /// A catalogue entry's recipe, resolved once the image's environment is known.
    Entry(EntryOp),
}

/// A catalogue entry a stage starts from: its release and the VM's OS, picked at its `FROM`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EntryOp {
    /// The entry and what fills its settings.
    pub call: EntryCall,
    /// The release picked.
    pub release: String,
    /// The OS it installs on.
    pub os: String,
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
            entry: None,
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
    /// The steps, in order.
    pub steps: Vec<Op>,
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
            Instruction::Copy(_) | Instruction::Add(_) => self.copy(f, i, st, line)?,
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

    /// The `COPY`/`ADD` at `line` of stage `i`; out of an image the catalogue has an entry for, the
    /// entry first, installing the files where the image has them on the VM itself.
    fn copy(&mut self, f: &File<'_>, i: usize, st: &Stage, line: &Line) -> Result<(), Untranslatable> {
        let (Instruction::Copy(c) | Instruction::Add(c)) = &line.instruction else {
            return Ok(());
        };
        let add = matches!(line.instruction, Instruction::Add(_));
        if let Some(call) = copied_entry(f, c, st) {
            let op = release(&call, Some(&st.img.os)).map_err(|(reason, fix)| f.at(line, reason, &fix))?;
            self.steps.push(Op::Entry(op));
        }
        let body = copy(f, i, c, add, st).map_err(|(reason, fix)| f.at(line, reason, fix))?;
        self.push(st, &body, true);
        Ok(())
    }

    /// The image `image_ref` (a `FROM` at `line` of `f`) as a VM: the matrix's entry, else a
    /// fetched Dockerfile translated first.
    fn base(&mut self, f: &File<'_>, line: &Line, image_ref: &str) -> Result<Image, Untranslatable> {
        match matrix::of(image_ref) {
            Ok(mut base) => {
                let entry = match base.entry.take() {
                    Some(call) => {
                        let op = release(&call, self.wanted).map_err(|(reason, fix)| f.at(line, reason, &fix))?;
                        base.os.clone_from(&op.os);
                        self.steps.push(Op::Entry(op));
                        Some(self.steps.len() - 1)
                    }
                    None => None,
                };
                // The base's install runs strict: a failed download or package stops the VM's set-up.
                self.steps.extend(base.prelude.iter().map(|p| Op::Shell(format!("set -eu\n{p}"))));
                Ok(Image { entry, ..Image::of(base) })
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
        let script = format!(
            "set -e\n{exports}export PATH=\"{SHIMS}:$PATH\"\nmkdir -p {0} && cd {0}\n{body}\n",
            quote(&st.img.workdir)
        );
        let step = match (&st.img.user, as_root) {
            (Some(user), false) => format!(
                "set -e\nu={}\nsu -s /bin/sh \"$(getent passwd \"$u\" | cut -d: -f1)\" -c {}\n",
                quote(user_name(user)),
                quote(&script)
            ),
            _ => script,
        };
        self.steps.push(Op::Shell(step));
    }
}

/// The release of `call`'s entry a VM installs (the newest its version picks among the releases
/// with a VM form, not only an official image) and the OS: `wanted` when the release runs on it,
/// else its first. Why there is none, and the fix, when there is none.
fn release(call: &EntryCall, wanted: Option<&str>) -> Result<EntryOp, (String, String)> {
    let catalogue = Catalogue::embedded().map_err(|e| (format!("Isoloom's catalogue doesn't load: {e}"), "report this to Isoloom".to_owned()))?;
    let entry = catalogue
        .entry(call.entry)
        .ok_or_else(|| (format!("the catalogue has no `{}`", call.entry), "report this to Isoloom".to_owned()))?;
    let def = entry.definition();
    let built = |os: &isoloom_spec::syntax::OsRef| !def.images.contains_key(os);
    let releases: Vec<_> = def.releases.iter().filter(|r| r.os.iter().any(built)).collect();
    let versions: Vec<&Version> = releases.iter().map(|r| &r.version).collect();
    let asked: Option<Version> = call.version.as_deref().and_then(|v| v.parse().ok());
    let picked = match &asked {
        Some(v) => pick(versions.iter().copied(), v),
        None => versions.iter().copied().max(),
    };
    let listed = versions.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
    let none = || {
        (
            format!(
                "the catalogue's `{}` has no release {} with a VM form (it has {listed})",
                call.entry,
                call.version.as_deref().unwrap_or("at all")
            ),
            "build FROM a release it has, or give the machine `os` and `app.steps`".to_owned(),
        )
    };
    let picked = picked.ok_or_else(none)?;
    let r = releases.iter().find(|r| &r.version == picked).ok_or_else(none)?;
    let os =
        r.os.iter()
            .find(|o| built(o) && wanted.is_some_and(|w| o.to_string() == w))
            .or_else(|| r.os.iter().find(|o| built(o)))
            .ok_or_else(none)?;
    Ok(EntryOp {
        call: call.clone(),
        release: picked.to_string(),
        os: os.to_string(),
    })
}
