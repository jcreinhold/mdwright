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

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions, contains_rejected_control_chars, semantically_equivalent};

/// Per-iter input cap: 64 KiB. Larger inputs eat fuzz budget without
/// reaching deeper structural coverage; the CLI enforces the same
/// shape via `--max-input-bytes`.
const MAX_INPUT: usize = 65_536;

/// See fuzz_verbatim_identity.rs::install_silent_panic_hook.
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
    // and accepts other C0 controls verbatim, both of which make the
    // gate undefined on these inputs. Skip rather than spend budget.
    if contains_rejected_control_chars(s) {
        return;
    }
    // Upstream pulldown-cmark panics on some inputs (see
    // tests/known_issues.rs); the oracle is undefined when parse
    // diverges, so swallow + skip rather than report a libFuzzer
    // crash for an upstream bug.
    let Ok(formatted) = catch_unwind(AssertUnwindSafe(|| {
        Document::parse(s).format(&FmtOptions::default())
    })) else {
        return;
    };
    let Ok(equivalent) = catch_unwind(AssertUnwindSafe(|| semantically_equivalent(s, &formatted))) else {
        return;
    };
    assert!(equivalent, "format changes meaning");
});
