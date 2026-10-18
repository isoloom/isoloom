//! Writing the Compose draft: the shared parts through [`Writer`], the `app:` block here.

use std::collections::BTreeSet;

use super::Machine;
use super::networks::Networks;
use crate::draft::quoting::{quoted, scalar};
use crate::draft::writer::{Header, Port, Writer};

/// What goes into the draft besides the machines.
#[derive(Debug)]
pub(super) struct Environment<'a> {
    pub(super) name: &'a str,
    pub(super) source: &'a str,
    pub(super) nets: &'a Networks,
    /// Every input a machine reads, for the spec's own `inputs`.
    pub(super) inputs: &'a BTreeSet<String>,
}

/// The draft: the header, the networks, the inputs, then the machines.
pub(super) fn draft_yaml(env: &Environment, machines: &[Machine]) -> String {
    let mut y = Writer::new(&Header {
        tool: "compose",
        source: env.source,
        review: "Review it with the notes the\nimport printed: what Compose said that this file doesn't (yet), and why.",
        next: "add `os` and `app.steps` to each machine for the VM targets, and `checks` that prove the behavior.",
        name: env.name,
    });
    let nets = env.nets.nets.values();
    let width = nets.clone().map(|n| n.name.len()).max().unwrap_or(0);
    for n in nets {
        y.network(&n.name, width, n.cidr, n.internet);
    }
    if !env.inputs.is_empty() {
        // Compose gave them to the containers as environment variables: not sensitive, so they
        // may stay environment values (a secret belongs in a file: `files: [{ path, input }]`).
        let each: Vec<String> = env.inputs.iter().map(|i| format!("{i}: {{ sensitive: false }}")).collect();
        y.line(format!("\ninputs: {{ {} }}", each.join(", ")));
    }
    y.machines();
    for m in machines {
        machine(&mut y, m);
    }
    y.finish()
}

fn machine(y: &mut Writer, m: &Machine) {
    y.machine(&m.name);
    y.machine_networks(m.networks.iter().map(|(n, o)| (n, *o)));
    y.ports(m.ports.iter().map(|key| Port {
        number: key.0,
        udp: !key.1.is_tcp(),
        publish: m.publish.get(key).copied(),
    }));
    if !m.inputs.is_empty() {
        let each: Vec<String> = m.inputs.iter().map(|i| format!("{i}: {{ input: {i} }}")).collect();
        y.line(format!("    environment: {{ {} }}", each.join(", ")));
    }
    y.resources(m.cpus, m.memory_mb);
    if !m.depends_on.is_empty() {
        y.line(format!("    depends_on: [{}]", m.depends_on.join(", ")));
    }
    if !m.volumes.is_empty() {
        let vols = m.volumes.iter().map(|(n, p)| format!("{n}: {}", scalar(p))).collect::<Vec<_>>().join(", ");
        y.line(format!("    volumes: {{ {vols} }}"));
    }
    app(y, m);
}

/// The `app:` block: the build when there is one (in full when it has a Dockerfile, a stage or
/// arguments), else the image (empty when neither).
fn app(y: &mut Writer, m: &Machine) {
    match (&m.build, &m.image) {
        (Some(b), _) if m.dockerfile.is_none() && m.target.is_none() && m.args.is_empty() => {
            y.line(format!("    app:\n      build: {}", quoted(b)));
        }
        (Some(b), _) => {
            y.line(format!("    app:\n      build:\n        context: {}", quoted(b)));
            if let Some(f) = &m.dockerfile {
                y.line(format!("        dockerfile: {}", quoted(f)));
            }
            if let Some(t) = &m.target {
                y.line(format!("        target: {}", quoted(t)));
            }
            if !m.args.is_empty() {
                let args = m
                    .args
                    .iter()
                    .map(|(k, v)| format!("{}: {}", scalar(k), quoted(v)))
                    .collect::<Vec<_>>()
                    .join(", ");
                y.line(format!("        args: {{ {args} }}"));
            }
        }
        (None, Some(img)) => y.line(format!("    app:\n      image: {}", quoted(img))),
        (None, None) => y.line("    app:\n      image: \"\""),
    }
}
