#![allow(
    clippy::literal_string_with_formatting_args,
    reason = "law tests generate exact TeX and Unicode source strings"
)]

use mdwright_latex::{Translation, TranslationStatus, translate_latex_to_unicode, translate_unicode_to_latex};
use proptest::prelude::*;

fn latex_to_unicode_text(source: &str) -> String {
    translate_latex_to_unicode(source).text().to_owned()
}

fn unicode_to_latex_text(source: &str) -> String {
    translate_unicode_to_latex(source).text().to_owned()
}

fn direct_latex_atom() -> impl Strategy<Value = String> {
    prop::sample::select(&[
        r"\alpha",
        r"\beta",
        r"\Gamma",
        r"\Omega",
        r"\le",
        r"\leq",
        r"\ge",
        r"\neq",
        r"\rightarrow",
        r"\to",
        r"\gets",
        r"\times",
        r"\land",
        r"\lor",
        r"\sum",
        r"\infty",
        r"\varnothing",
        r"\emptyset",
    ])
    .prop_map(str::to_owned)
}

fn plain_latex_atom() -> impl Strategy<Value = String> {
    prop::sample::select(&["x", "y", "n", "i", "0", "1", "2", "12"]).prop_map(str::to_owned)
}

fn scriptable_latex_atom() -> impl Strategy<Value = String> {
    prop_oneof![
        plain_latex_atom(),
        prop::sample::select(&[r"\alpha", r"\beta", r"\Gamma", r"\Omega"]).prop_map(str::to_owned),
    ]
}

fn script_latex_atom() -> impl Strategy<Value = String> {
    let base = scriptable_latex_atom();
    let script = prop::sample::select(&["_i", "_{n}", "_{12}", "^2", "^{n}", "^{-1}", "_i^2", "^{2}_{i}"]);
    (base, script).prop_map(|(base, script)| format!("{base}{script}"))
}

fn structured_latex_atom() -> impl Strategy<Value = String> {
    prop::sample::select(&[
        r"\sqrt{x}",
        r"\sqrt[n]{x}",
        r"\hat{x}",
        r"\bar{x}",
        r"\tilde{x}",
        r"\vec{v}",
    ])
    .prop_map(str::to_owned)
}

fn supported_latex_atom() -> impl Strategy<Value = String> {
    prop_oneof![
        plain_latex_atom(),
        direct_latex_atom(),
        script_latex_atom(),
        structured_latex_atom(),
    ]
}

fn supported_latex_source() -> impl Strategy<Value = String> {
    prop::collection::vec(supported_latex_atom(), 1..4).prop_map(|parts| parts.join(" + "))
}

fn direct_unicode_atom() -> impl Strategy<Value = String> {
    prop::sample::select(&[
        "α", "β", "Γ", "Ω", "≤", "≥", "≠", "→", "←", "×", "∧", "∨", "∑", "∞", "∅",
    ])
    .prop_map(str::to_owned)
}

fn plain_unicode_atom() -> impl Strategy<Value = String> {
    prop::sample::select(&["x", "y", "n", "i", "0", "1", "2", "12"]).prop_map(str::to_owned)
}

fn script_unicode_atom() -> impl Strategy<Value = String> {
    prop::sample::select(&["xᵢ", "xₙ", "x₁₂", "x²", "xⁿ", "x⁻¹", "αᵢ", "β²", "∑ₙ"]).prop_map(str::to_owned)
}

fn structured_unicode_atom() -> impl Strategy<Value = String> {
    prop::sample::select(&["√x", "ⁿ√x", "x\u{302}", "x\u{305}", "x\u{303}", "v\u{20d7}"]).prop_map(str::to_owned)
}

fn supported_unicode_atom() -> impl Strategy<Value = String> {
    prop_oneof![
        plain_unicode_atom(),
        direct_unicode_atom(),
        script_unicode_atom(),
        structured_unicode_atom(),
    ]
}

fn supported_unicode_source() -> impl Strategy<Value = String> {
    prop::collection::vec(supported_unicode_atom(), 1..4).prop_map(|parts| parts.join(" + "))
}

fn lossy_latex_source() -> impl Strategy<Value = String> {
    prop_oneof![
        prop::sample::select(&[r"\frac{a}{b}", r"\frac{\alpha}{x_i}", r"\frac{x}{\sqrt[n]{y}}"])
            .prop_map(str::to_owned),
        prop::sample::select(&[r"\color{red}{x}", r"\href{url}{x}", r"\text{x}", r"\newcommand{\x}{y}"])
            .prop_map(str::to_owned),
        supported_latex_atom().prop_map(|atom| format!("{atom} + \\frac{{a}}{{b}}")),
        supported_latex_atom().prop_map(|atom| format!("{atom} + \\color{{red}}{{x}}")),
    ]
}

fn recorded_issue_count(translation: &Translation) -> usize {
    translation
        .losses()
        .len()
        .saturating_add(translation.diagnostics().len())
}

fn diagnostics_are_in_bounds(translation: &Translation, source_len: usize) -> bool {
    translation
        .diagnostics()
        .iter()
        .all(|diagnostic| diagnostic.span().start() <= diagnostic.span().end() && diagnostic.span().end() <= source_len)
}

proptest! {
    #[test]
    fn latex_to_unicode_roundtrip_reaches_unicode_fixed_point(source in supported_latex_source()) {
        let unicode = latex_to_unicode_text(&source);
        let latex = unicode_to_latex_text(&unicode);
        let unicode_again = latex_to_unicode_text(&latex);

        prop_assert_eq!(unicode_again, unicode);
    }

    #[test]
    fn unicode_to_latex_roundtrip_reaches_latex_fixed_point(source in supported_unicode_source()) {
        let latex = unicode_to_latex_text(&source);
        let unicode = latex_to_unicode_text(&latex);
        let latex_again = unicode_to_latex_text(&unicode);

        prop_assert_eq!(latex_again, latex);
    }

    #[test]
    fn lossy_latex_translation_remains_visible_and_stable(source in lossy_latex_source()) {
        let translated = translate_latex_to_unicode(&source);
        prop_assert!(!translated.text().is_empty());
        prop_assert_eq!(translated.status(), TranslationStatus::Lossy);
        prop_assert!(recorded_issue_count(&translated) > 0);
        prop_assert!(diagnostics_are_in_bounds(&translated, source.len()));

        let first_normal_form = latex_to_unicode_text(&unicode_to_latex_text(translated.text()));
        prop_assert!(!first_normal_form.is_empty());
        let second_normal_form = latex_to_unicode_text(&unicode_to_latex_text(&first_normal_form));
        prop_assert_eq!(second_normal_form, first_normal_form);
    }
}
