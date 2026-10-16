//! One generated Ansible role per machine (`isoloom_machine_<m>`): its `tasks/main.yml` (the
//! recipe's steps in order), its `handlers/main.yml` (one restart handler per enabled service) and
//! its `files/rendered/<from>` (each template filled in at generation). The role reaches the state
//! the shell library reaches; `cargo xtask conformance --renderer ansible` proves it.

use isoloom_plan::configure::{Resource, ServicePlan};
use isoloom_render::configure::Configured;
use isoloom_render::{GeneratedFile, header};

use super::task::{Task, restart, yaml};
use super::tasks::{rendered, step_tasks};

/// A machine's role name (`isoloom_machine_web`).
#[must_use]
pub fn role_name(machine: &str) -> String {
    format!("isoloom_machine_{machine}")
}

/// The files of machine `c`'s role, under `dir` (its role folder): the tasks, the handlers (when it
/// has an enabled service) and the filled templates.
#[must_use]
pub fn files(c: &Configured<'_>, dir: &str) -> Vec<GeneratedFile> {
    let resources: Vec<&Resource> = c.recipe().resources().collect();
    let enabled: Vec<(usize, String)> = resources
        .iter()
        .enumerate()
        .filter_map(|(i, r)| match r {
            Resource::Service(ServicePlan { unit, enabled: true, .. }) => Some((i, unit.to_string())),
            _ => None,
        })
        .collect();
    let mut tasks = Vec::new();
    for (i, r) in resources.iter().enumerate() {
        let notify: Vec<String> = enabled.iter().filter(|(j, _)| *j > i).map(|(_, u)| u.clone()).collect();
        tasks.extend(step_tasks(c, r, &notify, i));
    }
    if !enabled.is_empty() {
        tasks.push(
            Task::new("apply the restarts")
                .module("ansible.builtin.meta", "flush_handlers".into())
                .into_value(),
        );
    }
    let mut out = vec![yaml_file(&format!("{dir}/tasks/main.yml"), tasks)];
    if !enabled.is_empty() {
        out.push(yaml_file(&format!("{dir}/handlers/main.yml"), handlers(&enabled)));
    }
    out.extend(templates(c, dir));
    out
}

/// One restart handler per enabled service, by unit name (deduplicated, in first-seen order).
fn handlers(enabled: &[(usize, String)]) -> Vec<serde_json::Value> {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for (_, unit) in enabled {
        if seen.contains(&unit.as_str()) {
            continue;
        }
        seen.push(unit);
        out.push(
            Task::new(restart(unit))
                .module("ansible.builtin.service", serde_json::json!({ "name": unit, "state": "restarted" }))
                .into_value(),
        );
    }
    out
}

/// Each template of the recipe, filled in, at `files/rendered/<from>` under the role (those that
/// can't be filled are left to the play's `fail` task).
fn templates(c: &Configured<'_>, dir: &str) -> Vec<GeneratedFile> {
    c.recipe()
        .resources()
        .filter_map(|r| match r {
            Resource::Template(t) => Some(GeneratedFile {
                path: format!("{dir}/files/{}", rendered(t.from.as_str())),
                contents: c.template(&t.from).ok()?,
            }),
            _ => None,
        })
        .collect()
}

/// A list of tasks as a generated YAML file, with the comment header every generated file has.
fn yaml_file(path: &str, tasks: Vec<serde_json::Value>) -> GeneratedFile {
    GeneratedFile {
        path: path.to_owned(),
        contents: format!("{}{}", header("#"), yaml(tasks)),
    }
}
