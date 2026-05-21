//! Unicode math source lexer internals.
//!
//! Design comparison:
//!
//! - Extending the TeX `logos` lexer would reuse machinery, but it would force
//!   Unicode math source into TeX categories such as command words and control
//!   symbols. That loses the distinction between already-LaTeX passthrough and
//!   Unicode source that still needs parsing.
//! - A separate `logos` lexer handles simple scalar classes well, but it cannot
//!   naturally keep precomposed and decomposed accent clusters tied to their
//!   original byte spans without a second cursor layered on top.
//! - A custom Unicode cursor is a little more code, but it hides the volatile
//!   cluster and span decisions in this module and gives the parser explicit
//!   tokens for styled runs, script runs, accents, arrows, and unknown scalars.
//!
//! The lexer recognises source shape only. It does not decide final LaTeX
//! spellings; that belongs to the parser, registry, and emitter.

use crate::SourceSpan;
use crate::registry::{
    MathAlphabetStyle, unicode_math_alphabet_char, unicode_sub_latex, unicode_super_latex, unicode_symbol_latex_source,
};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnicodeToken<'src> {
    kind: UnicodeTokenKind<'src>,
    span: SourceSpan,
}

impl<'src> UnicodeToken<'src> {
    fn new(kind: UnicodeTokenKind<'src>, span: SourceSpan) -> Self {
        Self { kind, span }
    }

    pub(crate) const fn kind(&self) -> &UnicodeTokenKind<'src> {
        &self.kind
    }

    pub(crate) const fn span(&self) -> SourceSpan {
        self.span
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnicodeTokenKind<'src> {
    ExistingLatex(&'src str),
    PlainWord(&'src str),
    Number(&'src str),
    StyledRun {
        style: MathAlphabetStyle,
        base: String,
    },
    UnicodeScriptRun {
        position: ScriptPosition,
        source: String,
    },
    SuperscriptMarker,
    SubscriptMarker,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftParen,
    RightParen,
    CombiningAccentCluster {
        base: String,
        accents: Vec<CombiningAccent>,
    },
    CombiningAccentMark(CombiningAccent),
    DirectSymbol(&'src str),
    SquareRoot,
    ArrowShaft(&'src str),
    ArrowHead(ArrowDirection),
    Whitespace(&'src str),
    Punctuation(&'src str),
    UnknownScalar(&'src str),
    Error,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScriptPosition {
    Superscript,
    Subscript,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombiningAccent {
    Tilde,
    Hat,
    Check,
    Bar,
    Breve,
    Dot,
    Ddot,
    Acute,
    Grave,
    Vec,
    Overleftarrow,
    Overleftrightarrow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArrowDirection {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnicodeLexDiagnostic {
    kind: UnicodeLexDiagnosticKind,
    span: SourceSpan,
    message: &'static str,
}

impl UnicodeLexDiagnostic {
    fn new(kind: UnicodeLexDiagnosticKind, span: SourceSpan, message: &'static str) -> Self {
        Self { kind, span, message }
    }

    pub(crate) const fn kind(&self) -> UnicodeLexDiagnosticKind {
        self.kind
    }

    pub(crate) const fn span(&self) -> SourceSpan {
        self.span
    }

    pub(crate) const fn message(&self) -> &'static str {
        self.message
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnicodeLexDiagnosticKind {
    DetachedCombiningMark,
    MalformedLatexPassthrough,
    ControlCharacter,
    UnknownSourceShape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnicodeTokenStream<'src> {
    source: &'src str,
    tokens: Vec<UnicodeToken<'src>>,
    diagnostics: Vec<UnicodeLexDiagnostic>,
}

impl<'src> UnicodeTokenStream<'src> {
    pub(crate) fn new(source: &'src str) -> Self {
        let mut lexer = UnicodeLexer::new(source);
        lexer.lex();
        let eof = SourceSpan::new(source.len(), source.len());
        lexer.tokens.push(UnicodeToken::new(UnicodeTokenKind::Eof, eof));
        Self {
            source,
            tokens: lexer.tokens,
            diagnostics: lexer.diagnostics,
        }
    }

    pub(crate) fn source(&self) -> &'src str {
        self.source
    }

    pub(crate) fn tokens(&self) -> &[UnicodeToken<'src>] {
        &self.tokens
    }

    pub(crate) fn diagnostics(&self) -> &[UnicodeLexDiagnostic] {
        &self.diagnostics
    }

    pub(crate) fn cursor(&self) -> UnicodeTokenCursor<'_, 'src> {
        UnicodeTokenCursor::new(&self.tokens)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnicodeTokenCursor<'stream, 'src> {
    tokens: &'stream [UnicodeToken<'src>],
    position: usize,
}

impl<'stream, 'src> UnicodeTokenCursor<'stream, 'src> {
    pub(crate) const fn new(tokens: &'stream [UnicodeToken<'src>]) -> Self {
        Self { tokens, position: 0 }
    }

    pub(crate) fn peek(&self) -> UnicodeToken<'src> {
        self.tokens
            .get(self.position)
            .or_else(|| self.tokens.get(self.eof_position()))
            .cloned()
            .unwrap_or_else(|| UnicodeToken::new(UnicodeTokenKind::Eof, SourceSpan::new(0, 0)))
    }

    pub(crate) fn advance(&mut self) -> UnicodeToken<'src> {
        let current = self.peek();
        if !matches!(current.kind(), UnicodeTokenKind::Eof) {
            self.position = self.position.saturating_add(1);
        }
        current
    }

    pub(crate) const fn checkpoint(&self) -> usize {
        self.position
    }

    pub(crate) fn restore(&mut self, checkpoint: usize) {
        self.position = checkpoint.min(self.eof_position());
    }

    pub(crate) fn is_eof(&self) -> bool {
        matches!(self.peek().kind(), UnicodeTokenKind::Eof)
    }

    fn eof_position(&self) -> usize {
        self.tokens.len().saturating_sub(1)
    }
}

struct UnicodeLexer<'src> {
    source: &'src str,
    cursor: usize,
    tokens: Vec<UnicodeToken<'src>>,
    diagnostics: Vec<UnicodeLexDiagnostic>,
}

impl<'src> UnicodeLexer<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            source,
            cursor: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn lex(&mut self) {
        while self.cursor < self.source.len() {
            self.lex_next();
        }
    }

    fn lex_next(&mut self) {
        let Some((start, ch, end)) = self.peek_char() else {
            return;
        };

        if ch == '\\' {
            self.push_existing_latex(start);
            return;
        }
        if ch.is_whitespace() {
            self.push_while(start, UnicodeTokenKind::Whitespace, char::is_whitespace);
            return;
        }
        if ch.is_ascii_alphabetic() {
            if self.push_combining_accent_cluster(start) {
                return;
            }
            self.push_while(start, UnicodeTokenKind::PlainWord, |candidate| {
                candidate.is_ascii_alphabetic()
            });
            return;
        }
        if ch.is_ascii_digit() {
            self.push_while(start, UnicodeTokenKind::Number, |candidate| candidate.is_ascii_digit());
            return;
        }
        if self.push_styled_run(start) {
            return;
        }
        if self.push_unicode_script_run(start) {
            return;
        }
        if self.push_combining_accent_cluster(start) {
            return;
        }
        if let Some(accent) = combining_accent(ch) {
            let span = SourceSpan::new(start, end);
            if self.previous_non_whitespace_is_not_group_close() {
                self.diagnostics.push(UnicodeLexDiagnostic::new(
                    UnicodeLexDiagnosticKind::DetachedCombiningMark,
                    span,
                    "combining accent has no lexical base",
                ));
            }
            self.cursor = end;
            self.tokens
                .push(UnicodeToken::new(UnicodeTokenKind::CombiningAccentMark(accent), span));
            return;
        }
        match ch {
            '^' => self.push_single(start, end, UnicodeTokenKind::SuperscriptMarker),
            '_' => self.push_single(start, end, UnicodeTokenKind::SubscriptMarker),
            '{' => self.push_single(start, end, UnicodeTokenKind::LeftBrace),
            '}' => self.push_single(start, end, UnicodeTokenKind::RightBrace),
            '[' => self.push_single(start, end, UnicodeTokenKind::LeftBracket),
            ']' => self.push_single(start, end, UnicodeTokenKind::RightBracket),
            '(' => self.push_single(start, end, UnicodeTokenKind::LeftParen),
            ')' => self.push_single(start, end, UnicodeTokenKind::RightParen),
            _ if is_arrow_shaft(ch) => self.push_arrow_shaft(start),
            _ if is_right_arrow_head(ch) => {
                self.push_single(start, end, UnicodeTokenKind::ArrowHead(ArrowDirection::Right));
            }
            _ if is_left_arrow_head(ch) => {
                self.push_single(start, end, UnicodeTokenKind::ArrowHead(ArrowDirection::Left));
            }
            '√' => self.push_single(start, end, UnicodeTokenKind::SquareRoot),
            _ if is_ascii_punctuation(ch) => {
                if self.push_combining_accent_cluster(start) {
                    return;
                }
                self.push_single(start, end, UnicodeTokenKind::Punctuation(&self.source[start..end]));
            }
            _ if self.is_direct_symbol(start, end) => {
                self.push_single(start, end, UnicodeTokenKind::DirectSymbol(&self.source[start..end]));
            }
            _ if ch.is_control() => {
                let span = SourceSpan::new(start, end);
                self.cursor = end;
                self.diagnostics.push(UnicodeLexDiagnostic::new(
                    UnicodeLexDiagnosticKind::ControlCharacter,
                    span,
                    "control character in Unicode math source",
                ));
                self.tokens.push(UnicodeToken::new(UnicodeTokenKind::Error, span));
            }
            _ => {
                let span = SourceSpan::new(start, end);
                self.cursor = end;
                self.diagnostics.push(UnicodeLexDiagnostic::new(
                    UnicodeLexDiagnosticKind::UnknownSourceShape,
                    span,
                    "unknown Unicode math source shape",
                ));
                self.tokens.push(UnicodeToken::new(
                    UnicodeTokenKind::UnknownScalar(&self.source[start..end]),
                    span,
                ));
            }
        }
    }

    fn push_existing_latex(&mut self, start: usize) {
        self.cursor = start.saturating_add(1);
        let Some((_next_start, next, _next_end)) = self.peek_char() else {
            let span = SourceSpan::new(start, self.cursor);
            self.diagnostics.push(UnicodeLexDiagnostic::new(
                UnicodeLexDiagnosticKind::MalformedLatexPassthrough,
                span,
                "malformed LaTeX passthrough command",
            ));
            self.tokens.push(UnicodeToken::new(UnicodeTokenKind::Error, span));
            return;
        };

        if next.is_ascii_alphabetic() {
            while let Some((_candidate_start, candidate, candidate_end)) = self.peek_char() {
                if !candidate.is_ascii_alphabetic() {
                    break;
                }
                self.cursor = candidate_end;
            }
        } else if matches!(next, '\r' | '\n') {
            let span = SourceSpan::new(start, self.cursor);
            self.diagnostics.push(UnicodeLexDiagnostic::new(
                UnicodeLexDiagnosticKind::MalformedLatexPassthrough,
                span,
                "malformed LaTeX passthrough command",
            ));
            self.tokens.push(UnicodeToken::new(UnicodeTokenKind::Error, span));
            return;
        } else {
            self.cursor = self.cursor.saturating_add(next.len_utf8());
        }

        while let Some((_accent_start, accent, accent_end)) = self.peek_char() {
            if !is_latex_passthrough_combining_mark(accent) {
                break;
            }
            self.cursor = accent_end;
        }

        let span = SourceSpan::new(start, self.cursor);
        self.tokens.push(UnicodeToken::new(
            UnicodeTokenKind::ExistingLatex(&self.source[span.as_range()]),
            span,
        ));
    }

    fn push_styled_run(&mut self, start: usize) -> bool {
        let Some((_start, ch, end)) = self.peek_char() else {
            return false;
        };
        let Some(first) = unicode_math_alphabet_char(ch) else {
            return false;
        };

        let style = first.style;
        let mut base = String::new();
        base.push(first.base);
        self.cursor = end;

        while let Some((_candidate_start, candidate, candidate_end)) = self.peek_char() {
            let Some(styled) = unicode_math_alphabet_char(candidate) else {
                break;
            };
            if styled.style != style {
                break;
            }
            base.push(styled.base);
            self.cursor = candidate_end;
        }

        let span = SourceSpan::new(start, self.cursor);
        self.tokens
            .push(UnicodeToken::new(UnicodeTokenKind::StyledRun { style, base }, span));
        true
    }

    fn push_unicode_script_run(&mut self, start: usize) -> bool {
        let Some((_start, ch, end)) = self.peek_char() else {
            return false;
        };
        let Some(position) = script_position(ch) else {
            return false;
        };

        let mut source = String::new();
        let Some(first_source) = script_source(ch, position) else {
            return false;
        };
        source.push_str(first_source);
        self.cursor = end;

        while let Some((_candidate_start, candidate, candidate_end)) = self.peek_char() {
            if script_position(candidate) != Some(position) {
                break;
            }
            let Some(candidate_source) = script_source(candidate, position) else {
                break;
            };
            source.push_str(candidate_source);
            self.cursor = candidate_end;
        }

        let span = SourceSpan::new(start, self.cursor);
        self.tokens.push(UnicodeToken::new(
            UnicodeTokenKind::UnicodeScriptRun { position, source },
            span,
        ));
        true
    }

    fn push_combining_accent_cluster(&mut self, start: usize) -> bool {
        let Some((_start, ch, end)) = self.peek_char() else {
            return false;
        };
        if combining_accent(ch).is_some() {
            return false;
        }

        let mut decomposed = ch.to_string().nfd().collect::<Vec<_>>();
        let Some(base) = decomposed.first().copied() else {
            return false;
        };
        if combining_accent(base).is_some() || is_group_delimiter(base) {
            return false;
        }
        let mut accents = decomposed.drain(1..).filter_map(combining_accent).collect::<Vec<_>>();
        let mut cursor = end;

        while let Some((_accent_start, accent, accent_end)) = self.peek_char_at(cursor) {
            let Some(kind) = combining_accent(accent) else {
                break;
            };
            accents.push(kind);
            cursor = accent_end;
        }

        if accents.is_empty() {
            return false;
        }

        self.cursor = cursor;
        let span = SourceSpan::new(start, cursor);
        self.tokens.push(UnicodeToken::new(
            UnicodeTokenKind::CombiningAccentCluster {
                base: base.to_string(),
                accents,
            },
            span,
        ));
        true
    }

    fn push_arrow_shaft(&mut self, start: usize) {
        while let Some((_candidate_start, candidate, candidate_end)) = self.peek_char() {
            if !is_arrow_shaft(candidate) {
                break;
            }
            self.cursor = candidate_end;
        }
        let span = SourceSpan::new(start, self.cursor);
        self.tokens.push(UnicodeToken::new(
            UnicodeTokenKind::ArrowShaft(&self.source[span.as_range()]),
            span,
        ));
    }

    fn push_while(
        &mut self,
        start: usize,
        constructor: impl FnOnce(&'src str) -> UnicodeTokenKind<'src>,
        predicate: impl Fn(char) -> bool,
    ) {
        while let Some((_candidate_start, candidate, candidate_end)) = self.peek_char() {
            if !predicate(candidate) {
                break;
            }
            self.cursor = candidate_end;
        }
        let span = SourceSpan::new(start, self.cursor);
        self.tokens
            .push(UnicodeToken::new(constructor(&self.source[span.as_range()]), span));
    }

    fn push_single(&mut self, start: usize, end: usize, kind: UnicodeTokenKind<'src>) {
        self.cursor = end;
        self.tokens.push(UnicodeToken::new(kind, SourceSpan::new(start, end)));
    }

    fn is_direct_symbol(&self, start: usize, end: usize) -> bool {
        unicode_symbol_latex_source(&self.source[start..end]).is_some()
    }

    fn previous_non_whitespace_is_not_group_close(&self) -> bool {
        self.source[..self.cursor]
            .chars()
            .rev()
            .find(|ch| !ch.is_whitespace())
            .is_none_or(|ch| !matches!(ch, '}'))
    }

    fn peek_char(&self) -> Option<(usize, char, usize)> {
        self.peek_char_at(self.cursor)
    }

    fn peek_char_at(&self, cursor: usize) -> Option<(usize, char, usize)> {
        let ch = self.source.get(cursor..)?.chars().next()?;
        Some((cursor, ch, cursor.saturating_add(ch.len_utf8())))
    }
}

fn script_position(ch: char) -> Option<ScriptPosition> {
    if unicode_super_latex(ch).is_some() {
        Some(ScriptPosition::Superscript)
    } else if unicode_sub_latex(ch).is_some() {
        Some(ScriptPosition::Subscript)
    } else {
        None
    }
}

fn script_source(ch: char, position: ScriptPosition) -> Option<&'static str> {
    match position {
        ScriptPosition::Superscript => unicode_super_latex(ch),
        ScriptPosition::Subscript => unicode_sub_latex(ch),
    }
}

fn combining_accent(ch: char) -> Option<CombiningAccent> {
    match ch {
        '\u{0303}' => Some(CombiningAccent::Tilde),
        '\u{0302}' => Some(CombiningAccent::Hat),
        '\u{030c}' => Some(CombiningAccent::Check),
        '\u{0304}' | '\u{0305}' => Some(CombiningAccent::Bar),
        '\u{0306}' => Some(CombiningAccent::Breve),
        '\u{0307}' => Some(CombiningAccent::Dot),
        '\u{0308}' => Some(CombiningAccent::Ddot),
        '\u{0301}' => Some(CombiningAccent::Acute),
        '\u{0300}' => Some(CombiningAccent::Grave),
        '\u{20d7}' => Some(CombiningAccent::Vec),
        '\u{20d6}' => Some(CombiningAccent::Overleftarrow),
        '\u{20e1}' => Some(CombiningAccent::Overleftrightarrow),
        _ => None,
    }
}

fn is_latex_passthrough_combining_mark(ch: char) -> bool {
    combining_accent(ch).is_some() || ch == '\u{0338}'
}

fn is_group_delimiter(ch: char) -> bool {
    matches!(ch, '{' | '}' | '[' | ']' | '(' | ')')
}

fn is_arrow_shaft(ch: char) -> bool {
    matches!(ch, '─' | '━' | '—')
}

fn is_right_arrow_head(ch: char) -> bool {
    matches!(ch, '→' | '▸')
}

fn is_left_arrow_head(ch: char) -> bool {
    ch == '←'
}

fn is_ascii_punctuation(ch: char) -> bool {
    ch.is_ascii_punctuation()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unicode_not_nfc,
        reason = "unicode lexer tests need decomposed combining-mark fixtures"
    )]

    use super::*;

    fn stream(source: &str) -> UnicodeTokenStream<'_> {
        UnicodeTokenStream::new(source)
    }

    fn non_trivia(source: &str) -> Vec<UnicodeTokenKind<'_>> {
        let stream = stream(source);
        assert_eq!(stream.diagnostics(), &[]);
        stream
            .tokens()
            .iter()
            .map(UnicodeToken::kind)
            .filter(|kind| !matches!(kind, UnicodeTokenKind::Whitespace(_)))
            .cloned()
            .collect()
    }

    #[test]
    fn styled_alphabet_runs_preserve_style_and_base_text() {
        let tokens = non_trivia("𝓗𝓸𝓶 𝚪 𝐟𝐠");

        assert_eq!(
            tokens,
            vec![
                UnicodeTokenKind::StyledRun {
                    style: MathAlphabetStyle::BoldScript,
                    base: "Hom".to_owned(),
                },
                UnicodeTokenKind::StyledRun {
                    style: MathAlphabetStyle::Bold,
                    base: "Γ".to_owned(),
                },
                UnicodeTokenKind::StyledRun {
                    style: MathAlphabetStyle::Bold,
                    base: "fg".to_owned(),
                },
                UnicodeTokenKind::Eof,
            ]
        );
    }

    #[test]
    fn unicode_script_runs_are_positioned_tokens() {
        let tokens = non_trivia(r"xᵐ D₊ iˢ_A ᵃ\phi");

        assert_eq!(
            tokens,
            vec![
                UnicodeTokenKind::PlainWord("x"),
                UnicodeTokenKind::UnicodeScriptRun {
                    position: ScriptPosition::Superscript,
                    source: "m".to_owned(),
                },
                UnicodeTokenKind::PlainWord("D"),
                UnicodeTokenKind::UnicodeScriptRun {
                    position: ScriptPosition::Subscript,
                    source: "+".to_owned(),
                },
                UnicodeTokenKind::PlainWord("i"),
                UnicodeTokenKind::UnicodeScriptRun {
                    position: ScriptPosition::Superscript,
                    source: "s".to_owned(),
                },
                UnicodeTokenKind::SubscriptMarker,
                UnicodeTokenKind::PlainWord("A"),
                UnicodeTokenKind::UnicodeScriptRun {
                    position: ScriptPosition::Superscript,
                    source: "a".to_owned(),
                },
                UnicodeTokenKind::ExistingLatex(r"\phi"),
                UnicodeTokenKind::Eof,
            ]
        );
    }

    #[test]
    fn grouped_and_bracketed_ascii_scripts_are_structural_tokens() {
        let tokens = non_trivia("M_[φ] x^(n)");

        assert_eq!(
            tokens,
            vec![
                UnicodeTokenKind::PlainWord("M"),
                UnicodeTokenKind::SubscriptMarker,
                UnicodeTokenKind::LeftBracket,
                UnicodeTokenKind::DirectSymbol("φ"),
                UnicodeTokenKind::RightBracket,
                UnicodeTokenKind::PlainWord("x"),
                UnicodeTokenKind::SuperscriptMarker,
                UnicodeTokenKind::LeftParen,
                UnicodeTokenKind::PlainWord("n"),
                UnicodeTokenKind::RightParen,
                UnicodeTokenKind::Eof,
            ]
        );
    }

    #[test]
    fn combining_accent_clusters_keep_original_spans() {
        let stream = stream("ũ ẑ c̄ M̃ Ω̂");
        assert_eq!(stream.diagnostics(), &[]);
        let clusters = stream
            .tokens()
            .iter()
            .filter_map(|token| {
                if let UnicodeTokenKind::CombiningAccentCluster { base, accents } = token.kind() {
                    Some((base.clone(), accents.clone(), token.span().as_range()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        assert_eq!(
            clusters,
            vec![
                ("u".to_owned(), vec![CombiningAccent::Tilde], 0..2),
                ("z".to_owned(), vec![CombiningAccent::Hat], 3..6),
                ("c".to_owned(), vec![CombiningAccent::Bar], 7..10),
                ("M".to_owned(), vec![CombiningAccent::Tilde], 11..14),
                ("Ω".to_owned(), vec![CombiningAccent::Hat], 15..19),
            ]
        );
    }

    #[test]
    fn accent_and_prime_adjacency_stays_lexically_distinct() {
        let y_bar_prime = non_trivia("Ȳ'");
        assert_eq!(
            y_bar_prime,
            vec![
                UnicodeTokenKind::CombiningAccentCluster {
                    base: "Y".to_owned(),
                    accents: vec![CombiningAccent::Bar],
                },
                UnicodeTokenKind::Punctuation("'"),
                UnicodeTokenKind::Eof,
            ]
        );

        let y_prime_bar = non_trivia("Y'̄");
        assert_eq!(
            y_prime_bar,
            vec![
                UnicodeTokenKind::PlainWord("Y"),
                UnicodeTokenKind::CombiningAccentCluster {
                    base: "'".to_owned(),
                    accents: vec![CombiningAccent::Bar],
                },
                UnicodeTokenKind::Eof,
            ]
        );

        let grouped = stream("{Y'}̄");
        assert_eq!(grouped.diagnostics(), &[]);
        assert_eq!(
            grouped
                .tokens()
                .iter()
                .map(UnicodeToken::kind)
                .filter(|kind| !matches!(kind, UnicodeTokenKind::Whitespace(_)))
                .cloned()
                .collect::<Vec<_>>(),
            vec![
                UnicodeTokenKind::LeftBrace,
                UnicodeTokenKind::PlainWord("Y"),
                UnicodeTokenKind::Punctuation("'"),
                UnicodeTokenKind::RightBrace,
                UnicodeTokenKind::CombiningAccentMark(CombiningAccent::Bar),
                UnicodeTokenKind::Eof,
            ]
        );
    }

    #[test]
    fn existing_latex_passthrough_keeps_overlay_marks() {
        let tokens = non_trivia(r"\phi \supset̸ \leqslant̸");

        assert_eq!(
            tokens,
            vec![
                UnicodeTokenKind::ExistingLatex(r"\phi"),
                UnicodeTokenKind::ExistingLatex(r"\supset̸"),
                UnicodeTokenKind::ExistingLatex(r"\leqslant̸"),
                UnicodeTokenKind::Eof,
            ]
        );
    }

    #[test]
    fn direct_symbols_arrows_and_linear_arrow_pieces_are_explicit() {
        let tokens = non_trivia("≤ ⩾ ⥲ A ─u→ B A ←u─ B");

        assert_eq!(
            tokens,
            vec![
                UnicodeTokenKind::DirectSymbol("≤"),
                UnicodeTokenKind::DirectSymbol("⩾"),
                UnicodeTokenKind::DirectSymbol("⥲"),
                UnicodeTokenKind::PlainWord("A"),
                UnicodeTokenKind::ArrowShaft("─"),
                UnicodeTokenKind::PlainWord("u"),
                UnicodeTokenKind::ArrowHead(ArrowDirection::Right),
                UnicodeTokenKind::PlainWord("B"),
                UnicodeTokenKind::PlainWord("A"),
                UnicodeTokenKind::ArrowHead(ArrowDirection::Left),
                UnicodeTokenKind::PlainWord("u"),
                UnicodeTokenKind::ArrowShaft("─"),
                UnicodeTokenKind::PlainWord("B"),
                UnicodeTokenKind::Eof,
            ]
        );
    }

    #[test]
    fn unknown_unicode_is_visible_and_diagnostic() {
        let stream = stream("🙂");

        assert_eq!(stream.diagnostics().len(), 1);
        assert_eq!(
            stream.diagnostics().first().map(UnicodeLexDiagnostic::kind),
            Some(UnicodeLexDiagnosticKind::UnknownSourceShape)
        );
        assert_eq!(
            stream.tokens().first().map(UnicodeToken::kind),
            Some(&UnicodeTokenKind::UnknownScalar("🙂"))
        );
    }

    #[test]
    fn detached_combining_mark_is_diagnostic() {
        let stream = stream("\u{0304}x");

        assert_eq!(stream.diagnostics().len(), 1);
        assert_eq!(
            stream.diagnostics().first().map(UnicodeLexDiagnostic::kind),
            Some(UnicodeLexDiagnosticKind::DetachedCombiningMark)
        );
        assert_eq!(
            stream
                .diagnostics()
                .first()
                .map(|diagnostic| diagnostic.span().as_range()),
            Some(0..2)
        );
    }

    #[test]
    fn malformed_latex_passthrough_is_diagnostic() {
        let stream = stream(r"\");

        assert_eq!(stream.diagnostics().len(), 1);
        assert_eq!(
            stream.diagnostics().first().map(UnicodeLexDiagnostic::kind),
            Some(UnicodeLexDiagnosticKind::MalformedLatexPassthrough)
        );
        assert!(
            stream
                .tokens()
                .iter()
                .any(|token| matches!(token.kind(), UnicodeTokenKind::Error))
        );
    }

    #[test]
    fn eof_and_cursor_checkpoint_restore_are_stable() {
        let stream = stream("x≤y");
        let mut cursor = stream.cursor();

        assert_eq!(cursor.peek().kind(), &UnicodeTokenKind::PlainWord("x"));
        let checkpoint = cursor.checkpoint();
        assert_eq!(cursor.advance().kind(), &UnicodeTokenKind::PlainWord("x"));
        assert_eq!(cursor.advance().kind(), &UnicodeTokenKind::DirectSymbol("≤"));
        cursor.restore(checkpoint);
        assert_eq!(cursor.advance().kind(), &UnicodeTokenKind::PlainWord("x"));
        while !cursor.is_eof() {
            cursor.advance();
        }
        assert_eq!(cursor.advance().kind(), &UnicodeTokenKind::Eof);
        assert_eq!(cursor.advance().kind(), &UnicodeTokenKind::Eof);
    }
}
