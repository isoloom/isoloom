//! The check runners: one per existing host with checks to run from it. A check that needs the
//! project there (a script, a playbook) or has no host to run from is refused by the plan first
//! (an existing host has no copy of the project), so the runners hold probes only.

use isoloom_plan::Environment;
use isoloom_plan::checks::{Host, Position};
use isoloom_render::checks::{Render, script};

/// Each runner: its path in the target's folder (`checks/<machine>.sh`) and its script.
pub(crate) fn runners(env: &Environment) -> Vec<(String, String)> {
    let host = |h: &Host, _: &Position| isoloom_render::checks::address_of(env, h);
    let render = Render {
        host: &host,
        script: None,
        playbook: None,
    };
    env.checks_by_position()
        .into_iter()
        .filter_map(|(pos, group)| {
            let Position::Machine(m) = &pos else { return None };
            env.machines.get(m)?.external.as_ref()?;
            Some((format!("checks/{m}.sh"), script(&pos, &group, &render)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::testing::environment;

    #[test]
    fn a_host_runs_its_probes() {
        let env = environment(
            "  a: { networks: { lab: 5 }, access: true, os: debian@12 }\n  b: { networks: { lab: 6 }, ports: { web: 80 }, os: debian@12 }\noverrides: { external: { machines: { a: { ssh: { address: 10.0.0.5 } }, b: { ssh: { address: 10.0.0.6 } } } } }\nchecks:\n  - { name: b answers, from: a, tcp: 'b:80' }\n",
        );
        let runners = super::runners(&env);
        let paths: Vec<&str> = runners.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["checks/a.sh"]);
        let script = runners.first().map(|(_, s)| s.as_str()).unwrap_or_default();
        assert!(script.contains("b answers"), "{script}");
    }
}
