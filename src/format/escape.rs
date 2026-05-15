//! Inline-text escape policy.
//!
//! [`escape_text`] is the only public entry point. It walks an
//! already-parsed text run (`pulldown-cmark` has resolved source-level
//! backslash escapes) and re-emits the bytes that need an escape so
//! the result round-trips back to the same logical text under
//! `CommonMark`.
//!
//! ## The policy
//!
//! Minimum-necessary escape: never insert a backslash unless the
//! `CommonMark` tokenizer would otherwise pick the byte up as syntax.
//! No bad-neighbour heuristics — every decision is local and tied to
//! a CM §-citation. Math-heavy identifiers like `id_S`, `Hom_{cart}`,
//! `a_b_c` are preserved verbatim because intraword `_` is text per
//! CM §6.2 rule 6 (this is CM-core, not GFM-only).
//!
//! The eight rules:
//!
//! | Byte    | Escape when                                            | CM §       |
//! |---------|--------------------------------------------------------|------------|
//! | `\\`    | next byte is in the CM punctuation set                 | §2.4       |
//! | `*`     | could open or close emphasis (left/right-flanking)     | §6.2       |
//! | `_`     | could open or close emphasis (stricter intraword rule) | §6.2 r. 6  |
//! | `` ` `` | always (text-path backticks would start a code span)   | §6.3       |
//! | `[`     | inside link text, or looks like a link/reference open  | §6.4       |
//! | `]`     | inside link text                                       | §6.4       |
//! | `<`     | next byte is `[A-Za-z/!?]` (HTML/autolink opener)      | §6.5, §6.6 |
//! | `&`     | followed by a well-formed entity sequence              | §6.5       |
//! | `\|`    | always inside a table cell                             | GFM tables |
//!
//! Out of scope (these live elsewhere): line-start positional escapes
//! that protect against *block* re-parsing — those are handled by
//! [`crate::format::block::escape_paragraph_line_starts`] on the
//! emitted `Doc` tree.

use std::borrow::Cow;

/// Non-local context the escape policy needs. The serializer fills
/// this in at each call site; the policy itself never inspects the
/// surrounding tree.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EscapeScope {
    /// We are emitting the text content of a link/image label. `[`
    /// and `]` inside would terminate the outer link, so escape
    /// every bracket.
    pub in_link_text: bool,
    /// We are emitting the content of a GFM table cell. `|` would
    /// close the cell, so escape every pipe.
    pub in_table_cell: bool,
    /// We are emitting the inline content of a heading. The escape
    /// policy does not branch on this; it is read by the inline
    /// serializer to switch hard breaks from `\` + newline (which
    /// `CommonMark` would parse as a paragraph hard break, not a
    /// heading break) to a literal `<br/>` tag.
    pub in_heading: bool,
}

/// Escape `text` for emission as inline text content. Allocation-free
/// fast path: if no byte needs escape (the common case in math
/// prose), returns `Cow::Borrowed(text)`. Otherwise builds a single
/// owned `String`.
pub(crate) fn escape_text(text: &str, scope: EscapeScope) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    // Fast scan: do any bytes need escaping?
    let first_escape = (0..bytes.len()).find(|&i| needs_escape_at(bytes, i, scope));
    let Some(start) = first_escape else {
        return Cow::Borrowed(text);
    };
    // Copy slices of the original `text` to preserve UTF-8 codepoints.
    // Insert `\` (one ASCII byte) before each byte that needs it; every
    // such byte is itself ASCII (`*`, `_`, `\\`, `[`, etc.), so the
    // insertion never lands inside a multi-byte sequence.
    let mut out = String::with_capacity(text.len().saturating_add(8));
    out.push_str(text.get(..start).unwrap_or(""));
    let mut prev_end = start;
    for i in start..bytes.len() {
        if needs_escape_at(bytes, i, scope) {
            out.push_str(text.get(prev_end..i).unwrap_or(""));
            out.push('\\');
            if let Some(byte) = bytes.get(i).copied() {
                out.push(char::from(byte));
            }
            prev_end = i.saturating_add(1);
        }
    }
    out.push_str(text.get(prev_end..).unwrap_or(""));
    Cow::Owned(out)
}

/// Per-byte decision. Pure; no allocation; the unit-testable heart.
pub(crate) fn needs_escape_at(bytes: &[u8], i: usize, scope: EscapeScope) -> bool {
    let Some(b) = bytes.get(i).copied() else {
        return false;
    };
    let right = bytes.get(i.saturating_add(1)).copied();
    match b {
        // §2.4 — `\` precedes an ASCII-punctuation byte to mean
        // "literal punctuation". And §6.7: a `\` immediately before
        // a line ending is parsed as a hard line break, so any `\`
        // followed by `\n` in the emitted output would re-parse as
        // `<br/>`. Escape it to keep the literal `\` and let the
        // line break stay a soft break (or paragraph end). Lone
        // trailing `\` at end of run with no continuation is text.
        b'\\' => right.is_some_and(|b| is_cm_punct(b) || b == b'\n'),

        // §6.2 — `*` may form an emphasis delimiter run. The CM
        // tokenizer flags every `*` as a potential delimiter, but
        // the matching pass leaves unpaired delimiters as text. So
        // escape iff this `*` would actually pair: it can open AND
        // a later `*` can close, OR it can close AND an earlier
        // `*` can open.
        b'*' => needs_emphasis_escape(bytes, i, b'*', AsteriskRules),

        // §6.2 rule 6 — `_` adds the intraword-is-text rule: both
        // neighbours alphanumeric ⇒ neither left- nor right-flanking
        // under the stricter `_` test. So `id_S`, `Hom_{cart}`,
        // `a_b_c` need no escape. This is CM-core, not GFM-only.
        b'_' => needs_emphasis_escape(bytes, i, b'_', UnderscoreRules),

        // §6.3 — Inline code spans are emitted by the inline path's
        // code branch, never as Text. Any backtick that reaches the
        // text path is a literal we must protect.
        b'`' => true,

        // §6.4 — `[` could open a link or reference. Inside link text,
        // always escape. Outside, scan for the pattern `]...(` or
        // `][` that would form a link.
        b'[' => {
            if scope.in_link_text {
                true
            } else {
                looks_like_link_open(bytes, i)
            }
        }

        // §6.4 — `]` is only dangerous as the closer of a link/ref.
        // Inside link text the outer parser would consume it; outside
        // a lone `]` is fine.
        b']' => scope.in_link_text,

        // §6.5/§6.6 — `<` opens an autolink, HTML tag, comment, or
        // processing instruction iff followed by ASCII letter or
        // `/`, `!`, `?`. `a < b` survives unescaped.
        b'<' => right.is_some_and(is_html_or_autolink_continuation),

        // §6.5 — `&` introduces an entity iff followed by `#`-digits-`;`,
        // `#x`-hex-`;`, or letters-`;`. Bare `&` is text.
        b'&' => looks_like_entity(bytes, i),

        // GFM tables — inside a cell, `|` would close the cell.
        b'|' => scope.in_table_cell,

        _ => false,
    }
}

/// CM §2.4 escapable punctuation: the 33 ASCII-punctuation bytes
/// listed in the `CommonMark` spec under "backslash escapes".
fn is_cm_punct(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'"'
            | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b'-'
            | b'.'
            | b'/'
            | b':'
            | b';'
            | b'<'
            | b'='
            | b'>'
            | b'?'
            | b'@'
            | b'['
            | b'\\'
            | b']'
            | b'^'
            | b'_'
            | b'`'
            | b'{'
            | b'|'
            | b'}'
            | b'~'
    )
}

/// ASCII-whitespace approximation of CM's "Unicode whitespace
/// character". Adequate for prose; non-ASCII bytes (UTF-8
/// continuation/lead bytes) are treated as "other" (alphanumeric-like)
/// for flanking purposes, which is correct for almost all natural
/// language and never splits a multi-byte codepoint because we only
/// insert escapes before ASCII syntax bytes.
fn is_ws_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

/// ASCII-punctuation approximation of CM's "Unicode punctuation
/// character".
fn is_punct_byte(b: u8) -> bool {
    is_cm_punct(b)
}

/// CM §6.2 flanking helpers. `left`/`right` are `None` at the run
/// boundary, which CM treats as Unicode whitespace.
fn left_flanking(left: Option<u8>, right: Option<u8>) -> bool {
    let next_is_ws = right.is_none_or(is_ws_byte);
    let next_is_punct = right.is_some_and(is_punct_byte);
    let prev_is_ws = left.is_none_or(is_ws_byte);
    let prev_is_punct = left.is_some_and(is_punct_byte);
    !next_is_ws && (!next_is_punct || prev_is_ws || prev_is_punct)
}

fn right_flanking(left: Option<u8>, right: Option<u8>) -> bool {
    let next_is_ws = right.is_none_or(is_ws_byte);
    let next_is_punct = right.is_some_and(is_punct_byte);
    let prev_is_ws = left.is_none_or(is_ws_byte);
    let prev_is_punct = left.is_some_and(is_punct_byte);
    !prev_is_ws && (!prev_is_punct || next_is_ws || next_is_punct)
}

/// Per-delimiter open/close test. Asterisk uses the plain
/// left/right-flanking rules (CM §6.2); underscore adds the
/// intraword exclusion (rule 6).
trait DelimRules: Copy {
    fn can_open(self, left: Option<u8>, right: Option<u8>) -> bool;
    fn can_close(self, left: Option<u8>, right: Option<u8>) -> bool;
}

#[derive(Copy, Clone)]
struct AsteriskRules;
impl DelimRules for AsteriskRules {
    fn can_open(self, left: Option<u8>, right: Option<u8>) -> bool {
        left_flanking(left, right)
    }
    fn can_close(self, left: Option<u8>, right: Option<u8>) -> bool {
        right_flanking(left, right)
    }
}

#[derive(Copy, Clone)]
struct UnderscoreRules;
impl DelimRules for UnderscoreRules {
    fn can_open(self, left: Option<u8>, right: Option<u8>) -> bool {
        let lf = left_flanking(left, right);
        let rf = right_flanking(left, right);
        let prev_is_punct = left.is_some_and(is_punct_byte);
        // CM §6.2 rule 6: `_` opens iff left-flanking AND (not
        // right-flanking OR preceded by punctuation).
        lf && (!rf || prev_is_punct)
    }
    fn can_close(self, left: Option<u8>, right: Option<u8>) -> bool {
        let lf = left_flanking(left, right);
        let rf = right_flanking(left, right);
        let next_is_punct = right.is_some_and(is_punct_byte);
        rf && (!lf || next_is_punct)
    }
}

fn neighbours_at(bytes: &[u8], i: usize) -> (Option<u8>, Option<u8>) {
    let left = i.checked_sub(1).and_then(|j| bytes.get(j).copied());
    let right = bytes.get(i.saturating_add(1)).copied();
    (left, right)
}

/// Escape iff this delimiter byte would actually pair under the CM
/// matching algorithm: it can open AND some later same-byte run can
/// close, or it can close AND some earlier same-byte run can open.
fn needs_emphasis_escape(bytes: &[u8], i: usize, byte: u8, rules: impl DelimRules) -> bool {
    let (l, r) = neighbours_at(bytes, i);
    let can_open_here = rules.can_open(l, r);
    let can_close_here = rules.can_close(l, r);
    if !can_open_here && !can_close_here {
        return false;
    }
    if can_open_here {
        for j in i.saturating_add(1)..bytes.len() {
            if bytes.get(j).copied() != Some(byte) {
                continue;
            }
            let (jl, jr) = neighbours_at(bytes, j);
            if rules.can_close(jl, jr) {
                return true;
            }
        }
    }
    if can_close_here {
        for j in 0..i {
            if bytes.get(j).copied() != Some(byte) {
                continue;
            }
            let (jl, jr) = neighbours_at(bytes, j);
            if rules.can_open(jl, jr) {
                return true;
            }
        }
    }
    false
}

/// `<` followed by ASCII letter, `/`, `!`, or `?` could open an HTML
/// tag, comment, processing instruction, or autolink (CM §6.5/§6.6).
fn is_html_or_autolink_continuation(b: u8) -> bool {
    b.is_ascii_alphabetic() || matches!(b, b'/' | b'!' | b'?')
}

/// Match a CM character/HTML entity reference starting at `i+1`
/// (`bytes[i]` is `&`). Three shapes: `&#<digits>;`, `&#x<hex>;`,
/// `&<name>;` (CM §6.5). Names must be ASCII letters.
fn looks_like_entity(bytes: &[u8], i: usize) -> bool {
    let mut j = i.saturating_add(1);
    let Some(&first) = bytes.get(j) else {
        return false;
    };
    if first == b'#' {
        j = j.saturating_add(1);
        let hex = bytes.get(j).copied() == Some(b'x') || bytes.get(j).copied() == Some(b'X');
        if hex {
            j = j.saturating_add(1);
            let start = j;
            while bytes.get(j).is_some_and(u8::is_ascii_hexdigit) {
                j = j.saturating_add(1);
            }
            j > start && bytes.get(j).copied() == Some(b';')
        } else {
            let start = j;
            while bytes.get(j).is_some_and(u8::is_ascii_digit) {
                j = j.saturating_add(1);
            }
            j > start && bytes.get(j).copied() == Some(b';')
        }
    } else if first.is_ascii_alphabetic() {
        let start = j;
        while bytes.get(j).is_some_and(u8::is_ascii_alphanumeric) {
            j = j.saturating_add(1);
        }
        j > start && bytes.get(j).copied() == Some(b';')
    } else {
        false
    }
}

/// Conservative scan: does `bytes[i] = b'['` look like the opener of
/// a link or reference? We look forward for the matching `]` and
/// check what follows — `(`, `[`, or `:` (link reference definition,
/// but those don't appear inside paragraphs).
fn looks_like_link_open(bytes: &[u8], i: usize) -> bool {
    debug_assert_eq!(bytes.get(i).copied(), Some(b'['));
    let mut j = i.saturating_add(1);
    let mut depth = 1u32;
    while let Some(byte) = bytes.get(j).copied() {
        match byte {
            b'\\' => {
                j = j.saturating_add(2);
                continue;
            }
            b'[' => depth = depth.saturating_add(1),
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return matches!(bytes.get(j.saturating_add(1)).copied(), Some(b'(' | b'['));
                }
            }
            _ => {}
        }
        j = j.saturating_add(1);
    }
    false
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn s(scope_init: impl FnOnce(&mut EscapeScope)) -> EscapeScope {
        let mut sc = EscapeScope::default();
        scope_init(&mut sc);
        sc
    }

    fn default_scope() -> EscapeScope {
        EscapeScope::default()
    }

    // --- Backslash (§2.4) -----------------------------------------

    #[test]
    fn backslash_before_punct_is_escaped() {
        // Source `\\\*` → pulldown gives text `\*`. Round-trip is
        // `\\*`: CM parses `\\` as literal `\` then `*` (unpaired) as
        // literal `*`, recovering the original two-byte text.
        assert_eq!(escape_text(r"\*", default_scope()), r"\\*");
    }

    #[test]
    fn backslash_before_paired_asterisk_escapes_both() {
        // `\*foo*` as text: now there's a partner, so escape the `*`s
        // *and* the leading `\`.
        assert_eq!(escape_text(r"\*foo*", default_scope()), r"\\\*foo\*");
    }

    #[test]
    fn backslash_before_letter_is_text() {
        assert_eq!(escape_text(r"\a", default_scope()), r"\a");
    }

    #[test]
    fn lone_trailing_backslash_is_text() {
        assert_eq!(escape_text("foo\\", default_scope()), "foo\\");
    }

    // --- Asterisk (§6.2) ------------------------------------------

    #[test]
    fn asterisk_with_spaces_around_is_text() {
        assert_eq!(escape_text("a * b", default_scope()), "a * b");
    }

    #[test]
    fn asterisk_word_boundary_left_is_escaped() {
        assert_eq!(escape_text("*foo*", default_scope()), r"\*foo\*");
    }

    #[test]
    fn asterisk_intraword_is_escaped() {
        // a*b is right-flanking on the second a and left-flanking on
        // the b side; CM would form an emphasis run if matched.
        assert_eq!(escape_text("a*b*c", default_scope()), r"a\*b\*c");
    }

    #[test]
    fn asterisk_at_start_no_partner_is_text() {
        // Single unpaired `*` — the matching pass would leave it as
        // text, so escape would be over-eager churn.
        assert_eq!(escape_text("*foo", default_scope()), "*foo");
    }

    #[test]
    fn asterisk_pair_at_start_and_end_is_escaped() {
        assert_eq!(escape_text("*foo bar*", default_scope()), r"\*foo bar\*");
    }

    // --- Underscore (§6.2 rule 6) — math resilience ----------------

    #[test]
    fn intraword_underscore_is_text_id_s() {
        assert_eq!(escape_text("id_S", default_scope()), "id_S");
    }

    #[test]
    fn intraword_underscore_is_text_hom_cart() {
        assert_eq!(escape_text("Hom_{cart}", default_scope()), "Hom_{cart}");
    }

    #[test]
    fn intraword_underscore_is_text_a_b_c() {
        assert_eq!(escape_text("a_b_c", default_scope()), "a_b_c");
    }

    #[test]
    fn underscore_word_boundary_is_escaped() {
        assert_eq!(escape_text("_foo_", default_scope()), r"\_foo\_");
    }

    #[test]
    fn underscore_after_space_before_letter_with_partner_is_escaped() {
        // The leading `_` is left-flanking and has a partner closer
        // later in the run, so it would pair as italics. Escape both.
        assert_eq!(escape_text("a _b_ c", default_scope()), r"a \_b\_ c");
    }

    #[test]
    fn underscore_unpaired_is_text() {
        // Single `_` with no possible partner: matching pass leaves
        // it as text; no escape.
        assert_eq!(escape_text("foo_bar", default_scope()), "foo_bar");
        assert_eq!(escape_text("foo _bar", default_scope()), "foo _bar");
    }

    // --- Backtick (§6.3) ------------------------------------------

    #[test]
    fn backtick_in_text_is_always_escaped() {
        assert_eq!(escape_text("a`b", default_scope()), r"a\`b");
    }

    #[test]
    fn lone_backtick_at_end_is_escaped() {
        assert_eq!(escape_text("foo`", default_scope()), r"foo\`");
    }

    // --- Brackets (§6.4) ------------------------------------------

    #[test]
    fn bracket_not_link_shape_is_text() {
        assert_eq!(escape_text("[foo bar baz", default_scope()), "[foo bar baz");
    }

    #[test]
    fn bracket_link_shape_is_escaped() {
        assert_eq!(escape_text("[foo](bar)", default_scope()), r"\[foo](bar)");
    }

    #[test]
    fn bracket_reference_shape_is_escaped() {
        assert_eq!(escape_text("[foo][bar]", default_scope()), r"\[foo][bar]");
    }

    #[test]
    fn closing_bracket_outside_link_is_text() {
        assert_eq!(escape_text("a] b", default_scope()), "a] b");
    }

    #[test]
    fn bracket_inside_link_text_is_escaped() {
        let sc = s(|s| s.in_link_text = true);
        assert_eq!(escape_text("[", sc), r"\[");
        assert_eq!(escape_text("]", sc), r"\]");
    }

    // --- Less-than (§6.5/§6.6) ------------------------------------

    #[test]
    fn less_than_before_space_is_text() {
        assert_eq!(escape_text("a < b", default_scope()), "a < b");
    }

    #[test]
    fn less_than_before_digit_is_text() {
        assert_eq!(escape_text("a <3 b", default_scope()), "a <3 b");
    }

    #[test]
    fn less_than_before_letter_is_escaped() {
        assert_eq!(escape_text("<tag", default_scope()), r"\<tag");
    }

    #[test]
    fn less_than_before_slash_is_escaped() {
        assert_eq!(escape_text("</a>", default_scope()), r"\</a>");
    }

    // --- Ampersand (§6.5) -----------------------------------------

    #[test]
    fn ampersand_with_space_is_text() {
        assert_eq!(escape_text("a & b", default_scope()), "a & b");
    }

    #[test]
    fn named_entity_is_escaped() {
        assert_eq!(escape_text("&amp;", default_scope()), r"\&amp;");
    }

    #[test]
    fn decimal_entity_is_escaped() {
        assert_eq!(escape_text("&#42;", default_scope()), r"\&#42;");
    }

    #[test]
    fn hex_entity_is_escaped() {
        assert_eq!(escape_text("&#x2A;", default_scope()), r"\&#x2A;");
    }

    #[test]
    fn ampersand_letter_without_semicolon_is_text() {
        assert_eq!(escape_text("&amp", default_scope()), "&amp");
    }

    // --- Pipe (GFM) -----------------------------------------------

    #[test]
    fn pipe_outside_table_is_text() {
        assert_eq!(escape_text("a|b", default_scope()), "a|b");
    }

    #[test]
    fn pipe_inside_table_is_escaped() {
        let sc = s(|s| s.in_table_cell = true);
        assert_eq!(escape_text("a|b", sc), r"a\|b");
    }

    // --- Boundary cases -------------------------------------------

    #[test]
    fn empty_string_borrows() {
        let out = escape_text("", default_scope());
        assert!(matches!(out, Cow::Borrowed("")));
    }

    #[test]
    fn plain_text_borrows_no_alloc() {
        let out = escape_text("Hello, world.", default_scope());
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn utf8_codepoint_preserved_next_to_asterisk() {
        // 4-byte UTF-8 emoji adjacent to a flanking `*`. We must not
        // split the codepoint; the escape goes before the ASCII byte.
        let input = "👋*hi*";
        assert_eq!(escape_text(input, default_scope()), r"👋\*hi\*");
    }

    #[test]
    fn utf8_letter_acts_as_alphanumeric_for_flanking() {
        // `é_x` — non-ASCII byte preceding `_`, ASCII letter after.
        // Our policy treats high bytes as "other" (alphanumeric-like),
        // so this is intraword → no escape.
        assert_eq!(escape_text("é_x", default_scope()), "é_x");
    }

    #[test]
    fn only_escapable_chars_partnered() {
        // Two `*`s pair, two `_`s pair, backticks always escape.
        assert_eq!(escape_text("**__``", default_scope()), r"\*\*\_\_\`\`");
    }

    #[test]
    fn combined_link_text_and_table_cell_scopes() {
        let sc = s(|s| {
            s.in_link_text = true;
            s.in_table_cell = true;
        });
        assert_eq!(escape_text("a|b[c]", sc), r"a\|b\[c\]");
    }

    #[test]
    fn mixed_escapes_in_one_run() {
        // `<c>` is a valid one-letter HTML tag opener, so the `<`
        // escapes; the entity `&amp;` escapes; the paired `*`s
        // both escape.
        assert_eq!(
            escape_text("a*b<c>*&amp;", default_scope()),
            r"a\*b\<c>\*\&amp;"
        );
    }

    #[test]
    fn asterisk_intraword_no_partner_is_text() {
        // `a*b` alone — single `*`, no partner, no escape.
        assert_eq!(escape_text("a*b", default_scope()), "a*b");
    }

    #[test]
    fn cm_punct_set_complete() {
        // Spot-check: every CM-punct byte should round-trip when
        // preceded by a backslash. Test escape on `\\<punct>` for a
        // few representative bytes.
        for b in [b'!', b'#', b'@', b'~', b'^', b'{', b'}'] {
            let s = format!("\\{}", char::from(b));
            let out = escape_text(&s, default_scope());
            assert_eq!(out, format!("\\\\{}", char::from(b)));
        }
    }
}
