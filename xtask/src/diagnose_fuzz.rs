//! `cargo xtask diagnose-fuzz <artifact>` — explain a libFuzzer crash artifact.
//!
//! The fuzz harnesses encode `FmtOptions` in the first input byte
//! (bit layout documented in `fuzz/fuzz_targets/fuzz_parse_format.rs`),
//! so a crash artifact's bytes alone aren't enough to reproduce the
//! failure by hand. This module replays the artifact the way the fuzz
//! target does, then surfaces the divergence summary produced by
//! [`mdwright::FormatError::SemanticDivergence`].
//!
//! Output shape:
//!
//! ```text
//! artifact: fuzz/artifacts/fuzz_parse_format/crash-…
//! option_byte: 0x20 (Wrap::Keep, italic=Underscore, no math normalise)
//! input (5 bytes): "# # #"
//! ----
//! formatter output:
//! # #
//! ----
//! divergence: event 1: source = Text("#"); formatted = End(Heading(H1))
//! ```
//!
//! The diagnostic doesn't attempt to identify the IR construct at
//! fault — that judgement belongs to the human reading the divergence
//! summary plus the source bytes.

use std::path::Path;

use anyhow::{Context, Result};
use mdwright::{
    Document, FmtOptions, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions, OrderedListStyle, StrongStyle,
    ThematicStyle, Wrap, first_divergence,
};

/// Structured diagnosis produced by [`diagnose`].
pub struct Diagnosis {
    pub option_byte: u8,
    pub opts_summary: String,
    pub input: Vec<u8>,
    pub input_utf8: Option<String>,
    /// `Some` when `format_validated` returned `Ok`; `None` when the
    /// artifact is skipped (option byte only, no payload; non-UTF-8;
    /// rejected control chars).
    pub formatted: Option<String>,
    /// `Some` when `format_validated` returned `SemanticDivergence`.
    pub divergence: Option<String>,
    /// Free-text note for the skip cases.
    pub note: Option<String>,
}

/// Diagnose one libFuzzer crash artifact.
///
/// # Errors
///
/// Surfaces I/O failures reading the artifact, or any panic propagated
/// from parsing or validating formatting (typically a pulldown-cmark
/// upstream bug — the fuzz target swallows these via `catch_unwind`;
/// here they propagate so the diagnostic doesn't silently mis-report).
pub fn diagnose(artifact_path: &Path) -> Result<Diagnosis> {
    let bytes = std::fs::read(artifact_path).with_context(|| format!("read {}", artifact_path.display()))?;
    let Some((&option_byte, rest)) = bytes.split_first() else {
        return Ok(Diagnosis {
            option_byte: 0,
            opts_summary: "(empty artifact)".to_owned(),
            input: Vec::new(),
            input_utf8: None,
            formatted: None,
            divergence: None,
            note: Some("artifact is empty".to_owned()),
        });
    };
    let opts = opts_from_byte(option_byte);
    let opts_summary = summarise_opts(option_byte);
    let input = rest.to_vec();

    let Ok(s) = std::str::from_utf8(rest) else {
        return Ok(Diagnosis {
            option_byte,
            opts_summary,
            input,
            input_utf8: None,
            formatted: None,
            divergence: None,
            note: Some("input is not valid UTF-8 (fuzz target would skip)".to_owned()),
        });
    };

    let input_utf8 = Some(s.to_owned());

    if mdwright::contains_rejected_control_chars(s) {
        return Ok(Diagnosis {
            option_byte,
            opts_summary,
            input,
            input_utf8,
            formatted: None,
            divergence: None,
            note: Some("input carries rejected C0 control bytes (fuzz target would skip)".to_owned()),
        });
    }

    // Mirror `fuzz/fuzz_targets/fuzz_parse_format.rs`'s oracle: format
    // once, then compare source against formatted for semantic
    // equivalence. This is stricter than `format_validated` (which
    // checks idempotence-on-mode, not source-vs-formatted) — the
    // diagnostic targets the same property the fuzz target asserts.
    let formatted = mdwright::format_document(&Document::parse(s), &opts);
    let divergence = first_divergence(s, &formatted);
    let note = if divergence.is_none() {
        Some("source ≅ formatted — artifact does not reproduce".to_owned())
    } else {
        None
    };
    Ok(Diagnosis {
        option_byte,
        opts_summary,
        input,
        input_utf8,
        formatted: Some(formatted),
        divergence,
        note,
    })
}

/// Mirror of `fuzz/fuzz_targets/fuzz_parse_format.rs::opts_from_byte`.
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
    // Bit 3 is reserved (kept aligned with the fuzz target's option byte).
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

/// One-line human-readable summary of the option byte's decoded knobs.
fn summarise_opts(byte: u8) -> String {
    let wrap = match byte & 0b11 {
        0 => "Wrap::Keep",
        1 => "Wrap::No",
        2 => "Wrap::At(80)",
        _ => "Wrap::At(120)",
    };
    let math = if byte & 0b100 != 0 {
        "math.normalise"
    } else {
        "no math normalise"
    };
    let mode = if byte & 0b1000 != 0 {
        "reserved bit set"
    } else {
        "reserved bit clear"
    };
    let canon = match (byte >> 4) & 0b1111 {
        0 => "no canon",
        1 => "italic=Asterisk",
        2 => "italic=Underscore",
        3 => "strong=Asterisk",
        4 => "strong=Underscore",
        5 => "list_marker=Dash",
        6 => "list_marker=Asterisk",
        7 => "list_marker=Plus",
        8 => "thematic=Dash",
        9 => "thematic=Asterisk",
        10 => "thematic=Underscore",
        11 => "ordered=Consistent",
        12 => "link_def=Bare",
        13 => "link_def=Angle",
        14 => "all-asterisk canon",
        _ => "all-underscore-and-dash canon",
    };
    format!("{wrap}, {mode}, {canon}, {math}")
}

/// Render the diagnosis to stdout in the format documented at the top
/// of this file.
pub fn render(artifact_path: &Path, d: &Diagnosis) {
    println!("artifact: {}", artifact_path.display());
    println!("option_byte: {:#04x} ({})", d.option_byte, d.opts_summary);
    if let Some(s) = &d.input_utf8 {
        println!("input ({} bytes): {s:?}", d.input.len());
    } else {
        println!("input ({} bytes, not UTF-8): {:?}", d.input.len(), d.input);
    }
    if let Some(note) = &d.note
        && d.divergence.is_none()
    {
        println!("note: {note}");
    }
    if let Some(out) = &d.formatted {
        println!("----");
        println!("formatter output ({} bytes): {out:?}", out.len());
    }
    if let Some(diff) = &d.divergence {
        println!("----");
        println!("divergence: {diff}");
    }
}
