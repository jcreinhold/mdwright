#![no_main]
//! Run every standard-library lint rule on the input. Asserts no
//! panics; lint paths touch raw bytes via prose-chunk iteration, a
//! different code path from format.

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, RuleSet};

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let rules = RuleSet::stdlib_all();
    let _ = Document::parse(s).lint(&rules);
});
