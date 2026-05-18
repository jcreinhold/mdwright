//! Pinned failure modes that cannot be fixed in-tree.
//!
//! Each test in this file asserts that an *upstream* defect still
//! exhibits the same observable behaviour. When upstream lands a fix
//! the corresponding direct-upstream test will start failing. The
//! mdwright-facing test asserts that the document boundary contains
//! the defect as a controlled parse error.
//!
//! No mdwright bug ever lives here. If a finding traces to mdwright
//! code, fix it; if it traces to a dependency, pin it here.

#![allow(clippy::expect_used, reason = "test scaffolding for an upstream panic pin")]

use std::panic::{AssertUnwindSafe, catch_unwind};

use mdwright_document::Document;
use pulldown_cmark::{Options, Parser};

const LINK_REF_TAB_REPRO: &str = "- [n]:Z\r\n\t\t";

/// Pulldown-cmark 0.13.3 panics with `Option::unwrap` on `None`
/// inside `parse.rs:2367` for the 11-byte input
/// `- [n]:Z\r\n\t\t`. The 32-byte reproducer the fuzz target found
/// was the long-form; the minimum is this one.
///
/// Upstream: <https://github.com/pulldown-cmark/pulldown-cmark/issues/1095>.
#[test]
fn pulldown_panics_on_link_ref_tab() {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let parser = Parser::new_ext(LINK_REF_TAB_REPRO, Options::all());
        for _ in parser {}
    }));
    assert!(
        result.is_err(),
        "pulldown-cmark no longer panics on `- [n]:Z\\r\\n\\t\\t` \
         — remove the upstream pin and the document-boundary containment regression",
    );
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
