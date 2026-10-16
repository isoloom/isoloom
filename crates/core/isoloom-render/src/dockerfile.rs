//! The Dockerfile Isoloom generates from a machine's recipe (`app.steps`): `FROM` its OS's
//! official image (pinned by digest), the files the steps use copied to the project folder
//! (`/opt/isoloom`, the same path a VM has), then each step. The project folder is the build
//! context; only the paths the steps name are copied.

use isoloom_plan::configure::{Recipe, Resource};
use isoloom_plan::{Build, Environment, Image, MachineShape};
use isoloom_spec::os::by_name;

use crate::steps::PROJECT_DIR;
use crate::{GeneratedFile, header};

/// The generated Dockerfiles of `env`'s containers, each at its path, its base image pulled
/// through the registry mirrors.
#[must_use]
pub fn files(env: &Environment) -> Vec<GeneratedFile> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Container)
        .filter_map(|(name, mp)| match (&mp.image, &mp.recipe) {
            (
                Some(Image::Container {
                    build: Some(Build::Generated { dockerfile, base, .. }),
                    ..
                }),
                Some(recipe),
            ) => Some(GeneratedFile {
                path: dockerfile.clone(),
                contents: render(name, &env.images.pull(base), recipe),
            }),
            _ => None,
        })
        .collect()
}

/// The Dockerfile of machine `machine`'s generated image, from `base`.
#[must_use]
pub fn render(machine: &str, base: &str, recipe: &Recipe) -> String {
    let installer = Installer::of(recipe.os.name());
    let steps: String = recipe.resources().map(|r| instructions(installer, r)).collect();
    format!(
        "{}# The container form of machine `{machine}`: its OS's image and its steps.\nFROM {base}\nWORKDIR {PROJECT_DIR}\n{steps}",
        header("#")
    )
}

/// One step's instructions.
fn instructions(installer: Installer, step: &Resource) -> String {
    match step {
        Resource::Run(run) => {
            let script = &run.script;
            let line = match &run.creates {
                Some(path) => format!("[ -e {path} ] || sh {script}"),
                None => format!("sh {script}"),
            };
            format!("COPY {script} {script}\nRUN {line}\n")
        }
        Resource::Copy(c) => {
            let owner = c.owner.as_ref().map(|o| format!("--chown={o} ")).unwrap_or_default();
            let mode = c.mode.map(|m| format!("--chmod={m} ")).unwrap_or_default();
            format!("COPY {owner}{mode}{} {}\n", c.from, c.to)
        }
        // A pinned version waits for the configuration renderers: lowering refuses it in a
        // container until then, so only names reach this line.
        Resource::Packages(packages) => {
            let names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
            format!("{}\n", installer.install(&names.join(" ")))
        }
        // A reboot or a playbook has no container form: lowering refuses it before a generated
        // image, and so does any step that comes with no container form.
        _ => String::new(),
    }
}

/// How an OS family installs packages in its image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Installer {
    /// Alpine's `apk`.
    Apk,
    /// The RHEL family's `dnf` (Rocky, AlmaLinux, Fedora).
    Dnf,
    /// CentOS 7's `yum`.
    Yum,
    /// Debian's and Ubuntu's `apt-get`, and Kali's.
    Apt,
}

impl Installer {
    /// The installer of the image-table OS `os`.
    fn of(os: &str) -> Installer {
        match by_name(os).map(|o| o.family) {
            Some("alpine") => Installer::Apk,
            Some("rocky" | "alma" | "fedora") => Installer::Dnf,
            Some("centos") => Installer::Yum,
            _ => Installer::Apt,
        }
    }

    /// The `RUN` line installing `packages`, its caches cleaned. Packages carry no version in
    /// the spec, so the pinning rules are waived on this line.
    fn install(self, packages: &str) -> String {
        match self {
            Installer::Apk => format!("# hadolint ignore=DL3018\nRUN apk add --no-cache {packages}"),
            Installer::Dnf => format!("# hadolint ignore=DL3041\nRUN dnf install -y {packages} && dnf clean all"),
            Installer::Yum => format!("# hadolint ignore=DL3033\nRUN yum install -y {packages} && yum clean all"),
            Installer::Apt => format!(
                "# hadolint ignore=DL3008\nRUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends {packages} && rm -rf /var/lib/apt/lists/*"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use isoloom_plan::configure::{DaemonTable, recipe};
    use isoloom_spec::syntax;

    use super::*;

    /// The Dockerfile of a machine `{ os: <os>, app: { steps: <steps> } }`.
    fn render(os: &str, steps: &str) -> String {
        let m: syntax::Machine = serde_yaml_ng::from_str(&format!("{{ os: {os}, app: {{ steps: {steps} }} }}")).unwrap();
        let r = recipe(m.os.unwrap(), &m, &DaemonTable::common()).unwrap();
        super::render("web", isoloom_spec::os::container_image(m.os.unwrap().name()).unwrap_or_default(), &r)
    }

    #[test]
    fn steps_become_instructions_from_the_os_image() {
        let d = render(
            "debian@12",
            "[{ packages: [nginx] }, { copy: { from: site/, to: /var/www/, owner: www-data, mode: \"0755\" } }, { run: provision/web.sh }]",
        );
        assert!(d.starts_with("# Generated by isoloom"));
        assert!(d.contains("# The container form of machine `web`: its OS's image and its steps.\nFROM "), "{d}");
        assert!(d.contains("FROM debian:12@sha256:"));
        assert!(d.contains("WORKDIR /opt/isoloom\n"));
        assert!(d.contains("apt-get install -y --no-install-recommends nginx && rm -rf /var/lib/apt/lists/*"));
        assert!(d.contains("COPY --chown=www-data --chmod=0755 site/ /var/www/\n"));
        assert!(d.contains("COPY provision/web.sh provision/web.sh\nRUN sh provision/web.sh\n"));
    }

    #[test]
    fn each_family_installs_its_way() {
        let curl = "[{ packages: [curl] }]";
        assert!(render("alpine@3.19", curl).contains("RUN apk add --no-cache curl"));
        assert!(render("rocky@9", curl).contains("RUN dnf install -y curl && dnf clean all"));
        assert!(render("centos@7", curl).contains("# hadolint ignore=DL3033\nRUN yum install -y curl && yum clean all\n"));
        assert!(render("ubuntu@24.04", curl).contains("apt-get install -y --no-install-recommends curl"));
        assert!(render("debian@12", "[{ run: a.sh, creates: /done }]").contains("RUN [ -e /done ] || sh a.sh"));
    }

    #[test]
    fn each_container_built_from_steps_gets_its_dockerfile() {
        let yaml = "version: 1\nname: d\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12, app: { steps: [{ packages: [nginx] }] } }\n  img: { networks: { lan: 11 }, app: { image: nginx } }\n";
        let built = files(&crate::testing::env(yaml, isoloom_spec::model::Target::Docker));
        assert_eq!(
            built.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            [".isoloom/docker/build/web/Dockerfile"]
        );
        assert!(built[0].contents.contains("FROM debian:12@sha256:") && built[0].contents.contains("install -y --no-install-recommends nginx"));
        assert_eq!(
            files(&crate::testing::env(yaml, isoloom_spec::model::Target::Vagrant)),
            [],
            "a VM sets itself up"
        );
    }
}
