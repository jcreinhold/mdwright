//! Regression driver for the `--math-render` flag.
//!
//! Fixtures under `tests/regressions/math_render_*.in` are formatted
//! with `MathRender::Dollar` and checked for:
//!
//! 1. **Shape** — `\[ … \]` and `\( … \)` regions are rewritten to
//!    the dollar form; `\begin{…}…\end{…}` environments are left
//!    untouched.
//! 2. **Idempotence-on-mode** — `format_validated` returns `Ok` under
//!    the chosen mode.
//!
//! The default-options gate (`regression_inputs_preserve_html` in
//! `tests/regressions.rs`) still picks up these fixtures and runs
//! them under `MathRender::None`, so the verbatim path stays
//! exercised too.

#![allow(clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use mdwright_document::Document;
use mdwright_format::{FmtOptions, FormatError, MathOptions, MathRender};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("regressions")
        .join(name)
}

fn dollar_opts() -> FmtOptions {
    FmtOptions::default().with_math(MathOptions {
        normalise: false,
        render: MathRender::Dollar,
    })
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn dollar_rewrites_bracket_and_paren_delimiters() {
    let path = fixture("math_render_dollar.in");
    let src = read(&path);
    let formatted = mdwright_format::format_document(&Document::parse(&src), &dollar_opts());

    // Inline `\(A\)` → `$A$`; display `\[ … \]` → `$$ … $$`. The
    // original delimiters must be gone.
    assert!(
        !formatted.contains(r"\["),
        "bracket display delimiters not rewritten:\n{formatted}"
    );
    assert!(
        !formatted.contains(r"\("),
        "paren inline delimiters not rewritten:\n{formatted}"
    );
    assert!(
        formatted.contains("$$ A v = \\lambda v $$"),
        "expected display dollar form:\n{formatted}"
    );
    assert!(formatted.contains("$A$"), "expected inline `$A$`:\n{formatted}");
}

#[test]
fn dollar_mode_is_idempotent_on_mode() {
    for name in ["math_render_dollar.in", "math_render_roundtrip.in"] {
        let path = fixture(name);
        let src = read(&path);
        match mdwright_format::format_validated(&Document::parse(&src), &dollar_opts()) {
            Ok(_) => {}
            Err(FormatError::SemanticDivergence {
                formatted,
                diff_summary,
                ..
            }) => panic!("{name} not idempotent on Dollar mode: {diff_summary}\n=== formatted ===\n{formatted}"),
        }
    }
}

#[test]
fn dollar_leaves_environments_unchanged() {
    let path = fixture("math_render_roundtrip.in");
    let src = read(&path);
    let formatted = mdwright_format::format_document(&Document::parse(&src), &dollar_opts());

    // `\begin{align*}` and `\end{align*}` must survive verbatim —
    // there is no dollar form of a LaTeX environment.
    assert!(
        formatted.contains(r"\begin{align*}"),
        "environment opener stripped:\n{formatted}"
    );
    assert!(
        formatted.contains(r"\end{align*}"),
        "environment closer stripped:\n{formatted}"
    );
}
