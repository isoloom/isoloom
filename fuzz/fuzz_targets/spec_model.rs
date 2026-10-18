//! The spec pipeline end to end on any spec that parses (the parser alone is `spec_syntax`'s): the
//! lab's own checks, then the spec lowered for every target the way `isoloom generate` plans it,
//! each target's refusal, and the validation of each lowered model, never a panic.

#![no_main]

use isoloom_plan::Lab;
use isoloom_spec::model::Target;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let Ok(spec) = isoloom_spec::parse(text) else { return };
    let lab = Lab::new(spec);
    let _problems = lab.problems();
    let _possible = lab.possible();
    for target in Target::ALL {
        let _refusal = lab.refusal(target);
        let _problems = isoloom_spec::validate(&lab.for_target(target));
    }
});
