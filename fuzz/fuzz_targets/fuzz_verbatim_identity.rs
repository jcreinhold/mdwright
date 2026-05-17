#![no_main]
//! Verbatim-mode invariants.
//!
//! 1. **Idempotence (always):** `format(parse(format(parse(s),V)),V)`
//!    equals `format(parse(s),V)` — verbatim must be a stable point.
//! 2. **Strict identity (gated):** when the input already satisfies
//!    the document-boundary normalisations (single trailing `\n`, no
//!    `\r`), `format(parse(s),V) == s` — verbatim must round-trip the
//!    source exactly.
//!
//! The gate filters out inputs the document-boundary post-processor
//! (`normalize_line_endings_lf`, `normalize_trailing_newline`) would
//! legitimately rewrite; everything past the gate exercises the
//! deeper claim that verbatim does not touch block bytes.

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions, FormatMode, contains_rejected_control_chars};

const MAX_INPUT: usize = 65_536;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    // Mirror `--reject-control-chars`: pulldown rewrites NUL → U+FFFD
    // even in verbatim mode (`Source::canonicalise` is upstream of the
    // format mode switch), so the strict identity gate is undefined.
    if contains_rejected_control_chars(s) {
        return;
    }
    let opts = FmtOptions::default().with_mode(FormatMode::Verbatim);

    let once = Document::parse(s).format(&opts);
    let twice = Document::parse(&once).format(&opts);
    assert_eq!(once, twice, "verbatim mode is not idempotent");

    // Strict identity only when the source is already in canonical
    // boundary form: ends with exactly one '\n' and contains no CR.
    let canonical_boundary =
        s.ends_with('\n') && !s.ends_with("\n\n") && !s.contains('\r');
    if canonical_boundary {
        assert_eq!(once, s, "verbatim mode changed source bytes");
    }
});
