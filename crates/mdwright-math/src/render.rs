//! Delimiter rewrite helper for dollar-shaped renderer output.
//!
//! mdwright never typesets math itself. Downstream renderers
//! (`KaTeX`, `MathJax`, `mkdocs-material`'s math plugin) do. The
//! recogniser tags each math region with its source delimiter family;
//! this module rewrites those delimiters at emit time so the resulting
//! bytes match the shape the downstream renderer expects.
//!
//! The pass is a function rather than a method on [`super::MathRegion`]
//! because canonicalisation already has the `(&MathSpan, &Range<usize>)`
//! pair in scope.

use std::borrow::Cow;
use std::ops::Range;

use super::span::MathSpan;
/// Rewrite the source bytes of a math region to dollar delimiters.
///
/// `range` is the **outer** byte range (delimiters + body, in source).
/// `span` is the scanner-tagged classification.
///
/// - [`MathSpan::Environment`]: borrow source unchanged — there is no
///   dollar form of `\begin{name}…\end{name}`.
/// - [`MathSpan::Inline`]: emit `${body}$`.
/// - [`MathSpan::Display`]: emit `$$ {body} $$`.
///
/// The body is read through [`super::span::MathBody::as_str`], which
/// already strips container prefixes (blockquote `>`, list-item
/// continuation indentation). Hand-slicing the source and stripping
/// the delimiters here would re-do that work and silently break for
/// container-nested math.
pub fn convert_for_dollar<'a>(source: &'a str, range: &Range<usize>, span: &MathSpan) -> Cow<'a, str> {
    match span {
        MathSpan::Environment { .. } => Cow::Borrowed(source.get(range.clone()).unwrap_or("")),
        MathSpan::Display { body, .. } => {
            let body = body.as_str(source);
            Cow::Owned(format!("$$ {} $$", body.trim()))
        }
        MathSpan::Inline { body, .. } => {
            let body = body.as_str(source);
            Cow::Owned(format!("${}$", body.trim()))
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;
    use crate::env::{EnvKind, KnownEnv};
    use crate::span::{DisplayDelim, InlineDelim, MathBody};

    #[test]
    fn dollar_rewrites_display_bracket() {
        let source = r"\[ A = B \]";
        let body = MathBody::new(2..9, Box::new([]));
        let span = MathSpan::Display {
            delim: DisplayDelim::Bracket,
            body,
        };
        let out = convert_for_dollar(source, &(0..source.len()), &span);
        assert_eq!(&*out, "$$ A = B $$");
    }

    #[test]
    fn dollar_rewrites_inline_paren() {
        let source = r"\(B\)";
        let body = MathBody::new(2..3, Box::new([]));
        let span = MathSpan::Inline {
            delim: InlineDelim::Paren,
            body,
        };
        let out = convert_for_dollar(source, &(0..source.len()), &span);
        assert_eq!(&*out, "$B$");
    }

    #[test]
    fn dollar_rewrites_inline_dollar_unchanged_shape() {
        // Source already uses dollar — output is still dollar, body trimmed.
        let source = "$x + y$";
        let body = MathBody::new(1..6, Box::new([]));
        let span = MathSpan::Inline {
            delim: InlineDelim::Dollar,
            body,
        };
        let out = convert_for_dollar(source, &(0..source.len()), &span);
        assert_eq!(&*out, "$x + y$");
    }

    #[test]
    fn dollar_passes_environment_through() {
        let source = "\\begin{align*}\na &= b\n\\end{align*}";
        // env-kind lookup wants the source slice; the actual kind
        // doesn't matter for the pass-through assertion.
        let env = EnvKind::Known(KnownEnv::AlignStar);
        let body = MathBody::new(14..22, Box::new([]));
        let span = MathSpan::Environment { env, body };
        let out = convert_for_dollar(source, &(0..source.len()), &span);
        assert!(matches!(out, Cow::Borrowed(_)));
        assert_eq!(&*out, source);
    }

    #[test]
    fn dollar_blockquote_nested_strips_transparent_runs() {
        // `> \[ x \]` — the body range covers ` x `; the transparent
        // run covers the `> ` prefix that would be doubled if we did
        // not consult MathBody::as_str.
        let source = "> \\[ x \\]";
        // Body between delimiters: ` x ` at bytes 4..7.
        let body = MathBody::new(4..7, Box::new([]));
        let span = MathSpan::Display {
            delim: DisplayDelim::Bracket,
            body,
        };
        let out = convert_for_dollar(source, &(2..9), &span);
        assert_eq!(&*out, "$$ x $$");
    }
}
