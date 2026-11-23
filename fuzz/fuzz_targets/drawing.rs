//! The compiler on any drawing the editor could send: the JSON is a drawing or an error, and a
//! drawing compiles or says what is wrong with it, never with what the compiler wrote; the lab's
//! own files stay inside the project and out of Isoloom's, each once, each ending with a line break.

#![no_main]

use isoloom_spec::ignore::{OUTPUTS, Rules};
use isoloom_spec::path::RelPath;
use isoloom_topology::{Error, Topology, compile};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Isoloom's outputs (`.isoloom/`, an instance's `.isoloom-<n>/`, any `.isoloom*` at the top).
    let outputs = Rules::parse(OUTPUTS);
    let Ok(topology) = serde_json::from_slice::<Topology>(data) else { return };
    match compile(&topology) {
        Ok(compiled) => {
            let (_, _, files) = compiled.into_parts();
            for (i, file) in files.iter().enumerate() {
                // Isoloom's rule for a path inside the project: relative, `/`-separated, no `..`
                // segment (`a..b.sh` stays inside).
                assert!(RelPath::parse(&file.path).is_ok(), "a file outside the project: {}", file.path);
                assert!(
                    !isoloom_spec::SPEC_FILES.contains(&file.path.as_str()) && !outputs.ignored(&file.path),
                    "a file Isoloom reads the spec from or writes: {}",
                    file.path
                );
                assert!(file.contents.ends_with('\n'), "{} doesn't end with a line break", file.path);
                assert!(files.iter().skip(i + 1).all(|f| f.path != file.path), "{} twice", file.path);
            }
        }
        Err(e @ (Error::Unreadable(_) | Error::PortName { .. } | Error::Yaml(_))) => {
            panic!("the compiler wrote what Isoloom doesn't read: {e}");
        }
        Err(_) => {}
    }
});
