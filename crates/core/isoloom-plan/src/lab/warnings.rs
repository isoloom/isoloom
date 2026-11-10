//! What a spec says that is likely not what its author means: a daemon a recipe installs that no
//! step says where to run, a best-effort seal, an access machine left to the runner that the image
//! table gives no form. Warnings, never problems: validation passes with them.

use std::collections::HashSet;

use isoloom_spec::model::Seal;
use isoloom_spec::syntax::{FieldPath, PackageName, Warning};

use super::Lab;
use crate::configure::{DaemonTable, unstarted};

impl Lab {
    /// What the spec says that is likely not what its author means, in file order: a daemon a
    /// recipe installs with no `service` step for it (it would start on Debian VMs only), a
    /// best-effort seal, an access machine left to the runner that the image table gives no
    /// form. Never a problem: validation passes with warnings.
    #[must_use]
    pub fn warnings(&self) -> Vec<Warning> {
        let mut out = self.unstarted_warnings();
        out.extend(self.seal_warnings());
        out.extend(self.unsupplied_warning());
        out.sort_by_key(|w| w.span.map(|s| (s.line, s.column)));
        out
    }

    /// A daemon a recipe installs with no `service` step for it, once per step and package of the
    /// spec's own lines (a machine's clones share them).
    fn unstarted_warnings(&self) -> Vec<Warning> {
        let daemons = DaemonTable::common();
        let mut said: HashSet<(FieldPath, PackageName)> = HashSet::new();
        let mut out = Vec::new();
        for (name, m) in &self.source.machines {
            let Some(os) = m.os else { continue };
            let base = self.written_as(name);
            for u in unstarted(os, m, &daemons) {
                let path = FieldPath::new("machines").key(base).key("app").key("steps").index(u.step);
                if !said.insert((path.clone(), u.package.clone())) {
                    continue;
                }
                let mut w = Warning::new(
                    path,
                    format!(
                        "`{}` installs a daemon that starts on Debian VMs only (their packages start it): nothing starts it in a container or on another family's VM",
                        u.package
                    ),
                )
                .fix(format!("`service: {unit}`, or `{{ service: {{ name: {unit}, enabled: false }} }}`", unit = u.unit));
                w.span = self.source.spans.find(&w.path);
                out.push(w);
            }
        }
        out
    }

    /// A best-effort seal, once per machine of the spec's own lines (a machine's clones share
    /// them).
    fn seal_warnings(&self) -> Vec<Warning> {
        let mut said: HashSet<FieldPath> = HashSet::new();
        let mut out = Vec::new();
        for (name, m) in &self.source.machines {
            let path = FieldPath::new("machines").key(self.written_as(name)).key("seal");
            if m.seal != Seal::BestEffort || !said.insert(path.clone()) {
                continue;
            }
            let mut w = Warning::new(
                path,
                "`seal: best-effort` makes this machine's `internet: false` seal advisory: it is applied, but a privileged container can lift it from inside, so treat the machine as able to reach the internet",
            );
            w.span = self.source.spans.find(&w.path);
            out.push(w);
        }
        out
    }

    /// The machine the spec writes machine `name` as: the base a clone is made of (`ws` for
    /// `ws-1`, from `count:`), else `name`. A warning about a clone is about its base's lines,
    /// once.
    fn written_as<'a>(&'a self, name: &'a str) -> &'a str {
        self.source
            .clones
            .iter()
            .find(|(_, c)| c.iter().any(|n| n == name))
            .map_or(name, |(b, _)| b.as_str())
    }
}

#[cfg(test)]
mod tests;
