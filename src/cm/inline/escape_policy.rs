//! Inline-text escape policy.
//!
//! The constructor of an [`super::run::InlineRun`] runs the bytes of
//! its coalesced text through [`escape_buffer`] before storing them.
//! Once construction is done, the stored bytes round-trip through the
//! `CommonMark` tokenizer under [`EscapeScope`].
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
//! Line-start positional escapes that protect against *block* re-parsing
//! live elsewhere (`crate::format::block::escape_paragraph_line_starts`)
//! on the emitted `Doc` tree.

/// Non-local context the escape policy needs. The IR builder fills
/// this in from its scope stack at construction time; once the bytes
/// inside [`super::run::InlineRun`] are escaped, the scope is no
/// longer needed by consumers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EscapeScope {
    /// We are emitting the text content of a link/image label. `[`
    /// and `]` inside would terminate the outer link, so escape
    /// every bracket.
    pub(crate) in_link_text: bool,
    /// We are emitting the content of a GFM table cell. `|` would
    /// close the cell, so escape every pipe.
    pub(crate) in_table_cell: bool,
    /// We are emitting the inline content of a heading. The escape
    /// policy does not branch on this; it is read by the run
    /// constructor to switch hard breaks from `\` + newline (which
    /// `CommonMark` would parse as a paragraph hard break) to a
    /// literal `<br/>` tag.
    pub(crate) in_heading: bool,
}

/// Escape `buf` for inline emission. `forced[i] = true` forces byte
/// `i` to be preceded by `\` in addition to the standard per-byte
/// policy. Returns owned `String` because the common caller (the
/// `InlineRun` constructor) already needed an owned buffer for run
/// coalescing.
pub(crate) fn escape_buffer(buf: &str, forced: &[bool], scope: EscapeScope) -> String {
    let bytes = buf.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(buf.len().saturating_add(8));
    for i in 0..bytes.len() {
        let need = forced.get(i).copied().unwrap_or(false) || needs_escape_at(bytes, i, scope);
        if need {
            out.push(b'\\');
        }
        if let Some(b) = bytes.get(i).copied() {
            out.push(b);
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Per-byte decision. Pure; no allocation; the unit-testable heart.
fn needs_escape_at(bytes: &[u8], i: usize, scope: EscapeScope) -> bool {
    let Some(b) = bytes.get(i).copied() else {
        return false;
    };
    let right = bytes.get(i.saturating_add(1)).copied();
    match b {
        b'\\' => right.is_some_and(|b| is_cm_punct(b) || b == b'\n'),
        b'*' => needs_emphasis_escape(bytes, i, b'*', AsteriskRules),
        b'_' => needs_emphasis_escape(bytes, i, b'_', UnderscoreRules),
        b'`' => true,
        b'[' => {
            if scope.in_link_text {
                true
            } else {
                looks_like_link_open(bytes, i)
            }
        }
        b']' => scope.in_link_text,
        b'<' => right.is_some_and(is_html_or_autolink_continuation),
        b'&' => looks_like_entity(bytes, i),
        b'|' => scope.in_table_cell,
        _ => false,
    }
}

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

fn is_ws_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn is_punct_byte(b: u8) -> bool {
    is_cm_punct(b)
}

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

fn is_html_or_autolink_continuation(b: u8) -> bool {
    b.is_ascii_alphabetic() || matches!(b, b'/' | b'!' | b'?')
}

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

    fn escape_no_forced(text: &str, scope: EscapeScope) -> String {
        escape_buffer(text, &vec![false; text.len()], scope)
    }

    #[test]
    fn backslash_before_punct_is_escaped() {
        assert_eq!(escape_no_forced(r"\*", default_scope()), r"\\*");
    }

    #[test]
    fn backslash_before_paired_asterisk_escapes_both() {
        assert_eq!(escape_no_forced(r"\*foo*", default_scope()), r"\\\*foo\*");
    }

    #[test]
    fn backslash_before_letter_is_text() {
        assert_eq!(escape_no_forced(r"\a", default_scope()), r"\a");
    }

    #[test]
    fn lone_trailing_backslash_is_text() {
        assert_eq!(escape_no_forced("foo\\", default_scope()), "foo\\");
    }

    #[test]
    fn asterisk_with_spaces_around_is_text() {
        assert_eq!(escape_no_forced("a * b", default_scope()), "a * b");
    }

    #[test]
    fn asterisk_word_boundary_left_is_escaped() {
        assert_eq!(escape_no_forced("*foo*", default_scope()), r"\*foo\*");
    }

    #[test]
    fn asterisk_intraword_is_escaped() {
        assert_eq!(escape_no_forced("a*b*c", default_scope()), r"a\*b\*c");
    }

    #[test]
    fn asterisk_at_start_no_partner_is_text() {
        assert_eq!(escape_no_forced("*foo", default_scope()), "*foo");
    }

    #[test]
    fn asterisk_pair_at_start_and_end_is_escaped() {
        assert_eq!(escape_no_forced("*foo bar*", default_scope()), r"\*foo bar\*");
    }

    #[test]
    fn intraword_underscore_is_text_id_s() {
        assert_eq!(escape_no_forced("id_S", default_scope()), "id_S");
    }

    #[test]
    fn intraword_underscore_is_text_hom_cart() {
        assert_eq!(escape_no_forced("Hom_{cart}", default_scope()), "Hom_{cart}");
    }

    #[test]
    fn intraword_underscore_is_text_a_b_c() {
        assert_eq!(escape_no_forced("a_b_c", default_scope()), "a_b_c");
    }

    #[test]
    fn underscore_word_boundary_is_escaped() {
        assert_eq!(escape_no_forced("_foo_", default_scope()), r"\_foo\_");
    }

    #[test]
    fn underscore_after_space_before_letter_with_partner_is_escaped() {
        assert_eq!(escape_no_forced("a _b_ c", default_scope()), r"a \_b\_ c");
    }

    #[test]
    fn underscore_unpaired_is_text() {
        assert_eq!(escape_no_forced("foo_bar", default_scope()), "foo_bar");
        assert_eq!(escape_no_forced("foo _bar", default_scope()), "foo _bar");
    }

    #[test]
    fn backtick_in_text_is_always_escaped() {
        assert_eq!(escape_no_forced("a`b", default_scope()), r"a\`b");
    }

    #[test]
    fn lone_backtick_at_end_is_escaped() {
        assert_eq!(escape_no_forced("foo`", default_scope()), r"foo\`");
    }

    #[test]
    fn bracket_not_link_shape_is_text() {
        assert_eq!(escape_no_forced("[foo bar baz", default_scope()), "[foo bar baz");
    }

    #[test]
    fn bracket_link_shape_is_escaped() {
        assert_eq!(escape_no_forced("[foo](bar)", default_scope()), r"\[foo](bar)");
    }

    #[test]
    fn bracket_reference_shape_is_escaped() {
        assert_eq!(escape_no_forced("[foo][bar]", default_scope()), r"\[foo][bar]");
    }

    #[test]
    fn closing_bracket_outside_link_is_text() {
        assert_eq!(escape_no_forced("a] b", default_scope()), "a] b");
    }

    #[test]
    fn bracket_inside_link_text_is_escaped() {
        let sc = s(|s| s.in_link_text = true);
        assert_eq!(escape_no_forced("[", sc), r"\[");
        assert_eq!(escape_no_forced("]", sc), r"\]");
    }

    #[test]
    fn less_than_before_space_is_text() {
        assert_eq!(escape_no_forced("a < b", default_scope()), "a < b");
    }

    #[test]
    fn less_than_before_digit_is_text() {
        assert_eq!(escape_no_forced("a <3 b", default_scope()), "a <3 b");
    }

    #[test]
    fn less_than_before_letter_is_escaped() {
        assert_eq!(escape_no_forced("<tag", default_scope()), r"\<tag");
    }

    #[test]
    fn less_than_before_slash_is_escaped() {
        assert_eq!(escape_no_forced("</a>", default_scope()), r"\</a>");
    }

    #[test]
    fn ampersand_with_space_is_text() {
        assert_eq!(escape_no_forced("a & b", default_scope()), "a & b");
    }

    #[test]
    fn named_entity_is_escaped() {
        assert_eq!(escape_no_forced("&amp;", default_scope()), r"\&amp;");
    }

    #[test]
    fn decimal_entity_is_escaped() {
        assert_eq!(escape_no_forced("&#42;", default_scope()), r"\&#42;");
    }

    #[test]
    fn hex_entity_is_escaped() {
        assert_eq!(escape_no_forced("&#x2A;", default_scope()), r"\&#x2A;");
    }

    #[test]
    fn ampersand_letter_without_semicolon_is_text() {
        assert_eq!(escape_no_forced("&amp", default_scope()), "&amp");
    }

    #[test]
    fn pipe_outside_table_is_text() {
        assert_eq!(escape_no_forced("a|b", default_scope()), "a|b");
    }

    #[test]
    fn pipe_inside_table_is_escaped() {
        let sc = s(|s| s.in_table_cell = true);
        assert_eq!(escape_no_forced("a|b", sc), r"a\|b");
    }

    #[test]
    fn empty_string_returns_empty() {
        assert_eq!(escape_no_forced("", default_scope()), "");
    }

    #[test]
    fn utf8_codepoint_preserved_next_to_asterisk() {
        let input = "👋*hi*";
        assert_eq!(escape_no_forced(input, default_scope()), r"👋\*hi\*");
    }

    #[test]
    fn utf8_letter_acts_as_alphanumeric_for_flanking() {
        assert_eq!(escape_no_forced("é_x", default_scope()), "é_x");
    }

    #[test]
    fn only_escapable_chars_partnered() {
        assert_eq!(escape_no_forced("**__``", default_scope()), r"\*\*\_\_\`\`");
    }

    #[test]
    fn combined_link_text_and_table_cell_scopes() {
        let sc = s(|s| {
            s.in_link_text = true;
            s.in_table_cell = true;
        });
        assert_eq!(escape_no_forced("a|b[c]", sc), r"a\|b\[c\]");
    }

    #[test]
    fn mixed_escapes_in_one_run() {
        assert_eq!(
            escape_no_forced("a*b<c>*&amp;", default_scope()),
            r"a\*b\<c>\*\&amp;"
        );
    }

    #[test]
    fn asterisk_intraword_no_partner_is_text() {
        assert_eq!(escape_no_forced("a*b", default_scope()), "a*b");
    }

    #[test]
    fn forced_escape_overrides_no_escape_decision() {
        let mut forced = vec![false; "abc".len()];
        if let Some(slot) = forced.get_mut(1) {
            *slot = true;
        }
        assert_eq!(escape_buffer("abc", &forced, default_scope()), r"a\bc");
    }
}
