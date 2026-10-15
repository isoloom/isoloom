//! What a machine runs from on a target: a container image (published, or built from the
//! project's Dockerfile or from the steps Isoloom turns into one) or a VM's OS.

use indexmap::IndexMap;
use isoloom_spec::model::{Machine, Spec};

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
    /// A Dockerfile Isoloom generates from the machine's portable steps (`app.steps`), built
    /// with the project folder as its context.
    Generated {
        /// Where the generated Dockerfile is written, from the project folder
        /// (`.isoloom/docker/build/<machine>/Dockerfile`).
        dockerfile: String,
        /// The image and steps it is generated from.
        recipe: isoloom_spec::model::Generated,
    },
}

/// Where a machine's generated Dockerfile is written, from the project folder.
#[must_use]
pub fn generated_dockerfile(machine: &str) -> String {
    format!(".isoloom/docker/build/{machine}/Dockerfile")
}

/// What machine `name` runs from on the target, in `shape`.
pub(super) fn of(spec: &Spec, shape: MachineShape, name: &str, m: &Machine) -> Option<Image> {
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
            let generated = d.generated.as_ref().map(|recipe| Build::Generated {
                dockerfile: generated_dockerfile(name),
                recipe: recipe.clone(),
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
