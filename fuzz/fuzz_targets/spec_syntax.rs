//! The spec format's parser on any text: a spec or its problems, and the checks on any spec that
//! parsed, never a panic.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data)
        && let Ok(spec) = isoloom_spec::syntax::parse(text)
    {
        let _problems = isoloom_spec::syntax::check(&spec);
    }
});
