//! The project copied into the machines, without what `.isoloomignore` leaves out: its rules
//! ([`ignore`]: a runner that copies the project follows them too), and the
//! same rules in the Terraform outputs, read when Terraform runs, exactly as the Vagrantfiles read
//! them (`vagrant_project.rb`), so every target leaves out the same files.
//!
//! Every copy leaves out what [`ignore::copy_rules`] adds after the project's own rules: Isoloom's
//! outputs (a VM reads none of them), version control and the tools' state at any depth.
//!
//! - Proxmox writes the project's files through cloud-init: `local.project_paths`, the files
//!   that go.
//! - The cloud modules archive the project on the host with `tar`, which walks it itself
//!   (symbolic links kept as links): `local.project_ignored`, the files that don't go, is its
//!   exclusion list (`-X`, a file next to the module: `local_file.isoloom_project`). Each entry
//!   starts with the project folder's name (the archive's top folder, stripped when unpacked),
//!   since bsdtar (macOS, Windows) matches exclusions at any depth. What appears once Terraform
//!   is applying (the archive itself, a state, a machine's login) is left out by `tar`'s own
//!   exclusions, of the same rules ([`tar_exclusions`]). A Windows machine there,
//!   which cloudbase-init sets up, gets the files that go one by one: `local.project_paths`.
//!
//! - A module that only makes the machines lists nothing: the runner archives the project and
//!   pipes it into each guest over SSH (`Copy::Runner`).
//!
//! A module gets only the lists it reads ([`lists`]).

pub use isoloom_spec::ignore;

/// How a Terraform module copies the project into its machines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Copy {
    /// Each file written into the machine by its cloud-init (Proxmox): the files that go.
    Files,
    /// An archive made where Terraform runs, with `tar`: its exclusion list is a file too.
    Archive,
    /// None: Terraform only makes the machines, and the runner copies the project in over SSH
    /// (piped to the guest), so the module lists nothing.
    Runner,
}

/// The rules, read as regular expressions, and each file's verdict (`project_keep`): what both
/// lists are made from. The `locals` block stays open for the lists. `COPY` is
/// [`ignore::copy_rules`], read after the project's file.
const RULES: &str = r##"
# The project's files the machines don't get: what .isoloomignore at the project's root lists
# (gitignore-style: one pattern per line, `#` comments, `!` brings a path back, a trailing `/`
# matches folders only, a `/` at the start or in the middle anchors the pattern at the root; `*`,
# `?`, `[abc]`, `**`; the last pattern matching a path, or a folder it is in, decides), then
# Isoloom's outputs, version control and the tools' state. Read as regular expressions, as the
# Vagrantfiles do.
locals {
  project_ignore_lines = [for l in split("\n", "${replace(try(file("${local.root}/.isoloomignore"), ""), "\r", "")}\nCOPY") : trimspace(l)]
  project_ignore_pats  = [for l in local.project_ignore_lines : { keep = startswith(l, "!"), pat = trimprefix(l, "!") } if l != "" && !startswith(l, "#")]
  project_ignore_globs = [for r in local.project_ignore_pats : merge(r, { glob = trimsuffix(trimsuffix(trimsuffix(r.pat, "/"), "/**"), "/") })]
  project_ignore = [for r in local.project_ignore_globs : {
    keep = r.keep
    re = format(r.glob == r.pat ? "^%s(/|$)" : "^%s/", replace(replace(replace(replace(replace(replace(replace(replace(
      length(split("/", r.glob)) > 1 ? trimprefix(r.glob, "/") : "**/${r.glob}",
    "/[.+^$(){}|\\\\]/", "\\$0"), "[!", "[^"), "**/", "\u0001"), "**", "\u0002"), "*", "[^/]*"), "?", "[^/]"), "\u0001", "(.*/)?"), "\u0002", ".*"))
  } if r.glob != ""]
  # Each file: whether it goes (the last matching pattern decides; none: it goes).
"##;

/// The files that go.
const PATHS: (&str, &str) = ("project_paths", "[for f, keep in local.project_keep : f if keep]");

/// The files that don't go.
const IGNORED: (&str, &str) = ("project_ignored", "[for f, keep in local.project_keep : f if !keep]");

/// Each file's verdict.
const KEEP: (&str, &str) = (
    "project_keep",
    r#"{ for f in fileset(local.root, "**") : f => reverse(concat([true], [for r in local.project_ignore : r.keep if length(regexall(r.re, f)) > 0]))[0] }"#,
);

const EXCLUSIONS: &str = r#"
# What `tar` leaves out of the project's archive: .isoloomignore's files, from the archive's top
# folder, their wildcard characters escaped.
resource "local_file" "isoloom_project" {
  filename = "${path.module}/.isoloom-project-ignored.txt"
  content  = join("", [for f in local.project_ignored : "${replace("${basename(local.root)}/${f}", "/[\\\\*?\\[]/", "\\$0")}\n"])
}
"#;

/// One of `tar`'s own exclusions: a pattern of `tar --exclude`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarExclusion {
    /// Under the archive's top folder (the project folder's name).
    UnderTop(&'static str),
    /// At any depth.
    Anywhere(&'static str),
}

/// `tar`'s own exclusions, for what appears in the project once Terraform is applying, after the
/// lists were read: Isoloom's outputs at the top, and what no copy takes
/// ([`ignore::never_copied`]) at any depth.
pub fn tar_exclusions() -> impl Iterator<Item = TarExclusion> {
    let outputs = TarExclusion::UnderTop(ignore::OUTPUTS.trim_start_matches('/'));
    std::iter::once(outputs).chain(ignore::never_copied().map(|p| TarExclusion::Anywhere(p.trim_end_matches('/'))))
}

/// The lists a module reads, as a fixed HCL fragment: `local.project_paths` when the rest of
/// the module (`module`, as written) names it; for an archive, `local.project_ignored` and
/// `tar`'s exclusion file, which reads it. Nothing when it reads neither, or when the runner
/// copies the project: a list nothing reads is a declaration Terraform's linters report.
#[must_use]
pub fn lists(copy: Copy, module: &str) -> String {
    if copy == Copy::Runner {
        return String::new();
    }
    let mut listed = Vec::new();
    if module.contains("local.project_paths") {
        listed.push(PATHS);
    }
    if copy == Copy::Archive {
        listed.push(IGNORED);
    }
    if listed.is_empty() {
        return String::new();
    }
    listed.insert(0, KEEP);
    // `terraform fmt` aligns the `=` of consecutive one-line attributes.
    let width = listed.iter().map(|(name, _)| name.len()).max().unwrap_or_default();
    let lines = listed
        .into_iter()
        .map(|(name, value)| format!("  {name:<width$} = {value}\n"))
        .collect::<Vec<_>>()
        .concat();
    let exclusions = if copy == Copy::Archive { EXCLUSIONS } else { "" };
    let rules = ignore::copy_rules(&[]).replace('\n', "\\n");
    format!("{}{lines}}}\n{exclusions}", RULES.replace("COPY", &rules))
}

#[cfg(test)]
mod tests {
    use isoloom_test_support::{hcl, hcl_attribute, hcl_body, hcl_text};

    use super::{Copy, lists};

    /// A module's body that reads the files that go.
    const READS_PATHS: &str = "content = join(\"\\n\", local.project_paths)";

    /// The locals the fragment `hcl` declares, in order, and the resources it adds.
    fn declared(hcl: &str) -> (Vec<String>, Vec<String>) {
        let body = hcl_body(hcl);
        let locals = body
            .blocks()
            .filter(|b| b.identifier() == "locals")
            .flat_map(|b| b.body().attributes().map(|a| a.key().to_owned()));
        let resources = body
            .blocks()
            .filter(|b| b.identifier() == "resource")
            .map(|b| b.labels().iter().map(hcl::BlockLabel::as_str).collect::<Vec<_>>().join("."));
        (locals.collect(), resources.collect())
    }

    /// The rules' own locals, which every list is made from.
    const RULES: [&str; 4] = ["project_ignore_lines", "project_ignore_pats", "project_ignore_globs", "project_ignore"];

    #[test]
    fn the_files_alone_list_only_the_files_that_go_when_the_module_reads_them() {
        let files = lists(Copy::Files, READS_PATHS);
        let (locals, resources) = declared(&files);
        assert_eq!(locals, [&RULES[..], &["project_keep", "project_paths"]].concat());
        assert_eq!(resources, Vec::<String>::new());
        let body = hcl_body(&files);
        let rules = body.blocks().find(|b| b.identifier() == "locals").expect("the rules' locals");
        let lines = hcl::format::to_string(hcl_attribute(rules.body(), "project_ignore_lines").expect("the rules' lines")).unwrap_or_default();
        let after_the_file = lines.split_once(".isoloomignore").map_or("", |(_, rest)| rest);
        assert!(
            after_the_file.contains("\n/.isoloom*\n.git/\n.terraform/\n*.tfstate\n"),
            "the copy's rules after the file's: {lines}"
        );
    }

    #[test]
    fn a_module_reading_no_list_gets_none() {
        assert_eq!(lists(Copy::Files, "resource \"x\" \"y\" {}"), "");
        // The runner's copy needs nothing of the module.
        assert_eq!(lists(Copy::Runner, READS_PATHS), "");
    }

    #[test]
    fn an_archive_lists_what_tar_leaves_out_and_the_files_that_go_only_when_read() {
        let archive = lists(Copy::Archive, "");
        let (locals, resources) = declared(&archive);
        assert_eq!(locals, [&RULES[..], &["project_keep", "project_ignored"]].concat());
        assert_eq!(resources, ["local_file.isoloom_project"]);
        let exclusions = isoloom_test_support::hcl_resource(&archive, "local_file", "isoloom_project");
        assert!(
            hcl_text(&exclusions, "content").contains("local.project_ignored"),
            "tar's list is the files that don't go: {archive}"
        );
        let (both, _) = declared(&lists(Copy::Archive, READS_PATHS));
        assert_eq!(both, [&RULES[..], &["project_keep", "project_paths", "project_ignored"]].concat());
    }
}
