//! What the unit tests share: environments planned for the `docker` target from YAML, and the
//! project's examples.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_spec::Spec;
use isoloom_spec::model::Target;

/// The environment of the spec `yaml`, planned for the `docker` target.
pub(crate) fn environment(yaml: &str) -> Environment {
    let spec = isoloom_spec::parse(yaml).expect("the test's spec parses");
    Environment::build(&spec, Target::Docker, None)
}

/// One of the project's examples.
pub(crate) struct Example {
    /// Its folder's name.
    pub(crate) name: String,
    /// Its spec.
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
            spec: isoloom_spec::load(&p).expect("the example loads"),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
