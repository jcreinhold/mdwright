#![allow(clippy::expect_used, clippy::panic, reason = "coverage fixtures should fail loudly")]

use mdwright_latex::{
    ArgumentShape, CommandCategory, LatexErrorKind, SupportStatus, TranslationStatus, is_known_unsupported_command,
    lookup_command, render_unicode_math, translate_latex_to_unicode, translate_unicode_to_latex,
};

struct DirectFixture {
    name: &'static str,
    category: CommandCategory,
    unicode: &'static str,
    preferred: &'static str,
    reverse: Option<&'static str>,
}

#[test]
fn mathjax_style_direct_command_families_have_unicode_fixtures() {
    let fixtures = [
        DirectFixture {
            name: "alpha",
            category: CommandCategory::Greek,
            unicode: "α",
            preferred: "alpha",
            reverse: Some(r"\alpha"),
        },
        DirectFixture {
            name: "Gamma",
            category: CommandCategory::Greek,
            unicode: "Γ",
            preferred: "Gamma",
            reverse: Some(r"\Gamma"),
        },
        DirectFixture {
            name: "times",
            category: CommandCategory::BinaryOperator,
            unicode: "×",
            preferred: "times",
            reverse: Some(r"\times"),
        },
        DirectFixture {
            name: "le",
            category: CommandCategory::Relation,
            unicode: "≤",
            preferred: "leq",
            reverse: Some(r"\leq"),
        },
        DirectFixture {
            name: "to",
            category: CommandCategory::Arrow,
            unicode: "→",
            preferred: "to",
            reverse: Some(r"\to"),
        },
        DirectFixture {
            name: "langle",
            category: CommandCategory::Delimiter,
            unicode: "⟨",
            preferred: "langle",
            reverse: Some(r"\langle"),
        },
        DirectFixture {
            name: "sum",
            category: CommandCategory::LargeOperator,
            unicode: "∑",
            preferred: "sum",
            reverse: Some(r"\sum"),
        },
        DirectFixture {
            name: "infty",
            category: CommandCategory::Symbol,
            unicode: "∞",
            preferred: "infty",
            reverse: Some(r"\infty"),
        },
        DirectFixture {
            name: "sin",
            category: CommandCategory::Function,
            unicode: "sin",
            preferred: "sin",
            reverse: None,
        },
    ];

    for fixture in fixtures {
        let info = lookup_command(fixture.name).expect("fixture command registered");
        assert_eq!(info.category(), fixture.category, "category for {}", fixture.name);
        assert_eq!(info.unicode(), Some(fixture.unicode), "unicode for {}", fixture.name);
        assert_eq!(
            info.preferred(),
            fixture.preferred,
            "preferred spelling for {}",
            fixture.name
        );
        assert_eq!(
            info.support(),
            SupportStatus::DirectUnicode,
            "support status for {}",
            fixture.name
        );

        let latex = format!(r"\{}", fixture.name);
        let translated = translate_latex_to_unicode(&latex);
        assert_eq!(translated.text(), fixture.unicode, "latex-to-unicode for {latex}");
        assert_eq!(
            render_unicode_math(&latex).expect("direct command renders").as_text(),
            fixture.unicode
        );

        if let Some(reverse) = fixture.reverse {
            let reversed = translate_unicode_to_latex(fixture.unicode);
            assert_eq!(reversed.text(), reverse, "unicode-to-latex for {}", fixture.unicode);
        }
    }
}

#[test]
fn structured_math_fixtures_parse_render_and_translate_conservatively() {
    let script = translate_latex_to_unicode(r"\alpha_i + x^{2}");
    assert_eq!(script.text(), "αᵢ + x²");
    assert_eq!(script.status(), TranslationStatus::Lossless);
    assert_eq!(translate_unicode_to_latex(script.text()).text(), r"\alpha_{i} + x^{2}");

    let root = translate_latex_to_unicode(r"\sqrt[n]{x}");
    assert_eq!(root.text(), "ⁿ√x");
    assert_eq!(root.status(), TranslationStatus::Lossless);
    assert_eq!(
        render_unicode_math(r"\sqrt[n]{x}").expect("root renders").as_text(),
        "ⁿ√x"
    );

    let accent = render_unicode_math(r"\vec{v}").expect("accent renders").as_text();
    assert_eq!(accent, "v\u{20d7}");

    let fraction = render_unicode_math(r"\frac{a}{b}").expect("fraction renders");
    assert_eq!(fraction.lines(), &["a".to_owned(), "─".to_owned(), "b".to_owned()]);
    assert_eq!(fraction.baseline(), 1);
    let translated_fraction = translate_latex_to_unicode(r"\frac{a}{b}");
    assert_eq!(translated_fraction.text(), r"\frac{a}{b}");
    assert_eq!(translated_fraction.status(), TranslationStatus::Lossy);
    assert!(
        translated_fraction
            .losses()
            .iter()
            .any(|loss| loss.reason().contains("fraction"))
    );

    let matrix_source = concat!(r"\begin", "{matrix}", r"a & b \\ c & d\end", "{matrix}");
    let matrix = render_unicode_math(matrix_source).expect("matrix renders");
    assert_eq!(matrix.lines(), &["a  b".to_owned(), "c  d".to_owned()]);
}

#[test]
fn registry_records_spacing_fonts_environments_and_unsupported_mathjax_commands() {
    let spacing = lookup_command("quad").expect("spacing command registered");
    assert_eq!(spacing.category(), CommandCategory::Spacing);
    assert_eq!(spacing.support(), SupportStatus::RecognisedNoOutput);

    let font = lookup_command("mathbb").expect("font command registered");
    assert_eq!(font.category(), CommandCategory::Font);
    assert_eq!(font.arguments(), ArgumentShape::OneRequired);
    assert_eq!(font.support(), SupportStatus::ParsedConstruct);

    let cases = lookup_command("cases").expect("environment registered");
    assert_eq!(cases.category(), CommandCategory::Environment);
    assert_eq!(cases.arguments(), ArgumentShape::EnvironmentBody);

    for command in ["newcommand", "require", "color", "href", "text"] {
        assert!(
            is_known_unsupported_command(command),
            "{command} should be known unsupported"
        );
        let info = lookup_command(command).expect("unsupported command has registry entry");
        assert_eq!(info.support(), SupportStatus::Unsupported);
    }
}

#[test]
fn unsupported_and_malformed_input_return_typed_errors_or_visible_source() {
    let unsupported = render_unicode_math(r"\color{red}{x}").expect_err("color is unsupported");
    assert_eq!(unsupported.kind(), &LatexErrorKind::Unsupported);
    assert!(unsupported.message().contains("unsupported"));

    let translated = translate_latex_to_unicode(r"\color{red}{x}");
    assert_eq!(translated.text(), r"\color{red}{x}");
    assert!(
        translated
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind() == &LatexErrorKind::Unsupported)
    );

    let malformed = render_unicode_math(r"\frac{a").expect_err("malformed fraction is rejected");
    assert_eq!(malformed.kind(), &LatexErrorKind::Syntax);
}
