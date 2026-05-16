#![no_main]
//! HTML-equivalence oracle: format must not change the rendered HTML.
//!
//! Mirrors property 2 of `tests/properties.rs` and the
//! `format_validated` CLI gate — bugs that silently change meaning
//! (drop bytes, reinterpret a construct, etc.) trip this even when no
//! panic occurs.

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions, render_html};

/// Per-iter input cap: 64 KiB. Larger inputs eat fuzz budget without
/// reaching deeper structural coverage; the CLI enforces the same
/// shape via `--max-input-bytes`.
const MAX_INPUT: usize = 65_536;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let formatted = Document::parse(s).format(&FmtOptions::default());
    let before = render_html(s);
    let after = render_html(&formatted);
    assert_eq!(before, after, "format changes HTML meaning");
});
