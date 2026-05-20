//! Pinned dependency behaviours that affect mdwright's parser boundary.
//!
//! Each test in this file asserts an upstream behaviour that has shaped
//! a document-boundary regression. If upstream changes again, update
//! the direct-upstream assertion and keep the mdwright-facing test at
//! the parser boundary.
//!
//! No mdwright bug ever lives here. If a finding traces to mdwright
//! code, fix it; if it traces to a dependency, pin it here.

#![allow(clippy::expect_used, reason = "test scaffolding for an upstream parser pin")]

use mdwright_document::Document;
use pulldown_cmark::{Options, Parser};

const LINK_REF_TAB_REPRO: &str = "- [n]:Z\r\n\t\t";

/// Pulldown-cmark used to panic with `Option::unwrap` on `None` for
/// the 11-byte input `- [n]:Z\r\n\t\t`. The direct parser no longer
/// panics on the pinned reproducer; keep this test so the old upstream
/// failure mode does not silently become ambiguous history.
///
/// Upstream: <https://github.com/pulldown-cmark/pulldown-cmark/issues/1095>.
#[test]
fn pulldown_accepts_link_ref_tab_without_panicking() {
    let parser = Parser::new_ext(LINK_REF_TAB_REPRO, Options::all());
    for _ in parser {}
}

#[test]
fn mdwright_document_contains_link_ref_tab_panic() {
    let err = Document::parse(LINK_REF_TAB_REPRO).expect_err("pulldown panic is contained");
    assert!(err.input_len() > 0);
    assert!(
        err.to_string().contains("Markdown parser failed"),
        "unexpected parse error text: {err}"
    );
}
