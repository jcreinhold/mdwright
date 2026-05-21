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

#![allow(clippy::expect_used, clippy::unwrap_used, let_underscore_drop)]

use std::collections::HashSet;
use std::io::Write;

use mdwright_document::Document;
use mdwright_format::{
    FmtOptions, HeadingAttrsStyle, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions, MathRender,
    OrderedListStyle, StrongStyle, TableStyle, ThematicStyle, Wrap, format_range, semantically_equivalent,
};
use mdwright_lint::RuleSet;
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

fn parse_prop(src: &str) -> Result<Document, TestCaseError> {
    Document::parse(src).map_err(|err| TestCaseError::reject(format!("parser rejected generated input: {err}")))
}

fn semantic_prop(source: &str, formatted: &str) -> Result<bool, TestCaseError> {
    semantically_equivalent(source, formatted)
        .map_err(|err| TestCaseError::reject(format!("semantic check rejected generated input: {err}")))
}

/// Heuristic: does `src` contain a top-level link-def or footnote-def
/// line? Both are document-scope constructs that the formatter may
/// reorder, breaking the literal substring contract for ranges that
/// straddle them. Used by `range_format_is_substring_of_whole` to
/// skip cases where the contract is documented not to hold.
fn has_reorderable_def(src: &str) -> bool {
    for line in src.lines() {
        let t = line.trim_start();
        // Link definitions look like `[label]: dest` at the start of
        // a non-indented line; footnote defs look like `[^label]: …`.
        if let Some(rest) = t.strip_prefix('[')
            && rest.contains("]:")
        {
            return true;
        }
    }
    false
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
        let once = mdwright_format::format_document(&parse_prop(&src)?, &opts);
        let twice = mdwright_format::format_document(&parse_prop(&once)?, &opts);
        prop_assert_eq!(&once, &twice);
        prop_assert!(semantic_prop(&src, &once)?);
    }

    #[test]
    fn idempotent(src in generators::arb_document()) {
        let opts = FmtOptions::default();
        let once = mdwright_format::format_document(&parse_prop(&src)?, &opts);
        let twice = mdwright_format::format_document(&parse_prop(&once)?, &opts);
        if once != twice {
            dump_counterexample("idempotent", &src);
        }
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn html_preserving(src in generators::arb_document()) {
        for opts in opts_matrix() {
            let formatted = mdwright_format::format_document(&parse_prop(&src)?, &opts);
            if !semantic_prop(&src, &formatted)? {
                dump_counterexample("html", &src);
            }
            prop_assert!(semantic_prop(&src, &formatted)?);
        }
    }

    /// Substring contract for `format_range`: for any well-formed
    /// source without document-scope reorderable constructs, the
    /// range-format output appears verbatim somewhere in the
    /// whole-document output.
    ///
    /// The filter excludes sources that contain link definitions or
    /// footnote definitions: those are document-scope and the
    /// formatter may reorder them per `LinkDefStyle` /
    /// `Placement` (e.g., link defs collected to document end).
    /// A sliced sub-document keeps the def adjacent to its reference;
    /// the whole-document output may put intervening blocks between
    /// them. See the `format_range` doc comment for the caveat.
    #[test]
    fn range_format_is_substring_of_whole(
        src in generators::arb_document(),
        range_pair in (0usize..2_000usize, 0usize..2_000usize),
    ) {
        prop_assume!(!has_reorderable_def(&src));
        let opts = FmtOptions::default();
        let doc = parse_prop(&src)?;
        let whole = mdwright_format::format_document(&doc, &opts);
        let len = src.len();
        let lo = range_pair.0.min(len);
        let hi = range_pair.1.min(len).max(lo);
        let part = format_range(&doc, &opts, lo..hi);
        if !whole.contains(&part) {
            dump_counterexample("range-substring", &src);
            eprintln!("range = {lo}..{hi}");
            eprintln!("part  = {part:?}");
            eprintln!("whole = {whole:?}");
        }
        prop_assert!(whole.contains(&part));
    }

    #[test]
    fn lint_preserving(src in generators::arb_document()) {
        let rules = RuleSet::stdlib_defaults();
        let doc = parse_prop(&src)?;
        let before: HashSet<String> = rules
            .check(&doc)
            .into_iter()
            .map(|d| d.rule.into_owned())
            .filter(|r| r != "bare-url")
            .collect();
        let formatted = mdwright_format::format_document(&doc, &FmtOptions::default());
        let formatted_doc = parse_prop(&formatted)?;
        let after: HashSet<String> = rules
            .check(&formatted_doc)
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
// Coupling tests to internal representation types would bake an
// implementation choice into the test suite.
// ---------------------------------------------------------------------------

fn check_idempotent(label: &str, src: &str) -> Result<(), TestCaseError> {
    let opts = FmtOptions::default();
    let once = mdwright_format::format_document(&parse_prop(src)?, &opts);
    let twice = mdwright_format::format_document(&parse_prop(&once)?, &opts);
    if once != twice {
        dump_counterexample(label, src);
    }
    prop_assert_eq!(once, twice);
    Ok(())
}

fn check_html_preserving(label: &str, src: &str) -> Result<(), TestCaseError> {
    for opts in opts_matrix() {
        let formatted = mdwright_format::format_document(&parse_prop(src)?, &opts);
        if !semantic_prop(src, &formatted)? {
            dump_counterexample(label, src);
        }
        prop_assert!(semantic_prop(src, &formatted)?);
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
        FmtOptions::default().with_table(TableStyle::Align),
        FmtOptions::default().with_table(TableStyle::Preserve),
        FmtOptions::default().with_wrap(Wrap::At(80)),
        FmtOptions::default().with_wrap(Wrap::At(120)),
        FmtOptions::default().with_wrap(Wrap::No),
    ]
}

fn fuzz_option_fa_options() -> FmtOptions {
    FmtOptions::default()
        .with_wrap(Wrap::At(80))
        .with_italic(ItalicStyle::Underscore)
        .with_strong(StrongStyle::Underscore)
        .with_list_marker(ListMarkerStyle::Dash)
        .with_thematic_break(ThematicStyle::Dash)
        .with_ordered_list(OrderedListStyle::Consistent)
        .with_link_def_style(LinkDefStyle::Angle)
}

fn fuzz_option_7e_options() -> FmtOptions {
    FmtOptions::default()
        .with_wrap(Wrap::At(80))
        .with_math(MathOptions {
            normalise: true,
            ..MathOptions::default()
        })
        .with_list_marker(ListMarkerStyle::Plus)
}

fn all_family_law_options() -> FmtOptions {
    FmtOptions::mdformat()
        .with_wrap(Wrap::At(64))
        .with_italic(ItalicStyle::Underscore)
        .with_strong(StrongStyle::Asterisk)
        .with_link_def_style(LinkDefStyle::Angle)
        .with_math(MathOptions {
            normalise: true,
            render: MathRender::Dollar,
        })
        .with_heading_attrs(HeadingAttrsStyle::Canonicalise)
        .with_preserve_frontmatter(false)
}

fn rewrite_law_opts() -> Vec<(&'static str, FmtOptions)> {
    vec![
        ("preserve", FmtOptions::default()),
        ("mdformat", FmtOptions::mdformat()),
        ("fuzz_option_0xfa", fuzz_option_fa_options()),
        ("fuzz_option_0x7e", fuzz_option_7e_options()),
        ("all_families", all_family_law_options()),
    ]
}

fn check_rewrite_law_profiles(label: &str, src: &str) -> Result<(), TestCaseError> {
    for (profile, opts) in rewrite_law_opts() {
        let doc = parse_prop(src)?;
        let (once, first_report) = mdwright_format::format_document_with_report(&doc, &opts);
        let once_doc = parse_prop(&once)?;
        let (twice, second_report) = mdwright_format::format_document_with_report(&once_doc, &opts);
        if once != twice {
            dump_counterexample(&format!("{label}_{profile}"), src);
        }
        prop_assert_eq!(
            first_report.rewrite_rejected_convergence,
            0,
            "rewrite convergence guard fired under {}",
            profile,
        );
        prop_assert_eq!(
            second_report.rewrite_rejected_convergence,
            0,
            "second pass convergence guard fired under {}",
            profile,
        );
        prop_assert_eq!(
            second_report.rewrite_committed,
            0,
            "second pass committed rewrites under {}: {:?}",
            profile,
            second_report,
        );
        prop_assert_eq!(&once, &twice, "format law failed under {}", profile);
    }
    Ok(())
}

fn check_source_convenience_rewrite_law(label: &str, src: &str) -> Result<(), TestCaseError> {
    for (profile, opts) in rewrite_law_opts() {
        let once = mdwright_format::format_source(src, &opts)
            .map_err(|err| TestCaseError::reject(format!("source formatter rejected generated input: {err}")))?;
        let twice = mdwright_format::format_source(&once, &opts)
            .map_err(|err| TestCaseError::reject(format!("source formatter rejected its own output: {err}")))?;
        if once != twice {
            dump_counterexample(&format!("{label}_{profile}_source"), src);
        }
        prop_assert_eq!(&once, &twice, "source formatter law failed under {}", profile);
    }
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

// ---------------------------------------------------------------------------
// Rewrite law gates.
//
// These properties target construct combinations that previously made
// the rewrite engine return safe partial progress: nested markers,
// nested inline slots, table parent rewrites with inline children,
// terminal wrap with atomics, link destination slots, math, and
// frontmatter. The oracle checks the public formatter law and also
// asserts that the second pass commits no rewrites.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, .. ProptestConfig::default() })]

    #[test]
    fn rewrite_interactions_are_profile_idempotent(s in generators::arb_rewrite_interaction_src()) {
        check_rewrite_law_profiles("rewrite_interactions_profile_idem", &s)?;
    }

    #[test]
    fn rewrite_interactions_source_api_is_profile_idempotent(s in generators::arb_rewrite_interaction_src()) {
        check_source_convenience_rewrite_law("rewrite_interactions_source_idem", &s)?;
    }

    #[test]
    fn nested_list_interactions_are_profile_idempotent(s in generators::arb_nested_list_interaction_src()) {
        check_rewrite_law_profiles("nested_list_interactions_profile_idem", &s)?;
    }

    #[test]
    fn nested_inline_interactions_are_profile_idempotent(s in generators::arb_nested_inline_interaction_src()) {
        check_rewrite_law_profiles("nested_inline_interactions_profile_idem", &s)?;
    }

    #[test]
    fn table_inline_interactions_are_profile_idempotent(s in generators::arb_table_inline_interaction_src()) {
        check_rewrite_law_profiles("table_inline_interactions_profile_idem", &s)?;
    }

    #[test]
    fn wrap_atomic_interactions_are_profile_idempotent(s in generators::arb_wrap_atomic_interaction_src()) {
        check_rewrite_law_profiles("wrap_atomic_interactions_profile_idem", &s)?;
    }

    #[test]
    fn link_destination_interactions_are_profile_idempotent(s in generators::arb_link_destination_interaction_src()) {
        check_rewrite_law_profiles("link_destination_interactions_profile_idem", &s)?;
    }

    #[test]
    fn math_frontmatter_interactions_are_profile_idempotent(s in generators::arb_math_frontmatter_interaction_src()) {
        check_rewrite_law_profiles("math_frontmatter_interactions_profile_idem", &s)?;
    }
}

// ---------------------------------------------------------------------------
// Canonicalisation laws.
//
// For each style knob, canonicalisation rewrite candidates must hold:
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
        let formatted = mdwright_format::format_document(&parse_prop(src)?, &opts);
        if !semantic_prop(src, &formatted)? {
            dump_counterexample(&format!("{label}_{name}"), src);
        }
        prop_assert!(semantic_prop(src, &formatted)?, "canonicalise drift under {name}",);
    }
    Ok(())
}

/// Strict byte idempotence: `format(format(s)) == format(s)` under
/// any canonicalisation mode. Holds for every input the generators
/// emit (no need to weaken to semantic equivalence).
fn check_canon_idempotent(label: &str, src: &str) -> Result<(), TestCaseError> {
    for (name, opts) in canon_opts() {
        let once = mdwright_format::format_document(&parse_prop(src)?, &opts);
        let twice = mdwright_format::format_document(&parse_prop(&once)?, &opts);
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
        let once = mdwright_format::format_document(&parse_prop(&src)?, &opts);
        let twice = mdwright_format::format_document(&parse_prop(&once)?, &opts);
        prop_assert_eq!(once, twice);
    }

    #[test]
    #[ignore = "slow; run with --ignored before release"]
    fn html_preserving_sweep(src in generators::arb_document()) {
        let opts = FmtOptions::default();
        let formatted = mdwright_format::format_document(&parse_prop(&src)?, &opts);
        prop_assert!(semantic_prop(&src, &formatted)?);
    }

    #[test]
    #[ignore = "slow; run with --ignored before release"]
    fn lint_preserving_sweep(src in generators::arb_document()) {
        let rules = RuleSet::stdlib_defaults();
        let doc = parse_prop(&src)?;
        let before: HashSet<String> = rules
            .check(&doc)
            .into_iter()
            .map(|d| d.rule.into_owned())
            .filter(|r| r != "bare-url")
            .collect();
        let formatted = mdwright_format::format_document(&doc, &FmtOptions::default());
        let formatted_doc = parse_prop(&formatted)?;
        let after: HashSet<String> = rules
            .check(&formatted_doc)
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

// Wrap-DP safety bounds. These exercise the `MAX_WRAP_TOKENS`
// and `MAX_WRAP_TIME` caps in the wrap pass. Gated behind
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
    let formatted =
        mdwright_format::format_document(&Document::parse(&src).expect("large generated document parses"), &opts);
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
    let _ = mdwright_format::format_document(&Document::parse(&src).expect("large generated document parses"), &opts);
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(30), "format took {elapsed:?}");
}
