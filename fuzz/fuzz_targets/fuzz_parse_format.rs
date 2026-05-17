#![no_main]
//! Semantic-equivalence oracle: format must not change the
//! document's meaning.
//!
//! Mirrors the `html_preserving` property in `tests/properties.rs`
//! and the `format_validated` CLI gate — bugs that silently change
//! meaning (drop bytes, reinterpret a construct, etc.) trip this
//! even when no panic occurs. Equivalence is defined on the
//! canonicalised pulldown-cmark event stream, not byte-equal
//! rendered HTML, so the oracle accepts well-behaved prose rewraps
//! and rejects only real semantic drift.

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions, contains_rejected_control_chars, semantically_equivalent};

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
    // Mirror `--reject-control-chars`: pulldown rewrites NUL → U+FFFD
    // and accepts other C0 controls verbatim, both of which make the
    // gate undefined on these inputs. Skip rather than spend budget.
    if contains_rejected_control_chars(s) {
        return;
    }
    let formatted = Document::parse(s).format(&FmtOptions::default());
    assert!(
        semantically_equivalent(s, &formatted),
        "format changes meaning"
    );
});
