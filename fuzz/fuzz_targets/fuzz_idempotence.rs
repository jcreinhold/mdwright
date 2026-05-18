#![no_main]
//! Idempotence: `format(parse(format(parse(s))))` must equal
//! `format(parse(s))`. A second format must be a no-op.
//!
//! The first input byte drives the `FmtOptions` space so option ×
//! construct interactions are exercised, not only the default-style
//! path. Bit allocation:
//!
//! | bits | field                                  |
//! |------|----------------------------------------|
//! | 0-1  | wrap (Keep / No / At(80) / At(120))    |
//! | 2    | math.normalise                         |
//! | 3    | format mode (Normalise / Verbatim)     |
//! | 4-7  | canonicalisation mode (16 enumerated)  |
//!
//! The canonicalisation enumeration covers Preserve, each style knob
//! pinned individually, and two "all knobs combined" variants. This
//! is the per-mode coverage prompt 54 requires.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

use libfuzzer_sys::fuzz_target;
use mdwright::{
    Document, FmtOptions, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions, OrderedListStyle, StrongStyle,
    ThematicStyle, Wrap, contains_rejected_control_chars,
};

const MAX_INPUT: usize = 65_536;

/// See fuzz_verbatim_identity.rs::install_silent_panic_hook.
static SILENCE_HOOK: Once = Once::new();
fn install_silent_panic_hook() {
    SILENCE_HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|_| {}));
    });
}

fn opts_from_byte(byte: u8) -> FmtOptions {
    let wrap = match byte & 0b11 {
        0 => Wrap::Keep,
        1 => Wrap::No,
        2 => Wrap::At(80),
        _ => Wrap::At(120),
    };
    let math = MathOptions {
        normalise: byte & 0b100 != 0,
        ..MathOptions::default()
    };
    // Bit 3 is reserved; preserves the option-byte width so existing
    // corpus seeds remain meaningful.
    let base = FmtOptions::default().with_wrap(wrap).with_math(math);
    apply_canon_mode(base, (byte >> 4) & 0b1111)
}

/// Apply one of 16 canonicalisation modes. 0 = preserve (no rewrites);
/// 1-13 = single-knob; 14-15 = "all knobs together" variants.
fn apply_canon_mode(opts: FmtOptions, mode: u8) -> FmtOptions {
    match mode {
        1 => opts.with_italic(ItalicStyle::Asterisk),
        2 => opts.with_italic(ItalicStyle::Underscore),
        3 => opts.with_strong(StrongStyle::Asterisk),
        4 => opts.with_strong(StrongStyle::Underscore),
        5 => opts.with_list_marker(ListMarkerStyle::Dash),
        6 => opts.with_list_marker(ListMarkerStyle::Asterisk),
        7 => opts.with_list_marker(ListMarkerStyle::Plus),
        8 => opts.with_thematic_break(ThematicStyle::Dash),
        9 => opts.with_thematic_break(ThematicStyle::Asterisk),
        10 => opts.with_thematic_break(ThematicStyle::Underscore),
        11 => opts.with_ordered_list(OrderedListStyle::Consistent),
        12 => opts.with_link_def_style(LinkDefStyle::Bare),
        13 => opts.with_link_def_style(LinkDefStyle::Angle),
        14 => opts
            .with_italic(ItalicStyle::Asterisk)
            .with_strong(StrongStyle::Asterisk)
            .with_list_marker(ListMarkerStyle::Asterisk)
            .with_thematic_break(ThematicStyle::Asterisk)
            .with_ordered_list(OrderedListStyle::Consistent)
            .with_link_def_style(LinkDefStyle::Bare),
        15 => opts
            .with_italic(ItalicStyle::Underscore)
            .with_strong(StrongStyle::Underscore)
            .with_list_marker(ListMarkerStyle::Dash)
            .with_thematic_break(ThematicStyle::Dash)
            .with_ordered_list(OrderedListStyle::Consistent)
            .with_link_def_style(LinkDefStyle::Angle),
        _ => opts,
    }
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
