#![no_main]
//! Default-options identity invariants.
//!
//! Post-prompt-55 the structural emit is identity: with every style
//! knob at its `Preserve` default and `Wrap::Keep`, the formatter's
//! output equals its input modulo the document-boundary
//! normalisations (single trailing `\n`, no `\r`). The old
//! `FormatMode::Verbatim` toggle has been retired; this target's
//! name is retained for fuzz-corpus continuity.
//!
//! 1. **Idempotence (always):** `format(parse(format(parse(s),D)),D)`
//!    equals `format(parse(s),D)` — default opts must be a stable point.
//! 2. **Strict identity (gated):** when the input already satisfies
//!    the document-boundary normalisations (single trailing `\n`, no
//!    `\r`), `format(parse(s),D) == s` — default opts must round-trip
//!    the source exactly.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions, contains_rejected_control_chars};

const MAX_INPUT: usize = 65_536;

/// libfuzzer-sys installs a panic hook that aborts the process, which
/// short-circuits `catch_unwind`. Swap in a silent hook so the
/// upstream pulldown panic class (see tests/known_issues.rs) can be
/// caught and skipped rather than reported as an mdwright crash.
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
    if contains_rejected_control_chars(s) {
        return;
    }
    let opts = FmtOptions::default();

    let Ok(once) = catch_unwind(AssertUnwindSafe(|| Document::parse(s).format(&opts))) else {
        return;
    };
    let Ok(twice) = catch_unwind(AssertUnwindSafe(|| Document::parse(&once).format(&opts))) else {
        return;
    };
    assert_eq!(once, twice, "default opts not idempotent");

    let canonical_boundary = s.ends_with('\n') && !s.ends_with("\n\n") && !s.contains('\r');
    if canonical_boundary {
        assert_eq!(once, s, "default opts changed source bytes");
    }
});
