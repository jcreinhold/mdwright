//! Pinned failure modes that cannot be fixed in-tree.
//!
//! Each test in this file asserts that an *upstream* defect still
//! exhibits the same observable behaviour. When upstream lands a fix
//! the corresponding test will start failing — that failure is the
//! signal to drop the workaround in this crate (typically a
//! `catch_unwind` skip in `fuzz/fuzz_targets/*`).
//!
//! No mdwright bug ever lives here. If a finding traces to mdwright
//! code, fix it; if it traces to a dependency, pin it here.

use std::panic::{AssertUnwindSafe, catch_unwind};

use pulldown_cmark::{Options, Parser};

/// Pulldown-cmark 0.13.3 panics with `Option::unwrap` on `None`
/// inside `parse.rs:2367` for the 11-byte input
/// `- [n]:Z\r\n\t\t`. The 32-byte reproducer the fuzz target found
/// was the long-form; the minimum is this one. The fuzz oracles
/// `fuzz_parse_format`, `fuzz_idempotence`, `fuzz_lint`, and
/// `fuzz_verbatim_identity` skip inputs that trigger this panic via
/// `catch_unwind`. If upstream fixes it, this test fails and those
/// `catch_unwind` wrappers can be removed.
///
/// PENDING: track at https://github.com/raphlinus/pulldown-cmark/
/// (file an issue with the 11-byte repro before removing this pin).
#[test]
fn pulldown_panics_on_link_ref_tab() {
    let input = b"- [n]:Z\r\n\t\t";
    let s = std::str::from_utf8(input).expect("utf-8");
    let result = catch_unwind(AssertUnwindSafe(|| {
        let parser = Parser::new_ext(s, Options::all());
        for _ in parser {}
    }));
    assert!(
        result.is_err(),
        "pulldown-cmark no longer panics on `- [n]:Z\\r\\n\\t\\t` \
         — remove the catch_unwind skips in fuzz/fuzz_targets/*.rs \
         and delete this pin",
    );
}
