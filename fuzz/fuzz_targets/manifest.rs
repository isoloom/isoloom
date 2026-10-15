//! The run manifest's parser on any text: a manifest or an error, never a panic; and a manifest
//! that parsed is written back and reads the same.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let Ok(manifest) = isoloom_manifest::parse(text) else { return };
    let written = isoloom_manifest::to_json(&manifest);
    assert!(written.is_ok(), "a manifest that parsed couldn't be written: {written:?}");
    let read_back = written.map(|json| isoloom_manifest::parse(&json));
    assert!(
        matches!(&read_back, Ok(Ok(again)) if *again == manifest),
        "a written manifest reads back differently: {read_back:?}"
    );
});
