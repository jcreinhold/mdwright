//! `CommonMark` line-start escape decisions for paragraph
//! continuation safety. Called from the run-stream walker in
//! [`crate::format::inline::pretty_paragraph_inline`] for each
//! `RunPart::Text` payload encountered at a line-start position.
//!
//! The three escape functions answer a single question per text
//! fragment at a line-start position: *"if I emit this fragment
//! verbatim, will pulldown reparse it as a different block?"* If yes,
//! return `Some(escaped)`; if no, return `None`. The caller decides
//! which of the three escape sets applies based on context:
//!
//! - [`escape_for_block_start`] — full CM block-start set (ATX,
//!   blockquote, list, fence, thematic, indented code). Applies at
//!   absolute line-start (after a `HardLine`) or after a soft break
//!   that follows a blank line.
//! - [`escape_for_paragraph_interrupt`] — strict CM §5 subset
//!   (paragraph-interrupters only). Applies after a soft break whose
//!   previous line carried text.
//! - [`escape_setext_underline`] — detects a pure `=` / `-` run that
//!   would otherwise reparse the previous text line as a setext
//!   heading. Applies in the same context as
//!   `escape_for_paragraph_interrupt`.

#![allow(dead_code)]
/// What follows the current text fragment on the same source line.
/// Several escape decisions depend on whether more content arrives
/// before the next line break (e.g. a digit fragment ending in `1`
/// is a list marker only if `.` or `)` follows).
#[derive(Copy, Clone, Debug)]
pub(crate) enum LineContext {
    MoreContent,
    EndOfLine,
}

/// Detect the setext-underline pair: a pure run of `=` or `-` filling
/// the rest of a line that follows a line of paragraph text. Returns
/// `Some("\\" + s)` to escape the leading byte so pulldown sees the
/// fragment as plain paragraph text instead of a setext underline.
///
/// The caller is responsible for the "previous line had text" check;
/// this helper only looks at the current fragment.
pub(crate) fn escape_setext_underline(s: &str, next: LineContext) -> Option<String> {
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
pub(crate) fn escape_for_paragraph_interrupt(s: &str, next: LineContext) -> Option<String> {
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
            matches!(bytes.get(1).copied(), Some(b' ' | b'\t')) && line_has_nonblank_after(bytes, 2, next)
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

pub(crate) fn escape_for_block_start(s: &str, next: LineContext) -> Option<String> {
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
                matches!(two, Some(b' ' | b'\t') | None) || (two == Some(first) && bytes.get(2).copied() == Some(first))
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
