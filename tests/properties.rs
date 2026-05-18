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

use mdwright::{
    Document, FmtOptions, ItalicStyle, LinkDefStyle, ListMarkerStyle, OrderedListStyle, RuleSet, StrongStyle,
    ThematicStyle, Wrap, semantically_equivalent,
};
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
        .prop_map(|(label, dest, title, kind): (String, String, String, &'static str)| (label, dest, title, kind))
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
        prop_assert!(semantically_equivalent(&src, &once));
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
        for opts in opts_matrix() {
            let formatted = Document::parse(&src).format(&opts);
            if !semantically_equivalent(&src, &formatted) {
                dump_counterexample("html", &src);
            }
            prop_assert!(semantically_equivalent(&src, &formatted));
        }
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
    for opts in opts_matrix() {
        let formatted = Document::parse(src).format(&opts);
        if !semantically_equivalent(src, &formatted) {
            dump_counterexample(label, src);
        }
        prop_assert!(semantically_equivalent(src, &formatted));
    }
    Ok(())
}

/// Exercise every wrap mode the property tests should cover.
///
/// The semantic-equivalence gate accepts each mode by construction
/// (it operates on canonical event streams), and `Doc::Text` is
/// atomic by the Wadler/Lindig discipline, so syntactically-atomic
/// emitters (table rows, ATX heading bodies, fenced-code info
/// strings) stay on one line at every wrap mode. The matrix below
/// is the regression fence for that contract: a future change that
/// makes any emitter break under `Wrap::At(n)` fails one of these
/// properties.
fn opts_matrix() -> Vec<FmtOptions> {
    vec![
        FmtOptions::default(),
        FmtOptions::default().with_wrap(Wrap::At(80)),
        FmtOptions::default().with_wrap(Wrap::At(120)),
        FmtOptions::default().with_wrap(Wrap::No),
    ]
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

// ---------------------------------------------------------------------------
// Canonicalisation laws.
//
// For each style knob, the canonicalisation pass at
// `src/format/canonicalise.rs` must hold:
//
// - **Semantic equivalence**: `semantically_equivalent(s, format(s))`
//   under any knob setting. Per-rewrite verification is the
//   load-bearing invariant; this property is the regression fence.
// - **Idempotence**: `format(format(s)) == format(s)` under the same
//   knob. A canonicalisation that produces a different result on its
//   own output is a bug.
//
// Generators are shared with the per-construct laws above; only the
// `FmtOptions` differ.
// ---------------------------------------------------------------------------

fn canon_opts() -> Vec<(&'static str, FmtOptions)> {
    vec![
        (
            "italic_asterisk",
            FmtOptions::default().with_italic(ItalicStyle::Asterisk),
        ),
        (
            "italic_underscore",
            FmtOptions::default().with_italic(ItalicStyle::Underscore),
        ),
        (
            "strong_asterisk",
            FmtOptions::default().with_strong(StrongStyle::Asterisk),
        ),
        (
            "strong_underscore",
            FmtOptions::default().with_strong(StrongStyle::Underscore),
        ),
        (
            "list_marker_dash",
            FmtOptions::default().with_list_marker(ListMarkerStyle::Dash),
        ),
        (
            "list_marker_asterisk",
            FmtOptions::default().with_list_marker(ListMarkerStyle::Asterisk),
        ),
        (
            "list_marker_plus",
            FmtOptions::default().with_list_marker(ListMarkerStyle::Plus),
        ),
        (
            "ordered_consistent",
            FmtOptions::default().with_ordered_list(OrderedListStyle::Consistent),
        ),
        (
            "thematic_dash",
            FmtOptions::default().with_thematic_break(ThematicStyle::Dash),
        ),
        (
            "thematic_asterisk",
            FmtOptions::default().with_thematic_break(ThematicStyle::Asterisk),
        ),
        (
            "thematic_underscore",
            FmtOptions::default().with_thematic_break(ThematicStyle::Underscore),
        ),
        (
            "link_def_bare",
            FmtOptions::default().with_link_def_style(LinkDefStyle::Bare),
        ),
        (
            "link_def_angle",
            FmtOptions::default().with_link_def_style(LinkDefStyle::Angle),
        ),
        ("all_asterisk", opts_all_asterisk()),
        ("all_underscore_or_dash", opts_all_underscore_or_dash()),
    ]
}

/// Every knob set simultaneously, asterisk-leaning. Exercises rewrite
/// interactions (e.g. italic vs strong delimiter choice on the same
/// run) that the per-knob modes don't reach on their own.
fn opts_all_asterisk() -> FmtOptions {
    FmtOptions::default()
        .with_italic(ItalicStyle::Asterisk)
        .with_strong(StrongStyle::Asterisk)
        .with_list_marker(ListMarkerStyle::Asterisk)
        .with_thematic_break(ThematicStyle::Asterisk)
        .with_ordered_list(OrderedListStyle::Consistent)
        .with_link_def_style(LinkDefStyle::Bare)
}

/// Every knob set simultaneously, opposite-leaning to
/// [`opts_all_asterisk`]. Different rewrite targets across knobs
/// ensure the matrix doesn't trivially collapse to one byte choice.
fn opts_all_underscore_or_dash() -> FmtOptions {
    FmtOptions::default()
        .with_italic(ItalicStyle::Underscore)
        .with_strong(StrongStyle::Underscore)
        .with_list_marker(ListMarkerStyle::Dash)
        .with_thematic_break(ThematicStyle::Dash)
        .with_ordered_list(OrderedListStyle::Consistent)
        .with_link_def_style(LinkDefStyle::Angle)
}

fn check_canon_semantic_equivalence(label: &str, src: &str) -> Result<(), TestCaseError> {
    for (name, opts) in canon_opts() {
        let formatted = Document::parse(src).format(&opts);
        if !semantically_equivalent(src, &formatted) {
            dump_counterexample(&format!("{label}_{name}"), src);
        }
        prop_assert!(
            semantically_equivalent(src, &formatted),
            "canonicalise drift under {name}",
        );
    }
    Ok(())
}

/// Strict byte idempotence: `format(format(s)) == format(s)` under
/// any canonicalisation mode. After the escape-policy + frontmatter
/// fixes alongside prompt 54, this holds for every input the
/// generators emit (no need to weaken to semantic equivalence).
fn check_canon_idempotent(label: &str, src: &str) -> Result<(), TestCaseError> {
    for (name, opts) in canon_opts() {
        let once = Document::parse(src).format(&opts);
        let twice = Document::parse(&once).format(&opts);
        if once != twice {
            dump_counterexample(&format!("{label}_{name}"), src);
        }
        prop_assert_eq!(&once, &twice, "canonicalise non-idempotent under {}", name);
    }
    Ok(())
}

proptest! {
    #[test]
    fn canonicalise_emphasis_semantic_equivalence(s in generators::arb_emphasis_src()) {
        check_canon_semantic_equivalence("canonicalise_emphasis_se", &s)?;
    }
    #[test]
    fn canonicalise_emphasis_idempotent(s in generators::arb_emphasis_src()) {
        check_canon_idempotent("canonicalise_emphasis_idem", &s)?;
    }

    #[test]
    fn canonicalise_strong_semantic_equivalence(s in generators::arb_strong_src()) {
        check_canon_semantic_equivalence("canonicalise_strong_se", &s)?;
    }
    #[test]
    fn canonicalise_strong_idempotent(s in generators::arb_strong_src()) {
        check_canon_idempotent("canonicalise_strong_idem", &s)?;
    }

    #[test]
    fn canonicalise_list_semantic_equivalence(s in generators::arb_list_src()) {
        check_canon_semantic_equivalence("canonicalise_list_se", &s)?;
    }
    #[test]
    fn canonicalise_list_idempotent(s in generators::arb_list_src()) {
        check_canon_idempotent("canonicalise_list_idem", &s)?;
    }

    #[test]
    fn canonicalise_thematic_semantic_equivalence(s in generators::arb_thematic_src()) {
        check_canon_semantic_equivalence("canonicalise_thematic_se", &s)?;
    }
    #[test]
    fn canonicalise_thematic_idempotent(s in generators::arb_thematic_src()) {
        check_canon_idempotent("canonicalise_thematic_idem", &s)?;
    }

    #[test]
    fn canonicalise_link_reference_semantic_equivalence(s in generators::arb_link_reference_src()) {
        check_canon_semantic_equivalence("canonicalise_linkref_se", &s)?;
    }
    #[test]
    fn canonicalise_link_reference_idempotent(s in generators::arb_link_reference_src()) {
        check_canon_idempotent("canonicalise_linkref_idem", &s)?;
    }

    #[test]
    fn canonicalise_link_inline_semantic_equivalence(s in generators::arb_link_inline_src()) {
        check_canon_semantic_equivalence("canonicalise_linkinline_se", &s)?;
    }
    #[test]
    fn canonicalise_link_inline_idempotent(s in generators::arb_link_inline_src()) {
        check_canon_idempotent("canonicalise_linkinline_idem", &s)?;
    }

    /// Whole-document sweep: every canonicalisation mode must preserve
    /// semantics on arbitrary documents and be idempotent on its own
    /// output. This is the per-construct laws' superset.
    #[test]
    fn canonicalise_document_semantic_equivalence(src in generators::arb_document()) {
        check_canon_semantic_equivalence("canonicalise_document_se", &src)?;
    }
    #[test]
    fn canonicalise_document_idempotent(src in generators::arb_document()) {
        check_canon_idempotent("canonicalise_document_idem", &src)?;
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
        prop_assert!(semantically_equivalent(&src, &formatted));
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

    #[test]
    #[ignore = "slow; run with --ignored before release"]
    fn canonicalise_document_semantic_equivalence_sweep(src in generators::arb_document()) {
        check_canon_semantic_equivalence("canonicalise_document_se_sweep", &src)?;
    }

    #[test]
    #[ignore = "slow; run with --ignored before release"]
    fn canonicalise_document_idempotent_sweep(src in generators::arb_document()) {
        check_canon_idempotent("canonicalise_document_idem_sweep", &src)?;
    }
}

// Wrap-DP safety bounds (Phase 4). These exercise the `MAX_WRAP_TOKENS`
// and `MAX_WRAP_TIME` caps in `src/format/wrap.rs`. Gated behind
// `#[ignore]` because a single case generates a megabyte-class
// paragraph; running them on every CI build would be wasteful.
#[test]
#[ignore = "slow: allocates a large paragraph; run with --ignored"]
fn wrap_completes_on_oversized_paragraph_within_time_budget() {
    // 200 000 tiny words ⇒ ~ 1 MB single paragraph, well past the
    // 100 000-token cap. The DP must short-circuit; the whole call
    // should return in well under one second on any machine that can
    // run the rest of the test suite.
    use std::time::{Duration, Instant};
    let mut src = String::with_capacity(2_000_000);
    for i in 0..200_000 {
        if i > 0 {
            src.push(' ');
        }
        src.push_str("word");
    }
    let opts = FmtOptions::default().with_wrap(Wrap::At(80));
    let start = Instant::now();
    let formatted = Document::parse(&src).format(&opts);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "format took {elapsed:?}; bounds should keep this under a second"
    );
    // Output is non-empty and contains the input words.
    assert!(formatted.contains("word"));
}

#[test]
#[ignore = "slow: dense paragraph just under the token cap; run with --ignored"]
fn wrap_completes_on_dense_paragraph_below_cap() {
    // Just under MAX_WRAP_TOKENS so the DP actually runs end-to-end.
    // Validates that the time-budget guard does not fire on
    // realistically-large inputs.
    use std::time::{Duration, Instant};
    let mut src = String::with_capacity(500_000);
    for i in 0..90_000 {
        if i > 0 {
            src.push(' ');
        }
        src.push('w');
    }
    let opts = FmtOptions::default().with_wrap(Wrap::At(80));
    let start = Instant::now();
    let _ = Document::parse(&src).format(&opts);
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(30), "format took {elapsed:?}");
}
