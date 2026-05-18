//! Regression driver for the `[fmt] heading-attrs` knob.
//!
//! The default-options gate (`regression_inputs_preserve_html` /
//! `regression_inputs_are_idempotent` in `tests/regressions.rs`) picks
//! up `extension_heading_attrs.in` under
//! [`HeadingAttrsStyle::Preserve`] (the default). This file additionally
//! exercises [`HeadingAttrsStyle::Canonicalise`]:
//!
//! 1. **Shape** — the canonical render uses `{#id .class₁ .class₂
//!    k=v}` order with id first, classes in source order, then
//!    `key=value` pairs.
//! 2. **Idempotence-on-mode** — `format(format(src, opts), opts) ==
//!    format(src, opts)` under the chosen style.

#![allow(clippy::panic)]

use mdwright_document::Document;
use mdwright_format::{FmtOptions, FormatError, HeadingAttrsStyle};

fn canonical_opts() -> FmtOptions {
    FmtOptions::default().with_heading_attrs(HeadingAttrsStyle::Canonicalise)
}

#[test]
fn canonicalise_emits_id_then_classes_then_attrs() {
    let src = "# Heading {key=val .alpha #my-id .beta}\n";
    let formatted = mdwright_format::format_document(&Document::parse(src), &canonical_opts());
    assert!(
        formatted.contains("{#my-id .alpha .beta key=val}"),
        "expected canonical order; got: {formatted}"
    );
}

#[test]
fn canonicalise_is_idempotent_on_mode() {
    // Note: pulldown-cmark 0.13's heading-attribute parser splits on
    // whitespace and does not honour double-quoted values
    // (`title="hello world"` becomes two attrs: `title="hello` and
    // `world"`). mdwright's emit path mirrors what pulldown parsed,
    // so quoted-value parity with mdformat-mkdocs is gated on a
    // pulldown upstream fix. See `docs/src/concepts/extensions.md`.
    for src in [
        "# Heading {#id .class}\n",
        "## Heading two {key=val .alpha #my-id .beta}\n",
        "### Heading three {data-x=1 .alpha}\n",
    ] {
        match mdwright_format::format_validated(&Document::parse(src), &canonical_opts()) {
            Ok(_) => {}
            Err(FormatError::SemanticDivergence {
                formatted,
                diff_summary,
                ..
            }) => panic!("not idempotent on Canonicalise: {diff_summary}\n=== formatted ===\n{formatted}"),
        }
    }
}

#[test]
fn preserve_round_trips_unusual_spacing() {
    // Source uses extra spaces inside the trailer; Preserve must keep
    // them. (Canonicalise normalises to single spaces.)
    let src = "# Heading {#id   .class}\n";
    let opts = FmtOptions::default();
    let formatted = mdwright_format::format_document(&Document::parse(src), &opts);
    assert!(
        formatted.contains("{#id   .class}"),
        "expected source spacing preserved; got: {formatted}"
    );
}
