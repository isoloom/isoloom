//! The project's own files, next to `isoloom.yml` where the spec's paths point: an appliance's
//! configuration (`configs/<machine>.cfg`), and the places the user's own files go (a Dynamips
//! router's IOS image, `images/<machine>.bin`).

use crate::{Kind, Topology};

/// One of the project's own files, next to `isoloom.yml` where its paths point: an appliance's
/// configuration, a starter set-up script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFile {
    /// Its path in the project, relative.
    pub path: String,
    /// What it holds.
    pub contents: String,
}

/// Where a Dynamips router's IOS image goes in the project (the user's to add).
pub(crate) fn firmware_path(machine: &str) -> String {
    format!("images/{machine}.bin")
}

/// Where an appliance's own configuration goes in the project.
pub(crate) fn config_path(machine: &str) -> String {
    format!("configs/{machine}.cfg")
}

/// An appliance's own configuration as drawn, when it has one: a blank one is none, so no file is
/// written for it and its spec names none.
pub(crate) fn own_config(config: Option<&str>) -> Option<&str> {
    config.filter(|c| !c.trim().is_empty())
}

/// The appliances' own configurations, as files of the project, each ending with a line break.
pub(crate) fn config_files(topo: &Topology) -> Vec<ProjectFile> {
    topo.nodes
        .iter()
        .filter_map(|(name, n)| match &n.kind {
            Kind::Container {
                appliance: Some(_), config, ..
            } => own_config(config.as_deref()).map(|c| ProjectFile {
                path: config_path(name),
                contents: if c.ends_with('\n') { c.to_owned() } else { format!("{c}\n") },
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_configuration_is_no_file_and_a_last_line_break_is_added_once() {
        let topo: Topology = serde_json::from_str(
            r#"{"name": "x", "nodes": {
              "r1": {"kind": {"container": {"image": "i", "appliance": "cisco-iol", "config": "  \n"}}},
              "r2": {"kind": {"container": {"image": "i", "appliance": "cisco-iol", "config": "hostname r2"}}},
              "r3": {"kind": {"container": {"image": "i", "appliance": "cisco-iol", "config": "hostname r3\n"}}},
              "pc": {"kind": {"container": {"image": "alpine:3.20"}}}}}"#,
        )
        .unwrap();
        let files: Vec<(String, String)> = config_files(&topo).into_iter().map(|f| (f.path, f.contents)).collect();
        assert_eq!(
            files,
            [
                ("configs/r2.cfg".to_owned(), "hostname r2\n".to_owned()),
                ("configs/r3.cfg".to_owned(), "hostname r3\n".to_owned())
            ]
        );
    }

    #[test]
    fn a_machines_files_are_named_after_it() {
        assert_eq!(config_path("r1"), "configs/r1.cfg");
        assert_eq!(firmware_path("r1"), "images/r1.bin");
    }
}
