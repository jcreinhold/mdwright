//! Tracks fuzz-found bugs whose fix is awaiting design discussion
//! (parked under `fuzz/known-issues/`).
//!
//! The discipline here matters: we do **not** assert the buggy output
//! bytes as if they were the contract. Two tests per known issue:
//!
//! 1. **`*_invariant`** — the real, correct invariant the formatter
//!    should satisfy, marked `#[ignore]` with the bug pointer. When
//!    the bug is fixed, drop the `#[ignore]`, run it, then move the
//!    fixture to `tests/regressions/` and delete this section.
//!
//! 2. **`*_reproduces`** — asserts only that the fixture still
//!    *exhibits the bug* in some form (e.g. idempotence breaks),
//!    without pinning the exact bytes. Catches silent drift: if the
//!    bug quietly disappears, this test fails and prompts moving the
//!    fixture; if the bug shape mutates, the corresponding
//!    `*_invariant` `#[ignore]`'d test will surface that on the next
//!    `--ignored` run.

#![allow(
    clippy::panic,
    reason = "test fixture loader; missing-file panics are the desired failure mode"
)]

use std::fs;
use std::path::PathBuf;

use mdwright::{Document, FmtOptions};

fn known_issue_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz/known-issues")
        .join(name)
}

fn read(name: &str) -> String {
    let p = known_issue_path(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn format_twice(src: &str) -> (String, String) {
    let opts = FmtOptions::default();
    let once = Document::parse(src).format(&opts);
    let twice = Document::parse(&once).format(&opts);
    (once, twice)
}

// ============================================================
// idempotence-formfeed-paragraph-resplit
//
// Form-feed-only "line" between `K` and `+` is a paragraph
// continuation on the first format pass but list-marker context on
// the second, so the `+` escape doesn't fire on pass 1 and pass 2
// re-splits the paragraph. See section in
// `fuzz/known-issues/README.md` for the design tradeoff.
// ============================================================

const FORMFEED_FIXTURE: &str = "idempotence-formfeed-paragraph-resplit.in";

#[test]
#[ignore = "known bug: fuzz/known-issues/idempotence-formfeed-paragraph-resplit.in"]
fn formfeed_paragraph_resplit_invariant() {
    let src = read(FORMFEED_FIXTURE);
    let (once, twice) = format_twice(&src);
    assert_eq!(once, twice, "format must be idempotent");
}

#[test]
fn formfeed_paragraph_resplit_reproduces() {
    let src = read(FORMFEED_FIXTURE);
    let (once, twice) = format_twice(&src);
    assert_ne!(
        once, twice,
        "fixture no longer reproduces the bug — \
         move it from fuzz/known-issues/ to tests/regressions/ and \
         delete this section from tests/known_issues.rs and from \
         fuzz/known-issues/README.md",
    );
}
