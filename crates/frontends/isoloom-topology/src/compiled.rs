//! [`compile`]: the drawing's spec, written and read back by Isoloom, with the project's own files.

use isoloom_spec::Spec;

use crate::document::{ProvisionDoc, SpecDoc};
use crate::draft::Draft;
use crate::{Error, ProjectFile, Result, Topology, cables, lans, machines, project, routers, starters, vlans};

/// A compiled project: the `isoloom.yml` text, the spec Isoloom reads from it, and the project's own files
/// where the spec points (the appliances' configurations, a starter for each provisioning step).
#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    yaml: String,
    spec: Spec,
    files: Vec<ProjectFile>,
}

impl Compiled {
    /// The spec's text (ready to write to `isoloom.yml`), the spec and the project's own files,
    /// next to `isoloom.yml` where its paths point: the appliances' configurations, then a starter
    /// for each provisioning step (each path once, inside the project).
    #[must_use]
    pub fn into_parts(self) -> (String, Spec, Vec<ProjectFile>) {
        (self.yaml, self.spec, self.files)
    }
}

/// Compiles a drawing into an Isoloom spec and the project's own files.
///
/// # Errors
///
/// [`Error`]: what the drawing can't say (a cable to an unknown node, a LAN that's full, two
/// machines asking for one address...). [`Error::Unreadable`] would mean the compiler wrote
/// something Isoloom's format doesn't hold: a bug.
pub fn compile(topo: &Topology) -> Result<Compiled> {
    cables::check_count(topo)?;
    vlans::check(topo)?;
    let mut draft = Draft::default();
    machines::add(topo, &mut draft)?;
    lans::address(topo, &mut draft)?;
    cables::allocate(topo, &mut draft)?;
    routers::add_reach_rules(&mut draft);
    routers::cisco_gateways(topo, &mut draft)?;
    vlans::nest(topo, &mut draft);
    let doc = SpecDoc {
        version: 1,
        name: topo.name.clone(),
        networks: draft.networks,
        reach: draft.reach,
        machines: draft.machines,
        provision: draft.controller.iter().map(ProvisionDoc::from).collect(),
    };
    let yaml = serde_yaml_ng::to_string(&doc).map_err(Error::Yaml)?;
    let spec = isoloom_spec::syntax::parse(&yaml).map_err(Error::Unreadable)?;
    let mut files = project::config_files(topo);
    starters::add_starters(&mut files, &spec, &draft.blocks, &draft.controller);
    Ok(Compiled { yaml, spec, files })
}
