use mdwright_config::Config;
use mdwright_document::{
    AutolinkOrigin, Document, ExtensionOptions, GfmAutolinkPolicy, GfmOptions, ParseOptions, render_html,
    render_html_with_options,
};
use mdwright_format::{FmtOptions, Wrap, format_document, semantically_equivalent};
use mdwright_lint::{RuleSet, apply_safe_fixes};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn default_rendering_recognises_gfm_url_and_email_autolinks() -> TestResult {
    let html = render_html("Visit www.commonmark.org/help, https://example.com, and hello@example.com.\n")?;
    assert!(html.contains(r#"<a href="http://www.commonmark.org/help">www.commonmark.org/help</a>"#));
    assert!(html.contains(r#"<a href="https://example.com">https://example.com</a>"#));
    assert!(html.contains(r#"<a href="mailto:hello@example.com">hello@example.com</a>"#));
    Ok(())
}

#[test]
fn document_autolinks_report_commonmark_url_and_email_origins() -> TestResult {
    let doc = Document::parse("explicit <https://example.org> bare https://example.com mail hello@example.com\n")?;
    let facts: Vec<_> = doc
        .autolinks()
        .iter()
        .map(|fact| (fact.origin(), fact.text().to_owned(), fact.href().to_owned()))
        .collect();
    assert_eq!(
        facts,
        [
            (
                AutolinkOrigin::CommonMark,
                "https://example.org".to_owned(),
                "https://example.org".to_owned()
            ),
            (
                AutolinkOrigin::GfmUrl,
                "https://example.com".to_owned(),
                "https://example.com".to_owned()
            ),
            (
                AutolinkOrigin::GfmEmail,
                "hello@example.com".to_owned(),
                "mailto:hello@example.com".to_owned()
            ),
        ]
    );
    Ok(())
}

#[test]
fn parse_option_disables_gfm_autolinks() -> TestResult {
    let opts = ParseOptions::default().with_extensions(ExtensionOptions {
        gfm: GfmOptions {
            autolinks: GfmAutolinkPolicy::Disabled,
            ..GfmOptions::default()
        },
        ..ExtensionOptions::default()
    });
    let doc = Document::parse_with_options("Visit https://example.com and hello@example.com.\n", opts)?;
    assert!(doc.autolinks().is_empty());
    let html = render_html_with_options("Visit https://example.com and hello@example.com.\n", opts)?;
    assert_eq!(html, "<p>Visit https://example.com and hello@example.com.</p>\n");
    Ok(())
}

#[test]
fn parse_option_can_enable_urls_without_email_autolinks() -> TestResult {
    let opts = ParseOptions::default().with_extensions(ExtensionOptions {
        gfm: GfmOptions {
            autolinks: GfmAutolinkPolicy::Urls,
            ..GfmOptions::default()
        },
        ..ExtensionOptions::default()
    });
    let doc = Document::parse_with_options("Visit https://example.com and hello@example.com.\n", opts)?;
    let origins: Vec<_> = doc.autolinks().iter().map(|fact| fact.origin()).collect();
    assert_eq!(origins, [AutolinkOrigin::GfmUrl]);
    let html = render_html_with_options("Visit https://example.com and hello@example.com.\n", opts)?;
    assert!(html.contains(r#"<a href="https://example.com">https://example.com</a>"#));
    assert!(html.contains("hello@example.com"));
    assert!(!html.contains("mailto:hello@example.com"));
    Ok(())
}

#[test]
fn config_controls_gfm_autolinks_and_tagfilter() -> TestResult {
    let path = tempfile::NamedTempFile::new()?.into_temp_path();
    std::fs::write(
        &path,
        "[parse.extensions.gfm]\nautolinks = \"disabled\"\ntagfilter = false\n",
    )?;
    let cfg = Config::load_explicit(&path)?;
    assert_eq!(
        cfg.parse_options().extensions().gfm.autolinks,
        GfmAutolinkPolicy::Disabled
    );
    assert!(!cfg.parse_options().extensions().gfm.tagfilter);
    Ok(())
}

#[test]
fn tagfilter_is_enabled_by_default_and_can_be_disabled() -> TestResult {
    let html = render_html("<script>alert(1)</script>\n")?;
    assert_eq!(html, "&lt;script>alert(1)&lt;/script>\n");

    let opts = ParseOptions::default().with_extensions(ExtensionOptions {
        gfm: GfmOptions {
            tagfilter: false,
            ..GfmOptions::default()
        },
        ..ExtensionOptions::default()
    });
    let html = render_html_with_options("<script>alert(1)</script>\n", opts)?;
    assert_eq!(html, "<script>alert(1)</script>\n");
    Ok(())
}

#[test]
fn gfm_autolinks_do_not_apply_inside_code_blocks() -> TestResult {
    let src = "```yaml\n- uses: jcreinhold/mdwright@v0.1.0\n- url: https://example.com\n```\n";
    let doc = Document::parse(src)?;
    assert!(doc.autolinks().is_empty());
    let html = render_html(src)?;
    assert!(!html.contains("<a href="));
    assert!(html.contains("mdwright@v0.1.0"));
    assert!(html.contains("https://example.com"));
    Ok(())
}

#[test]
fn bare_url_lint_flags_gfm_url_autolinks_but_not_explicit_or_email_autolinks() -> TestResult {
    let doc = Document::parse("bare https://example.com explicit <https://example.org> email hello@example.com\n")?;
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
fn formatter_wrap_keeps_gfm_autolinks_atomic() -> TestResult {
    let src = "See https://example.com/a/really/long/path for details about wrapping.\n";
    let doc = Document::parse(src)?;
    let opts = FmtOptions::default().with_wrap(Wrap::At(24));
    let formatted = format_document(&doc, &opts);
    assert!(formatted.contains("https://example.com/a/really/long/path"));
    assert!(semantically_equivalent(src, &formatted)?);
    Ok(())
}

#[test]
fn semantic_signature_rejects_split_gfm_autolink() -> TestResult {
    assert!(!semantically_equivalent(
        "See https://example.com/a/b for details.\n",
        "See https://example.com/a\nb for details.\n"
    )?);
    assert!(!semantically_equivalent(
        "Mail hello@example.com for details.\n",
        "Mail hello@\nexample.com for details.\n"
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
