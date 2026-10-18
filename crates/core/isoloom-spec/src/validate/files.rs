//! The paths a spec mentions: inside the project folder, and present there.

use std::path::Path;

use super::Problem;
use crate::model::{Check, Spec};
use crate::syntax::Code;

/// The problems with the project's paths, in the order the spec mentions them.
struct Paths<'a> {
    lab_dir: &'a Path,
    problems: Vec<Problem>,
}

impl Paths<'_> {
    /// `path` must be relative, stay in the project and exist there.
    fn check(&mut self, at: String, path: &str) {
        if path.starts_with('/') || path.split('/').any(|s| s == "..") {
            self.problems.push(Problem::new(
                Code::InvalidValue,
                at,
                format!("`{path}` must be a path inside the project folder"),
            ));
        } else if !self.lab_dir.join(path).exists() {
            self.problems
                .push(Problem::new(Code::InvalidValue, at, format!("`{path}` doesn't exist in the project folder")));
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

/// The machines' build contexts, Dockerfiles, appliance files, files and provisioning
/// scripts.
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
                        Code::InvalidValue,
                        format!("machines.{name}.files[{i}].source"),
                        format!("`{s}` is a folder: name a file"),
                    ));
                }
            }
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
            Code::InvalidValue,
            "cloud.terraform",
            format!("`{}` holds no .tf file: it must be a Terraform root module", c.terraform),
        )
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;

    use super::validate_files;
    use crate::testing::{lab, model};

    /// A test's outcome: a failed step is its report.
    type Outcome<T = ()> = Result<T, Box<dyn Error>>;

    /// Every file a spec of machines can name, in its project.
    const MACHINES: &str = "machines:
  web: { networks: { lan: 10 }, docker: { build: app, dockerfile: app/Dockerfile, init: [init.sh] }, files: [{ path: /etc/a.ini, source: conf/a.ini }] }
  r1: { networks: { lan: 20 }, docker: { image: x, appliance: cisco-iol, config: r1.cfg, firmware: fw.bin } }
  vm1: { networks: { lan: 30 }, vm: { os: debian@12, provision: [setup.sh, reboot], image: { vagrant: my/box, box_url: boxes/my.box } } }
  vm2: { networks: { lan: 40 }, vm: { os: debian@12, image: { vagrant: my/box, box_url: 'https://boxes.corp/my.box' } } }
provision: [{ ansible: site.yml, inventory: [inv.ini], requirements: req.yml }]
checks: [check.sh, { script: v1..2/probe.sh, from: web }]
";

    /// The files `MACHINES` names.
    const MACHINES_FILES: &[&str] = &[
        "app/Dockerfile",
        "init.sh",
        "conf/a.ini",
        "r1.cfg",
        "fw.bin",
        "setup.sh",
        "boxes/my.box",
        "site.yml",
        "inv.ini",
        "req.yml",
        "check.sh",
        "v1..2/probe.sh",
    ];

    /// A cloud-services spec with its module in `infra` and a `before` script.
    const CLOUD: &str = "version: 1\nname: t\ncloud: { provider: aws, terraform: infra, before: [prep.sh] }\n";

    /// What `validate_files` finds for the lowered spec `yaml` in a fresh project folder (named
    /// after `test`) holding `files`, each written empty; a name ending in `/` is a folder.
    fn found(test: &str, files: &[&str], yaml: &str) -> Outcome<Vec<String>> {
        let dir = crate::testing::folder(&format!("files-{test}"))?;
        for f in files {
            let path = dir.join(f);
            if f.ends_with('/') {
                fs::create_dir_all(&path)?;
            } else {
                fs::create_dir_all(path.parent().unwrap_or(&dir))?;
                fs::write(&path, "")?;
            }
        }
        let spec = model(yaml)?;
        let problems = validate_files(&spec, &dir).iter().map(ToString::to_string).collect();
        fs::remove_dir_all(&dir)?;
        Ok(problems)
    }

    #[test]
    fn every_path_present_in_the_project_passes() -> Outcome {
        assert_eq!(found("present", MACHINES_FILES, &lab(MACHINES))?, Vec::<String>::new());
        assert_eq!(found("present-cloud", &["infra/main.tf", "prep.sh"], CLOUD)?, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn every_missing_path_is_named_where_the_spec_mentions_it() -> Outcome {
        let missing = |at: &str, path: &str| format!("{at}: `{path}` doesn't exist in the project folder");
        let expect = [
            missing("machines.web.docker.build", "app"),
            missing("machines.web.docker.dockerfile", "app/Dockerfile"),
            missing("machines.web.docker.init[0]", "init.sh"),
            missing("machines.web.files[0].source", "conf/a.ini"),
            missing("machines.r1.docker.config", "r1.cfg"),
            missing("machines.r1.docker.firmware", "fw.bin"),
            missing("machines.vm1.vm.provision[0]", "setup.sh"),
            missing("machines.vm1.vm.image.box_url", "boxes/my.box"),
            missing("provision[0].ansible", "site.yml"),
            missing("provision[0].inventory[0]", "inv.ini"),
            missing("provision[0].requirements", "req.yml"),
            missing("checks[0]", "check.sh"),
            missing("checks[1].script", "v1..2/probe.sh"),
        ];
        assert_eq!(found("missing", &[], &lab(MACHINES))?, expect);
        Ok(())
    }

    #[test]
    fn a_path_leaving_the_project_is_refused() -> Outcome {
        let checks = "machines: { web: { networks: { lan: 10 }, docker: { image: nginx } } }\nchecks: [/etc/hosts, ../x.sh, a/../../x.sh]\n";
        let outside = |i: usize, path: &str| format!("checks[{i}]: `{path}` must be a path inside the project folder");
        assert_eq!(
            found("outside", &["a/"], &lab(checks))?,
            [outside(0, "/etc/hosts"), outside(1, "../x.sh"), outside(2, "a/../../x.sh")]
        );
        Ok(())
    }

    #[test]
    fn a_files_source_is_a_file_not_a_folder() -> Outcome {
        let machine = "machines: { web: { networks: { lan: 10 }, docker: { image: nginx }, files: [{ path: /etc/a, source: conf }] } }\n";
        assert_eq!(
            found("folder", &["conf/"], &lab(machine))?,
            ["machines.web.files[0].source: `conf` is a folder: name a file"]
        );
        Ok(())
    }

    #[test]
    fn a_cloud_module_is_a_folder_holding_terraform_files() -> Outcome {
        assert_eq!(
            found("cloud-missing", &[], CLOUD)?,
            [
                "cloud.terraform: `infra` doesn't exist in the project folder",
                "cloud.before[0]: `prep.sh` doesn't exist in the project folder"
            ]
        );
        assert_eq!(
            found("cloud-no-tf", &["infra/README.md", "prep.sh"], CLOUD)?,
            ["cloud.terraform: `infra` holds no .tf file: it must be a Terraform root module"]
        );
        Ok(())
    }
}
