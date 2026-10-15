//! The model's spec parser on any text (VLANs, clones, groups and OS names unfolded on the way):
//! a spec or an error, and validation of any spec that parsed, never a panic.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data)
        && let Ok(spec) = isoloom_spec::parse(text)
    {
        let _problems = isoloom_spec::validate(&spec);
    }
});
