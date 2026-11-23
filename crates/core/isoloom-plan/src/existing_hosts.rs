//! What hosts that already exist (the `external` target's) can't do for a check: run a project
//! script, which they have no copy of, or run a check with none of them to run it from.

use std::fmt;

use isoloom_spec::model::Spec;

use crate::checks::{self, Position, Probe};
use crate::environment::MachineShape;
use crate::requirements::Requirement;

/// What the checks ask of hosts that already exist, given each machine's shape on the target: a
/// project script run on one (it has no copy of the project), and a check with none of them to
/// run from (no `from`, no access machine). At most one, the first check that asks; only when
/// every machine is an existing host, as on `external`.
pub(crate) fn check_requirement(spec: &Spec, shape: impl Fn(&str) -> Option<MachineShape>) -> Option<Requirement> {
    if spec.machines.is_empty() || !spec.machines.keys().all(|n| shape(n) == Some(MachineShape::External)) {
        return None;
    }
    checks::plan(spec).into_iter().find_map(|c| match (&c.probe, &c.position) {
        // A playbook is refused first, as the checks' own requirement (`PlaybookChecks`).
        (Probe::Playbook { .. }, _) => None,
        (Probe::Script { .. }, _) => Some(Requirement::ExistingHostScriptCheck { check: c.name }),
        (_, Position::Networks) => Some(Requirement::ExistingHostCheckFromNoHost { check: c.name }),
        _ => None,
    })
}

/// Why existing hosts refuse a check that runs a project script.
pub(crate) fn no_script_check(f: &mut fmt::Formatter<'_>, check: &str) -> fmt::Result {
    write!(
        f,
        "check \"{check}\" runs a project script, and an existing host has no copy of the project (Isoloom copies nothing there): probe it with `http`, `tcp` or `exec` instead"
    )
}

/// Why existing hosts refuse a check with none of them to run from.
pub(crate) fn no_host_to_run_from(f: &mut fmt::Formatter<'_>, check: &str) -> fmt::Result {
    write!(
        f,
        "check \"{check}\" has no host to run from: an existing host's checks run on one of the hosts, so give it `from:` or make a machine `access: true`"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    /// What the checks ask of an environment with `machines` and `checks`, each machine an existing host
    /// when `existing`.
    fn existing_host_checks_of(machines: &str, checks: &str, existing: bool) -> Option<Requirement> {
        let spec = crate::testing::spec(&format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n{machines}checks: {checks}\n"
        ));
        let shape = if existing { MachineShape::External } else { MachineShape::Vm };
        super::check_requirement(&spec, |_| Some(shape))
    }

    #[test]
    fn a_check_existing_hosts_cant_run_is_a_requirement_only_there() {
        let a = "  a: { os: debian@12, networks: { lan: 10 } }\n";
        let access = "  a: { os: debian@12, access: true, networks: { lan: 10 } }\n";
        let script = |check: &str| Some(Requirement::ExistingHostScriptCheck { check: check.into() });
        assert_eq!(existing_host_checks_of(access, "[checks/a.sh]", true), script("checks/a.sh"));
        assert_eq!(existing_host_checks_of(a, "[{ name: s, from: a, script: s.sh }]", true), script("s"));
        assert_eq!(
            existing_host_checks_of(a, "[{ name: t, tcp: 'a:22' }]", true),
            Some(Requirement::ExistingHostCheckFromNoHost { check: "t".into() })
        );
        assert_eq!(
            existing_host_checks_of(access, "[{ name: t, tcp: 'a:22' }]", true),
            None,
            "from the access machine"
        );
        assert_eq!(
            existing_host_checks_of(a, "[checks/a.yml]", true),
            None,
            "a playbook is the checks' own requirement"
        );
        assert_eq!(
            existing_host_checks_of(a, "[checks/a.sh, { tcp: 'a:22' }]", false),
            None,
            "machines the target makes"
        );
    }
}
