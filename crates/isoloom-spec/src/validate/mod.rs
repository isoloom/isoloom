//! Checks a spec for mistakes the generators would turn into broken files. Every problem
//! names the field it's about (`machines.web.networks.dmz`) and says what to do, so an
//! author (human or AI) can fix it without reading the generators.
//!
//! [`validate`] runs one validator per section of the spec, in a fixed order (the order of
//! the problems is part of the output): the top level, constraints, cloud services, networks,
//! reach rules, inputs and secrets, machines (with their settings, volumes and ports, then
//! their own process), provisioning and the controller, tools, link impairment, the message,
//! groups, checks and targets.

mod boot;
mod checks;
mod cidr;
mod cidr6;
mod cloud;
mod constraints;
mod dns;
mod files;
mod groups;
mod hardware;
mod ipv6;
mod links;
mod machine_files;
mod machines;
mod networks;
mod os;
mod outside;
mod ports;
mod process;
mod provision;
mod reach;
mod secrets;
mod settings;
mod storage;
mod targets;
mod tools;
mod top;
mod volumes;
mod windows;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use crate::model::{Machine, Spec};

pub use cidr::{Cidr, DOCKER_BLOCK, PRIVATE, docker_cidrs};
pub use cidr6::{Cidr6, PREFIX as IPV6_PREFIX};
pub use files::validate_files;
pub use settings::CONTAINER_SYSCTLS;

/// What the spec still writes the way it did before 1.0 (a machine's `services`, now `ports`):
/// warnings, not problems, so the spec stays valid while its author renames them.
pub fn warnings(spec: &Spec) -> &[Problem] {
    &spec.warnings
}

/// One thing wrong with a spec: the field's path and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{at}: {message}")]
pub struct Problem {
    /// Where in the file, e.g. `machines.web.depends_on`.
    pub at: String,
    /// The reason, and the fix when there is one.
    pub message: String,
}

impl Problem {
    /// A problem at the field path `at`.
    pub fn new(at: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            at: at.into(),
            message: message.into(),
        }
    }

    /// The top-level section of the spec the problem is in: `machines` for
    /// `machines.web.depends_on[0]`, `checks` for `checks[2].http`.
    pub fn section(&self) -> &str {
        let end = self.at.find(['.', '[']).unwrap_or(self.at.len());
        &self.at[..end]
    }
}

/// The problems found so far, in the order the validators report them.
#[derive(Debug, Default)]
struct Problems(Vec<Problem>);

impl Problems {
    fn add(&mut self, at: impl Into<String>, message: impl Into<String>) {
        self.0.push(Problem::new(at, message));
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The networks whose `cidr` parses, in declaration order, with their block.
type Blocks = Vec<(String, Cidr)>;

/// The block of network `name`, when its `cidr` parses.
fn block(blocks: &Blocks, name: &str) -> Option<Cidr> {
    blocks.iter().find(|(n, _)| n == name).map(|(_, c)| *c)
}

/// Checks the spec itself (no file system access). See [`validate_files`] for paths.
pub fn validate(spec: &Spec) -> Vec<Problem> {
    let mut p = Problems::default();
    top::version_and_name(spec, &mut p);
    networks::declared(spec, &mut p);
    constraints::check(spec, &mut p);
    cloud::check(spec, &mut p);
    let blocks = networks::blocks(spec, &mut p);
    networks::gateways(spec, &blocks, &mut p);
    outside::networks(spec, &mut p);
    reach::check(spec, &mut p);
    outside::reach(spec, &mut p);
    top::inputs(spec, &mut p);
    secrets::check(spec, &mut p);
    machines::declared(spec, &mut p);
    machines::appliances(spec, &mut p);
    networks::switches(spec, &mut p);
    networks::management(spec, &mut p);
    volumes::shared(spec, &mut p);
    machines::each(spec, &blocks, &mut p);
    dns::options(spec, &mut p);
    outside::first_networks(spec, &mut p);
    machine_files::check(spec, &mut p);
    windows::check(spec, &mut p);
    process::check(spec, &mut p);
    dns::private(spec, &mut p);
    machines::cycle(spec, &mut p);
    boot::check(spec, &mut p);
    provision::controller(spec, &mut p);
    provision::steps(spec, &mut p);
    tools::reserved(spec, &blocks, &mut p);
    tools::each(spec, &mut p);
    machines::external(spec, &mut p);
    networks::tc(spec, &mut p);
    links::check(spec, &mut p);
    ipv6::check(spec, &mut p);
    top::message(spec, &mut p);
    groups::check(spec, &mut p);
    checks::check(spec, &mut p);
    targets::check(spec, &mut p);
    // On Docker, the networks (moved or not) mustn't overlap either: only once the blocks are
    // known to be valid.
    if p.is_empty() {
        networks::docker_overlaps(spec, &mut p);
    }
    p.0
}

fn kebab(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 63
        && s.split('-')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

/// Machine and network names become DNS names (machines resolve each other by name).
fn dns_label(s: &str) -> bool {
    kebab(s) && !s.starts_with(|c: char| c.is_ascii_digit())
}

/// An environment variable's (or Ansible variable's) name: letters, digits and `_`, not
/// starting with a digit.
fn variable_name(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The machines a machine starts after: its `depends_on`, then the gateways of its networks.
pub fn starts_after<'a>(spec: &'a Spec, name: &str, m: &'a Machine) -> Vec<&'a str> {
    let mut after: Vec<&str> = m.depends_on.iter().map(String::as_str).collect();
    for net in m.networks.keys() {
        if let Some(gw) = spec.networks.get(net).and_then(|n| n.gateway.as_deref())
            && gw != name
            && !after.contains(&gw)
        {
            after.push(gw);
        }
    }
    // A shared volume's users start once its owner holds it (serves it, on the VM targets).
    for v in m.volumes.keys() {
        if let Some(owner) = spec.volume_owner(v)
            && owner != name
            && spec.machines.contains_key(owner)
            && !after.contains(&owner)
        {
            after.push(owner);
        }
    }
    after
}

/// The first cycle in the machines' start order (`depends_on`, gateways, shared volumes), as
/// the names along it, the first repeated at the end.
fn dependency_cycle(spec: &Spec) -> Option<Vec<String>> {
    fn visit<'a>(spec: &'a Spec, n: &'a str, stack: &mut Vec<&'a str>, done: &mut BTreeSet<&'a str>) -> Option<Vec<String>> {
        if let Some(pos) = stack.iter().position(|s| *s == n) {
            let mut c: Vec<String> = stack[pos..].iter().map(std::string::ToString::to_string).collect();
            c.push(n.to_string());
            return Some(c);
        }
        if done.contains(n) {
            return None;
        }
        stack.push(n);
        for dep in spec.machines.get(n).map(|m| starts_after(spec, n, m)).unwrap_or_default() {
            if spec.machines.contains_key(dep)
                && let Some(c) = visit(spec, dep, stack, done)
            {
                return Some(c);
            }
        }
        stack.pop();
        done.insert(n);
        None
    }
    let mut done = BTreeSet::new();
    spec.machines.keys().find_map(|n| visit(spec, n, &mut Vec::new(), &mut done))
}
