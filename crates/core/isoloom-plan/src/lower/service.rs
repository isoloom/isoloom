//! A machine's catalogue service in lowering (design §4.9, steps 6 to 8): each service machine
//! resolved once through the catalogue, then lowered as the machine its entry makes of it. The
//! entry gives its ports (the author's `publish` kept), its health unless the author gives one,
//! its data as the machine's volumes, and its passwords as value files; the same path or name
//! given twice is a `conflict`. Its configuration is the entry's three phases, resolved.

use std::borrow::Cow;

use indexmap::IndexMap;
use isoloom_catalogue::{Catalogue, Password, Resolved};
use isoloom_spec::model::{self, Mount};
use isoloom_spec::syntax::{self, Code, FieldPath, File, FileSource, Kind, Mode, Problem, Problems};

/// Every service machine of a lab, resolved: its services, or the problems of its `service`,
/// `os` and `with`.
#[derive(Debug, Clone, Default)]
pub(super) struct Services(IndexMap<String, Result<Vec<Resolved>, Problems>>);

impl Services {
    /// The service machines of `source`, resolved in `catalogue`.
    pub(super) fn of(catalogue: &Catalogue, source: &syntax::Spec) -> Services {
        let resolved = source
            .machines
            .iter()
            .filter(|(_, m)| matches!(m.kind, Kind::Service(_)))
            .map(|(name, _)| (name.clone(), catalogue.resolve(source, name)))
            .collect();
        Services(resolved)
    }

    /// Machine `name`'s services, when it runs any.
    pub(super) fn get(&self, name: &str) -> Option<&Result<Vec<Resolved>, Problems>> {
        self.0.get(name)
    }

    /// What the catalogue finds wrong, machine after machine.
    pub(super) fn problems(&self) -> Problems {
        self.0.values().filter_map(|r| r.as_ref().err()).flatten().cloned().collect()
    }
}

/// Machine `name` as its services make it (the author's machine itself when it runs none, or
/// when they don't resolve), and the conflicts between the two.
pub(super) fn effective<'a>(name: &str, m: &'a syntax::Machine, services: Option<&Result<Vec<Resolved>, Problems>>) -> (Cow<'a, syntax::Machine>, Problems) {
    let Some(Ok(resolved)) = services else {
        return (Cow::Borrowed(m), Vec::new());
    };
    let at = FieldPath::new("machines").key(name);
    let mut out = m.clone();
    let mut problems = ports(&at, &mut out, resolved);
    if out.healthcheck.is_none() {
        out.healthcheck = resolved.iter().find_map(|r| r.health.clone());
    }
    for r in resolved {
        problems.extend(data(&at, &mut out, r));
        problems.extend(value_files(&at, &mut out, r));
    }
    (Cow::Owned(out), problems)
}

/// The machine's ports: each entry's, by name, with the author's `publish` and `dormant`; a port
/// the author names that no entry has is an `unknown-reference`.
fn ports(at: &FieldPath, m: &mut syntax::Machine, resolved: &[Resolved]) -> Problems {
    let mut ports = IndexMap::new();
    for (name, p) in resolved.iter().flat_map(|r| r.ports.iter()) {
        let written = m.ports.get(name);
        ports.insert(
            name.clone(),
            syntax::Port {
                port: Some(p.port),
                protocol: p.protocol,
                publish: written.and_then(|w| w.publish),
                dormant: written.is_some_and(|w| w.dormant),
            },
        );
    }
    let known: Vec<String> = ports.keys().map(|k| format!("`{k}`")).collect();
    let entries: Vec<&str> = resolved.iter().map(|r| r.entry.as_str()).collect();
    let problems = m
        .ports
        .keys()
        .filter(|n| !ports.contains_key(*n))
        .map(|n| {
            Problem::new(
                Code::UnknownReference,
                at.key("ports").key(n.as_str()),
                format!("{} has no port `{n}`", entries.join(" and ")),
            )
            .fix(format!("its ports: {}", known.join(", ")))
        })
        .collect();
    m.ports = ports;
    problems
}

/// The entry's data, as the machine's volumes at the same paths.
fn data(at: &FieldPath, m: &mut syntax::Machine, r: &Resolved) -> Problems {
    let mut problems = Vec::new();
    for (name, path) in &r.data {
        let taken = m.volumes.iter().find(|(n, v)| n.as_str() == name.as_str() || mount_path(v) == path.as_str());
        match taken {
            Some((n, _)) => problems.push(
                Problem::new(
                    Code::Conflict,
                    at.key("volumes").key(n),
                    format!("{} keeps its `{name}` data at {path}", r.entry),
                )
                .fix("name your volume and its path apart from the entry's"),
            ),
            None => {
                m.volumes.insert(name.to_string(), Mount::Path(path.to_string()));
            }
        }
    }
    problems
}

/// A mount's path.
fn mount_path(m: &Mount) -> &str {
    match m {
        Mount::Path(p) => p,
        Mount::Part(p) => &p.path,
    }
}

/// The entry's passwords, as value files: a literal as the file's content, a secret or an input
/// as its value, mode `0400`, owned by root (the entry's `start` hands it to its owner).
fn value_files(at: &FieldPath, m: &mut syntax::Machine, r: &Resolved) -> Problems {
    let mut problems = Vec::new();
    for v in &r.value_files {
        if let Some(i) = m.files.iter().position(|f| f.path == v.path) {
            problems.push(
                Problem::new(
                    Code::Conflict,
                    at.key("files").index(i),
                    format!("{} places a setting's value file at {}", r.entry, v.path),
                )
                .fix("put your file elsewhere"),
            );
            continue;
        }
        let source = match &v.value {
            Password::Literal(text) => FileSource::Content(text.clone()),
            Password::Secret(name) => FileSource::Secret(name.clone()),
            Password::Input(name) => FileSource::Input(name.clone()),
            _ => continue,
        };
        m.files.push(File {
            path: v.path.clone(),
            source,
            uid: None,
            gid: None,
            mode: Some(Mode::VALUE),
        });
    }
    problems
}

/// What configures a service machine: its services' phases, one service's after the other's on
/// a VM that runs several, with every template's text.
pub(super) fn configuration(resolved: &[Resolved]) -> Option<model::Configuration> {
    let first = resolved.first()?;
    let mut phases: [Vec<syntax::Step>; 3] = Default::default();
    let mut files = std::collections::BTreeMap::new();
    for r in resolved {
        let [install, configure, start] = &mut phases;
        install.extend(r.install.iter().cloned());
        configure.extend(r.configure.iter().cloned());
        start.extend(r.start.iter().cloned());
        files.extend(r.files.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    let [install, configure, start] = phases;
    Some(model::Configuration::entry(first.os, install, configure, start, files))
}

/// The variables the official image reads, when the container runs it.
pub(super) fn image_environment(resolved: &[Resolved]) -> IndexMap<String, String> {
    resolved
        .iter()
        .filter_map(|r| r.image.as_ref())
        .flat_map(|i| i.environment.iter().map(|(k, v)| (k.clone(), v.clone())))
        .collect()
}

#[cfg(test)]
mod tests;
