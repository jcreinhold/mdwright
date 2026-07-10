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
//! | 3    | both heading blank-line knobs          |
//! | 4-7  | canonicalisation mode (16 enumerated)  |
//!
//! The canonicalisation enumeration covers Preserve, each style knob
//! pinned individually, and two "all knobs combined" variants, so
//! every public formatting mode is exercised.
//!
//! Bit 3 turns on both heading blank-line knobs at once, which is the
//! configuration where a single gap is claimed by both. Modes 14 and 15
//! each turn on one side alone, so bit 3 crossed with the mode nibble
//! reaches every combination of the two knobs.

use libfuzzer_sys::fuzz_target;
use mdwright_document::{Document, contains_rejected_control_chars};
use mdwright_format::{
    BlankLine, FmtOptions, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions, OrderedListStyle, StrongStyle,
    ThematicStyle, Wrap,
};

const MAX_INPUT: usize = 65_536;

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
    let mut base = FmtOptions::default().with_wrap(wrap).with_math(math);
    if byte & 0b1000 != 0 {
        base = base
            .with_blank_line_before_heading(BlankLine::One)
            .with_blank_line_after_heading(BlankLine::One);
    }
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
            .with_link_def_style(LinkDefStyle::Bare)
            .with_blank_line_before_heading(BlankLine::One),
        15 => opts
            .with_italic(ItalicStyle::Underscore)
            .with_strong(StrongStyle::Underscore)
            .with_list_marker(ListMarkerStyle::Dash)
            .with_thematic_break(ThematicStyle::Dash)
            .with_ordered_list(OrderedListStyle::Consistent)
            .with_link_def_style(LinkDefStyle::Angle)
            .with_blank_line_after_heading(BlankLine::One),
        _ => opts,
    }
}

fuzz_target!(|data: &[u8]| {
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
    let Ok(doc) = Document::parse(s) else {
        return;
    };
    let once = mdwright_format::format_document(&doc, &opts);
    let twice = mdwright_format::format_document(&Document::parse(&once).expect("formatter output parses"), &opts);
    assert_eq!(once, twice, "format is not idempotent (opt byte {option_byte:#04x})");
});
