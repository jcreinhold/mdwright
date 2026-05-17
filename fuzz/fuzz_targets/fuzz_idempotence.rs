#![no_main]
//! Idempotence: `format(parse(format(parse(s))))` must equal
//! `format(parse(s))`. A second format must be a no-op.
//!
//! The first input byte drives the `FmtOptions` space (wrap × mode ×
//! math.normalise) so option × construct interactions are exercised,
//! not only the default-style path.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions, FormatMode, MathOptions, Wrap, contains_rejected_control_chars};

const MAX_INPUT: usize = 65_536;

/// See fuzz_verbatim_identity.rs::install_silent_panic_hook.
static SILENCE_HOOK: Once = Once::new();
fn install_silent_panic_hook() {
    SILENCE_HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|_| {}));
    });
}

fn opts_from_byte(byte: u8) -> FmtOptions {
    // Rotate wrap across the full design space: keep, flatten, and
    // three column targets that bracket common doc-budget choices.
    // `At(120)` is in the rotation specifically to exercise the
    // atomicity contract — table rows, ATX heading bodies, fenced
    // info strings must stay on one line at every budget.
    let wrap = match byte & 0b111 {
        0 => Wrap::Keep,
        1 => Wrap::No,
        2 => Wrap::At(60),
        3 => Wrap::At(80),
        _ => Wrap::At(120),
    };
    let mode = if byte & 0b100 != 0 {
        FormatMode::Verbatim
    } else {
        FormatMode::Normalise
    };
    let math = MathOptions {
        normalise: byte & 0b1000 != 0,
    };
    FmtOptions::default()
        .with_wrap(wrap)
        .with_mode(mode)
        .with_math(math)
}

fuzz_target!(|data: &[u8]| {
    install_silent_panic_hook();
    if data.len() > MAX_INPUT {
        return;
    }
    let Some((&option_byte, rest)) = data.split_first() else {
        return;
    };
    let Ok(s) = std::str::from_utf8(rest) else {
        return;
    };
    // Skip C0-control inputs: pulldown's NUL→U+FFFD rewrite means
    // `parse(parse_back(s))` isn't a fixed point, so the oracle is
    // ill-typed on them. Mirrors CLI `--reject-control-chars`.
    if contains_rejected_control_chars(s) {
        return;
    }
    let opts = opts_from_byte(option_byte);
    // Upstream pulldown-cmark panics on some inputs (see
    // tests/known_issues.rs); swallow + skip so the oracle isn't
    // tripped by an upstream bug.
    let Ok(once) = catch_unwind(AssertUnwindSafe(|| Document::parse(s).format(&opts))) else {
        return;
    };
    let Ok(twice) = catch_unwind(AssertUnwindSafe(|| Document::parse(&once).format(&opts))) else {
        return;
    };
    assert_eq!(once, twice, "format is not idempotent (opt byte {option_byte:#04x})");
});
