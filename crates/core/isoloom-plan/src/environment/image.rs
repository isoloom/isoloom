//! What a machine runs from on a target: a container image (published, or built from the
//! project's Dockerfile or from the steps Isoloom turns into one) or a VM's OS.

use indexmap::IndexMap;
use isoloom_spec::model::{Machine, Spec};

use crate::configure::{ContainerProcess, Recipe};

use super::MachineShape;
use crate::images::{Images, tag_of};

/// What a machine runs from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// A container image.
    Container {
        /// The reference to run (pulled through a mirror when one is set; a built image's tag).
        reference: String,
        /// How the project builds it, if it does.
        build: Option<Build>,
    },
    /// A VM's operating system.
    Vm {
        /// The OS id (`ubuntu-24.04`, `windows-2022`...).
        os: String,
    },
}

/// A container image the project builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Build {
    /// The project's own Dockerfile (`app.build`).
    Own {
        /// The build context, from the project folder.
        context: String,
        /// The Dockerfile, from the project folder, when not the context's own.
        dockerfile: Option<String>,
        /// The Dockerfile's stage to build, when not its last.
        target: Option<String>,
        /// The spec's build arguments.
        args: IndexMap<String, String>,
    },
    /// A Dockerfile Isoloom generates from the machine's recipe ([`super::MachinePlan::recipe`]),
    /// built with the project folder as its context.
    Generated {
        /// Where the generated Dockerfile is written, from the project folder
        /// (`.isoloom/docker/build/<machine>/Dockerfile`).
        dockerfile: String,
        /// The image it starts `FROM`: the OS's official image, pinned by digest.
        base: String,
        /// What the container runs: Isoloom's runner as its entrypoint for its services, its own
        /// command, or nothing (idle).
        process: ContainerProcess,
    },
}

/// Where a machine's generated Dockerfile is written, from the project folder.
#[must_use]
pub(crate) fn generated_dockerfile(machine: &str) -> String {
    format!(".isoloom/docker/build/{machine}/Dockerfile")
}

/// What machine `name` runs from on the target, in `shape`, its configuration resolved as
/// `recipe`.
pub(super) fn of(spec: &Spec, shape: MachineShape, name: &str, m: &Machine, recipe: Option<&Recipe>) -> Option<Image> {
    match shape {
        MachineShape::Container => {
            let d = m.docker.as_ref()?;
            let tag = tag_of(spec, name, d.image.as_deref());
            let own = d.build.as_ref().map(|context| Build::Own {
                context: context.clone(),
                dockerfile: d.dockerfile.clone(),
                target: d.target.clone(),
                args: d.args.clone(),
            });
            let generated = d.generated.as_ref().map(|g| Build::Generated {
                dockerfile: generated_dockerfile(name),
                base: g.base.clone(),
                // Lowering refuses a container whose services it can't run, so this resolves.
                process: recipe
                    .and_then(|r| ContainerProcess::resolve(r, m.command.as_ref(), !m.ports.is_empty()).ok())
                    .unwrap_or(ContainerProcess::Idle),
            });
            Some(Image::Container {
                reference: Images::of(spec).pull(&tag),
                build: own.or(generated),
            })
        }
        MachineShape::Vm => m.vm.as_ref().map(|vm| Image::Vm { os: vm.os.clone() }),
        MachineShape::Supplied | MachineShape::External => None,
    }
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::{Build, Image};
    use crate::configure::ContainerProcess;
    use crate::testing;

    #[test]
    fn a_container_runs_its_image_its_build_or_the_one_isoloom_generates_and_a_vm_its_os() {
        let env = testing::environment(
            "version: 1\nname: lab\nmachines:\n  pub: { app: { image: nginx } }\n  own: { app: { build: { context: web, dockerfile: web/Dockerfile.dev, target: prod, args: { A: '1' } } } }\n  gen: { os: debian@12, app: { steps: [{ packages: [curl] }] } }\n  user: { access: true }\n",
            Target::Docker,
        );
        let image = |m: &str| env.machines.get(m).and_then(|m| m.image.clone());
        assert_eq!(
            image("pub"),
            Some(Image::Container {
                reference: "nginx".into(),
                build: None
            })
        );
        let Some(Image::Container {
            reference,
            build: Some(Build::Own {
                context,
                dockerfile,
                target,
                args,
            }),
        }) = image("own")
        else {
            panic!("a build of the project's own")
        };
        assert_eq!(
            (reference.as_str(), context.as_str(), dockerfile.as_deref(), target.as_deref()),
            ("isoloom/lab-own", "web", Some("web/Dockerfile.dev"), Some("prod"))
        );
        assert_eq!(args.get("A").map(String::as_str), Some("1"));
        let Some(Image::Container {
            reference: generated,
            build: Some(Build::Generated {
                dockerfile: written,
                base,
                process,
            }),
        }) = image("gen")
        else {
            panic!("a generated build")
        };
        assert_eq!(
            (generated.as_str(), written.as_str()),
            ("isoloom/lab-gen", ".isoloom/docker/build/gen/Dockerfile")
        );
        assert!(base.starts_with("debian:12@sha256:"));
        assert_eq!(process, ContainerProcess::Idle, "nothing to run: kept idle");
        assert_eq!(image("user"), None, "a supplied machine runs from nothing of ours");
        let vm = testing::environment("version: 1\nname: lab\nmachines:\n  m: { os: debian@12 }\n", Target::Vagrant);
        assert_eq!(vm.machines.get("m").and_then(|m| m.image.clone()), Some(Image::Vm { os: "debian-12".into() }));
    }
}
