//! What a spec says that is likely not what its author means: a daemon a recipe installs that no
//! step says where to run, a machine with two recipes (`app.build` and `app.steps`), a best-effort
//! seal, an access machine left to the runner that the image table gives no form, a sensitive
//! input's default that reads like a real secret. Warnings, never problems: validation passes
//! with them.

use std::collections::HashSet;

use isoloom_spec::model::Seal;
use isoloom_spec::syntax::{FieldPath, PackageName, Warning};

use super::Lab;
use super::dockerfiles::{self, TwoRecipes};
use crate::configure::unstarted;

impl Lab {
    /// What the spec says that is likely not what its author means, in file order: a daemon a
    /// recipe installs with no `service` step for it (it would start on Debian VMs only), a
    /// machine with both `app.build` and `app.steps` (two recipes that drift apart), a
    /// best-effort seal, an access machine left to the runner that the image table gives no
    /// form, a sensitive input's default that reads like a real secret. Never a problem:
    /// validation passes with warnings.
    #[must_use]
    pub fn warnings(&self) -> Vec<Warning> {
        let mut out = self.unstarted_warnings();
        out.extend(self.two_recipe_warnings());
        out.extend(self.seal_warnings());
        out.extend(self.unsupplied_warning());
        out.extend(self.secret_default_warnings());
        out.sort_by_key(|w| w.span.map(|s| (s.line, s.column)));
        out
    }

    /// A daemon a recipe installs with no `service` step for it, once per step and package of the
    /// spec's own lines (a machine's clones share them).
    fn unstarted_warnings(&self) -> Vec<Warning> {
        let daemons = self.daemons();
        let mut said: HashSet<(FieldPath, PackageName)> = HashSet::new();
        let mut out = Vec::new();
        for (name, m) in &self.source.machines {
            let Some(os) = m.os else { continue };
            let base = self.written_as(name);
            for u in unstarted(os, m, daemons) {
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

    /// A machine written with both `app.build` and `app.steps`, once per machine of the spec's own
    /// lines: containers run the Dockerfile and VMs the steps, two descriptions of one machine
    /// that drift apart. When its Dockerfile was read, the warning says whether it translates to
    /// a VM form on its own (the steps aren't needed) or has none (they are, until it does).
    fn two_recipe_warnings(&self) -> Vec<Warning> {
        const TWICE: &str =
            "`app.build` and `app.steps` are two recipes for one machine: containers run the Dockerfile, VMs the steps, and the two drift apart";
        const ONCE: &str = "describe the machine once, so every target comes from one description: a catalogue `service`, or `os` and portable `steps` alone";
        let mut said: HashSet<FieldPath> = HashSet::new();
        let mut out = Vec::new();
        for (name, found) in dockerfiles::two_recipes(&self.source, &self.dockerfiles, &self.bases) {
            let path = FieldPath::new("machines").key(self.written_as(&name)).key("app").key("steps");
            if !said.insert(path.clone()) {
                continue;
            }
            let (reason, fix) = match found {
                TwoRecipes::Unread => (TWICE.to_owned(), format!("{ONCE} (keep both only while the Dockerfile has no VM form)")),
                TwoRecipes::Translates => (
                    format!("{TWICE}. Its Dockerfile translates to a VM form on its own, so the steps aren't needed"),
                    "describe the machine once: `build` alone (VMs run its translation), a catalogue `service`, or `os` and portable `steps` alone".to_owned(),
                ),
                TwoRecipes::NoVmForm(u) => (
                    format!("{TWICE}. Both are needed here while its Dockerfile has no VM form: {u}"),
                    format!("{ONCE}; else keep the two in step"),
                ),
            };
            let mut w = Warning::new(path, reason).fix(fix);
            w.span = self.source.spans.find(&w.path);
            out.push(w);
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

    /// A sensitive input's default that reads like a real secret ([`reads_like_a_secret`]): every
    /// generated file holds it as it is, so it is public.
    fn secret_default_warnings(&self) -> Vec<Warning> {
        let mut out = Vec::new();
        for (name, input) in &self.source.inputs.0 {
            let Some(default) = input.default.as_deref().filter(|d| input.sensitive && !d.is_empty() && reads_like_a_secret(d)) else {
                continue;
            };
            let mut w = Warning::new(
                FieldPath::new("inputs").key(name.as_str()).key("default"),
                format!(
                    "`{name}` is sensitive, and its default is written as it is in every generated file, where anyone reads it: `{}` doesn't read as a development value",
                    shown(default)
                ),
            )
            .fix("a public development value that says so (`dev-…`, `example-…`, `test-…`), or no default");
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

/// The words a public development value says itself with: a default holding one (any case)
/// reads as a stand-in, unless it has a real credential's shape.
const STAND_IN_WORDS: [&str; 13] = [
    "dev",
    "test",
    "example",
    "demo",
    "local",
    "sample",
    "placeholder",
    "insecure",
    "dummy",
    "fake",
    "changeme",
    "default",
    "public",
];

/// How real credentials start (a private key, cloud and forge tokens): a default starting so is a
/// secret whatever it says.
const CREDENTIAL_PREFIXES: [&str; 12] = [
    "-----BEGIN",
    "AKIA",
    "ASIA",
    "AIza",
    "ghp_",
    "gho_",
    "github_pat_",
    "glpat-",
    "xox",
    "sk-",
    "sk_live_",
    "rk_live_",
];

/// Whether `value`, a sensitive input's default, reads like a real secret: a real credential's
/// shape, or no word saying it is a stand-in ([`STAND_IN_WORDS`]).
fn reads_like_a_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    CREDENTIAL_PREFIXES.iter().any(|p| value.starts_with(p)) || !STAND_IN_WORDS.iter().any(|w| lower.contains(w))
}

/// `value` as a warning shows it: its first 12 characters, then `…` when it is longer.
fn shown(value: &str) -> String {
    let mut chars = value.chars();
    let head: String = chars.by_ref().take(12).collect();
    if chars.next().is_some() { format!("{head}…") } else { head }
}
