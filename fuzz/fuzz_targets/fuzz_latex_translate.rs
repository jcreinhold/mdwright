#![no_main]
//! Translate TeX math source to Unicode and Unicode math source to
//! preferred LaTeX. Diagnostics and losses must stay in-bounds.

use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 65_536;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };

    let to_unicode = mdwright_latex::translate_latex_to_unicode(source);
    assert_translation_spans(source, &to_unicode);

    let to_latex = mdwright_latex::translate_unicode_to_latex(source);
    assert_translation_spans(source, &to_latex);
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
