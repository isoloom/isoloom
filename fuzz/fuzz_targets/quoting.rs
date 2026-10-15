//! The quoting helpers on any text: each literal (HCL, Ruby, POSIX sh, PowerShell) decodes back to
//! the text, so nothing in it ends the literal early or expands. The decoders are the quoting
//! properties' own (`crates/core/isoloom-render/tests/quoting/decode.rs`).

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../crates/core/isoloom-render/tests/quoting/decode.rs"]
mod decode;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    assert_eq!(decode::hcl_decode(&isoloom_render::hcl(text)).as_deref(), Some(text), "HCL");
    assert_eq!(decode::rb_decode(&isoloom_render::rb(text)).as_deref(), Some(text), "Ruby");
    assert_eq!(decode::sh_decode(&isoloom_spec::shell::quote(text)).as_deref(), Some(text), "sh");
    assert_eq!(decode::ps_decode(&isoloom_render::ps(text)).as_deref(), Some(text), "PowerShell");
});
