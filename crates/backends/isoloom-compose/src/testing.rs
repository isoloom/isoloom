//! What the unit tests share: environments planned for the `docker` target from YAML, and the
//! project's examples.

use std::path::Path;

use isoloom_plan::{Environment, Lab};
use isoloom_spec::model::{Spec, Target};

/// The environment of the spec `yaml` (the authored format), planned for the `docker` target.
pub(crate) fn environment(yaml: &str) -> Environment {
    let lab = Lab::parse(yaml).expect("the test's spec parses");
    Environment::build(&lab.for_target(Target::Docker), Target::Docker, None)
}

/// One of the project's examples.
pub(crate) struct Example {
    /// Its folder's name.
    pub(crate) name: String,
    /// Its spec, lowered for the `docker` target.
    pub(crate) spec: Spec,
}

/// Every example of the project (`examples/`), by name.
pub(crate) fn examples() -> Vec<Example> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
    let mut out: Vec<Example> = std::fs::read_dir(root)
        .expect("the examples folder")
        .map(|e| e.expect("an example's entry").path())
        .filter(|p| p.join("isoloom.yml").is_file())
        .map(|p| Example {
            name: p.file_name().expect("a folder name").to_string_lossy().into_owned(),
            spec: Lab::new(isoloom_spec::load(&p).expect("the example loads")).for_target(Target::Docker),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
