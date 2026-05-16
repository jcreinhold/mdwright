#![no_main]
//! Idempotence: `format(parse(format(parse(s))))` must equal
//! `format(parse(s))`. A second format must be a no-op.

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions};

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let opts = FmtOptions::default();
    let once = Document::parse(s).format(&opts);
    let twice = Document::parse(&once).format(&opts);
    assert_eq!(once, twice, "format is not idempotent on input");
});
