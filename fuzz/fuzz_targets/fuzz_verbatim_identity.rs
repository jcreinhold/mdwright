#![no_main]
//! Default-options identity invariants.
//!
//! Structural emit is the identity baseline: when the formatter commits
//! no rewrite, its output equals its input modulo the document-boundary
//! normalisations (single trailing `\n`, no `\r`). Default options are
//! not pure preserve, though — GFM tables default to `TableStyle::Compact`
//! — so a table-bearing source is intentionally rewritten to compact
//! normal form. The strict-identity claim is therefore gated on the
//! formatter reporting zero committed rewrites. The target's name is
//! retained for fuzz-corpus continuity.
//!
//! 1. **Idempotence (always):** `format(parse(format(parse(s),D)),D)`
//!    equals `format(parse(s),D)` — default opts must be a stable point.
//! 2. **Strict identity (gated):** when the formatter commits no rewrite
//!    *and* the input already satisfies the document-boundary
//!    normalisations (single trailing `\n`, no `\r`),
//!    `format(parse(s),D) == s`. A committed rewrite (e.g. a table
//!    compacted to normal form) is expected to change bytes; identity is
//!    asserted only when nothing was rewritten.

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
    let (once, report) = mdwright_format::format_document_with_report(&doc, &opts);
    let twice = mdwright_format::format_document(&Document::parse(&once).expect("formatter output parses"), &opts);
    assert_eq!(once, twice, "default opts not idempotent");

    // Identity holds only when no rewrite fired: default options compact
    // GFM tables (`TableStyle::Compact`), so a table-bearing source is
    // intentionally changed and `rewrite_committed > 0` records it. With
    // nothing rewritten, structural emit is the identity baseline, so a
    // source already at the document-boundary normal form round-trips.
    let canonical_boundary = s.ends_with('\n') && !s.ends_with("\n\n") && !s.contains('\r');
    if canonical_boundary && report.rewrite_committed == 0 {
        assert_eq!(once, s, "default opts changed source bytes");
    }
});
