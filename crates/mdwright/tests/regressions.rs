//! Regression driver for property-test counterexamples.
//!
//! Drop a minimal failing input as `tests/regressions/*.md` (header
//! comment naming the property and the date), commit it, then fix
//! the formatter. The fix is done when this test goes green.
//!
//! Only idempotence is enforced here. Property tests in
//! `tests/properties.rs` also check HTML and lint preservation; if
//! a counterexample comes from one of those, the property test
//! itself is the regression test — this driver just makes sure the
//! same input does not regress idempotence.

#![allow(clippy::expect_used, reason = "test fixtures should fail loudly")]
#![allow(clippy::panic, clippy::format_collect)]

use std::fs;
use std::path::{Path, PathBuf};

use mdwright_document::Document;
use mdwright_format::{
    FmtOptions, FormatError, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions, OrderedListStyle, StrongStyle,
    ThematicStyle, Wrap,
};

fn regressions_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("regressions")
}

/// Regression inputs use the `.in` suffix (matching the
/// `tests/golden_*/*.in` convention) so the project's `mdformat`
/// pre-commit hook — which globs `*.md` — does not canonicalise
/// the very inputs we want to preserve.
///
/// A fixture stem ending in `.idem` (e.g. `foo.idem.in`) marks the
/// input as **idempotence-only**: the HTML-equivalence gate is
/// skipped. Reserved for inputs whose source contains bytes that
/// pulldown elides during parse (control characters that form
/// whitespace-only lines), where the trip from source → events
/// already loses information mdwright cannot reconstruct. The
/// `format-validated` gate in `mdwright fmt` refuses to write such
/// outputs in production; the regression harness records the
/// idempotence invariant that the fix actually delivers.
fn input_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "in"))
        .collect();
    out.sort();
    out
}

/// True for fixtures whose filename stem ends in `.idem`. Compared
/// byte-wise via `Path::extension` on the stem so the lookup does not
/// rely on case-insensitive string suffix matching.
fn is_idempotence_only(path: &Path) -> bool {
    let Some(stem) = path.file_stem() else {
        return false;
    };
    Path::new(stem).extension().is_some_and(|ext| ext == "idem")
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

fn inline_slot_options() -> FmtOptions {
    FmtOptions::default()
        .with_italic(ItalicStyle::Asterisk)
        .with_strong(StrongStyle::Asterisk)
        .with_link_def_style(LinkDefStyle::Angle)
}

fn table_normal_form_options() -> FmtOptions {
    FmtOptions::mdformat()
        .with_italic(ItalicStyle::Asterisk)
        .with_strong(StrongStyle::Asterisk)
        .with_link_def_style(LinkDefStyle::Angle)
}

/// Every regression input must round-trip under the HTML-equivalence
/// gate that `mdwright fmt --check` enforces in production. A new
/// `.in` fixture is the canonical way to lock in a previously broken
/// shape: if the formatter ever re-introduces an HTML divergence on
/// any of these inputs, this test fails with the offending file and
/// a diff of the two HTML renderings.
#[test]
fn regression_inputs_preserve_html() {
    let opts = FmtOptions::default();
    let mut failures: Vec<(PathBuf, String, String)> = Vec::new();
    for path in input_files(&regressions_dir()) {
        if is_idempotence_only(&path) {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("regression {} unreadable: {e}", path.display()));
        let doc = Document::parse(&src).expect("fixture parses");
        if let Err(FormatError::SemanticDivergence {
            formatted,
            diff_summary,
            ..
        }) = mdwright_format::format_validated(&doc, &opts)
        {
            failures.push((path, diff_summary, formatted));
        }
    }
    assert!(
        failures.is_empty(),
        "regression inputs whose formatted output diverges semantically from source:\n{}",
        failures
            .iter()
            .map(|(p, summary, formatted)| format!(
                "--- {} ---\n{summary}\n=== formatted ===\n{formatted}\n",
                p.display()
            ))
            .collect::<String>(),
    );
}

#[test]
fn regression_inputs_are_idempotent() {
    let opts = FmtOptions::default();
    let mut failures: Vec<(PathBuf, String, String)> = Vec::new();
    for path in input_files(&regressions_dir()) {
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("regression {} unreadable: {e}", path.display()));
        let once = mdwright_format::format_document(&Document::parse(&src).expect("fixture parses"), &opts);
        let twice = mdwright_format::format_document(&Document::parse(&once).expect("fixture parses"), &opts);
        if once != twice {
            failures.push((path, once, twice));
        }
    }
    assert!(
        failures.is_empty(),
        "non-idempotent regression inputs:\n{}",
        failures
            .iter()
            .map(|(p, a, b)| format!("--- {} ---\n=== once ===\n{a}\n=== twice ===\n{b}\n", p.display()))
            .collect::<String>(),
    );
}

#[test]
fn regression_formfeed_thematic_break_is_idempotent_under_fuzz_profile() {
    let path = regressions_dir().join("fuzz_thematic_formfeed.idem.in");
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("regression {} unreadable: {e}", path.display()));
    let opts = fuzz_option_fa_options();
    let once = mdwright_format::format_document(&Document::parse(&src).expect("fixture parses"), &opts);
    let twice = mdwright_format::format_document(&Document::parse(&once).expect("fixture parses"), &opts);
    assert_eq!(once, twice);
}

#[test]
fn regression_nested_list_markers_are_idempotent_under_fuzz_profile() {
    let opts = fuzz_option_7e_options();
    for name in [
        "fuzz_nested_list_marker_round1.in",
        "fuzz_nested_list_marker_round2.in",
        "fuzz_nested_list_marker_round3.in",
    ] {
        let path = regressions_dir().join(name);
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("regression {} unreadable: {e}", path.display()));
        let once = mdwright_format::format_document(&Document::parse(&src).expect("fixture parses"), &opts);
        let twice = mdwright_format::format_document(&Document::parse(&once).expect("fixture parses"), &opts);
        assert_eq!(once, twice, "non-idempotent fixture: {}", path.display());
    }
}

#[test]
fn regression_inline_slot_canonicalisers_are_idempotent() {
    let opts = inline_slot_options();
    for name in [
        "inline_slot_nested_emphasis.in",
        "inline_slot_emphasis_link_mix.in",
        "inline_slot_adjacent_links.in",
        "inline_slot_container_destinations.in",
        "inline_slot_math_adjacent.in",
    ] {
        let path = regressions_dir().join(name);
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("regression {} unreadable: {e}", path.display()));
        let once = mdwright_format::format_document(&Document::parse(&src).expect("fixture parses"), &opts);
        let twice = mdwright_format::format_document(&Document::parse(&once).expect("fixture parses"), &opts);
        assert_eq!(once, twice, "non-idempotent fixture: {}", path.display());
    }
}

#[test]
fn regression_table_normal_form_is_idempotent_after_child_normalisers() {
    let opts = table_normal_form_options();
    for name in [
        "table_normal_form_inline_delimiters.in",
        "table_normal_form_inline_links.in",
        "table_normal_form_code_and_escaped_pipes.in",
        "table_normal_form_math_and_alignments.in",
        "table_normal_form_ragged_and_padded.in",
    ] {
        let path = regressions_dir().join(name);
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("regression {} unreadable: {e}", path.display()));
        let once = mdwright_format::format_document(&Document::parse(&src).expect("fixture parses"), &opts);
        let twice = mdwright_format::format_document(&Document::parse(&once).expect("fixture parses"), &opts);
        assert_eq!(once, twice, "non-idempotent fixture: {}", path.display());
    }
}
