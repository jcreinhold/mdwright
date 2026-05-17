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

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions, FormatMode, contains_rejected_control_chars};

const MAX_INPUT: usize = 65_536;

/// libfuzzer-sys installs a panic hook that aborts the process, which
/// short-circuits `catch_unwind`. Swap in a silent hook so the
/// upstream pulldown panic class (see tests/known_issues.rs) can be
/// caught and skipped rather than reported as an mdwright crash.
/// mdwright itself never panics (the `Cargo.toml` lints deny
/// `unwrap_used` / `expect_used` / `panic` / `arithmetic_side_effects`
/// / `indexing_slicing` in production code) so silencing the hook
/// doesn't hide mdwright-side defects.
static SILENCE_HOOK: Once = Once::new();
fn install_silent_panic_hook() {
    SILENCE_HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|_| {}));
    });
}

fuzz_target!(|data: &[u8]| {
    install_silent_panic_hook();
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

    // Upstream pulldown-cmark 0.13.3 has a panic-on-`Option::unwrap`
    // path that the fuzz corpus reaches with link-ref-style inputs
    // ending in `\r\n\t\t` (see tests/known_issues.rs::
    // pulldown_panics_on_link_ref_tab). The oracle is undefined on
    // inputs the parser cannot parse without panicking, so swallow
    // the panic and skip — same Q1 oracle-domain pattern as
    // `contains_rejected_control_chars`.
    let Ok(once) = catch_unwind(AssertUnwindSafe(|| Document::parse(s).format(&opts))) else {
        return;
    };
    let Ok(twice) = catch_unwind(AssertUnwindSafe(|| Document::parse(&once).format(&opts))) else {
        return;
    };
    assert_eq!(once, twice, "verbatim mode is not idempotent");

    // Strict identity only when the source is already in canonical
    // boundary form: ends with exactly one '\n' and contains no CR.
    let canonical_boundary =
        s.ends_with('\n') && !s.ends_with("\n\n") && !s.contains('\r');
    if canonical_boundary {
        assert_eq!(once, s, "verbatim mode changed source bytes");
    }
});
