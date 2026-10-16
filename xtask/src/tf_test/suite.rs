//! The suites: each `.tftest.hcl` under `tests/terraform/`, and the generated module it tests.
//!
//! - `tests/terraform/<target>/<cloud>/<example>.tftest.hcl` tests the example's module for a
//!   target on a cloud (`.isoloom/cloud-vm/aws/`);
//! - `tests/terraform/<target>/<example>.tftest.hcl` tests the example's module for a target
//!   with one module (`.isoloom/proxmox/`);
//! - a first line `# module: <folder>` names another folder of the example instead: the user's
//!   own module on `cloud-services`, which the generated scripts apply.
//!
//! `tests/terraform/mocks/<provider>/` holds the mocked providers' shared values; every suite
//! gets them as `tests/mocks/` beside it (`mock_provider "aws" { source = "./tests/mocks/aws" }`).
//!
//! How a suite is written:
//! - every provider is a `mock_provider` with `override_during = plan`, and every run is
//!   `command = plan`: a planned value is what Terraform would create, and no provisioner runs;
//! - the values a comparison needs that only the cloud would know (ids, public addresses) are
//!   pinned with `override_resource`, so an assertion can say which subnet a machine is on;
//! - a comparison puts the planned value on the left and the wanted one on the right, which the
//!   report shows as `planned:` and `wanted:`; a firewall's rules are compared as a sorted list of
//!   `<protocol> <ports> <source>` strings, all of them, so a rule the spec doesn't call for fails
//!   too;
//! - a run named `known_bug_<what>` says what the spec wants where the module doesn't do it yet:
//!   it must fail (see `report`), with a comment above it saying what the module does instead.

use std::path::{Path, PathBuf};

use crate::Res;

/// Where the suites are, from the repository's root.
pub(crate) const DIR: &str = "tests/terraform";

/// The folder of shared mocks, under [`DIR`].
pub(crate) const MOCKS: &str = "mocks";

/// The examples of the fast subset (`--fast`, what a plain `check-all` runs): one rich example,
/// on every target the suites cover.
const FAST: [&str; 1] = ["segmented"];

/// One test file and the module it runs against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Suite {
    /// The `.tftest.hcl` file.
    pub(crate) file: PathBuf,
    /// The example it tests (a folder of `examples/`).
    pub(crate) example: String,
    /// The target, and the cloud after a `/` when there is one (`cloud-vm/aws`).
    pub(crate) target: String,
    /// The module's folder in the generated tree, `/`-separated
    /// (`segmented/.isoloom/cloud-vm/aws`).
    pub(crate) module: String,
}

/// Every suite under `root`'s [`DIR`], sorted by file; only the fast subset's with `fast`, only
/// those whose file path contains `filter` when there is one.
pub(crate) fn discover(root: &Path, fast: bool, filter: Option<&str>) -> Res<Vec<Suite>> {
    let dir = root.join(DIR);
    let mut files = Vec::new();
    collect(&dir, &mut files)?;
    files.sort();
    let mut suites = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(&dir)?
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let first = std::fs::read_to_string(&file)?.lines().next().unwrap_or_default().to_owned();
        let suite = parse(&rel, &first).map_err(|e| format!("{}: {e}", file.display()))?;
        let rel_text = rel.join("/");
        if (!fast || FAST.contains(&suite.example.as_str())) && filter.is_none_or(|f| rel_text.contains(f)) {
            suites.push(Suite { file, ..suite });
        }
    }
    Ok(suites)
}

/// The `.tftest.hcl` files under `dir`, the mocks' folder left out.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Res<()> {
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if path.parent() != Some(dir) || entry.file_name() != MOCKS {
                collect(&path, out)?;
            }
        } else if path.to_string_lossy().ends_with(".tftest.hcl") {
            out.push(path);
        }
    }
    Ok(())
}

/// The suite a test file is, from its path under [`DIR`] (`["cloud-vm", "aws",
/// "segmented.tftest.hcl"]`) and its first line (a `# module: <folder>` there names the module).
/// Its `file` is left empty.
pub(crate) fn parse(rel: &[String], first_line: &str) -> Result<Suite, String> {
    let (name, parents) = rel.split_last().ok_or("an empty path")?;
    let example = name.strip_suffix(".tftest.hcl").filter(|e| !e.is_empty()).ok_or("not a .tftest.hcl file")?;
    if parents.is_empty() || parents.len() > 2 {
        return Err(format!("expected {DIR}/<target>[/<cloud>]/<example>.tftest.hcl"));
    }
    let target = parents.join("/");
    let module = match first_line.trim().strip_prefix("# module:").map(str::trim) {
        Some(folder) if !folder.is_empty() && !folder.split('/').any(|p| p == ".." || p.is_empty()) => format!("{example}/{folder}"),
        Some(folder) => return Err(format!("`# module: {folder}` must name a folder inside the example")),
        None => format!("{example}/.isoloom/{target}"),
    };
    Ok(Suite {
        file: PathBuf::new(),
        example: example.to_owned(),
        target,
        module,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(path: &str) -> Vec<String> {
        path.split('/').map(str::to_owned).collect()
    }

    #[test]
    fn a_cloud_suite_tests_the_example_s_module_on_that_cloud() {
        let s = parse(&rel("cloud-vm/aws/segmented.tftest.hcl"), "# Routing.").unwrap();
        assert_eq!((s.example.as_str(), s.target.as_str()), ("segmented", "cloud-vm/aws"));
        assert_eq!(s.module, "segmented/.isoloom/cloud-vm/aws");
    }

    #[test]
    fn a_single_module_target_has_no_cloud() {
        let s = parse(&rel("proxmox/segmented.tftest.hcl"), "").unwrap();
        assert_eq!((s.target.as_str(), s.module.as_str()), ("proxmox", "segmented/.isoloom/proxmox"));
    }

    #[test]
    fn a_module_line_names_another_folder_of_the_example() {
        let s = parse(&rel("cloud-services/cloud-bucket.tftest.hcl"), "# module: terraform").unwrap();
        assert_eq!(s.module, "cloud-bucket/terraform");
        for outside in ["# module: ../x", "# module: a//b", "# module:"] {
            assert!(parse(&rel("cloud-services/cloud-bucket.tftest.hcl"), outside).is_err(), "{outside}");
        }
    }

    #[test]
    fn a_file_outside_the_layout_is_refused() {
        assert!(parse(&rel("segmented.tftest.hcl"), "").is_err(), "no target");
        assert!(parse(&rel("a/b/c/segmented.tftest.hcl"), "").is_err(), "too deep");
        assert!(parse(&rel("proxmox/notes.txt"), "").is_err(), "not a test file");
    }
}
