//! Internal helpers for compiling regular expressions from source
//! literals.
//!
//! Centralising the `Regex::new(...).unwrap()` pattern keeps the
//! `clippy::unwrap_used` exception confined to a single, well-named
//! function whose precondition (the pattern is a `&'static str`,
//! i.e. fixed at source-write time) is encoded in the type signature.

use regex::Regex;

/// Compile a regular expression from a source-literal pattern.
///
/// The pattern must be a `&'static str`; in practice every call site
/// passes a raw string literal embedded in the binary. Compilation
/// failure indicates a malformed literal in this crate's source, not
/// runtime input, so it is treated as a build-time bug surfaced
/// through panic at first call.
#[allow(
    clippy::unwrap_used,
    reason = "static regex literal verified at first use; failure is a build-time bug in this crate"
)]
pub(crate) fn compile_static(pattern: &'static str) -> Regex {
    Regex::new(pattern).unwrap()
}
