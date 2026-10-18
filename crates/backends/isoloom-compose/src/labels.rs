//! What Isoloom marks an environment's Docker resources with: whose they are (so leftovers can
//! be told apart and collected without guessing), which volumes `down` keeps, and what each
//! machine serves.

use isoloom_plan::Environment;
use isoloom_spec::model;

use crate::file::ComposeFile;

/// On everything an environment creates on Docker: `isoloom gc` collects only what has it.
pub const MANAGED_LABEL: &str = "isoloom.managed";
/// The environment a Docker resource belongs to.
pub const ENVIRONMENT_LABEL: &str = "isoloom.environment";
/// On a volume `isoloom down` keeps (`down --volumes` removes it too).
pub(crate) const PERSISTENT_LABEL: &str = "isoloom.persistent";

/// Labels everything the environment creates (containers, built images, networks, volumes) as
/// Isoloom's and the environment's; an existing network or volume isn't the environment's and
/// stays unlabelled.
pub(crate) fn mark(env: &Environment, file: &mut ComposeFile) {
    let ours = [(MANAGED_LABEL.to_owned(), "true".to_owned()), (ENVIRONMENT_LABEL.to_owned(), env.name.clone())];
    for svc in file.services.values_mut() {
        svc.labels.extend(ours.clone());
        if let Some(b) = &mut svc.build {
            b.labels.extend(ours.clone());
        }
    }
    for n in file.networks.values_mut() {
        n.labels.extend(ours.clone());
    }
    for v in file.volumes.values_mut().filter(|v| !v.external) {
        v.labels.extend(ours.clone());
    }
}

/// What a machine serves, for tools reading the running containers (dashboards, runners):
/// `isoloom.port.<name>: "<http|tcp|udp>:<port>"` (an unnamed UDP port's key is `<port>-udp`,
/// so a port served over both keeps both).
pub(crate) fn ports(m: &model::Machine) -> impl Iterator<Item = (String, String)> + '_ {
    m.ports.iter().map(|p| {
        let kind = if p.http { "http" } else { p.protocol.id() };
        (format!("isoloom.port.{}", m.port_label(p)), format!("{kind}:{}", p.port))
    })
}
