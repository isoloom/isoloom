//! The one-shot `setup` jobs of the containers, and how a Compose project is started around
//! them.

use isoloom_spec::model::{Machine, Spec};

/// The one-shot jobs of a machine, in order: `<machine>-init-<n>`.
#[must_use]
pub fn init_names(name: &str, m: &Machine) -> Vec<String> {
    let n = m.docker.as_ref().map_or(0, |d| d.init.len());
    (1..=n).map(|i| format!("{name}-init-{i}")).collect()
}

/// The one-shot jobs no running service waits for: those of machines nothing depends on.
/// Compose's `up --wait` fails when such a job exits, even with 0, so starters bring the rest
/// up with `--wait` first, then run these attached, in order (see [`start_commands`]).
#[must_use]
pub fn leaf_jobs(spec: &Spec) -> Vec<String> {
    let waited_for = |name: &str| spec.machines.values().any(|o| o.docker.is_some() && o.depends_on.iter().any(|d| d == name));
    spec.machines
        .iter()
        .filter(|(name, m)| m.docker.is_some() && !waited_for(name))
        .flat_map(|(name, m)| init_names(name, m))
        .collect()
}

/// The shell command that starts a Compose project with `compose` (e.g. `docker compose -f x`):
/// plain `up -d --build --wait` when no job is left over, else everything but the jobs up and
/// waited for, then each job attached. `jobs` are the leaf jobs ([`leaf_jobs`]); the services
/// to wait for are listed at run time.
#[must_use]
pub fn start_commands(compose: &str, jobs: &[String], wait_timeout: Option<u32>) -> String {
    let timeout = wait_timeout.map(|t| format!(" --wait-timeout {t}")).unwrap_or_default();
    if jobs.is_empty() {
        return format!("{compose} up -d --build --wait{timeout}");
    }
    let exclude: String = jobs.iter().map(|j| format!(" -e {j}")).collect::<Vec<_>>().concat();
    let attached: String = jobs
        .iter()
        .map(|j| format!(" && {compose} up --no-deps --exit-code-from {j} {j}"))
        .collect::<Vec<_>>()
        .concat();
    format!("{compose} up -d --build --wait{timeout} $({compose} config --services | grep -vx{exclude}){attached}")
}
