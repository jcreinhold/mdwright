//! Semantic-equivalence comparison for formatter verification.
//!
//! The formatter owns the policy question ("did this rewrite preserve
//! meaning?"). The document crate owns the parser mechanics used to
//! produce comparable Markdown signatures.

use mdwright_document::{ParseError, ParseOptions, markdown_signature};

/// True iff the two Markdown sources parse to the same canonical
/// document signature under default recognition policy.
///
/// # Errors
///
/// Returns [`ParseError`] if either input cannot be parsed safely.
pub fn semantically_equivalent(source: &str, formatted: &str) -> Result<bool, ParseError> {
    semantically_equivalent_with_options(source, formatted, ParseOptions::default())
}

pub(crate) fn semantically_equivalent_with_options(
    source: &str,
    formatted: &str,
    parse_options: ParseOptions,
) -> Result<bool, ParseError> {
    Ok(markdown_signature(source, parse_options)? == markdown_signature(formatted, parse_options)?)
}

/// If `source` and `formatted` are not semantically equivalent,
/// return a short human-readable description of the first divergent
/// event pair. Returns `None` if the streams agree.
///
/// # Errors
///
/// Returns [`ParseError`] if either input cannot be parsed safely.
pub fn first_divergence(source: &str, formatted: &str) -> Result<Option<String>, ParseError> {
    first_divergence_with_options(source, formatted, ParseOptions::default())
}

pub(crate) fn first_divergence_with_options(
    source: &str,
    formatted: &str,
    parse_options: ParseOptions,
) -> Result<Option<String>, ParseError> {
    let source_sig = markdown_signature(source, parse_options)?;
    let formatted_sig = markdown_signature(formatted, parse_options)?;
    Ok(source_sig.first_divergence(&formatted_sig))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::semantically_equivalent;

    #[test]
    fn prose_rewrap_inside_paragraph_is_equivalent() {
        let a = "alpha beta gamma\ndelta epsilon zeta\n";
        let b = "alpha beta gamma delta\nepsilon zeta\n";
        assert!(semantically_equivalent(a, b).expect("semantic check parses"));
    }

    #[test]
    fn prose_rewrap_inside_blockquote_is_equivalent() {
        let a = "> alpha beta gamma\n> delta epsilon zeta\n";
        let b = "> alpha beta gamma delta epsilon\n> zeta\n";
        assert!(semantically_equivalent(a, b).expect("semantic check parses"));
    }

    #[test]
    fn whitespace_change_inside_fenced_code_is_rejected() {
        let a = "```\nfoo\nbar\n```\n";
        let b = "```\nfoo bar\n```\n";
        assert!(!semantically_equivalent(a, b).expect("semantic check parses"));
    }

    #[test]
    fn whitespace_change_inside_inline_code_is_rejected() {
        let a = "see `x  y` here\n";
        let b = "see `x y` here\n";
        assert!(!semantically_equivalent(a, b).expect("semantic check parses"));
    }

    #[test]
    fn dropped_emphasis_is_rejected() {
        let a = "foo *bar* baz\n";
        let b = "foo bar baz\n";
        assert!(!semantically_equivalent(a, b).expect("semantic check parses"));
    }

    #[test]
    fn link_target_change_is_rejected() {
        let a = "[label](https://a.example)\n";
        let b = "[label](https://b.example)\n";
        assert!(!semantically_equivalent(a, b).expect("semantic check parses"));
    }

    #[test]
    fn link_text_rewrap_is_equivalent() {
        let a = "[label one\ntwo](https://x.example)\n";
        let b = "[label one two](https://x.example)\n";
        assert!(semantically_equivalent(a, b).expect("semantic check parses"));
    }

    #[test]
    fn table_cell_whitespace_rewrap_is_equivalent() {
        let a = "| a | b |\n|---|---|\n| x | y |\n";
        let b = "| a   | b   |\n| --- | --- |\n| x   | y   |\n";
        assert!(semantically_equivalent(a, b).expect("semantic check parses"));
    }

    #[test]
    fn dropped_heading_level_is_rejected() {
        let a = "## foo\n";
        let b = "### foo\n";
        assert!(!semantically_equivalent(a, b).expect("semantic check parses"));
    }
}
