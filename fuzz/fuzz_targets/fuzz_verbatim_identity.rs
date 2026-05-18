#![no_main]
//! Default-options identity invariants.
//!
//! Structural emit is identity: with every style knob at its
//! `Preserve` default and `Wrap::Keep`, the formatter's output equals
//! its input modulo the document-boundary normalisations (single
//! trailing `\n`, no `\r`). The target's name is retained for
//! fuzz-corpus continuity.
//!
//! 1. **Idempotence (always):** `format(parse(format(parse(s),D)),D)`
//!    equals `format(parse(s),D)` — default opts must be a stable point.
//! 2. **Strict identity (gated):** when the input already satisfies
//!    the document-boundary normalisations (single trailing `\n`, no
//!    `\r`), `format(parse(s),D) == s` — default opts must round-trip
//!    the source exactly.

use libfuzzer_sys::fuzz_target;
use mdwright_document::{Document, contains_rejected_control_chars};
use mdwright_format::FmtOptions;

const MAX_INPUT: usize = 65_536;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if contains_rejected_control_chars(s) {
        return;
    }
    let opts = FmtOptions::default();

    let Ok(doc) = Document::parse(s) else {
        return;
    };
    let once = mdwright_format::format_document(&doc, &opts);
    let twice =
        mdwright_format::format_document(&Document::parse(&once).expect("formatter output parses"), &opts);
    assert_eq!(once, twice, "default opts not idempotent");

    let canonical_boundary = s.ends_with('\n') && !s.ends_with("\n\n") && !s.contains('\r');
    if canonical_boundary {
        assert_eq!(once, s, "default opts changed source bytes");
    }
});
