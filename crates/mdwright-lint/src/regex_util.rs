use regex::Regex;

/// Compile a regular expression from a source-literal pattern.
#[allow(
    clippy::unwrap_used,
    reason = "static regex literal verified at first use; failure is a build-time bug in this crate"
)]
pub(crate) fn compile_static(pattern: &'static str) -> Regex {
    Regex::new(pattern).unwrap()
}
