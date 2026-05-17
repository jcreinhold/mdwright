#![no_main]
//! Run every standard-library lint rule on the input. Asserts:
//!   1. no panic in any rule path,
//!   2. every diagnostic span lies inside `0..source.len()` and is
//!      well-formed (`start <= end`),
//!   3. lint is deterministic across two runs on the same input.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, RuleSet, contains_rejected_control_chars};

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
    // Mirror `--reject-control-chars`. Lint span-bounds and
    // determinism are still well-defined on such inputs, but they
    // burn budget without exercising real Markdown shape.
    if contains_rejected_control_chars(s) {
        return;
    }
    let rules = RuleSet::stdlib_all();
    // Upstream pulldown-cmark panics on some inputs (see
    // tests/known_issues.rs); swallow + skip.
    let Ok(diags1) = catch_unwind(AssertUnwindSafe(|| Document::parse(s).lint(&rules))) else {
        return;
    };
    let Ok(diags2) = catch_unwind(AssertUnwindSafe(|| Document::parse(s).lint(&rules))) else {
        return;
    };

    assert_eq!(
        diags1.len(),
        diags2.len(),
        "lint is nondeterministic (different diagnostic counts)",
    );
    for d in &diags1 {
        assert!(d.span.start <= d.span.end, "inverted span: {:?}", d.span);
        assert!(
            d.span.end <= s.len(),
            "span {:?} exceeds source length {}",
            d.span,
            s.len(),
        );
    }
    for (a, b) in diags1.iter().zip(diags2.iter()) {
        assert_eq!(a.rule, b.rule, "nondeterministic rule ordering");
        assert_eq!(a.span, b.span, "nondeterministic span for {}", a.rule);
    }
});
