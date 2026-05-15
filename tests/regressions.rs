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

#![allow(clippy::panic, clippy::format_collect)]

use std::fs;
use std::path::{Path, PathBuf};

use mdwright::{Document, FmtOptions, FormatError};

fn regressions_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("regressions")
}

/// Regression inputs use the `.in` suffix (matching the
/// `tests/golden_*/*.in` convention) so the project's `mdformat`
/// pre-commit hook — which globs `*.md` — does not canonicalise
/// the very inputs we want to preserve.
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
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("regression {} unreadable: {e}", path.display()));
        let doc = Document::parse(&src);
        if let Err(FormatError::HtmlDivergence {
            source_html,
            formatted_html,
            ..
        }) = doc.format_validated(&opts)
        {
            failures.push((path, source_html, formatted_html));
        }
    }
    assert!(
        failures.is_empty(),
        "regression inputs whose formatted HTML diverges from source HTML:\n{}",
        failures
            .iter()
            .map(|(p, a, b)| format!(
                "--- {} ---\n=== source HTML ===\n{a}\n=== formatted HTML ===\n{b}\n",
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
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("regression {} unreadable: {e}", path.display()));
        let once = Document::parse(&src).format(&opts);
        let twice = Document::parse(&once).format(&opts);
        if once != twice {
            failures.push((path, once, twice));
        }
    }
    assert!(
        failures.is_empty(),
        "non-idempotent regression inputs:\n{}",
        failures
            .iter()
            .map(|(p, a, b)| format!(
                "--- {} ---\n=== once ===\n{a}\n=== twice ===\n{b}\n",
                p.display()
            ))
            .collect::<String>(),
    );
}
