//! The VM forms an environment's Dockerfiles give: on a target that runs VMs, a machine written as
//! `app.build` with no `app.steps` of its own takes the explicit VM form its Dockerfile
//! translates to ([`crate::dockerfile`]), so the target runs it as a true VM; one whose
//! Dockerfile doesn't translate is refused at its `app.build`, naming the line and the
//! instruction. The container targets read the spec as written. A machine written with both
//! `app.build` and `app.steps` has two recipes: its Dockerfile is read too, to say whether it
//! would translate on its own ([`two_recipes`]).

use std::collections::BTreeMap;

use isoloom_spec::model::{Shape, Target};
use isoloom_spec::syntax::{self, Code, FieldPath, Kind, Problem, ProjectPath, Recipe};

use crate::dockerfile::{self, Build, Sources, Translation, Untranslatable};

/// The spec with the VM forms its Dockerfiles give, and each machine whose Dockerfile gave none,
/// with why.
#[derive(Debug, Clone)]
pub(super) struct Translated {
    /// The spec, each translated machine with `os` and `app.steps`.
    pub source: syntax::Spec,
    /// The machines whose Dockerfile doesn't translate: the refusal at their `app.build`.
    pub refused: BTreeMap<String, Problem>,
    /// The texts of the templates the translations place that aren't the project's (catalogue
    /// entries' files and value files), by the path their steps' `from` names.
    pub files: BTreeMap<ProjectPath, String>,
}

/// A machine's build and its Dockerfile's path, and whether the machine has `app.steps` of its
/// own beside it.
fn build(m: &syntax::Machine) -> Option<(&syntax::Build, String, bool)> {
    let Kind::App(app) = &m.kind else { return None };
    let Some(Recipe::Build(b)) = &app.container else { return None };
    let path = b
        .dockerfile
        .clone()
        .unwrap_or_else(|| format!("{}/Dockerfile", b.context.trim_end_matches('/')));
    Some((b, path.trim_start_matches("./").to_owned(), !app.steps.is_empty()))
}

/// The Dockerfile a machine's VM form is translated from: its build's, when it has no steps of
/// its own.
fn dockerfile(m: &syntax::Machine) -> Option<(&syntax::Build, String)> {
    build(m).filter(|(_, _, steps)| !steps).map(|(b, p, _)| (b, p))
}

/// The Dockerfiles the environment's VM forms are translated from, each once.
pub(super) fn paths(source: &syntax::Spec) -> Vec<ProjectPath> {
    each_once(source.machines.values().filter_map(dockerfile).map(|(_, p)| p))
}

/// Every build's Dockerfile, each once: those [`paths`] has, and those `app.steps` stand beside.
pub(super) fn all_paths(source: &syntax::Spec) -> Vec<ProjectPath> {
    each_once(source.machines.values().filter_map(build).map(|(_, p, _)| p))
}

/// The project paths among `paths`, each once, in order.
fn each_once(paths: impl Iterator<Item = String>) -> Vec<ProjectPath> {
    let mut out: Vec<ProjectPath> = Vec::new();
    for p in paths.filter_map(|p| p.parse().ok()) {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// `m`'s build, translated from its Dockerfile `text` at `path`, its bases from the matrix or
/// `bases`.
fn translation(m: &syntax::Machine, build: &syntax::Build, path: &str, text: &str, bases: &Sources) -> Result<Translation, Untranslatable> {
    let args: Vec<(String, String)> = build.args.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let os = m.os.map(|o| o.to_string());
    let b = Build {
        path,
        text,
        context: build.context.trim_start_matches("./").trim_end_matches('/'),
        args: &args,
        os: os.as_deref(),
    };
    dockerfile::translate(&b, bases)
}

/// Whether a machine with two recipes needs both: its Dockerfile's text wasn't given, or it
/// translates to a VM form on its own, or it has none, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TwoRecipes {
    /// The Dockerfile wasn't read: Isoloom can't tell.
    Unread,
    /// The Dockerfile translates to a VM form: the steps aren't needed.
    Translates,
    /// The Dockerfile has no VM form: the steps are needed.
    NoVmForm(Untranslatable),
}

/// Each machine written with both `app.build` and `app.steps`, in the spec's order, and whether
/// its Dockerfile (`texts`, by project path) would give its VM form on its own.
pub(super) fn two_recipes(source: &syntax::Spec, texts: &BTreeMap<ProjectPath, String>, bases: &Sources) -> Vec<(String, TwoRecipes)> {
    let mut out = Vec::new();
    for (name, m) in &source.machines {
        let Some((b, path, true)) = build(m) else { continue };
        let found = match path.parse::<ProjectPath>().ok().and_then(|p| texts.get(&p)) {
            None => TwoRecipes::Unread,
            Some(text) => match translation(m, b, &path, text, bases) {
                Ok(_) => TwoRecipes::Translates,
                Err(u) => TwoRecipes::NoVmForm(u),
            },
        };
        out.push((name.clone(), found));
    }
    out
}

/// Whether `target` runs machines as VMs, so it reads the translated spec.
pub(super) fn wants_vm(target: Target) -> bool {
    matches!(target.needs(), Shape::Vm | Shape::VmOrLxc | Shape::External)
}

/// `source` with the VM forms its Dockerfiles (`texts`, by project path) give, their bases from
/// the matrix or `bases`. A machine whose Dockerfile text isn't given keeps no VM form.
pub(super) fn translate(source: &syntax::Spec, texts: &BTreeMap<ProjectPath, String>, bases: &Sources) -> Translated {
    let mut out = source.clone();
    let mut refused = BTreeMap::new();
    let mut files = BTreeMap::new();
    for (name, m) in &mut out.machines {
        let Some((build, path)) = dockerfile(m) else { continue };
        let Some(text) = path.parse::<ProjectPath>().ok().and_then(|p| texts.get(&p)) else {
            continue;
        };
        match translation(m, build, &path, text, bases) {
            Ok(t) => {
                files.extend(t.files);
                m.os = Some(t.os);
                if let Kind::App(app) = &mut m.kind {
                    app.steps = t.steps;
                }
            }
            Err(u) => {
                let at = FieldPath::new("machines").key(name).key("app").key("build");
                refused.insert(
                    name.clone(),
                    Problem::new(Code::Refused, at, format!("its Dockerfile has no VM form: {u}")).fix(u.fix),
                );
            }
        }
    }
    Translated { source: out, refused, files }
}

impl Translated {
    /// `p`, or the translator's refusal when `p` is the bare "no VM form" at a translated
    /// machine's `app.build`: the line and the instruction say more.
    pub(super) fn explain(&self, p: Problem) -> Problem {
        let name = p.path.as_str().strip_prefix("machines.").and_then(|rest| rest.strip_suffix(".app.build"));
        match name.and_then(|n| self.refused.get(n)) {
            Some(why) if p.code == Code::Refused => why.clone(),
            _ => p,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lab;

    const LAB: &str = "version: 1\nname: t\nmachines:\n  web: { app: { build: build/web }, ports: { http: { port: 80 } } }\n  db: { app: { build: { context: build/db, dockerfile: build/db/Dockerfile.lab } } }\n  own: { os: debian@12, app: { build: build/own, steps: [{ run: setup.sh }] } }\n";

    fn texts(pairs: &[(&str, &str)]) -> BTreeMap<ProjectPath, String> {
        pairs.iter().map(|(p, t)| (p.parse().unwrap(), (*t).to_owned())).collect()
    }

    #[test]
    fn the_dockerfiles_translated_are_the_builds_without_steps() {
        let lab = Lab::parse(LAB).unwrap();
        let found: Vec<String> = paths(lab.source()).iter().map(ToString::to_string).collect();
        assert_eq!(found, ["build/web/Dockerfile", "build/db/Dockerfile.lab"]);
        let read: Vec<String> = lab.all_dockerfile_paths().iter().map(ToString::to_string).collect();
        assert_eq!(read, ["build/web/Dockerfile", "build/db/Dockerfile.lab", "build/own/Dockerfile"]);
    }

    #[test]
    fn a_build_beside_steps_is_never_translated() {
        let lab = Lab::parse(LAB).unwrap().with_dockerfiles(texts(&[(
            "build/own/Dockerfile",
            "FROM debian:12
",
        )]));
        let vm = lab.for_target(Target::Vagrant);
        assert_eq!(vm.machines["own"].vm.as_ref().map(|v| v.os.as_str()), Some("debian-12"));
        let steps = vm.machines["own"].configuration.as_ref().map(|c| c.steps().count());
        assert_eq!(steps, Some(1), "its own step, not the Dockerfile's");
    }

    #[test]
    fn a_vm_target_runs_the_translation_and_a_container_target_the_build() {
        let lab = Lab::parse(LAB).unwrap().with_dockerfiles(texts(&[
            ("build/web/Dockerfile", "FROM php:8.2-apache\nCOPY src/ .\n"),
            (
                "build/db/Dockerfile.lab",
                "FROM mariadb:10.11\nENV MARIADB_ROOT_PASSWORD=root MARIADB_DATABASE=app\n",
            ),
        ]));
        assert_eq!(lab.refusal(Target::Vagrant), None);
        let vm = lab.for_target(Target::Vagrant);
        assert_eq!(vm.machines["web"].vm.as_ref().map(|v| v.os.as_str()), Some("debian-12"));
        assert!(vm.machines["web"].configuration.is_some());
        let docker = lab.for_target(Target::Docker);
        assert_eq!(docker.machines["web"].docker.as_ref().and_then(|d| d.build.as_deref()), Some("build/web"));
        assert_eq!(docker.machines["web"].vm, None);
    }

    #[test]
    fn a_dockerfile_that_doesnt_translate_is_refused_at_its_line() {
        let lab = Lab::parse(LAB).unwrap().with_dockerfiles(texts(&[
            (
                "build/web/Dockerfile",
                "FROM php:8.2-apache\nCOPY --from=mlocati/php-extension-installer /usr/bin/install-php-extensions /usr/bin/\n",
            ),
            ("build/db/Dockerfile.lab", "FROM mariadb:10.11\nENV MARIADB_ROOT_PASSWORD=root\n"),
        ]));
        let refusal = lab.refusal(Target::Vagrant).unwrap();
        assert_eq!(refusal.path.as_str(), "machines.web.app.build");
        assert_eq!(
            refusal.reason,
            "its Dockerfile has no VM form: build/web/Dockerfile line 2 (`COPY`): `--from=mlocati/php-extension-installer` copies out of another image, which the VM doesn't have"
        );
        assert!(matches!(lab.not_possible(Target::Vagrant), Some(crate::NotPossible::MissingForm(p)) if p == refusal));
        assert_eq!(lab.refusal(Target::Docker), None);
    }
}
