use logos::{Lexer, Logos};

use crate::SourceSpan;

/// One TeX math-body token with a byte span in the original source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Token<'src> {
    kind: TokenKind<'src>,
    span: SourceSpan,
}

impl<'src> Token<'src> {
    const fn new(kind: TokenKind<'src>, span: SourceSpan) -> Self {
        Self { kind, span }
    }

    pub(crate) const fn kind(self) -> TokenKind<'src> {
        self.kind
    }

    pub(crate) const fn span(self) -> SourceSpan {
        self.span
    }
}

/// Lexical categories for TeX math input.
///
/// The lexer records TeX surface shape only. Command meaning, such as whether
/// `\frac` expects two arguments, belongs to the parser and command registry.
#[derive(Logos, Clone, Copy, Debug, PartialEq, Eq)]
enum RawTokenKind<'src> {
    #[regex(r"\\[A-Za-z]+", |lex| lex.slice(), priority = 6)]
    CommandWord(&'src str),
    #[token("\\\\", priority = 5)]
    RowSeparator,
    #[regex(r"\\[^\r\nA-Za-z]", |lex| lex.slice(), priority = 2)]
    ControlSymbol(&'src str),
    #[token("{")]
    LeftBrace,
    #[token("}")]
    RightBrace,
    #[token("[")]
    LeftBracket,
    #[token("]")]
    RightBracket,
    #[token("(")]
    LeftParen,
    #[token(")")]
    RightParen,
    #[token("^")]
    Superscript,
    #[token("_")]
    Subscript,
    #[token("&")]
    Alignment,
    #[token("%", lex_comment)]
    Comment(&'src str),
    #[regex(r"[ \t\r\n]+", |lex| lex.slice())]
    Whitespace(&'src str),
    #[regex(r"[0-9]+(?:\.[0-9]+)?", |lex| lex.slice())]
    Number(&'src str),
    #[regex(r"[A-Za-z]+", |lex| lex.slice())]
    Identifier(&'src str),
    #[regex(r#"[+\-=*/.,;:|<>!?@#~$'`"]"#, |lex| lex.slice())]
    Punctuation(&'src str),
    #[regex(r"[^\x00-\x7F]", |lex| lex.slice())]
    UnicodeSymbol(&'src str),
}

/// Public within the crate so the parser can match token shape without seeing
/// logos internals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind<'src> {
    CommandWord(&'src str),
    ControlSymbol(&'src str),
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftParen,
    RightParen,
    Superscript,
    Subscript,
    Alignment,
    RowSeparator,
    Comment(&'src str),
    Whitespace(&'src str),
    Number(&'src str),
    Identifier(&'src str),
    Punctuation(&'src str),
    UnicodeSymbol(&'src str),
    Error,
    Eof,
}

impl<'src> From<RawTokenKind<'src>> for TokenKind<'src> {
    fn from(kind: RawTokenKind<'src>) -> Self {
        match kind {
            RawTokenKind::CommandWord(text) => Self::CommandWord(text),
            RawTokenKind::RowSeparator => Self::RowSeparator,
            RawTokenKind::ControlSymbol(text) => Self::ControlSymbol(text),
            RawTokenKind::LeftBrace => Self::LeftBrace,
            RawTokenKind::RightBrace => Self::RightBrace,
            RawTokenKind::LeftBracket => Self::LeftBracket,
            RawTokenKind::RightBracket => Self::RightBracket,
            RawTokenKind::LeftParen => Self::LeftParen,
            RawTokenKind::RightParen => Self::RightParen,
            RawTokenKind::Superscript => Self::Superscript,
            RawTokenKind::Subscript => Self::Subscript,
            RawTokenKind::Alignment => Self::Alignment,
            RawTokenKind::Comment(text) => Self::Comment(text),
            RawTokenKind::Whitespace(text) => Self::Whitespace(text),
            RawTokenKind::Number(text) => Self::Number(text),
            RawTokenKind::Identifier(text) => Self::Identifier(text),
            RawTokenKind::Punctuation(text) => Self::Punctuation(text),
            RawTokenKind::UnicodeSymbol(text) => Self::UnicodeSymbol(text),
        }
    }
}

/// One lexer diagnostic with a stable kind and source span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LexDiagnostic {
    kind: LexDiagnosticKind,
    span: SourceSpan,
    message: &'static str,
}

impl LexDiagnostic {
    const fn new(kind: LexDiagnosticKind, span: SourceSpan, message: &'static str) -> Self {
        Self { kind, span, message }
    }

    pub(crate) const fn kind(&self) -> LexDiagnosticKind {
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
pub(crate) enum LexDiagnosticKind {
    MalformedEscape,
    ControlCharacter,
    UnknownToken,
}

/// Byte-span-preserving TeX math token stream.
///
/// The stream keeps whitespace and comments as tokens so later source
/// translation can choose whether to preserve or normalise trivia. It always
/// ends with an explicit EOF token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TokenStream<'src> {
    source: &'src str,
    tokens: Vec<Token<'src>>,
    diagnostics: Vec<LexDiagnostic>,
}

impl<'src> TokenStream<'src> {
    pub(crate) fn new(source: &'src str) -> Self {
        let mut lexer = RawTokenKind::lexer(source);
        let mut tokens = Vec::new();
        let mut diagnostics = Vec::new();

        while let Some(next) = lexer.next() {
            let range = lexer.span();
            let span = SourceSpan::new(range.start, range.end);
            match next {
                Ok(kind) => tokens.push(Token::new(kind.into(), span)),
                Err(()) => {
                    diagnostics.push(diagnostic_for_invalid_slice(source, span));
                    tokens.push(Token::new(TokenKind::Error, span));
                }
            }
        }

        let eof = SourceSpan::new(source.len(), source.len());
        tokens.push(Token::new(TokenKind::Eof, eof));

        Self {
            source,
            tokens,
            diagnostics,
        }
    }

    pub(crate) fn source(&self) -> &'src str {
        self.source
    }

    pub(crate) fn tokens(&self) -> &[Token<'src>] {
        &self.tokens
    }

    pub(crate) fn diagnostics(&self) -> &[LexDiagnostic] {
        &self.diagnostics
    }

    pub(crate) fn cursor(&self) -> TokenCursor<'_, 'src> {
        TokenCursor::new(&self.tokens)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TokenCursor<'stream, 'src> {
    tokens: &'stream [Token<'src>],
    position: usize,
}

impl<'stream, 'src> TokenCursor<'stream, 'src> {
    pub(crate) const fn new(tokens: &'stream [Token<'src>]) -> Self {
        Self { tokens, position: 0 }
    }

    pub(crate) fn peek(&self) -> Token<'src> {
        self.tokens
            .get(self.position)
            .copied()
            .unwrap_or_else(|| self.eof_token())
    }

    pub(crate) fn advance(&mut self) -> Token<'src> {
        let current = self.peek();
        if !matches!(current.kind(), TokenKind::Eof) {
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
        matches!(self.peek().kind(), TokenKind::Eof)
    }

    pub(crate) fn previous_span(&self) -> SourceSpan {
        let previous_index = self.position.saturating_sub(1);
        self.tokens
            .get(previous_index)
            .map_or_else(|| SourceSpan::new(0, 0), |token| token.span())
    }

    fn eof_position(&self) -> usize {
        self.tokens.len().saturating_sub(1)
    }

    fn eof_token(&self) -> Token<'src> {
        self.tokens
            .last()
            .copied()
            .unwrap_or_else(|| Token::new(TokenKind::Eof, SourceSpan::new(0, 0)))
    }
}

fn lex_comment<'src>(lex: &mut Lexer<'src, RawTokenKind<'src>>) -> &'src str {
    let bytes = lex.remainder().as_bytes();
    let mut extra: usize = 0;
    for byte in bytes {
        if matches!(*byte, b'\r' | b'\n') {
            break;
        }
        extra = extra.saturating_add(1);
    }
    lex.bump(extra);
    lex.slice()
}

fn diagnostic_for_invalid_slice(source: &str, span: SourceSpan) -> LexDiagnostic {
    let text = &source[span.as_range()];
    if text == "\\" || text.starts_with("\\\r") || text.starts_with("\\\n") {
        return LexDiagnostic::new(LexDiagnosticKind::MalformedEscape, span, "invalid TeX escape sequence");
    }
    if text
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\t' | '\r' | '\n'))
    {
        return LexDiagnostic::new(
            LexDiagnosticKind::ControlCharacter,
            span,
            "invalid control character in TeX math",
        );
    }
    LexDiagnostic::new(LexDiagnosticKind::UnknownToken, span, "invalid TeX math token")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_kinds(source: &str) -> Vec<TokenKind<'_>> {
        let stream = TokenStream::new(source);
        assert_eq!(stream.diagnostics(), &[]);
        stream.tokens().iter().map(|token| token.kind()).collect()
    }

    fn non_trivia_kinds(source: &str) -> Vec<TokenKind<'_>> {
        token_kinds(source)
            .into_iter()
            .filter(|kind| !matches!(kind, TokenKind::Whitespace(_) | TokenKind::Comment(_)))
            .collect()
    }

    #[test]
    fn command_words_are_lexical_not_semantic() {
        let kinds = non_trivia_kinds(r"\alpha \operatorname \begin");

        assert_eq!(
            kinds,
            vec![
                TokenKind::CommandWord(r"\alpha"),
                TokenKind::CommandWord(r"\operatorname"),
                TokenKind::CommandWord(r"\begin"),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn control_symbols_and_row_separators_are_distinct() {
        let kinds = non_trivia_kinds(r"\, \{ \\ \_");

        assert_eq!(
            kinds,
            vec![
                TokenKind::ControlSymbol(r"\,"),
                TokenKind::ControlSymbol(r"\{"),
                TokenKind::RowSeparator,
                TokenKind::ControlSymbol(r"\_"),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn groups_scripts_and_nested_delimiters_keep_byte_spans() {
        let stream = TokenStream::new(r"x_i^{2} [y](z)");
        assert_eq!(stream.diagnostics(), &[]);
        let tokens = stream.tokens();

        assert!(tokens.iter().any(|token| token.kind() == TokenKind::Subscript));
        assert!(tokens.iter().any(|token| token.kind() == TokenKind::Superscript));
        assert!(tokens.iter().any(|token| token.kind() == TokenKind::LeftBrace));
        assert!(tokens.iter().any(|token| token.kind() == TokenKind::RightBrace));
        assert!(tokens.iter().any(|token| token.kind() == TokenKind::LeftBracket));
        assert!(tokens.iter().any(|token| token.kind() == TokenKind::RightBracket));
        assert!(tokens.iter().any(|token| token.kind() == TokenKind::LeftParen));
        assert!(tokens.iter().any(|token| token.kind() == TokenKind::RightParen));
        assert_eq!(
            tokens
                .iter()
                .find(|token| token.kind() == TokenKind::Subscript)
                .map(|token| token.span().as_range()),
            Some(1..2)
        );
    }

    #[test]
    fn comments_and_whitespace_are_preserved_as_tokens() {
        let kinds = token_kinds("a % comment\nb");

        assert_eq!(
            kinds,
            vec![
                TokenKind::Identifier("a"),
                TokenKind::Whitespace(" "),
                TokenKind::Comment("% comment"),
                TokenKind::Whitespace("\n"),
                TokenKind::Identifier("b"),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn matrix_alignment_and_row_separators_are_explicit() {
        let kinds = non_trivia_kinds(r"a & b \\ c & d");

        assert_eq!(
            kinds,
            vec![
                TokenKind::Identifier("a"),
                TokenKind::Alignment,
                TokenKind::Identifier("b"),
                TokenKind::RowSeparator,
                TokenKind::Identifier("c"),
                TokenKind::Alignment,
                TokenKind::Identifier("d"),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn unicode_math_symbols_are_scalar_tokens() {
        let kinds = non_trivia_kinds("α≤β");

        assert_eq!(
            kinds,
            vec![
                TokenKind::UnicodeSymbol("α"),
                TokenKind::UnicodeSymbol("≤"),
                TokenKind::UnicodeSymbol("β"),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn malformed_escape_sequences_get_typed_diagnostics() {
        let stream = TokenStream::new(r"\");

        assert_eq!(stream.diagnostics().len(), 1);
        assert_eq!(
            stream.diagnostics().first().map(LexDiagnostic::kind),
            Some(LexDiagnosticKind::MalformedEscape)
        );
        assert_eq!(
            stream
                .diagnostics()
                .first()
                .map(|diagnostic| diagnostic.span().as_range()),
            Some(0..1)
        );
        assert!(stream.tokens().iter().any(|token| token.kind() == TokenKind::Error));
        assert!(matches!(
            stream.tokens().last().map(|token| token.kind()),
            Some(TokenKind::Eof)
        ));
    }

    #[test]
    fn invalid_control_bytes_get_typed_diagnostics() {
        let stream = TokenStream::new("x\u{0000}y");

        assert_eq!(stream.diagnostics().len(), 1);
        assert_eq!(
            stream.diagnostics().first().map(LexDiagnostic::kind),
            Some(LexDiagnosticKind::ControlCharacter)
        );
    }

    #[test]
    fn cursor_peeks_advances_and_restores() {
        let stream = TokenStream::new(r"x+y");
        let mut cursor = stream.cursor();

        assert_eq!(cursor.peek().kind(), TokenKind::Identifier("x"));
        let checkpoint = cursor.checkpoint();
        assert_eq!(cursor.advance().kind(), TokenKind::Identifier("x"));
        assert_eq!(cursor.advance().kind(), TokenKind::Punctuation("+"));
        cursor.restore(checkpoint);
        assert_eq!(cursor.advance().kind(), TokenKind::Identifier("x"));
        while !cursor.is_eof() {
            cursor.advance();
        }
        assert_eq!(cursor.advance().kind(), TokenKind::Eof);
        assert_eq!(cursor.advance().kind(), TokenKind::Eof);
    }

    #[test]
    fn empty_stream_contains_only_eof() {
        let stream = TokenStream::new("");

        assert_eq!(stream.tokens().len(), 1);
        assert!(matches!(
            stream.tokens().first().map(|token| token.kind()),
            Some(TokenKind::Eof)
        ));
    }
}
