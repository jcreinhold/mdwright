#![no_main]
//! Parse and render TeX math bodies as Unicode. The oracle is
//! no-panic: malformed or unsupported input must return typed errors.

use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 65_536;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    let _rendered = mdwright_latex::render_unicode_math(source);
});
