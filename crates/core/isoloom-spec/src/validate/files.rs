//! The paths a spec mentions: inside the project folder, and present there.

use std::path::Path;

use super::Problem;
use crate::model::{Check, Spec};

/// The problems with the project's paths, in the order the spec mentions them.
struct Paths<'a> {
    lab_dir: &'a Path,
    problems: Vec<Problem>,
}

impl Paths<'_> {
    /// `path` must be relative, stay in the project and exist there.
    fn check(&mut self, at: String, path: &str) {
        if path.starts_with('/') || path.split('/').any(|s| s == "..") {
            self.problems
                .push(Problem::new(at, format!("`{path}` must be a path inside the project folder")));
        } else if !self.lab_dir.join(path).exists() {
            self.problems.push(Problem::new(at, format!("`{path}` doesn't exist in the project folder")));
        }
    }
}

/// Checks that every path the spec mentions exists in the project folder.
#[must_use]
pub fn validate_files(spec: &Spec, lab_dir: &Path) -> Vec<Problem> {
    let mut paths = Paths { lab_dir, problems: Vec::new() };
    if let Some(c) = &spec.cloud {
        paths.check("cloud.terraform".into(), &c.terraform);
        for (i, b) in c.before.iter().enumerate() {
            paths.check(format!("cloud.before[{i}]"), b);
        }
    }
    machines(spec, &mut paths);
    for (i, step) in spec.provision.iter().enumerate() {
        paths.check(format!("provision[{i}].ansible"), &step.ansible);
        for (j, inv) in step.inventory.iter().enumerate() {
            paths.check(format!("provision[{i}].inventory[{j}]"), inv);
        }
        if let Some(r) = &step.requirements {
            paths.check(format!("provision[{i}].requirements"), r);
        }
    }
    for (i, c) in spec.checks.iter().enumerate() {
        if let Some(path) = c.path() {
            let at = match c {
                Check::Script(_) => format!("checks[{i}]"),
                Check::Declared(_) => format!("checks[{i}].script"),
            };
            paths.check(at, path);
        }
    }
    paths.problems.extend(cloud_module(spec, lab_dir));
    paths.problems
}

/// The machines' build contexts, Dockerfiles, appliance files, files, `before` steps and
/// provisioning scripts.
fn machines(spec: &Spec, paths: &mut Paths) {
    for (name, m) in &spec.machines {
        if let Some(d) = &m.docker {
            let fields = [
                ("build", &d.build),
                ("dockerfile", &d.dockerfile),
                ("config", &d.config),
                ("firmware", &d.firmware),
            ];
            for (field, value) in fields {
                if let Some(v) = value {
                    paths.check(format!("machines.{name}.docker.{field}"), v);
                }
            }
            for (i, s) in d.init.iter().enumerate() {
                paths.check(format!("machines.{name}.docker.init[{i}]"), s);
            }
        }
        for (i, f) in m.files.iter().enumerate() {
            if let Some(s) = &f.source {
                paths.check(format!("machines.{name}.files[{i}].source"), s);
                if paths.lab_dir.join(s).is_dir() {
                    paths.problems.push(Problem::new(
                        format!("machines.{name}.files[{i}].source"),
                        format!("`{s}` is a folder: name a file"),
                    ));
                }
            }
        }
        for (i, s) in m.before.iter().enumerate() {
            paths.check(format!("machines.{name}.before[{i}]"), s);
        }
        if let Some(v) = &m.vm {
            for (i, s) in v.provision.iter().enumerate().filter(|(_, s)| !crate::model::is_reboot(s)) {
                paths.check(format!("machines.{name}.vm.provision[{i}]"), s);
            }
            // A box file in the project (a URL is Vagrant's to fetch).
            if let Some(u) = v.image.as_ref().and_then(|i| i.box_url.as_deref()).filter(|u| !u.contains("://")) {
                paths.check(format!("machines.{name}.vm.image.box_url"), u);
            }
        }
    }
}

/// A cloud-services module is a folder of .tf files.
fn cloud_module(spec: &Spec, lab_dir: &Path) -> Option<Problem> {
    let c = spec.cloud.as_ref()?;
    let dir = lab_dir.join(&c.terraform);
    let tf = std::fs::read_dir(&dir).is_ok_and(|d| d.flatten().any(|e| e.path().extension().is_some_and(|x| x == "tf")));
    (!tf && dir.is_dir()).then(|| {
        Problem::new(
            "cloud.terraform",
            format!("`{}` holds no .tf file: it must be a Terraform root module", c.terraform),
        )
    })
}
