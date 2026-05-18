//! Markdown formatter.
//!
//! The structural emit is identity: [`document::format_document`]
//! returns the source bytes after the canonicalise pass and the
//! wrap pass have rewritten the buffer per [`crate::FmtOptions`].

pub(crate) mod canonicalise;
pub(crate) mod document;
pub(crate) mod semantic;
pub(crate) mod wrap_pass;

use crate::config::{EndOfLine, TrailingNewline};

/// Apply the trailing-newline policy at the document boundary.
///
/// `Preserve` (the default) shapes the output to match the source's
/// trailing-newline run: one terminating `\n` if the source had any,
/// none otherwise. This is the only policy that survives
/// `Document::format_validated` on inputs ending in an indented or
/// fenced code block, where any LF the post-pass introduces lands
/// inside the code body on re-parse and changes the rendered HTML.
/// See `docs/architecture/pulldown-model.md` §2 for the trailing-blank-
/// line rule this post-pass exists to defend against.
///
/// The "did the source end with `\n`?" probe ignores trailing
/// horizontal whitespace (`' '` / `'\t'`). Pulldown treats a final
/// line of only spaces/tabs as a stripped trailing blank line: the
/// effective document ends one `\n` earlier than the byte count
/// suggests. Without the trim, source `\t|\n\t` (indented code,
/// content `|\n`, trailing tab-only blank line) reads as
/// "no trailing `\n`", so the boundary strips the code block's
/// content `\n` and the re-parse sees content `|` instead of `|\n`
/// (`fuzz_indented_code_trailing_ws_drop.in`).
///
/// `Strip` drops every trailing `\n`. `Ensure` forces exactly one
/// trailing `\n`.
pub(crate) fn normalize_trailing_newline(out: &mut String, policy: TrailingNewline, source: &str) {
    match policy {
        TrailingNewline::Preserve => {
            // Match the source's trailing-LF count exactly. Constructs
            // whose body content includes trailing LFs (e.g. an
            // unclosed fenced code block whose body is one LF: source
            // `` ```\n\n `` has the second LF as body content, the
            // first as the opener terminator) would be corrupted by a
            // strip-and-add-one policy. Counting source LFs (modulo
            // trailing horizontal whitespace, see
            // [`source_has_effective_trailing_newline`]) and matching
            // exactly preserves the body without truncation.
            let source_lf = trailing_lf_count(source);
            let mut out_lf = trailing_lf_count(out);
            while out_lf > source_lf {
                let _ = out.pop();
                out_lf = out_lf.saturating_sub(1);
            }
            while out_lf < source_lf {
                out.push('\n');
                out_lf = out_lf.saturating_add(1);
            }
        }
        TrailingNewline::Strip => {
            while out.ends_with('\n') {
                let _ = out.pop();
            }
        }
        TrailingNewline::Ensure => {
            while out.ends_with('\n') {
                let _ = out.pop();
            }
            out.push('\n');
        }
    }
}

/// Count of trailing `\n` bytes after first trimming any horizontal
/// whitespace (`' '` / `'\t'`) suffix. The trim matches pulldown's
/// effective-trailing-blank-line rule (CM §4.4 / 4.6): a final line of
/// only spaces/tabs is stripped, so the document's effective
/// trailing-LF count is the LF run immediately before that trailing
/// whitespace.
fn trailing_lf_count(s: &str) -> usize {
    let trimmed = s.trim_end_matches([' ', '\t']);
    trimmed.bytes().rev().take_while(|b| *b == b'\n').count()
}

/// Normalise every `\r\n` and lone `\r` in `out` to `\n`.
///
/// `format_document` operates on `Source::canonical()` bytes, which
/// are already LF-only. This pass is cheap belt-and-braces
/// (`.contains('\r')` early-out; zero allocation when clean) for
/// callers that bypass the canonicalisation.
pub(crate) fn normalize_line_endings_lf(out: &mut String) {
    if !out.contains('\r') {
        return;
    }
    let normalized = out.replace("\r\n", "\n").replace('\r', "\n");
    *out = normalized;
}

/// Apply the end-of-line policy to a freshly-rendered `String`.
/// Caller invariant: `out` contains only `\n` line terminators
/// (enforced by [`normalize_line_endings_lf`] inside
/// `format_document`). Converting to CRLF is then a straightforward
/// replace; `Keep` adopts the source's first newline style.
pub(crate) fn apply_end_of_line(out: &mut String, policy: EndOfLine, source: &str) {
    let target = match policy {
        EndOfLine::Lf => "\n",
        EndOfLine::Crlf => "\r\n",
        EndOfLine::Keep => {
            if source.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            }
        }
    };
    if target == "\n" {
        return;
    }
    *out = out.replace('\n', target);
}

pub(crate) use document::format_document;
