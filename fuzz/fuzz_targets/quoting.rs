//! The quoting builders on any text: each literal (HCL through the builder, Ruby, POSIX sh,
//! PowerShell) reads back as the text, so nothing in it ends the literal early or expands. The
//! literals and decoders are the quoting properties' own
//! (`crates/core/isoloom-render/tests/quoting/literals.rs`, `tests/quoting.rs`).

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../crates/core/isoloom-render/tests/quoting/literals.rs"]
mod literals;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let hcl = literals::hcl_string(text).expect("the HCL builder writes any text");
    assert_eq!(literals::hcl_decode(&hcl).as_deref(), Some(text), "HCL");
    assert_eq!(literals::rb_decode(&isoloom_render::ruby::quote(text)).as_deref(), Some(text), "Ruby");
    assert_eq!(literals::sh_decode(&isoloom_spec::shell::quote(text)).as_deref(), Some(text), "sh");
    assert_eq!(
        literals::ps_decode(&isoloom_render::powershell::quote(text)).as_deref(),
        Some(text),
        "PowerShell"
    );
});
