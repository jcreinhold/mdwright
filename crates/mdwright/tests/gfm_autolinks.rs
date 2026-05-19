use mdwright_config::Config;
use mdwright_document::{Document, ExtensionOptions, GfmOptions, ParseOptions, render_html, render_html_with_options};
use mdwright_format::{FmtOptions, Wrap, format_document, semantically_equivalent};
use mdwright_lint::{RuleSet, apply_safe_fixes};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn default_rendering_recognises_gfm_bare_autolinks() -> TestResult {
    let html = render_html("Visit www.commonmark.org/help and https://example.com.\n")?;
    assert!(html.contains(r#"<a href="http://www.commonmark.org/help">www.commonmark.org/help</a>"#));
    assert!(html.contains(r#"<a href="https://example.com">https://example.com</a>."#));
    Ok(())
}

#[test]
fn parse_option_disables_gfm_bare_autolinks() -> TestResult {
    let opts = ParseOptions::default().with_extensions(ExtensionOptions {
        gfm: GfmOptions {
            bare_url_autolinks: false,
        },
        ..ExtensionOptions::default()
    });
    let doc = Document::parse_with_options("Visit https://example.com.\n", opts)?;
    assert!(doc.gfm_bare_autolinks().is_empty());
    let html = render_html_with_options("Visit https://example.com.\n", opts)?;
    assert_eq!(html, "<p>Visit https://example.com.</p>\n");
    Ok(())
}

#[test]
fn config_controls_gfm_bare_autolinks() -> TestResult {
    let path = tempfile::NamedTempFile::new()?.into_temp_path();
    std::fs::write(&path, "[parse.extensions.gfm]\nbare-url-autolinks = false\n")?;
    let cfg = Config::load_explicit(&path)?;
    assert!(!cfg.parse_options().extensions().gfm.bare_url_autolinks);
    Ok(())
}

#[test]
fn bare_url_lint_flags_gfm_bare_autolinks_but_not_explicit_autolinks() -> TestResult {
    let doc = Document::parse("bare https://example.com explicit <https://example.org>\n")?;
    let rules = RuleSet::stdlib_all();
    let rule = rules
        .by_name("bare-url")
        .ok_or_else(|| std::io::Error::other("bare-url rule exists"))?;
    let diagnostics = rule.check_one(&doc);
    let [diagnostic] = diagnostics.as_slice() else {
        assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
        return Ok(());
    };
    assert_eq!(diagnostic.span, 5..24);
    Ok(())
}

#[test]
fn bare_url_safe_fix_wraps_bare_origin_autolink() -> TestResult {
    let doc = Document::parse("See https://example.com now.\n")?;
    let rules = RuleSet::stdlib_all();
    let rule = rules
        .by_name("bare-url")
        .ok_or_else(|| std::io::Error::other("bare-url rule exists"))?;
    let diagnostics = rule.check_one(&doc);
    let (fixed, applied) = apply_safe_fixes(&doc, &diagnostics);
    assert_eq!(applied, 1);
    assert_eq!(fixed, "See <https://example.com> now.\n");
    Ok(())
}

#[test]
fn formatter_wrap_keeps_gfm_bare_autolinks_atomic() -> TestResult {
    let src = "See https://example.com/a/really/long/path for details about wrapping.\n";
    let doc = Document::parse(src)?;
    let opts = FmtOptions::default().with_wrap(Wrap::At(24));
    let formatted = format_document(&doc, &opts);
    assert!(formatted.contains("https://example.com/a/really/long/path"));
    assert!(semantically_equivalent(src, &formatted)?);
    Ok(())
}

#[test]
fn semantic_signature_rejects_split_gfm_bare_autolink() -> TestResult {
    assert!(!semantically_equivalent(
        "See https://example.com/a/b for details.\n",
        "See https://example.com/a\nb for details.\n"
    )?);
    Ok(())
}

trait CheckOne {
    fn check_one(&self, doc: &Document) -> Vec<mdwright_lint::Diagnostic>;
}

impl CheckOne for dyn mdwright_lint::LintRule + '_ {
    fn check_one(&self, doc: &Document) -> Vec<mdwright_lint::Diagnostic> {
        let mut out = Vec::new();
        self.check(doc, &mut out);
        for d in &mut out {
            d.rule = std::borrow::Cow::Owned(self.name().to_owned());
        }
        out
    }
}
