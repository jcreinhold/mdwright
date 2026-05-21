use logos::Logos;

use crate::error::{LatexError, LatexErrorKind, SourceSpan};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Token<'src> {
    kind: TokenKind<'src>,
    span: SourceSpan,
}

impl<'src> Token<'src> {
    const fn new(kind: TokenKind<'src>, span: SourceSpan) -> Self {
        Self { kind, span }
    }
}

#[derive(Logos, Clone, Copy, Debug, PartialEq, Eq)]
#[logos(skip r"[ \t\r\n]+")]
pub(crate) enum TokenKind<'src> {
    #[regex(r"\\[A-Za-z]+", |lex| lex.slice(), priority = 6)]
    CommandWord(&'src str),
    #[regex(r#"\\."#, |lex| lex.slice(), priority = 2)]
    ControlSymbol(&'src str),
    #[token("{")]
    LeftBrace,
    #[token("}")]
    RightBrace,
    #[token("[")]
    LeftBracket,
    #[token("]")]
    RightBracket,
    #[token("^")]
    Superscript,
    #[token("_")]
    Subscript,
    #[token("&")]
    Alignment,
    #[token("\\\\", priority = 5)]
    RowSeparator,
    #[regex(r"[0-9]+(?:\.[0-9]+)?", |lex| lex.slice())]
    Number(&'src str),
    #[regex(r"[A-Za-z]+", |lex| lex.slice())]
    Identifier(&'src str),
    #[regex(r"[%#~$]", |lex| lex.slice())]
    SpecialChar(&'src str),
    #[regex(r"[^\\{}\[\]^_&%#~$\sA-Za-z0-9]+", |lex| lex.slice())]
    Text(&'src str),
}

pub(crate) fn lex(source: &str) -> (Vec<Token<'_>>, Vec<LatexError>) {
    let mut lexer = TokenKind::lexer(source);
    let mut tokens = Vec::new();
    let mut errors = Vec::new();

    while let Some(next) = lexer.next() {
        let span = SourceSpan::new(lexer.span().start, lexer.span().end);
        match next {
            Ok(kind) => tokens.push(Token::new(kind, span)),
            Err(()) => errors.push(LatexError::new(LatexErrorKind::Lexical, span, "invalid TeX math token")),
        }
    }

    let eof = SourceSpan::new(source.len(), source.len());
    tokens.push(Token::new(TokenKind::Text(""), eof));

    (tokens, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind<'_>> {
        let (tokens, errors) = lex(source);
        assert_eq!(errors, Vec::new());
        tokens.into_iter().map(|token| token.kind).collect()
    }

    #[test]
    fn tokenises_commands_symbols_braces_and_scripts_with_byte_spans() {
        let (tokens, errors) = lex(r"\alpha_i^{2} + \, x");

        assert_eq!(errors, Vec::new());
        assert!(matches!(
            tokens.first().map(|token| token.kind),
            Some(TokenKind::CommandWord(r"\alpha"))
        ));
        assert_eq!(tokens.first().map(|token| token.span.as_range()), Some(0..6));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Subscript));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Superscript));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::LeftBrace));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::RightBrace));
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::ControlSymbol(r"\,")))
        );
    }

    #[test]
    fn tokenises_alignment_and_row_separators() {
        let kinds = kinds(r"a & b \\ c");

        assert!(kinds.contains(&TokenKind::Alignment));
        assert!(kinds.contains(&TokenKind::RowSeparator));
    }

    #[test]
    fn lexical_errors_keep_source_spans() {
        let (_, errors) = lex(r"\");

        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors.as_slice(),
            [first] if first.kind() == &LatexErrorKind::Lexical && first.span().as_range() == (0..1)
        ));
    }
}
