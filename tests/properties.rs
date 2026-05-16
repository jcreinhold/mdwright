//! Property-based tests for mdwright.
//!
//! Three properties hold for every parseable Markdown source `x`:
//!
//! 1. Idempotence:        `format(format(x)) == format(x)`.
//! 2. HTML preservation:  `html(x) == html(format(x))`.
//! 3. Lint preservation:  `format` introduces no new default-on
//!    diagnostics, modulo `bare-url` (which the formatter is
//!    allowed to remove by normalising to `<…>` autolink form).
//!
//! Default 256 cases per property; a 4096-case sweep is gated behind
//! `#[ignore]` for pre-release runs.

#![allow(clippy::unwrap_used, let_underscore_drop)]

use std::collections::HashSet;
use std::io::Write;

use mdwright::{Document, FmtOptions, RuleSet, render_html};
use proptest::prelude::*;

#[path = "common/proptest_gen.rs"]
mod generators;

/// Dump a failing input to a temp file so multi-line counterexamples
/// are inspectable in an editor. Best-effort: failures here are
/// swallowed because the underlying assertion is the real signal.
fn dump_counterexample(label: &str, src: &str) {
    let Ok(mut f) = tempfile::Builder::new()
        .prefix(&format!("mdwright-{label}-"))
        .suffix(".md")
        .tempfile()
    else {
        return;
    };
    let _ = f.write_all(src.as_bytes());
    if let Ok(path) = f.keep() {
        eprintln!("counterexample dumped to {}", path.1.display());
    }
}

/// Synthesise a single `[label]: dest "title"` def plus a reference
/// link. Labels and dests are bounded ASCII to keep the round-trip
/// property focused on the resolver, not on Unicode quirks of other
/// passes.
fn arb_reference_triple() -> impl Strategy<Value = (String, String, String, &'static str)> {
    (
        "[a-zA-Z][a-zA-Z0-9_ -]{0,15}",
        "/[a-zA-Z0-9._/-]{0,32}",
        "[a-zA-Z0-9 .,!?-]{0,32}",
        prop_oneof![Just("full"), Just("collapsed"), Just("shortcut")],
    )
        .prop_map(
            |(label, dest, title, kind): (String, String, String, &'static str)| {
                (label, dest, title, kind)
            },
        )
}

proptest! {
    /// Resolver round-trip: a synthesised `[label]` reference plus its
    /// `[label]: dest "title"` def must format → reparse → format
    /// unchanged and produce identical HTML on both sides.
    #[test]
    fn reference_resolver_round_trips(
        (label, dest, title, kind) in arb_reference_triple(),
    ) {
        let reference = match kind {
            "full" => format!("[{label}][{label}]"),
            "collapsed" => format!("[{label}][]"),
            _ => format!("[{label}]"),
        };
        let src = format!("{reference}\n\n[{label}]: {dest} \"{title}\"\n");
        let opts = FmtOptions::default();
        let once = Document::parse(&src).format(&opts);
        let twice = Document::parse(&once).format(&opts);
        prop_assert_eq!(&once, &twice);
        prop_assert_eq!(render_html(&src), render_html(&once));
    }

    #[test]
    fn idempotent(src in generators::arb_document()) {
        let opts = FmtOptions::default();
        let once = Document::parse(&src).format(&opts);
        let twice = Document::parse(&once).format(&opts);
        if once != twice {
            dump_counterexample("idempotent", &src);
        }
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn html_preserving(src in generators::arb_document()) {
        let opts = FmtOptions::default();
        let formatted = Document::parse(&src).format(&opts);
        let before = render_html(&src);
        let after = render_html(&formatted);
        if before != after {
            dump_counterexample("html", &src);
        }
        prop_assert_eq!(before, after);
    }

    #[test]
    fn lint_preserving(src in generators::arb_document()) {
        let rules = RuleSet::stdlib_defaults();
        let before: HashSet<String> = Document::parse(&src)
            .lint(&rules)
            .into_iter()
            .map(|d| d.rule.into_owned())
            .filter(|r| r != "bare-url")
            .collect();
        let formatted = Document::parse(&src).format(&FmtOptions::default());
        let after: HashSet<String> = Document::parse(&formatted)
            .lint(&rules)
            .into_iter()
            .map(|d| d.rule.into_owned())
            .filter(|r| r != "bare-url")
            .collect();
        let new: Vec<&String> = after.difference(&before).collect();
        if !new.is_empty() {
            dump_counterexample("lint", &src);
        }
        prop_assert!(
            new.is_empty(),
            "format introduced new diagnostics: {new:?}",
        );
    }
}

// ---------------------------------------------------------------------------
// Per-construct laws.
//
// Each `arb_*_src` generator from `tests/common/proptest_gen.rs` emits
// a Markdown fragment shaped like one CM/GFM construct, biased toward
// boundary cases for that construct. The contract is the same one the
// whole-document laws above check (idempotence + html-preservation
// through `Document::parse + format`); per-construct laws shrink
// failures to a single-construct fragment instead of a tangled
// multi-block document.
//
// Per-construct laws live at the same layer as the document laws on
// purpose: the user-visible contract is "formatted Markdown reparses
// to the same meaning", which only exists at the public surface.
// Coupling tests to internal IR types (`cm::inline::EmphasisRun`, etc.)
// would bake an implementation choice into the test suite — see
// `/Users/jcreinhold/.claude/plans/phase-r-per-construct-keen-firefly.md`.
// ---------------------------------------------------------------------------

fn check_idempotent(label: &str, src: &str) -> Result<(), TestCaseError> {
    let opts = FmtOptions::default();
    let once = Document::parse(src).format(&opts);
    let twice = Document::parse(&once).format(&opts);
    if once != twice {
        dump_counterexample(label, src);
    }
    prop_assert_eq!(once, twice);
    Ok(())
}

fn check_html_preserving(label: &str, src: &str) -> Result<(), TestCaseError> {
    let opts = FmtOptions::default();
    let formatted = Document::parse(src).format(&opts);
    let before = render_html(src);
    let after = render_html(&formatted);
    if before != after {
        dump_counterexample(label, src);
    }
    prop_assert_eq!(before, after);
    Ok(())
}

proptest! {
    #[test]
    fn emphasis_fragments_idempotent(s in generators::arb_emphasis_src()) {
        check_idempotent("emphasis_fragments_idempotent", &s)?;
    }
    #[test]
    fn emphasis_fragments_html_preserving(s in generators::arb_emphasis_src()) {
        check_html_preserving("emphasis_fragments_html_preserving", &s)?;
    }

    #[test]
    fn strong_fragments_idempotent(s in generators::arb_strong_src()) {
        check_idempotent("strong_fragments_idempotent", &s)?;
    }
    #[test]
    fn strong_fragments_html_preserving(s in generators::arb_strong_src()) {
        check_html_preserving("strong_fragments_html_preserving", &s)?;
    }

    #[test]
    fn link_inline_fragments_idempotent(s in generators::arb_link_inline_src()) {
        check_idempotent("link_inline_fragments_idempotent", &s)?;
    }
    #[test]
    fn link_inline_fragments_html_preserving(s in generators::arb_link_inline_src()) {
        check_html_preserving("link_inline_fragments_html_preserving", &s)?;
    }

    #[test]
    fn link_reference_fragments_idempotent(s in generators::arb_link_reference_src()) {
        check_idempotent("link_reference_fragments_idempotent", &s)?;
    }
    #[test]
    fn link_reference_fragments_html_preserving(s in generators::arb_link_reference_src()) {
        check_html_preserving("link_reference_fragments_html_preserving", &s)?;
    }

    #[test]
    fn autolink_fragments_idempotent(s in generators::arb_autolink_src()) {
        check_idempotent("autolink_fragments_idempotent", &s)?;
    }
    #[test]
    fn autolink_fragments_html_preserving(s in generators::arb_autolink_src()) {
        check_html_preserving("autolink_fragments_html_preserving", &s)?;
    }

    #[test]
    fn code_span_fragments_idempotent(s in generators::arb_code_span_src()) {
        check_idempotent("code_span_fragments_idempotent", &s)?;
    }
    #[test]
    fn code_span_fragments_html_preserving(s in generators::arb_code_span_src()) {
        check_html_preserving("code_span_fragments_html_preserving", &s)?;
    }

    #[test]
    fn heading_fragments_idempotent(s in generators::arb_heading_src()) {
        check_idempotent("heading_fragments_idempotent", &s)?;
    }
    #[test]
    fn heading_fragments_html_preserving(s in generators::arb_heading_src()) {
        check_html_preserving("heading_fragments_html_preserving", &s)?;
    }

    #[test]
    fn fenced_code_fragments_idempotent(s in generators::arb_fenced_code_src()) {
        check_idempotent("fenced_code_fragments_idempotent", &s)?;
    }
    #[test]
    fn fenced_code_fragments_html_preserving(s in generators::arb_fenced_code_src()) {
        check_html_preserving("fenced_code_fragments_html_preserving", &s)?;
    }

    #[test]
    fn quote_fragments_idempotent(s in generators::arb_quote_src()) {
        check_idempotent("quote_fragments_idempotent", &s)?;
    }
    #[test]
    fn quote_fragments_html_preserving(s in generators::arb_quote_src()) {
        check_html_preserving("quote_fragments_html_preserving", &s)?;
    }

    #[test]
    fn list_fragments_idempotent(s in generators::arb_list_src()) {
        check_idempotent("list_fragments_idempotent", &s)?;
    }
    #[test]
    fn list_fragments_html_preserving(s in generators::arb_list_src()) {
        check_html_preserving("list_fragments_html_preserving", &s)?;
    }

    #[test]
    fn table_fragments_idempotent(s in generators::arb_table_src()) {
        check_idempotent("table_fragments_idempotent", &s)?;
    }
    #[test]
    fn table_fragments_html_preserving(s in generators::arb_table_src()) {
        check_html_preserving("table_fragments_html_preserving", &s)?;
    }

    #[test]
    fn thematic_fragments_idempotent(s in generators::arb_thematic_src()) {
        check_idempotent("thematic_fragments_idempotent", &s)?;
    }
    #[test]
    fn thematic_fragments_html_preserving(s in generators::arb_thematic_src()) {
        check_html_preserving("thematic_fragments_html_preserving", &s)?;
    }

    #[test]
    fn footnote_fragments_idempotent(s in generators::arb_footnote_src()) {
        check_idempotent("footnote_fragments_idempotent", &s)?;
    }
    #[test]
    fn footnote_fragments_html_preserving(s in generators::arb_footnote_src()) {
        check_html_preserving("footnote_fragments_html_preserving", &s)?;
    }
}

// ----- 4096-case sweep, run with `--ignored`. -----

proptest! {
    #![proptest_config(ProptestConfig { cases: 4096, .. ProptestConfig::default() })]

    #[test]
    #[ignore = "slow; run with --ignored before release"]
    fn idempotent_sweep(src in generators::arb_document()) {
        let opts = FmtOptions::default();
        let once = Document::parse(&src).format(&opts);
        let twice = Document::parse(&once).format(&opts);
        prop_assert_eq!(once, twice);
    }

    #[test]
    #[ignore = "slow; run with --ignored before release"]
    fn html_preserving_sweep(src in generators::arb_document()) {
        let opts = FmtOptions::default();
        let formatted = Document::parse(&src).format(&opts);
        prop_assert_eq!(render_html(&src), render_html(&formatted));
    }

    #[test]
    #[ignore = "slow; run with --ignored before release"]
    fn lint_preserving_sweep(src in generators::arb_document()) {
        let rules = RuleSet::stdlib_defaults();
        let before: HashSet<String> = Document::parse(&src)
            .lint(&rules)
            .into_iter()
            .map(|d| d.rule.into_owned())
            .filter(|r| r != "bare-url")
            .collect();
        let formatted = Document::parse(&src).format(&FmtOptions::default());
        let after: HashSet<String> = Document::parse(&formatted)
            .lint(&rules)
            .into_iter()
            .map(|d| d.rule.into_owned())
            .filter(|r| r != "bare-url")
            .collect();
        prop_assert!(after.is_subset(&before));
    }
}
