#![no_main]
//! Parse + format the input. Asserts no panics on arbitrary UTF-8.

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions};

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let doc = Document::parse(s);
    let _ = doc.format(&FmtOptions::default());
});
