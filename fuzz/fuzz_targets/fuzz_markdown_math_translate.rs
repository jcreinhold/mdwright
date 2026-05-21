#![no_main]
//! Scan Markdown math spans, then translate only the math bodies. This
//! exercises the `mdwright-math` to `mdwright-latex` boundary without
//! moving Markdown parsing into the TeX language crate.

use libfuzzer_sys::fuzz_target;
use mdwright_math::{MathConfig, scan_math_regions};

const MAX_INPUT: usize = 65_536;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    let cfg = MathConfig {
        double_dollar: true,
        single_dollar: true,
        ..MathConfig::default()
    };
    let (regions, _errors) = scan_math_regions(source, &[], &[], cfg);
    let ranges = regions
        .iter()
        .map(|region| region.span().body().source_range())
        .collect::<Vec<_>>();

    let to_unicode = mdwright_latex::translate_latex_ranges_to_unicode(source, &ranges);
    assert_translation_spans(source, &to_unicode);

    let to_latex = mdwright_latex::translate_unicode_ranges_to_latex(source, &ranges);
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
    }
    for loss in translation.losses() {
        assert!(loss.span().start() <= loss.span().end(), "inverted loss span");
        assert!(loss.span().end() <= source.len(), "loss span exceeds source length");
    }
}
