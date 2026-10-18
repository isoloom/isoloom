//! The files dev-link writes in a consumer: a `.cargo/config.toml` per linked workspace (with a
//! lockfile of its own beside it), listed in the consumer's `.git/info/exclude` between two
//! marker lines.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::GIT_URL;
use crate::{Res, process, repo};

/// The first line of every config dev-link writes.
const MARKER: &str = "# Written by isoloom `cargo xtask dev-link`.";
/// The first line of the configs the shell version wrote, still recognised.
const OLD_MARKER: &str = "# Written by isoloom scripts/dev-link.sh.";
/// The exclude block's first line, and the prefix that recognises an older one.
const BLOCK_BEGIN: &str = "# >>> isoloom dev-link (cargo xtask dev-link)";
const BLOCK_PREFIX: &str = "# >>> isoloom dev-link";
const BLOCK_END: &str = "# <<< isoloom dev-link";
/// The folder beside a config that holds its lockfile.
pub(crate) const ALT_DIR: &str = "isoloom-dev-link";

/// The config's path in a consumer for a workspace configured at `at`.
pub(super) fn config_path(at: &str) -> String {
    if at == "." {
        ".cargo/config.toml".into()
    } else {
        format!("{at}/.cargo/config.toml")
    }
}

fn written_by_us(text: &str) -> bool {
    text.lines().any(|l| l == MARKER || l == OLD_MARKER)
}

/// The Isoloom checkout a config points at (its `# isoloom:` line).
pub(crate) fn linked_isoloom(config: &str) -> Option<String> {
    config.lines().find_map(|l| l.strip_prefix("# isoloom: ")).map(str::to_owned)
}

fn exclude_file(c: &Path) -> Res<PathBuf> {
    let path = process::output(
        Command::new("git")
            .arg("-C")
            .arg(c)
            .args(["rev-parse", "--path-format=absolute", "--git-path", "info/exclude"]),
    )?;
    Ok(PathBuf::from(path.trim()))
}

/// The lines inside the dev-link block of an exclude file.
fn block_lines(text: &str) -> Vec<String> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with(BLOCK_PREFIX) {
            inside = true;
        } else if line == BLOCK_END {
            inside = false;
        } else if inside {
            out.push(line.to_owned());
        }
    }
    out
}

/// The exclude file's text with its dev-link block replaced by one holding `lines`.
fn with_block(text: &str, lines: &[String]) -> String {
    let mut inside = false;
    let mut out = String::new();
    for line in text.lines() {
        if line.starts_with(BLOCK_PREFIX) {
            inside = true;
        } else if line == BLOCK_END {
            inside = false;
        } else if !inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str(BLOCK_BEGIN);
    out.push('\n');
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(BLOCK_END);
    out.push('\n');
    out
}

/// The paths (relative to consumer `c`, with a leading `/`) in its dev-link block.
fn linked_paths(c: &Path) -> Vec<String> {
    exclude_file(c)
        .ok()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| block_lines(&t))
        .unwrap_or_default()
}

/// The dev-link configs present in checkout `dir` (paths relative to it).
pub(crate) fn linked_configs(dir: &Path) -> Vec<String> {
    linked_paths(dir)
        .iter()
        .filter(|p| p.ends_with("config.toml"))
        .map(|p| p.trim_start_matches('/').to_owned())
        .filter(|p| std::fs::read_to_string(dir.join(p)).is_ok_and(|t| written_by_us(&t)))
        .collect()
}

/// Adds `paths` to consumer `c`'s dev-link block.
pub(super) fn add_excludes(c: &Path, paths: &[String]) -> Res<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let file = exclude_file(c)?;
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let mut lines = block_lines(&text);
    lines.extend(paths.iter().cloned());
    lines.sort();
    lines.dedup();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&file, with_block(&text, &lines))?;
    Ok(())
}

/// Fails when `rel` in consumer `c` is a file dev-link must not touch: committed, or written by
/// someone else.
pub(super) fn guard(c: &Path, rel: &str) -> Res<()> {
    if process::quiet(Command::new("git").arg("-C").arg(c).args(["ls-files", "--error-unmatch", "--", rel])).is_ok() {
        return Err(format!("{rel} is committed in {}: add the [patch] there by hand", c.display()).into());
    }
    let path = c.join(rel);
    if path.is_file() && !std::fs::read_to_string(&path).is_ok_and(|t| written_by_us(&t)) {
        return Err(format!("{} exists and dev-link did not write it: move it away first", path.display()).into());
    }
    Ok(())
}

/// Removes what dev-link wrote in consumer `c`; the exclude entries stay.
pub(super) fn unlink(c: &Path) -> Res<()> {
    for path in linked_paths(c) {
        let full = c.join(path.trim_start_matches('/'));
        if path.ends_with("/config.toml") {
            if std::fs::read_to_string(&full).is_ok_and(|t| written_by_us(&t)) {
                std::fs::remove_file(&full)?;
            }
        } else if path.trim_end_matches('/').ends_with(ALT_DIR) && full.is_dir() {
            std::fs::remove_dir_all(&full)?;
        }
        // The folder goes once empty; one that still holds files stays (an error left unread).
        if let Some(parent) = full.parent() {
            std::fs::remove_dir(parent).ok();
        }
    }
    Ok(())
}

/// One workspace's link: where its config goes and the crates it patches.
pub(super) struct Linked<'a> {
    /// This Isoloom checkout.
    pub root: &'a Path,
    /// Where the config goes, relative to the consumer (`.` for its root).
    pub at: &'a str,
    /// The workspace, relative to the consumer.
    pub workspace: &'a str,
    /// Each crate patched, with its folder here.
    pub patches: &'a [(String, PathBuf)],
}

/// The config's text; `base` is the folder it goes in.
fn config_text(l: &Linked, base: &Path) -> String {
    let patches: Vec<String> = l
        .patches
        .iter()
        .map(|(name, dir)| format!("{name} = {{ path = \"{}\" }}\n", repo::slashed(dir)))
        .collect();
    format!(
        "{MARKER}\n# Local only (in .git/info/exclude); `cargo xtask dev-link unlink` removes it.\n# isoloom: {}\n# workspace: {}\n\n\
         # The Isoloom crates this workspace pins by git come from the local checkout instead.\n[patch.\"{GIT_URL}\"]\n{}\n\
         # The linked resolution has its own lockfile, so the committed Cargo.lock keeps the git pin.\n[resolver]\n\
         lockfile-path = \"{}/.cargo/{ALT_DIR}/Cargo.lock\"\n",
        repo::slashed(l.root),
        l.workspace,
        patches.concat(),
        repo::slashed(base)
    )
}

/// Writes workspace `l`'s config and its lockfile in consumer `c`; the paths to exclude.
pub(super) fn write(c: &Path, l: &Linked) -> Res<Vec<String>> {
    let base = if l.at == "." { c.to_path_buf() } else { c.join(l.at) };
    std::fs::create_dir_all(base.join(".cargo").join(ALT_DIR))?;
    std::fs::copy(c.join(l.workspace).join("Cargo.lock"), base.join(".cargo").join(ALT_DIR).join("Cargo.lock"))?;
    std::fs::write(base.join(".cargo/config.toml"), config_text(l, &base))?;
    let rel = if l.at == "." { "/.cargo".to_owned() } else { format!("/{}/.cargo", l.at) };
    Ok(vec![format!("{rel}/config.toml"), format!("{rel}/{ALT_DIR}/")])
}

/// Each linked config of consumer `c`, with the checkout and crates it points at.
pub(super) fn status(c: &Path) -> Vec<String> {
    let lines: Vec<String> = linked_configs(c)
        .iter()
        .filter_map(|rel| {
            let text = std::fs::read_to_string(c.join(rel)).ok()?;
            let crates: Vec<&str> = text
                .lines()
                .skip_while(|l| !l.starts_with("[patch"))
                .skip(1)
                .take_while(|l| !l.is_empty())
                .filter_map(|l| l.split_whitespace().next())
                .collect();
            Some(format!("{rel}: {} ({})", linked_isoloom(&text).unwrap_or_default(), crates.join(" ")))
        })
        .collect();
    if lines.is_empty() { vec!["git pins (not linked)".into()] } else { lines }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_is_replaced_and_the_rest_kept() {
        let text = "*.log\n# >>> isoloom dev-link (scripts/dev-link.sh)\n/.cargo/config.toml\n# <<< isoloom dev-link\n.env\n";
        assert_eq!(block_lines(text), ["/.cargo/config.toml"]);
        let new = with_block(text, &["/a/.cargo/config.toml".to_owned()]);
        assert_eq!(new, format!("*.log\n.env\n{BLOCK_BEGIN}\n/a/.cargo/config.toml\n{BLOCK_END}\n"));
        assert_eq!(block_lines(&new), ["/a/.cargo/config.toml"]);
    }

    #[test]
    fn the_config_patches_each_crate_and_keeps_its_own_lockfile() {
        let patches = [("isoloom-spec".to_owned(), PathBuf::from("/code/isoloom/crates/core/isoloom-spec"))];
        let l = Linked {
            root: Path::new("/code/isoloom"),
            at: ".",
            workspace: "src-tauri",
            patches: &patches,
        };
        let text = config_text(&l, Path::new("/code/launcher"));
        assert!(text.starts_with(MARKER) && written_by_us(&text));
        assert!(
            text.contains("isoloom-spec = { path = \"/code/isoloom/crates/core/isoloom-spec\" }\n"),
            "{text}"
        );
        assert!(text.contains("lockfile-path = \"/code/launcher/.cargo/isoloom-dev-link/Cargo.lock\""), "{text}");
        assert_eq!(linked_isoloom(&text).as_deref(), Some("/code/isoloom"));
        assert_eq!(config_path("src-tauri"), "src-tauri/.cargo/config.toml");
    }
}
