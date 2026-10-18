//! The daemons a recipe installs and no step starts. A package that ships a daemon starts it on
//! Debian's VMs only (its maintainer scripts do), never in a container or on the other families'
//! VMs, so the same steps would behave differently by form and by family. A `service` step for
//! the daemon (enabled, or `enabled: false`) says where it runs; until one does, the author gets
//! a warning with the fix.

use isoloom_spec::syntax::{self, OsRef, PackageName, Step, UnitName};

use super::daemons::DaemonTable;
use super::family::Family;

/// A daemon a `packages` step installs that no `service` step names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unstarted {
    /// The `packages` step, by its index in `app.steps`.
    pub step: usize,
    /// The package that ships the daemon.
    pub package: PackageName,
    /// The daemon's unit on the machine's family (what a `service` step names).
    pub unit: UnitName,
}

/// The daemons machine `m` (on `os`) installs without a `service` step for them, in step order.
/// Windows features aren't packages that start daemons: none there.
#[must_use]
pub fn unstarted(os: OsRef, m: &syntax::Machine, daemons: &DaemonTable) -> Vec<Unstarted> {
    let family = Family::from(os);
    let Some(app) = m.app().filter(|_| family != Family::Windows) else {
        return Vec::new();
    };
    let named: Vec<&UnitName> = app
        .steps
        .iter()
        .filter_map(|s| match s {
            Step::Service(service) => daemons.find(service.name(), family).map(|(entry, _)| &entry.key),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    for (i, step) in app.steps.iter().enumerate() {
        let Step::Packages(packages) = step else { continue };
        for p in packages {
            if let Some((entry, form)) = daemons.shipped_by(&p.name, family)
                && !named.contains(&&entry.key)
            {
                out.push(Unstarted {
                    step: i,
                    package: p.name.clone(),
                    unit: form.daemon.unit.clone(),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(yaml: &str) -> Vec<(usize, String)> {
        let m: syntax::Machine = serde_yaml_ng::from_str(yaml).unwrap_or_else(|e| panic!("{yaml}: {e}"));
        let os = m.os.expect("an os");
        unstarted(os, &m, &DaemonTable::common())
            .into_iter()
            .map(|u| (u.step, u.unit.to_string()))
            .collect()
    }

    #[test]
    fn a_daemon_package_without_its_service_step_is_found() {
        assert_eq!(
            found("{ os: debian@12, app: { steps: [{ run: a.sh }, { packages: [curl, nginx] }] } }"),
            [(1, "nginx".to_owned())]
        );
        assert_eq!(
            found("{ os: rocky@9, app: { steps: [{ packages: [httpd, openssh-server] }] } }"),
            [(0, "httpd".to_owned()), (0, "sshd".to_owned())]
        );
    }

    #[test]
    fn a_service_step_for_the_daemon_says_where_it_runs() {
        assert_eq!(found("{ os: debian@12, app: { steps: [{ packages: [nginx] }, { service: nginx }] } }"), []);
        assert_eq!(
            found("{ os: debian@12, app: { steps: [{ packages: [nginx] }, { service: { name: nginx, enabled: false } }] } }"),
            []
        );
        // The table's key and the family's unit both name it.
        assert_eq!(
            found("{ os: debian@12, app: { steps: [{ packages: [openssh-server] }, { service: ssh }] } }"),
            []
        );
        assert_eq!(
            found("{ os: rocky@9, app: { steps: [{ packages: [openssh-server] }, { service: sshd }] } }"),
            []
        );
    }

    #[test]
    fn windows_features_start_nothing_to_warn_about() {
        assert_eq!(found("{ os: windows-server@2022, app: { steps: [{ packages: [Web-Server] }] } }"), []);
    }
}
