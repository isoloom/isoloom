//! The project copied into the machines, without what `.isoloomignore` leaves out: its rules
//! ([`ignore`], the run manifest's: the runner's `project.archive` follows them too), and the
//! same rules in the Terraform outputs, read when Terraform runs, exactly as the Vagrantfiles read
//! them (`vagrant_project.rb`), so every target leaves out the same files.
//!
//! - Proxmox writes the project's files through cloud-init: `local.project_paths`, the files
//!   that go (not version control, the tools' state, or Isoloom's outputs).
//! - The cloud modules archive the project on the host with `tar`, which walks it itself
//!   (symbolic links kept as links): `local.project_ignored`, the files that don't go, is its
//!   exclusion list (`-X`, a file next to the module: `local_file.isoloom_project`). Each entry
//!   starts with the project folder's name (the archive's top folder, stripped when unpacked),
//!   since bsdtar (macOS, Windows) matches exclusions at any depth.
//! - A module that only makes the machines lists nothing: the runner archives the project and
//!   pipes it into each guest over SSH (`Copy::Runner`).

pub use isoloom_manifest::ignore;

/// How a Terraform module copies the project into its machines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Copy {
    /// Each file written into the machine by its cloud-init (Proxmox): the files that go.
    Files,
    /// An archive made where Terraform runs, with `tar`: its exclusion list is a file too.
    Archive,
    /// None: Terraform only makes the machines, and the runner copies the project in over SSH
    /// (the run manifest's `project.archive`, piped to the guest), so the module lists nothing.
    Runner,
}

/// The rules and both lists. `SKIP` is a regular expression of the paths Proxmox leaves out.
const LISTS: &str = r##"
# The project's files the machines don't get: version control and the tools' state, and what
# .isoloomignore at the project's root lists (gitignore-style: one pattern per line, `#` comments,
# `!` brings a path back, a trailing `/` matches folders only, a `/` at the start or in the middle
# anchors the pattern at the root; `*`, `?`, `[abc]`, `**`; the last pattern matching a path, or
# a folder it is in, decides). Read as regular expressions, as the Vagrantfiles do.
locals {
  project_ignore_lines = [for l in split("\n", replace(try(file("${local.root}/.isoloomignore"), ""), "\r", "")) : trimspace(l)]
  project_ignore_pats  = [for l in local.project_ignore_lines : { keep = startswith(l, "!"), pat = trimprefix(l, "!") } if l != "" && !startswith(l, "#")]
  project_ignore_globs = [for r in local.project_ignore_pats : merge(r, { glob = trimsuffix(trimsuffix(trimsuffix(r.pat, "/"), "/**"), "/") })]
  project_ignore = [for r in local.project_ignore_globs : {
    keep = r.keep
    re = format(r.glob == r.pat ? "^%s(/|$)" : "^%s/", replace(replace(replace(replace(replace(replace(replace(replace(
      length(split("/", r.glob)) > 1 ? trimprefix(r.glob, "/") : "**/${r.glob}",
    "/[.+^$(){}|\\\\]/", "\\$0"), "[!", "[^"), "**/", "\u0001"), "**", "\u0002"), "*", "[^/]*"), "?", "[^/]"), "\u0001", "(.*/)?"), "\u0002", ".*"))
  } if r.glob != ""]
  # Each file: whether it goes (the last matching pattern decides; none: it goes).
  project_keep    = { for f in fileset(local.root, "**") : f => reverse(concat([true], [for r in local.project_ignore : r.keep if length(regexall(r.re, f)) > 0]))[0] }
  project_paths   = [for f, keep in local.project_keep : f if keep && length(regexall("SKIP", f)) == 0]
  project_ignored = [for f, keep in local.project_keep : f if !keep]
}
"##;

const EXCLUSIONS: &str = r#"
# What `tar` leaves out of the project's archive: .isoloomignore's files, from the archive's top
# folder, their wildcard characters escaped.
resource "local_file" "isoloom_project" {
  filename = "${path.module}/.isoloom-project-ignored.txt"
  content  = join("", [for f in local.project_ignored : "${replace("${basename(local.root)}/${f}", "/[\\\\*?\\[]/", "\\$0")}\n"])
}
"#;

/// Proxmox leaves out version control and the tools' state at any depth (a `.vagrant` holds
/// private keys), and Isoloom's outputs.
const SKIP: &str = r"^\\.isoloom|(^|/)\\.(git|vagrant|terraform)/";

/// The lists as a fixed HCL fragment, for a module that copies the project as `copy` says: an
/// archive also gets `tar`'s exclusion file (the cloud modules), the files alone the lists only
/// (Proxmox).
#[must_use]
pub fn lists(copy: Copy) -> String {
    let lists = LISTS.replace("SKIP", SKIP);
    match copy {
        Copy::Files => lists,
        Copy::Archive => lists + EXCLUSIONS,
        Copy::Runner => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Copy, lists};

    #[test]
    fn an_archive_also_lists_what_tar_leaves_out() {
        let files = lists(Copy::Files);
        assert!(files.contains("project_paths   = [for f, keep in local.project_keep : f if keep && length(regexall(\"^\\\\.isoloom|(^|/)\\\\.(git|vagrant|terraform)/\", f)) == 0]"), "{files}");
        assert!(!files.contains("local_file"), "{files}");
        let archive = lists(Copy::Archive);
        assert!(
            archive.starts_with(&files) && archive.contains("resource \"local_file\" \"isoloom_project\""),
            "{archive}"
        );
        // The runner's copy needs nothing of the module.
        assert_eq!(lists(Copy::Runner), "");
    }
}
