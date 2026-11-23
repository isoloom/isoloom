//! The one-shot `setup` jobs of the containers.

use isoloom_spec::model::Machine;

/// The one-shot jobs of a machine, in order: `isoloom-init-<machine>-<n>`, under the prefix a
/// environment's machines can't take, so no machine's name is another's job.
#[must_use]
pub fn init_names(name: &str, m: &Machine) -> Vec<String> {
    let n = m.docker.as_ref().map_or(0, |d| d.init.len());
    (1..=n).map(|i| format!("isoloom-init-{name}-{i}")).collect()
}
