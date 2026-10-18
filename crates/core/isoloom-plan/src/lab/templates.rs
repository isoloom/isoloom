//! The project's template files a lab's steps fill in: which ones its machines name (so the
//! caller that generates reads them, at the edge), and their texts added to each machine's
//! configuration once lowered, so the renderers install them without reading the project.

use std::collections::BTreeMap;

use isoloom_spec::model;
use isoloom_spec::syntax::{self, ProjectPath, Step};

/// The texts of a project's templates, by their path.
pub(super) type Texts = BTreeMap<ProjectPath, String>;

/// The project files the app steps of `source` fill in as templates, each once, in file order.
pub(super) fn paths(source: &syntax::Spec) -> Vec<ProjectPath> {
    let mut out: Vec<ProjectPath> = Vec::new();
    let steps = source.machines.values().filter_map(|m| m.app()).flat_map(|a| a.steps.iter());
    for step in steps {
        if let Step::Template(t) = step
            && !out.contains(&t.from)
        {
            out.push(t.from.clone());
        }
    }
    out
}

/// `spec` with `texts` added to each machine's configuration, for the templates its steps name
/// and don't carry already (an entry's own come with it).
pub(super) fn add(mut spec: model::Spec, texts: &Texts) -> model::Spec {
    if texts.is_empty() {
        return spec;
    }
    for c in spec.machines.values_mut().filter_map(|m| m.configuration.as_mut()) {
        let named: Vec<ProjectPath> = c
            .steps()
            .filter_map(|s| match s {
                Step::Template(t) => Some(t.from.clone()),
                _ => None,
            })
            .collect();
        for from in named {
            if let Some(text) = texts.get(&from) {
                c.files.entry(from).or_insert_with(|| text.clone());
            }
        }
    }
    spec
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lab;

    const LAB: &str = "version: 1\nname: t\nmachines:\n  a: { os: debian@12, app: { steps: [{ template: { from: conf/a.conf, to: /etc/a.conf } }, { template: { from: conf/b.conf, to: /etc/b.conf } }] } }\n  b: { os: debian@12, app: { steps: [{ template: { from: conf/a.conf, to: /etc/a.conf } }] } }\n";

    #[test]
    fn each_template_a_step_names_is_read_once() {
        let lab = Lab::parse(LAB).unwrap();
        let found: Vec<String> = paths(lab.source()).iter().map(ToString::to_string).collect();
        assert_eq!(found, ["conf/a.conf", "conf/b.conf"]);
    }

    #[test]
    fn a_machine_gets_the_texts_of_the_templates_it_names() {
        let texts = Texts::from([("conf/a.conf".parse().unwrap(), "A {{ machine.name }}\n".to_owned())]);
        let spec = Lab::parse(LAB).unwrap().with_templates(texts).spec();
        let files = |m: &str| spec.machines[m].configuration.as_ref().map(|c| c.files.len());
        assert_eq!((files("a"), files("b")), (Some(1), Some(1)), "conf/b.conf wasn't read: left out");
    }
}
