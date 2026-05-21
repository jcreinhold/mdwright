#![no_main]
//! Drive the parser-backed Unicode-to-LaTeX translator over supported
//! Unicode math source, then check the canonical fixed-point law:
//! `L(U(L(y))) == L(y)`.

use libfuzzer_sys::fuzz_target;

const MAX_ATOMS: usize = 256;

const ATOMS: &[&str] = &[
    "x",
    "y",
    "12",
    "α",
    "β",
    "Γ",
    "Ω",
    "𝓗𝓸𝓶",
    "𝓟𝓻𝓸𝓳",
    "𝒟ℯ𝓇",
    "𝚪_*",
    "𝐟𝐠",
    "xᵢ",
    "x⁻¹",
    "D₊",
    "iˢ_A",
    "M_[φ]",
    "x^(n)",
    "Ȳ'",
    "{Y'}̄",
    "√x",
    "ⁿ√x",
    "lim⃗",
    "lim⃖",
    "≤",
    "⩽",
    "≽",
    "≼",
    "≫",
    "⊉",
    "⊄",
    "⊀",
    "⋂",
    "⋯",
    "⨁",
    "□",
    "A ─u→ B",
    "A ←u─ B",
    "A ⥲ B",
    "A ⤏ B",
];

const SEPARATORS: &[&str] = &[" ", " + ", ", ", "; ", "\n"];

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let mut source = String::new();
    for (index, byte) in data.iter().take(MAX_ATOMS).enumerate() {
        if index > 0 {
            source.push_str(SEPARATORS[usize::from(byte >> 5) % SEPARATORS.len()]);
        }
        source.push_str(ATOMS[usize::from(*byte) % ATOMS.len()]);
    }

    let latex = mdwright_latex::translate_unicode_to_latex(&source);
    assert_translation_spans(&source, &latex);

    let unicode = mdwright_latex::translate_latex_to_unicode(latex.text());
    assert_translation_spans(latex.text(), &unicode);

    let latex_again = mdwright_latex::translate_unicode_to_latex(unicode.text());
    assert_translation_spans(unicode.text(), &latex_again);
    assert_eq!(latex_again.text(), latex.text());
});

fn assert_translation_spans(source: &str, translation: &mdwright_latex::Translation) {
    for diagnostic in translation.diagnostics() {
        assert!(
            diagnostic.span().start() <= diagnostic.span().end(),
            "inverted diagnostic span"
        );
        assert!(
            diagnostic.span().end() <= source.len(),
            "diagnostic span exceeds source length"
        );
        assert!(
            source.is_char_boundary(diagnostic.span().start()) && source.is_char_boundary(diagnostic.span().end()),
            "diagnostic span is not UTF-8 aligned"
        );
    }
    for loss in translation.losses() {
        assert!(loss.span().start() <= loss.span().end(), "inverted loss span");
        assert!(loss.span().end() <= source.len(), "loss span exceeds source length");
        assert!(
            source.is_char_boundary(loss.span().start()) && source.is_char_boundary(loss.span().end()),
            "loss span is not UTF-8 aligned"
        );
    }
}
