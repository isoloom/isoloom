//! A `COPY` or `ADD` as shell: from the project's copy of the build context, an earlier stage
//! on the same VM, an image a catalogue entry installed, or a URL.

use super::super::instruction::{Copy, Source};
use super::super::matrix::{self, EntryCall};
use super::stages::stage_index;
use super::words::{absolute, glob_quote, quote, substitute};
use super::{File, Stage};

/// The catalogue entry `c` copies out of: its `--from` names an image (not a stage) the matrix
/// knows files of ([`matrix::copied`]).
pub(super) fn copied_entry(f: &File<'_>, c: &Copy, st: &Stage) -> Option<EntryCall> {
    match &c.source {
        Source::From(name) if stage_index(&f.stages, name).is_none() && name.parse::<usize>().is_err() => matrix::copied(&substitute(name, &st.vars())),
        _ => None,
    }
}

/// A `COPY`/`ADD`'s shell (stage `i` of `f`), or why it has none and the fix. Out of an image a
/// catalogue entry installed ([`copied_entry`]), its files are on the VM itself.
pub(super) fn copy(f: &File<'_>, i: usize, c: &Copy, add: bool, st: &Stage) -> Result<String, (String, &'static str)> {
    let installed = copied_entry(f, c, st).is_some();
    let vars = st.vars();
    let dest = substitute(&c.dest, &vars);
    let into = dest.ends_with('/') || dest == "." || c.sources.len() > 1;
    let dest = absolute(&st.img.workdir, &dest);
    let root = match &c.source {
        Source::Context => Some(format!("/opt/isoloom/{}", f.df.context.trim_end_matches('/'))),
        Source::From(_) if installed => Some(String::new()),
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
        // A build context's path stays in the context: Docker reads `/` as the context itself.
        Some(_) if rel.contains('\\') || rel.split('/').any(|part| part == "..") => {
            return Err((
                format!("`{src}` goes out of the build context"),
                "copy files from inside the build context, with `/` between folders",
            ));
        }
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
const COPY_HELPERS: &str = r#"isoloom_copy_into() { [ -e "$1" ] || { echo "missing: $1" >&2; exit 1; }; [ "$1" -ef "$2/${1##*/}" ] && return 0; mkdir -p "$2"; if [ -d "$1" ]; then cp -a "$1"/. "$2"/; else cp -a "$1" "$2"/; fi; }
isoloom_copy_to() { [ -e "$1" ] || { echo "missing: $1" >&2; exit 1; }; [ "$1" -ef "$2" ] && return 0; if [ -d "$1" ]; then mkdir -p "$2"; cp -a "$1"/. "$2"/; else mkdir -p "$(dirname "$2")"; cp -a "$1" "$2"; fi; }
isoloom_untar() { mkdir -p "$2"; tar -xf "$1" -C "$2"; }
isoloom_fetch() { command -v curl >/dev/null || { apt-get update && apt-get install -y curl; } || apk add curl || dnf install -y curl; mkdir -p "$(dirname "$2")"; curl -fsSL "$1" -o "$2"; }
"#;
