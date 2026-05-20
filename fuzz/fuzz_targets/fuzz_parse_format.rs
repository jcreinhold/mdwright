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
//!
//! The first input byte drives `FmtOptions` (same bit allocation as
//! `fuzz_idempotence`): so each fuzz iteration exercises a different
//! point in the wrap × mode × math × canonicalisation space.

use libfuzzer_sys::fuzz_target;
use mdwright_document::{Document, contains_rejected_control_chars, markdown_signature};
use mdwright_format::{
    FmtOptions, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions, OrderedListStyle, StrongStyle, ThematicStyle,
    Wrap,
};

/// Per-iter input cap: 64 KiB. Larger inputs eat fuzz budget without
/// reaching deeper structural coverage; the CLI enforces the same
/// shape via `--max-input-bytes`.
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
    // Bit 3 is reserved; preserves the option-byte width so existing
    // corpus seeds remain meaningful.
    let base = FmtOptions::default().with_wrap(wrap).with_math(math);
    apply_canon_mode(base, (byte >> 4) & 0b1111)
}

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
    if data.len() > MAX_INPUT {
        return;
    }
    let Some((&option_byte, rest)) = data.split_first() else {
        return;
    };
    let Ok(s) = std::str::from_utf8(rest) else {
        return;
    };
    // Mirror `--reject-control-chars`: pulldown rewrites NUL → U+FFFD
    // and accepts other C0 controls verbatim, both of which make the
    // gate undefined on these inputs. Skip rather than spend budget.
    if contains_rejected_control_chars(s) {
        return;
    }
    let opts = opts_from_byte(option_byte);
    let Ok(doc) = Document::parse(s) else {
        return;
    };
    let Ok(source_sig) = markdown_signature(s, doc.parse_options()) else {
        return;
    };
    let formatted = mdwright_format::format_document(&doc, &opts);
    let formatted_sig = markdown_signature(&formatted, doc.parse_options()).expect("formatter output parses");
    let equivalent = source_sig == formatted_sig;
    assert!(equivalent, "format changes meaning (opt byte {option_byte:#04x})");
});
