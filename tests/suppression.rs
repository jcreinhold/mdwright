//! End-to-end tests for `<!-- mdwright: ... -->` suppression comments.
//!
//! These exercise the full `Document::lint` path under default options
//! (suppressions respected) and under
//! `LintOptions { respect_suppressions: false }` (the `--no-suppress`
//! CLI flag's behaviour).

use anyhow::Result;
use mdwright::{Diagnostic, Document, LintOptions, RuleSet};

fn diags(src: &str) -> Vec<Diagnostic> {
    Document::parse(src).lint(&RuleSet::stdlib_all())
}

fn diags_unsuppressed(src: &str) -> Vec<Diagnostic> {
    Document::parse(src).lint_with(
        &RuleSet::stdlib_all(),
        LintOptions {
            respect_suppressions: false,
        },
    )
}

#[test]
fn allow_next_block_silences_one_rule() -> Result<()> {
    let src = "<!-- mdwright: allow heading-punctuation -->\n# Title.\n";
    let d = diags(src);
    assert!(
        !d.iter().any(|d| d.rule == "heading-punctuation"),
        "heading-punctuation should be suppressed; got: {d:?}"
    );
    Ok(())
}

#[test]
fn allow_next_block_does_not_leak_to_other_rules() -> Result<()> {
    // The `allow` directive names heading-punctuation only; bare-url
    // on the same heading must still fire.
    let src = "<!-- mdwright: allow heading-punctuation -->\n# Title. https://example.com\n";
    let d = diags(src);
    assert!(!d.iter().any(|d| d.rule == "heading-punctuation"));
    assert!(
        d.iter().any(|d| d.rule == "bare-url"),
        "bare-url should still fire; got: {d:?}"
    );
    Ok(())
}

#[test]
fn allow_next_line_silences_one_rule() -> Result<()> {
    let src = "<!-- mdwright: allow-next-line trailing-whitespace -->\ntrailing   \nrest\n";
    let d = diags(src);
    assert!(
        !d.iter().any(|d| d.rule == "trailing-whitespace"),
        "trailing-whitespace should be suppressed; got: {d:?}"
    );
    Ok(())
}

#[test]
fn disable_enable_spans_multiple_blocks() -> Result<()> {
    let src = "Outside: https://before.example.com\n\n\
               <!-- mdwright: disable bare-url -->\n\n\
               Inside one: https://inside-one.example.com\n\n\
               Inside two: https://inside-two.example.com\n\n\
               <!-- mdwright: enable bare-url -->\n\n\
               After: https://after.example.com\n";
    let d = diags(src);
    let url_messages: Vec<String> = d
        .iter()
        .filter(|d| d.rule == "bare-url")
        .map(|d| d.message.clone())
        .collect();
    // Two URLs survive: the one before and the one after the disable
    // region.
    assert_eq!(
        url_messages.len(),
        2,
        "expected exactly 2 bare-url diagnostics outside the region; got: {url_messages:?}"
    );
    Ok(())
}

#[test]
fn disable_all_silences_every_rule() -> Result<()> {
    let src = "<!-- mdwright: disable-all -->\n\n\
               # Bad heading.\n\n\
               https://bare.example.com\n";
    let d = diags(src);
    // No non-advisory diagnostics should remain (the `suppression`
    // pseudo-rule itself is advisory, so it would be allowed even if
    // it fires — but here there's nothing to flag).
    let non_advisory: Vec<&str> = d
        .iter()
        .filter(|d| !d.advisory)
        .map(|d| d.rule.as_ref())
        .collect();
    assert!(
        non_advisory.is_empty(),
        "expected no diagnostics; got: {non_advisory:?}"
    );
    Ok(())
}

#[test]
fn multiple_rules_in_one_comment() -> Result<()> {
    let src = "<!-- mdwright: allow heading-punctuation, bare-url -->\n\
               # Title. https://example.com\n";
    let d = diags(src);
    assert!(!d.iter().any(|d| d.rule == "heading-punctuation"));
    assert!(!d.iter().any(|d| d.rule == "bare-url"));
    Ok(())
}

#[test]
fn unknown_rule_name_surfaces_advisory() -> Result<()> {
    let src = "<!-- mdwright: allow nonexistent-rule -->\n# Title.\n";
    let d = diags(src);
    let advisories: Vec<&Diagnostic> = d.iter().filter(|d| d.rule == "suppression").collect();
    assert_eq!(
        advisories.len(),
        1,
        "expected one suppression diag; got: {advisories:?}"
    );
    let adv = advisories
        .first()
        .copied()
        .ok_or_else(|| anyhow::anyhow!("missing"))?;
    assert!(adv.advisory, "suppression diagnostic must be advisory");
    assert!(
        adv.message.contains("nonexistent-rule"),
        "message should name the unknown rule; got: {}",
        adv.message
    );
    Ok(())
}

#[test]
fn no_suppress_flag_returns_raw_diagnostics() -> Result<()> {
    // The same input that allow_next_block_silences_one_rule mutes,
    // but with respect_suppressions: false.
    let src = "<!-- mdwright: allow heading-punctuation -->\n# Title.\n";
    let d = diags_unsuppressed(src);
    assert!(
        d.iter().any(|d| d.rule == "heading-punctuation"),
        "with respect_suppressions=false, heading-punctuation must fire; got: {d:?}"
    );
    Ok(())
}
