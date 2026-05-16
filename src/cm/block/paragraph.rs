//! Paragraphs (CM §4.8).
//!
//! A paragraph carries no per-instance data: its body is its sequence
//! of inline children, and serialisation is the inline body plus a
//! terminating hard newline. The interesting work is the *line-start
//! escape* pass: any text fragment that starts a logical line and
//! happens to open a block construct (ATX heading, list marker,
//! blockquote, fence, thematic break, indented-code prefix) must be
//! backslash-escaped or the round-trip reparses as a different block.

use std::borrow::Cow;

use crate::config::Wrap;
use crate::format::doc::{Doc, concat, hard_line, text};
use crate::format::pretty::PrettyCtx;
use crate::tree::{NodeId, NodeKind};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Paragraph;

impl Paragraph {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new() -> Self {
        Self
    }

    /// Build the paragraph body and append the block terminator. The
    /// inline body is wrapped in [`ParagraphBody`], whose constructor
    /// is the only way to apply paragraph-continuation safety — so
    /// every emitted paragraph is, by construction, immune to the
    /// "continuation line re-tokenises as a different block" family
    /// of round-trip bugs.
    #[tracing::instrument(level = "trace", skip_all)]
    #[allow(clippy::unused_self)]
    pub(crate) fn pretty<'a>(self, ctx: &PrettyCtx<'a>, id: NodeId) -> Doc<'a> {
        let body = crate::format::inline::pretty_inline_children(ctx, id);
        let safe = ParagraphBody::from_inline(body).into_doc();
        concat([safe, hard_line()])
    }

    /// True iff this paragraph can round-trip through verbatim
    /// emission without losing any normalisation. Used by the
    /// document-root overlay to short-circuit IR-driven emission for
    /// paragraphs whose source bytes already match the canonical form.
    ///
    /// Requirements: (a) every inline child is a single-text-segment
    /// [`InlineRun`](crate::cm::inline::run::InlineRun) (no soft/hard
    /// breaks, no structural inlines like emphasis/code/links), so
    /// source-byte emission cannot drop a break the IR would
    /// otherwise have flattened or rewrapped; (b) the wrap policy is
    /// [`Wrap::Keep`] — both [`Wrap::No`] and [`Wrap::At(_)`] require
    /// an IR-driven pass.
    pub(crate) fn is_verbatim_eligible(ctx: &PrettyCtx<'_>, id: NodeId) -> bool {
        if !matches!(ctx.opts.wrap(), Wrap::Keep) {
            return false;
        }
        for child in ctx.tree.children(id) {
            let Some(node) = ctx.tree.node(child) else {
                continue;
            };
            let NodeKind::Run(run) = &node.kind else {
                return false;
            };
            use crate::cm::inline::run::RunPart;
            let mut text_count = 0usize;
            for part in run.parts() {
                match part {
                    RunPart::Text(_) => {
                        text_count = text_count.saturating_add(1);
                        if text_count > 1 {
                            return false;
                        }
                    }
                    RunPart::SoftBreak | RunPart::HardLineBreak | RunPart::HardBreakTag => {
                        return false;
                    }
                }
            }
        }
        true
    }
}

// ============================================================
// ParagraphBody — typed paragraph-body wrapper.
// ============================================================
//
// `Paragraph::pretty` and `list::render_item_body` are the only sites
// that need to emit paragraph-shaped inline content. Both wrap their
// inline `Doc` in this type. The constructor is the only way to
// produce a `ParagraphBody`, and it always runs the safety pass
// (`apply_paragraph_safety`), so the bug class "a paragraph
// continuation line re-tokenises as a different block on reparse"
// is **unrepresentable**: there is no path that constructs a
// `ParagraphBody` without the pass having run.
//
// Adding coverage for a newly-discovered interrupter character (say,
// `<` HTML-block start) is a one-line edit inside the safety pass,
// not a new rule-per-caller-path.

pub(crate) struct ParagraphBody<'a>(Doc<'a>);

impl<'a> ParagraphBody<'a> {
    /// Build from raw inline content. The body is walked and every
    /// text fragment that begins a source line is checked: bytes
    /// that would start a different CM block on reparse are
    /// backslash-escaped.
    pub(crate) fn from_inline(inline: Doc<'a>) -> Self {
        Self(apply_paragraph_safety(inline))
    }

    pub(crate) fn into_doc(self) -> Doc<'a> {
        self.0
    }
}

/// Walk the inline `Doc` and escape any text byte that would start a
/// new block on reparse. Three pieces of state:
///
/// - `at_line_start` — true after `HardLine` (and at entry). The
///   long-standing trigger for the full block-interrupter escape set.
/// - `after_break` — true after `HardLine` *or* `Line` (soft break).
///   `Wrap::Keep` (default) promotes `Line` to a hard newline, so a
///   continuation line behind a soft break is just as line-start in
///   the final byte stream as one behind a hard break.
/// - `prev_line_had_text` — committed when a break is observed.
///   Gates the soft-break escape so we do **not** escape characters
///   at the very first line of a paragraph (where no soft break has
///   yet happened) and so we do **not** escape inside empty-line
///   stretches. Without this gate, the broad form regresses two
///   GFM-spec cases — the gate is what makes the rule narrow enough
///   to ship.
fn apply_paragraph_safety<'a>(doc: Doc<'a>) -> Doc<'a> {
    let mut parts: Vec<Doc<'a>> = Vec::new();
    flatten(doc, &mut parts);
    coalesce_adjacent_text(&mut parts);
    let mut at_line_start = true;
    let mut after_break = true;
    let mut this_line_has_text = false;
    let mut prev_line_had_text = false;
    let mut out: Vec<Doc<'a>> = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        match part {
            Doc::Text(s) => {
                if !s.is_empty() {
                    let escaped = if at_line_start {
                        escape_for_block_start(s.as_ref(), next_on_same_line(&parts, i))
                    } else if after_break && prev_line_had_text {
                        let next = next_on_same_source_line(&parts, i);
                        escape_for_paragraph_interrupt(s.as_ref(), next)
                            .or_else(|| escape_setext_underline(s.as_ref(), next))
                    } else {
                        None
                    };
                    if let Some(esc) = escaped {
                        out.push(text(esc));
                        at_line_start = false;
                        after_break = false;
                        this_line_has_text = true;
                        continue;
                    }
                    at_line_start = false;
                    after_break = false;
                    this_line_has_text = true;
                }
            }
            Doc::HardLine => {
                at_line_start = true;
                after_break = true;
                prev_line_had_text = this_line_has_text;
                this_line_has_text = false;
            }
            Doc::Line => {
                // Soft break: leave `at_line_start` false. The
                // `at_line_start` branch above retains the long-standing
                // hard-break escape behaviour; the soft-break path
                // is governed by the new `after_break +
                // prev_line_had_text` branch.
                at_line_start = false;
                after_break = true;
                prev_line_had_text = this_line_has_text;
                this_line_has_text = false;
            }
            Doc::Concat(_) | Doc::Atomic(_) | Doc::Prefix(_, _) => {
                at_line_start = false;
                after_break = false;
                this_line_has_text = true;
            }
        }
        out.push(part.clone());
    }
    concat(out)
}

fn next_on_same_line(parts: &[Doc<'_>], i: usize) -> LineContext {
    match parts.get(i.saturating_add(1)) {
        Some(Doc::HardLine) | None => LineContext::EndOfLine,
        Some(_) => LineContext::MoreContent,
    }
}

/// Same as `next_on_same_line` but also treats `Doc::Line` (soft
/// break) as the end of the current source line. The setext-underline
/// check uses this variant because the dangerous case is precisely
/// "this text fills the rest of the source line, then the wrap pass
/// turns the soft break into a hard one."
fn next_on_same_source_line(parts: &[Doc<'_>], i: usize) -> LineContext {
    match parts.get(i.saturating_add(1)) {
        Some(Doc::HardLine | Doc::Line) | None => LineContext::EndOfLine,
        Some(_) => LineContext::MoreContent,
    }
}

#[derive(Copy, Clone, Debug)]
enum LineContext {
    MoreContent,
    EndOfLine,
}

fn coalesce_adjacent_text<'a>(parts: &mut Vec<Doc<'a>>) {
    if parts.len() < 2 {
        return;
    }
    let drained: Vec<Doc<'a>> = std::mem::take(parts);
    let mut merged: Vec<Doc<'a>> = Vec::with_capacity(drained.len());
    for part in drained {
        match (merged.last_mut(), part) {
            (Some(Doc::Text(prev)), Doc::Text(next)) => {
                let mut joined = String::with_capacity(prev.len().saturating_add(next.len()));
                joined.push_str(prev.as_ref());
                joined.push_str(next.as_ref());
                *prev = Cow::Owned(joined);
            }
            (_, part) => merged.push(part),
        }
    }
    *parts = merged;
}

// Iterative: a naive recursive walk blows the stack on adversarial
// inputs with deeply nested `Doc::Concat`. `Atomic` and `Prefix` stay
// opaque (matching the previous behaviour); only `Concat` is splayed.
fn flatten<'a>(doc: Doc<'a>, out: &mut Vec<Doc<'a>>) {
    let mut stack: Vec<Doc<'a>> = vec![doc];
    while let Some(node) = stack.pop() {
        match node {
            Doc::Concat(items) => {
                // Push children in reverse so the leftmost pops first
                // and the visit order matches the recursive version.
                for item in items.into_vec().into_iter().rev() {
                    stack.push(item);
                }
            }
            leaf @ (Doc::Text(_)
            | Doc::Line
            | Doc::HardLine
            | Doc::Atomic(_)
            | Doc::Prefix(_, _)) => {
                out.push(leaf);
            }
        }
    }
}

/// Detect the setext-underline pair: a pure run of `=` or `-` filling
/// the rest of a line that follows a line of paragraph text. Returns
/// `Some("\\" + s)` to escape the leading byte so pulldown sees the
/// fragment as plain paragraph text instead of a setext underline.
///
/// The caller is responsible for the "previous line had text" check;
/// this helper only looks at the current fragment.
fn escape_setext_underline(s: &str, next: LineContext) -> Option<String> {
    let bytes = s.as_bytes();
    let first = *bytes.first()?;
    if !matches!(first, b'=' | b'-') {
        return None;
    }
    let mut i = 1usize;
    while i < bytes.len() && bytes.get(i).copied() == Some(first) {
        i = i.saturating_add(1);
    }
    // The run must fill this fragment AND nothing else may follow on
    // the same line (otherwise pulldown sees paragraph text, not an
    // underline).
    if i != bytes.len() || matches!(next, LineContext::MoreContent) {
        return None;
    }
    let mut esc = String::with_capacity(s.len().saturating_add(1));
    esc.push('\\');
    esc.push_str(s);
    Some(esc)
}

/// Strict subset of [`escape_for_block_start`] for the soft-break
/// case. The CM rule "what can interrupt a paragraph" (§5) is stricter
/// than "what starts a block at a hard line": indented code never
/// interrupts; bullet lists must have non-empty content; ordered lists
/// must additionally start at `1`. Mirror those rules here so we do
/// not insert spurious `\` characters that would either
/// (a) make the formatter non-byte-identity on previously-correct
/// inputs, or (b) split pulldown's text events (e.g. `1\.` becomes
/// two `Text` events vs `1.`'s one), which the spec snapshot test
/// detects as an AST regression.
fn escape_for_paragraph_interrupt(s: &str, next: LineContext) -> Option<String> {
    let bytes = s.as_bytes();
    let first = *bytes.first()?;
    let needs = match first {
        // §5.1 blockquote — `>` always interrupts a paragraph.
        b'>' => true,
        // §4.2 ATX heading — `#{1..=6}` followed by space, tab, or
        // end-of-line. Bare `#abc` does not interrupt.
        b'#' => {
            let hash_count = bytes.iter().take_while(|&&b| b == b'#').count();
            if !(1..=6).contains(&hash_count) {
                false
            } else if hash_count == bytes.len() {
                matches!(next, LineContext::EndOfLine)
            } else {
                matches!(bytes.get(hash_count).copied(), Some(b' ' | b'\t'))
            }
        }
        // §4.1 thematic break (`***`/`---`/`___`) — 3+ same chars,
        // optionally separated by spaces or tabs, fills the line.
        // `---` is also handled by `escape_setext_underline`; the
        // overlap is benign (either helper returns `Some`).
        b'*' | b'-' | b'_' if is_thematic_break_line(bytes) => true,
        // §5.2 list bullet — `*`/`-`/`+` then space/tab then NON-BLANK
        // content. An empty marker (`* ` or just `*` on a line) does
        // not interrupt a paragraph.
        b'*' | b'-' | b'+' => {
            matches!(bytes.get(1).copied(), Some(b' ' | b'\t'))
                && line_has_nonblank_after(bytes, 2, next)
        }
        // §5.2 ordered list — only `1.` / `1)` (start = 1) can
        // interrupt. Other digits cannot, and the digit run must be
        // followed by `.` or `)` then space/tab + non-blank content.
        b'1' => {
            matches!(bytes.get(1).copied(), Some(b'.' | b')'))
                && matches!(bytes.get(2).copied(), Some(b' ' | b'\t'))
                && line_has_nonblank_after(bytes, 3, next)
        }
        // §4.5 fenced code block — `` ``` `` or `~~~` of 3+.
        b'`' | b'~' => {
            let run = bytes.iter().take_while(|&&b| b == first).count();
            run >= 3
        }
        _ => false,
    };
    if !needs {
        return None;
    }
    let mut esc = String::with_capacity(s.len().saturating_add(1));
    esc.push('\\');
    esc.push_str(s);
    Some(esc)
}

/// True iff the fragment from `start..` contains at least one
/// non-blank byte, or the fragment ends here and the *next* sibling
/// continues the same source line (so the non-blank content might
/// live there).
fn line_has_nonblank_after(bytes: &[u8], start: usize, next: LineContext) -> bool {
    let tail = bytes.get(start..).unwrap_or(&[]);
    if tail.iter().any(|&b| !matches!(b, b' ' | b'\t')) {
        true
    } else {
        matches!(next, LineContext::MoreContent)
    }
}

/// True iff `bytes` is a CM §4.1 thematic-break line: at least three
/// of the same char (`*`, `-`, or `_`), with only spaces or tabs
/// allowed between them, and nothing else on the line.
fn is_thematic_break_line(bytes: &[u8]) -> bool {
    let Some(first @ (b'*' | b'-' | b'_')) = bytes.first().copied() else {
        return false;
    };
    let mut count = 0usize;
    for &b in bytes {
        if b == first {
            count = count.saturating_add(1);
        } else if !matches!(b, b' ' | b'\t') {
            return false;
        }
    }
    count >= 3
}

fn escape_for_block_start(s: &str, next: LineContext) -> Option<String> {
    let bytes = s.as_bytes();
    let first = *bytes.first()?;
    let two: Option<u8> = bytes.get(1).copied();
    let fragment_continues_inline = two.is_none() && matches!(next, LineContext::MoreContent);
    let needs_escape = match first {
        b'#' => true,
        b'>' => true,
        b'-' | b'+' | b'*' => {
            if fragment_continues_inline {
                false
            } else {
                matches!(two, Some(b' ' | b'\t') | None)
                    || (two == Some(first) && bytes.get(2).copied() == Some(first))
            }
        }
        b'=' => two == Some(b'='),
        b'`' | b'~' => two == Some(first) && bytes.get(2).copied() == Some(first),
        b'0'..=b'9' => {
            let mut i = 1usize;
            while i < bytes.len() && bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i = i.saturating_add(1);
            }
            let punct = bytes.get(i).copied();
            let after = bytes.get(i.saturating_add(1)).copied();
            if punct.is_none() && matches!(next, LineContext::MoreContent) {
                false
            } else {
                matches!(punct, Some(b'.' | b')')) && matches!(after, Some(b' ' | b'\t') | None)
            }
        }
        b' ' if bytes.starts_with(b"    ") => true,
        _ => false,
    };
    if !needs_escape {
        return None;
    }
    let mut esc = String::with_capacity(s.len().saturating_add(2));
    if first.is_ascii_digit() {
        let mut i = 0usize;
        while i < bytes.len() && bytes.get(i).is_some_and(u8::is_ascii_digit) {
            esc.push(char::from(*bytes.get(i)?));
            i = i.saturating_add(1);
        }
        esc.push('\\');
        if let Some(b) = bytes.get(i).copied() {
            esc.push(char::from(b));
            i = i.saturating_add(1);
        }
        esc.push_str(s.get(i..)?);
    } else {
        esc.push('\\');
        esc.push_str(s);
    }
    Some(esc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_is_uniquely_inhabited() {
        assert_eq!(Paragraph::new(), Paragraph);
    }
}
