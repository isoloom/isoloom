//! The check runners: one per existing host with probes to run from it. Scripts and playbooks
//! need the project on the machine, which an existing host doesn't have: they are left out of
//! the runners, not refused.

use isoloom_plan::Environment;
use isoloom_plan::checks::{self, Host, Position, Probe, Render, Resolved};

/// What a script check runs instead, should one reach a runner: why it can't run, and failure.
const NO_SCRIPT: &str = "echo 'scripts need the project on the machine; the external target runs probes only' >&2; exit 1";

/// Each runner: its path in the target's folder (`checks/<machine>.sh`) and its script.
pub(crate) fn runners(env: &Environment) -> Vec<(String, String)> {
    let host = |h: &Host, _: &Position| reach(env, h);
    let no_script = |_: &str| NO_SCRIPT.to_owned();
    let render = Render {
        host: &host,
        script: &no_script,
        playbook: None,
    };
    checks::by_position(env.spec(), &env.checks)
        .into_iter()
        .filter_map(|(pos, group)| {
            let Position::Machine(m) = &pos else { return None };
            env.machines.get(m)?.external.as_ref()?;
            let probes: Vec<&Resolved> = group
                .into_iter()
                .filter(|c| !matches!(c.probe, Probe::Script { .. } | Probe::Playbook { .. }))
                .collect();
            (!probes.is_empty()).then(|| (format!("checks/{m}.sh"), checks::script(&pos, &probes, &render)))
        })
        .collect()
}

/// A probe's host as the existing hosts reach it: a machine at its address on the network.
fn reach(env: &Environment, host: &Host) -> String {
    match host {
        Host::Machine { name, network } => env.probe_address(name, network),
        Host::Literal(literal) => literal.clone(),
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::environment;

    #[test]
    fn a_host_runs_its_probes_and_leaves_its_scripts_out() {
        let env = environment(
            "  a: { networks: { lab: 5 }, access: true, vm: { os: debian@12 }, external: { address: 10.0.0.5 } }\n  b: { networks: { lab: 6 }, ports: [{ port: 80 }], vm: { os: debian@12 }, external: { address: 10.0.0.6 } }\nchecks:\n  - { name: b answers, from: a, tcp: 'b:80' }\n  - { name: own, from: a, script: checks/own.sh }\n",
        );
        let runners = super::runners(&env);
        let paths: Vec<&str> = runners.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["checks/a.sh"]);
        let script = runners.first().map(|(_, s)| s.as_str()).unwrap_or_default();
        assert!(script.contains("b answers"), "{script}");
        assert!(!script.contains("checks/own.sh"), "{script}");
    }
}
