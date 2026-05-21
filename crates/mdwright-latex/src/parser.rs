//! Recursive-descent parser internals.
//!
//! Design comparison: a generic node with optional children would be easy to
//! extend, but it would also allow impossible values such as a fraction without
//! a denominator or a script with no base. This parser uses specific variants
//! with required child fields. The implementation is larger, but malformed
//! input stays in diagnostics instead of becoming a renderable tree.

#![allow(
    clippy::wildcard_enum_match_arm,
    reason = "parser recovery groups token variants that share the same fallback"
)]

use crate::SourceSpan;
use crate::lexer::{LexDiagnostic, Token, TokenCursor, TokenKind, TokenStream};
use crate::registry::{CommandCategory, CommandInfo, SupportStatus, lookup_command};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MathBody<'src> {
    elements: Vec<Node<'src>>,
    span: SourceSpan,
}

impl<'src> MathBody<'src> {
    fn new(elements: Vec<Node<'src>>, span: SourceSpan) -> Self {
        Self { elements, span }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Node<'src> {
    kind: NodeKind<'src>,
    span: SourceSpan,
}

impl<'src> Node<'src> {
    fn new(kind: NodeKind<'src>, span: SourceSpan) -> Self {
        Self { kind, span }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NodeKind<'src> {
    Atom(Atom<'src>),
    Group(Group<'src>),
    Fraction(Fraction<'src>),
    Sqrt(Sqrt<'src>),
    Accent(Accent<'src>),
    Script(Script<'src>),
    Delimited(Delimited<'src>),
    Environment(Environment<'src>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Atom<'src> {
    Identifier(&'src str),
    Number(&'src str),
    Punctuation(&'src str),
    UnicodeSymbol(&'src str),
    ControlSymbol(&'src str),
    CommandSymbol(&'src str),
    Delimiter(Delimiter<'src>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Group<'src> {
    body: MathBody<'src>,
    delimiter: GroupDelimiter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupDelimiter {
    Brace,
    Bracket,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fraction<'src> {
    command: FractionCommand,
    numerator: Group<'src>,
    denominator: Group<'src>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FractionCommand {
    Frac,
    DisplayFrac,
    TextFrac,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sqrt<'src> {
    degree: Option<Group<'src>>,
    body: Group<'src>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Accent<'src> {
    accent: AccentKind,
    body: Group<'src>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccentKind {
    Hat,
    Bar,
    Tilde,
    Vec,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Script<'src> {
    base: Box<ScriptBase<'src>>,
    subscript: Option<ScriptArgument<'src>>,
    superscript: Option<ScriptArgument<'src>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScriptBase<'src> {
    Atom(Atom<'src>),
    Group(Group<'src>),
    Fraction(Fraction<'src>),
    Sqrt(Sqrt<'src>),
    Accent(Accent<'src>),
    Delimited(Delimited<'src>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScriptArgument<'src> {
    Atom { atom: Atom<'src>, span: SourceSpan },
    Group(Group<'src>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Delimited<'src> {
    opener: Delimiter<'src>,
    body: MathBody<'src>,
    closer: Delimiter<'src>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delimiter<'src> {
    Source(&'src str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Environment<'src> {
    name: &'src str,
    preamble: Option<&'src str>,
    rows: Vec<Row<'src>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row<'src> {
    cells: Vec<Cell<'src>>,
    span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Cell<'src> {
    body: MathBody<'src>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParseDiagnostic {
    kind: ParseDiagnosticKind,
    span: SourceSpan,
    message: String,
}

impl ParseDiagnostic {
    fn new(kind: ParseDiagnosticKind, span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParseDiagnosticKind {
    Lexical,
    UnexpectedToken,
    MissingRequiredArgument,
    UnbalancedGroup,
    UnmatchedEnvironmentEnd,
    ScriptWithoutBase,
    DuplicateSubscript,
    DuplicateSuperscript,
    UnsupportedCommand,
    UnsupportedEnvironment,
}

pub(crate) fn parse_math_body(source: &str) -> Result<MathBody<'_>, Vec<ParseDiagnostic>> {
    let stream = TokenStream::new(source);
    let mut diagnostics = stream
        .diagnostics()
        .iter()
        .map(convert_lex_diagnostic)
        .collect::<Vec<_>>();
    let mut parser = Parser::new(stream.source(), stream.tokens());
    let body = parser.parse_sequence(Stop::TopLevel);
    diagnostics.extend(parser.diagnostics);
    if diagnostics.is_empty() {
        Ok(body)
    } else {
        Err(diagnostics)
    }
}

fn convert_lex_diagnostic(diagnostic: &LexDiagnostic) -> ParseDiagnostic {
    ParseDiagnostic::new(ParseDiagnosticKind::Lexical, diagnostic.span(), diagnostic.message())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    TopLevel,
    Group(GroupDelimiter),
    Delimited,
    Cell,
}

struct Parser<'stream, 'src> {
    source: &'src str,
    cursor: TokenCursor<'stream, 'src>,
    diagnostics: Vec<ParseDiagnostic>,
}

impl<'stream, 'src> Parser<'stream, 'src> {
    fn new(source: &'src str, tokens: &'stream [Token<'src>]) -> Self {
        Self {
            source,
            cursor: TokenCursor::new(tokens),
            diagnostics: Vec::new(),
        }
    }

    fn parse_sequence(&mut self, stop: Stop) -> MathBody<'src> {
        let start = self.cursor.peek().span();
        let mut elements = Vec::new();
        while !self.cursor.is_eof() {
            self.skip_trivia();
            if self.at_stop(stop) {
                break;
            }
            if matches!(self.cursor.peek().kind(), TokenKind::RightBrace) {
                self.diagnostics.push(ParseDiagnostic::new(
                    ParseDiagnosticKind::UnbalancedGroup,
                    self.cursor.peek().span(),
                    "unmatched `}` in TeX math",
                ));
                self.cursor.advance();
                continue;
            }
            if let Some(node) = self.parse_element() {
                elements.push(node);
            }
        }
        let end = elements.last().map_or(start, |node| node.span);
        MathBody::new(elements, join_spans(start, end))
    }

    fn parse_element(&mut self) -> Option<Node<'src>> {
        let primary = self.parse_primary()?;
        self.parse_scripts(primary)
    }

    fn parse_primary(&mut self) -> Option<Node<'src>> {
        self.skip_trivia();
        let token = self.cursor.peek();
        match token.kind() {
            TokenKind::Identifier(text) => {
                self.cursor.advance();
                Some(Node::new(NodeKind::Atom(Atom::Identifier(text)), token.span()))
            }
            TokenKind::Number(text) => {
                self.cursor.advance();
                Some(Node::new(NodeKind::Atom(Atom::Number(text)), token.span()))
            }
            TokenKind::Punctuation(text) => {
                self.cursor.advance();
                Some(Node::new(NodeKind::Atom(Atom::Punctuation(text)), token.span()))
            }
            TokenKind::UnicodeSymbol(text) => {
                self.cursor.advance();
                Some(Node::new(NodeKind::Atom(Atom::UnicodeSymbol(text)), token.span()))
            }
            TokenKind::ControlSymbol(text) => {
                self.cursor.advance();
                Some(Node::new(NodeKind::Atom(Atom::ControlSymbol(text)), token.span()))
            }
            TokenKind::LeftParen | TokenKind::RightParen | TokenKind::LeftBracket | TokenKind::RightBracket => {
                self.cursor.advance();
                let text = token_text(token);
                Some(Node::new(
                    NodeKind::Atom(Atom::Delimiter(Delimiter::Source(text))),
                    token.span(),
                ))
            }
            TokenKind::LeftBrace => self.parse_group_node(),
            TokenKind::CommandWord(command) => self.parse_command(command, token.span()),
            TokenKind::Subscript | TokenKind::Superscript => {
                self.diagnostics.push(ParseDiagnostic::new(
                    ParseDiagnosticKind::ScriptWithoutBase,
                    token.span(),
                    "script marker has no base",
                ));
                self.cursor.advance();
                drop(self.parse_script_argument());
                None
            }
            TokenKind::Alignment | TokenKind::RowSeparator => {
                self.diagnostics.push(ParseDiagnostic::new(
                    ParseDiagnosticKind::UnexpectedToken,
                    token.span(),
                    "alignment marker outside an environment",
                ));
                self.cursor.advance();
                None
            }
            TokenKind::RightBrace
            | TokenKind::Comment(_)
            | TokenKind::Whitespace(_)
            | TokenKind::Error
            | TokenKind::Eof => {
                self.cursor.advance();
                None
            }
        }
    }

    fn parse_group_node(&mut self) -> Option<Node<'src>> {
        let open = self.cursor.advance();
        let body = self.parse_sequence(Stop::Group(GroupDelimiter::Brace));
        let close = self.consume_group_close(GroupDelimiter::Brace, open.span())?;
        let span = join_spans(open.span(), close.span());
        Some(Node::new(
            NodeKind::Group(Group {
                body,
                delimiter: GroupDelimiter::Brace,
            }),
            span,
        ))
    }

    fn parse_command(&mut self, command: &'src str, span: SourceSpan) -> Option<Node<'src>> {
        self.cursor.advance();
        let name = command_name(command);
        match name {
            "frac" | "dfrac" | "tfrac" => self.parse_fraction(name, span),
            "sqrt" => self.parse_sqrt(span),
            "hat" | "bar" | "tilde" | "vec" => self.parse_accent(name, span),
            "left" => self.parse_delimited(span),
            "begin" => self.parse_environment(span),
            "end" => {
                let env = self.parse_environment_name();
                let detail = env.map_or_else(
                    || "unmatched `\\end`".to_owned(),
                    |name| format!("unmatched `\\end{{{name}}}`"),
                );
                self.diagnostics.push(ParseDiagnostic::new(
                    ParseDiagnosticKind::UnmatchedEnvironmentEnd,
                    span,
                    detail,
                ));
                None
            }
            _ => self.parse_registry_command(name, span),
        }
    }

    fn parse_registry_command(&mut self, name: &'src str, span: SourceSpan) -> Option<Node<'src>> {
        let Some(command) = lookup_command(name) else {
            self.diagnostics.push(ParseDiagnostic::new(
                ParseDiagnosticKind::UnsupportedCommand,
                span,
                format!("unsupported TeX command `\\{name}`"),
            ));
            return None;
        };
        match command.support() {
            SupportStatus::DirectUnicode if command_is_renderable_atom(command) => {
                Some(Node::new(NodeKind::Atom(Atom::CommandSymbol(name)), span))
            }
            SupportStatus::RecognisedNoOutput => Some(Node::new(NodeKind::Atom(Atom::CommandSymbol(name)), span)),
            _ => {
                self.diagnostics.push(ParseDiagnostic::new(
                    ParseDiagnosticKind::UnsupportedCommand,
                    span,
                    format!("known but unsupported TeX command `\\{name}`"),
                ));
                None
            }
        }
    }

    fn parse_fraction(&mut self, name: &str, span: SourceSpan) -> Option<Node<'src>> {
        let numerator = self.parse_required_group("fraction numerator")?;
        let denominator = self.parse_required_group("fraction denominator")?;
        let end = denominator.body.span;
        Some(Node::new(
            NodeKind::Fraction(Fraction {
                command: match name {
                    "dfrac" => FractionCommand::DisplayFrac,
                    "tfrac" => FractionCommand::TextFrac,
                    _ => FractionCommand::Frac,
                },
                numerator,
                denominator,
            }),
            join_spans(span, end),
        ))
    }

    fn parse_sqrt(&mut self, span: SourceSpan) -> Option<Node<'src>> {
        let degree = self.parse_optional_bracket_group();
        let body = self.parse_required_group("square-root body")?;
        let end = body.body.span;
        Some(Node::new(NodeKind::Sqrt(Sqrt { degree, body }), join_spans(span, end)))
    }

    fn parse_accent(&mut self, name: &str, span: SourceSpan) -> Option<Node<'src>> {
        let body = self.parse_required_group("accent body")?;
        let end = body.body.span;
        Some(Node::new(
            NodeKind::Accent(Accent {
                accent: match name {
                    "hat" => AccentKind::Hat,
                    "bar" => AccentKind::Bar,
                    "tilde" => AccentKind::Tilde,
                    _ => AccentKind::Vec,
                },
                body,
            }),
            join_spans(span, end),
        ))
    }

    fn parse_delimited(&mut self, span: SourceSpan) -> Option<Node<'src>> {
        let opener = self.parse_delimiter("left delimiter")?;
        let body = self.parse_sequence(Stop::Delimited);
        let right = self.cursor.peek();
        if command_name_from_token(right) != Some("right") {
            self.diagnostics.push(ParseDiagnostic::new(
                ParseDiagnosticKind::UnexpectedToken,
                right.span(),
                "missing `\\right` for `\\left` delimiter",
            ));
            return None;
        }
        self.cursor.advance();
        let closer = self.parse_delimiter("right delimiter")?;
        Some(Node::new(
            NodeKind::Delimited(Delimited { opener, body, closer }),
            join_spans(span, self.previous_span()),
        ))
    }

    fn parse_environment(&mut self, span: SourceSpan) -> Option<Node<'src>> {
        let name = self.parse_environment_name()?;
        if !is_supported_environment(name) {
            self.diagnostics.push(ParseDiagnostic::new(
                ParseDiagnosticKind::UnsupportedEnvironment,
                span,
                format!("unsupported TeX environment `{name}`"),
            ));
            self.skip_until_environment_end(name);
            return None;
        }
        let preamble = (name == "array")
            .then(|| self.parse_raw_braced_text("array column specification"))
            .flatten();
        let rows = self.parse_environment_rows();
        let end = self.cursor.peek();
        if command_name_from_token(end) == Some("end") {
            self.cursor.advance();
            let end_name = self.parse_environment_name();
            if end_name != Some(name) {
                self.diagnostics.push(ParseDiagnostic::new(
                    ParseDiagnosticKind::UnmatchedEnvironmentEnd,
                    end.span(),
                    format!("expected `\\end{{{name}}}`"),
                ));
                return None;
            }
        } else {
            self.diagnostics.push(ParseDiagnostic::new(
                ParseDiagnosticKind::UnmatchedEnvironmentEnd,
                end.span(),
                format!("missing `\\end{{{name}}}`"),
            ));
            return None;
        }
        Some(Node::new(
            NodeKind::Environment(Environment { name, preamble, rows }),
            join_spans(span, self.previous_span()),
        ))
    }

    fn parse_environment_rows(&mut self) -> Vec<Row<'src>> {
        let mut rows = Vec::new();
        loop {
            self.skip_trivia();
            if self.cursor.is_eof() || command_name_from_token(self.cursor.peek()) == Some("end") {
                break;
            }
            let start = self.cursor.peek().span();
            let mut cells = Vec::new();
            loop {
                let body = self.parse_sequence(Stop::Cell);
                cells.push(Cell { body });
                self.skip_trivia();
                match self.cursor.peek().kind() {
                    TokenKind::Alignment => {
                        self.cursor.advance();
                    }
                    TokenKind::RowSeparator => {
                        let end = self.cursor.advance();
                        rows.push(Row {
                            cells,
                            span: join_spans(start, end.span()),
                        });
                        break;
                    }
                    _ => {
                        let end = cells.last().map_or(start, |cell| cell.body.span);
                        rows.push(Row {
                            cells,
                            span: join_spans(start, end),
                        });
                        break;
                    }
                }
            }
        }
        rows
    }

    fn parse_scripts(&mut self, base: Node<'src>) -> Option<Node<'src>> {
        let base_span = base.span;
        let script_base = match ScriptBase::from_node(base) {
            Ok(base) => base,
            Err(node) => return Some(*node),
        };
        let mut subscript = None;
        let mut superscript = None;
        let mut end = base_span;
        loop {
            self.skip_trivia();
            match self.cursor.peek().kind() {
                TokenKind::Subscript => {
                    let marker = self.cursor.advance();
                    if subscript.is_some() {
                        self.diagnostics.push(ParseDiagnostic::new(
                            ParseDiagnosticKind::DuplicateSubscript,
                            marker.span(),
                            "duplicate subscript on the same base",
                        ));
                        drop(self.parse_script_argument());
                        continue;
                    }
                    if let Some(argument) = self.parse_script_argument() {
                        end = argument.span();
                        subscript = Some(argument);
                    }
                }
                TokenKind::Superscript => {
                    let marker = self.cursor.advance();
                    if superscript.is_some() {
                        self.diagnostics.push(ParseDiagnostic::new(
                            ParseDiagnosticKind::DuplicateSuperscript,
                            marker.span(),
                            "duplicate superscript on the same base",
                        ));
                        drop(self.parse_script_argument());
                        continue;
                    }
                    if let Some(argument) = self.parse_script_argument() {
                        end = argument.span();
                        superscript = Some(argument);
                    }
                }
                _ => break,
            }
        }
        if subscript.is_none() && superscript.is_none() {
            return Some(script_base.into_node(base_span));
        }
        Some(Node::new(
            NodeKind::Script(Script {
                base: Box::new(script_base),
                subscript,
                superscript,
            }),
            join_spans(base_span, end),
        ))
    }

    fn parse_script_argument(&mut self) -> Option<ScriptArgument<'src>> {
        self.skip_trivia();
        match self.cursor.peek().kind() {
            TokenKind::LeftBrace => self.parse_group_node().and_then(|node| match node.kind {
                NodeKind::Group(group) => Some(ScriptArgument::Group(group)),
                _ => None,
            }),
            TokenKind::Identifier(text) => {
                let token = self.cursor.advance();
                Some(ScriptArgument::Atom {
                    atom: Atom::Identifier(text),
                    span: token.span(),
                })
            }
            TokenKind::Number(text) => {
                let token = self.cursor.advance();
                Some(ScriptArgument::Atom {
                    atom: Atom::Number(text),
                    span: token.span(),
                })
            }
            TokenKind::Punctuation(text) => {
                let token = self.cursor.advance();
                Some(ScriptArgument::Atom {
                    atom: Atom::Punctuation(text),
                    span: token.span(),
                })
            }
            TokenKind::UnicodeSymbol(text) => {
                let token = self.cursor.advance();
                Some(ScriptArgument::Atom {
                    atom: Atom::UnicodeSymbol(text),
                    span: token.span(),
                })
            }
            TokenKind::ControlSymbol(text) => {
                let token = self.cursor.advance();
                Some(ScriptArgument::Atom {
                    atom: Atom::ControlSymbol(text),
                    span: token.span(),
                })
            }
            TokenKind::CommandWord(command) if is_direct_script_command(command_name(command)) => {
                let token = self.cursor.advance();
                Some(ScriptArgument::Atom {
                    atom: Atom::CommandSymbol(command_name(command)),
                    span: token.span(),
                })
            }
            _ => {
                self.diagnostics.push(ParseDiagnostic::new(
                    ParseDiagnosticKind::MissingRequiredArgument,
                    self.cursor.peek().span(),
                    "missing script argument",
                ));
                None
            }
        }
    }

    fn parse_required_group(&mut self, label: &str) -> Option<Group<'src>> {
        self.skip_trivia();
        if !matches!(self.cursor.peek().kind(), TokenKind::LeftBrace) {
            self.diagnostics.push(ParseDiagnostic::new(
                ParseDiagnosticKind::MissingRequiredArgument,
                self.cursor.peek().span(),
                format!("missing required {label}"),
            ));
            return None;
        }
        let open = self.cursor.advance();
        let body = self.parse_sequence(Stop::Group(GroupDelimiter::Brace));
        self.consume_group_close(GroupDelimiter::Brace, open.span())?;
        Some(Group {
            body: MathBody::new(body.elements, join_spans(open.span(), self.previous_span())),
            delimiter: GroupDelimiter::Brace,
        })
    }

    fn parse_optional_bracket_group(&mut self) -> Option<Group<'src>> {
        self.skip_trivia();
        if !matches!(self.cursor.peek().kind(), TokenKind::LeftBracket) {
            return None;
        }
        let open = self.cursor.advance();
        let body = self.parse_sequence(Stop::Group(GroupDelimiter::Bracket));
        let _ = self.consume_group_close(GroupDelimiter::Bracket, open.span())?;
        Some(Group {
            body: MathBody::new(body.elements, join_spans(open.span(), self.previous_span())),
            delimiter: GroupDelimiter::Bracket,
        })
    }

    fn consume_group_close(&mut self, delimiter: GroupDelimiter, owner: SourceSpan) -> Option<Token<'src>> {
        self.skip_trivia();
        let token = self.cursor.peek();
        let matches_close = matches!(
            (delimiter, token.kind()),
            (GroupDelimiter::Brace, TokenKind::RightBrace) | (GroupDelimiter::Bracket, TokenKind::RightBracket)
        );
        if matches_close {
            return Some(self.cursor.advance());
        }
        self.diagnostics.push(ParseDiagnostic::new(
            ParseDiagnosticKind::UnbalancedGroup,
            owner,
            "unbalanced TeX group",
        ));
        None
    }

    fn parse_environment_name(&mut self) -> Option<&'src str> {
        self.parse_raw_braced_text("environment name")
    }

    fn parse_raw_braced_text(&mut self, label: &str) -> Option<&'src str> {
        self.skip_trivia();
        let open = self.cursor.peek();
        if !matches!(open.kind(), TokenKind::LeftBrace) {
            self.diagnostics.push(ParseDiagnostic::new(
                ParseDiagnosticKind::MissingRequiredArgument,
                open.span(),
                format!("missing required {label}"),
            ));
            return None;
        }
        self.cursor.advance();
        self.skip_trivia();
        let first = self.cursor.peek();
        let start = first.span().start();
        let mut end = first.span().start();
        while !matches!(self.cursor.peek().kind(), TokenKind::RightBrace | TokenKind::Eof) {
            let token = self.cursor.advance();
            end = token.span().end();
        }
        if !matches!(self.cursor.peek().kind(), TokenKind::RightBrace) {
            self.diagnostics.push(ParseDiagnostic::new(
                ParseDiagnosticKind::UnbalancedGroup,
                open.span(),
                format!("unbalanced {label} group"),
            ));
            return None;
        }
        self.cursor.advance();
        self.source.get(start..end).or_else(|| Some(token_text(first)))
    }

    fn parse_delimiter(&mut self, label: &str) -> Option<Delimiter<'src>> {
        self.skip_trivia();
        let token = self.cursor.peek();
        let delimiter = match token.kind() {
            TokenKind::LeftParen
            | TokenKind::RightParen
            | TokenKind::LeftBracket
            | TokenKind::RightBracket
            | TokenKind::LeftBrace
            | TokenKind::RightBrace
            | TokenKind::Punctuation(_)
            | TokenKind::ControlSymbol(_) => Delimiter::Source(token_text(token)),
            _ => {
                self.diagnostics.push(ParseDiagnostic::new(
                    ParseDiagnosticKind::MissingRequiredArgument,
                    token.span(),
                    format!("missing {label}"),
                ));
                return None;
            }
        };
        self.cursor.advance();
        Some(delimiter)
    }

    fn at_stop(&self, stop: Stop) -> bool {
        match stop {
            Stop::TopLevel => self.cursor.is_eof(),
            Stop::Group(GroupDelimiter::Brace) => matches!(self.cursor.peek().kind(), TokenKind::RightBrace),
            Stop::Group(GroupDelimiter::Bracket) => matches!(self.cursor.peek().kind(), TokenKind::RightBracket),
            Stop::Delimited => command_name_from_token(self.cursor.peek()) == Some("right") || self.cursor.is_eof(),
            Stop::Cell => {
                self.cursor.is_eof()
                    || command_name_from_token(self.cursor.peek()) == Some("end")
                    || matches!(
                        self.cursor.peek().kind(),
                        TokenKind::Alignment | TokenKind::RowSeparator
                    )
            }
        }
    }

    fn skip_until_environment_end(&mut self, name: &str) {
        while !self.cursor.is_eof() {
            if command_name_from_token(self.cursor.peek()) == Some("end") {
                let checkpoint = self.cursor.checkpoint();
                self.cursor.advance();
                if self.parse_environment_name() == Some(name) {
                    return;
                }
                self.cursor.restore(checkpoint);
            }
            self.cursor.advance();
        }
    }

    fn skip_trivia(&mut self) {
        while matches!(
            self.cursor.peek().kind(),
            TokenKind::Whitespace(_) | TokenKind::Comment(_)
        ) {
            self.cursor.advance();
        }
    }

    fn previous_span(&self) -> SourceSpan {
        self.cursor.previous_span()
    }
}

impl<'src> ScriptBase<'src> {
    fn from_node(node: Node<'src>) -> Result<Self, Box<Node<'src>>> {
        match node.kind {
            NodeKind::Atom(atom) => Ok(Self::Atom(atom)),
            NodeKind::Group(group) => Ok(Self::Group(group)),
            NodeKind::Fraction(fraction) => Ok(Self::Fraction(fraction)),
            NodeKind::Sqrt(sqrt) => Ok(Self::Sqrt(sqrt)),
            NodeKind::Accent(accent) => Ok(Self::Accent(accent)),
            NodeKind::Delimited(delimited) => Ok(Self::Delimited(delimited)),
            NodeKind::Environment(_) | NodeKind::Script(_) => Err(Box::new(node)),
        }
    }

    fn into_node(self, span: SourceSpan) -> Node<'src> {
        match self {
            Self::Atom(atom) => Node::new(NodeKind::Atom(atom), span),
            Self::Group(group) => Node::new(NodeKind::Group(group), span),
            Self::Fraction(fraction) => Node::new(NodeKind::Fraction(fraction), span),
            Self::Sqrt(sqrt) => Node::new(NodeKind::Sqrt(sqrt), span),
            Self::Accent(accent) => Node::new(NodeKind::Accent(accent), span),
            Self::Delimited(delimited) => Node::new(NodeKind::Delimited(delimited), span),
        }
    }
}

impl ScriptArgument<'_> {
    fn span(&self) -> SourceSpan {
        match self {
            Self::Atom { span, .. } => *span,
            Self::Group(group) => group.body.span,
        }
    }
}

fn command_name(command: &str) -> &str {
    command.strip_prefix('\\').unwrap_or(command)
}

fn command_name_from_token(token: Token<'_>) -> Option<&str> {
    match token.kind() {
        TokenKind::CommandWord(command) => Some(command_name(command)),
        _ => None,
    }
}

fn command_is_renderable_atom(command: CommandInfo) -> bool {
    matches!(
        command.category(),
        CommandCategory::Symbol
            | CommandCategory::Greek
            | CommandCategory::BinaryOperator
            | CommandCategory::Relation
            | CommandCategory::Arrow
            | CommandCategory::Delimiter
            | CommandCategory::LargeOperator
            | CommandCategory::Function
    )
}

fn is_direct_script_command(name: &str) -> bool {
    lookup_command(name)
        .is_some_and(|command| command.support() == SupportStatus::DirectUnicode && command_is_renderable_atom(command))
}

fn is_supported_environment(name: &str) -> bool {
    lookup_command(name).is_some_and(|command| {
        command.category() == CommandCategory::Environment && command.support() == SupportStatus::ParsedConstruct
    })
}

fn token_text(token: Token<'_>) -> &str {
    match token.kind() {
        TokenKind::CommandWord(text)
        | TokenKind::ControlSymbol(text)
        | TokenKind::Comment(text)
        | TokenKind::Whitespace(text)
        | TokenKind::Number(text)
        | TokenKind::Identifier(text)
        | TokenKind::Punctuation(text)
        | TokenKind::UnicodeSymbol(text) => text,
        TokenKind::LeftBrace => "{",
        TokenKind::RightBrace => "}",
        TokenKind::LeftBracket => "[",
        TokenKind::RightBracket => "]",
        TokenKind::LeftParen => "(",
        TokenKind::RightParen => ")",
        TokenKind::Superscript => "^",
        TokenKind::Subscript => "_",
        TokenKind::Alignment => "&",
        TokenKind::RowSeparator => r"\\",
        TokenKind::Error | TokenKind::Eof => "",
    }
}

fn join_spans(start: SourceSpan, end: SourceSpan) -> SourceSpan {
    SourceSpan::new(start.start(), end.end())
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::literal_string_with_formatting_args,
        clippy::panic,
        clippy::unwrap_used,
        reason = "parser tests inspect private AST invariants directly"
    )]

    use super::*;

    fn parse_ok(source: &str) -> MathBody<'_> {
        match parse_math_body(source) {
            Ok(body) => body,
            Err(diagnostics) => panic!("expected parse success, got {diagnostics:?}"),
        }
    }

    fn parse_err(source: &str) -> Vec<ParseDiagnostic> {
        match parse_math_body(source) {
            Ok(body) => panic!("expected parse failure, got {body:?}"),
            Err(diagnostics) => diagnostics,
        }
    }

    #[test]
    fn nested_groups_parse_as_group_nodes() {
        let body = parse_ok(r"{a {b}}");

        assert_eq!(body.elements.len(), 1);
        let NodeKind::Group(outer) = &body.elements[0].kind else {
            panic!("expected outer group");
        };
        assert_eq!(outer.body.elements.len(), 2);
        assert!(matches!(outer.body.elements[1].kind, NodeKind::Group(_)));
    }

    #[test]
    fn scripts_attach_to_atoms_and_groups() {
        let body = parse_ok(r"x_i^{2} {x}_n");

        assert_eq!(body.elements.len(), 2);
        let NodeKind::Script(first) = &body.elements[0].kind else {
            panic!("expected first script");
        };
        assert!(matches!(*first.base, ScriptBase::Atom(Atom::Identifier("x"))));
        assert!(first.subscript.is_some());
        assert!(first.superscript.is_some());

        let NodeKind::Script(second) = &body.elements[1].kind else {
            panic!("expected second script");
        };
        assert!(matches!(*second.base, ScriptBase::Group(_)));
    }

    #[test]
    fn script_without_base_is_a_syntax_error() {
        let diagnostics = parse_err("_i");

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == ParseDiagnosticKind::ScriptWithoutBase && diagnostic.span.as_range() == (0..1)
        }));
    }

    #[test]
    fn fractions_and_roots_require_typed_arguments() {
        let body = parse_ok(r"\frac{a}{b} \sqrt[3]{x}");

        assert_eq!(body.elements.len(), 2);
        assert!(matches!(body.elements[0].kind, NodeKind::Fraction(_)));
        let NodeKind::Sqrt(sqrt) = &body.elements[1].kind else {
            panic!("expected sqrt");
        };
        assert!(sqrt.degree.is_some());
    }

    #[test]
    fn accents_own_exactly_one_group_body() {
        let body = parse_ok(r"\hat{x} \bar{y}");

        assert_eq!(body.elements.len(), 2);
        assert!(matches!(
            body.elements[0].kind,
            NodeKind::Accent(Accent {
                accent: AccentKind::Hat,
                ..
            })
        ));
        assert!(matches!(
            body.elements[1].kind,
            NodeKind::Accent(Accent {
                accent: AccentKind::Bar,
                ..
            })
        ));
    }

    #[test]
    fn left_right_delimiters_own_body_and_closer() {
        let body = parse_ok(r"\left( x \right)");

        assert_eq!(body.elements.len(), 1);
        let NodeKind::Delimited(delimited) = &body.elements[0].kind else {
            panic!("expected delimited node");
        };
        assert_eq!(delimited.opener, Delimiter::Source("("));
        assert_eq!(delimited.closer, Delimiter::Source(")"));
        assert_eq!(delimited.body.elements.len(), 1);
    }

    #[test]
    fn matrix_environment_parses_rows_and_cells() {
        let body = parse_ok(r"\begin{matrix}a & b \\ c & d\end{matrix}");

        assert_eq!(body.elements.len(), 1);
        let NodeKind::Environment(environment) = &body.elements[0].kind else {
            panic!("expected environment");
        };
        assert_eq!(environment.name, "matrix");
        assert_eq!(environment.rows.len(), 2);
        assert_eq!(environment.rows[0].cells.len(), 2);
        assert_eq!(environment.rows[1].cells.len(), 2);
    }

    #[test]
    fn cases_and_array_environments_are_supported() {
        let cases = parse_ok(r"\begin{cases}x & y\end{cases}");
        let array = parse_ok(r"\begin{array}{cc}x & y\end{array}");

        assert!(matches!(cases.elements[0].kind, NodeKind::Environment(_)));
        let NodeKind::Environment(environment) = &array.elements[0].kind else {
            panic!("expected array environment");
        };
        assert_eq!(environment.name, "array");
        assert_eq!(environment.preamble, Some("cc"));
    }

    #[test]
    fn unknown_commands_are_unsupported_not_syntax() {
        let diagnostics = parse_err(r"\unknown{x}");

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == ParseDiagnosticKind::UnsupportedCommand && diagnostic.span.as_range() == (0..8)
        }));
    }

    #[test]
    fn malformed_groups_report_byte_spans() {
        let diagnostics = parse_err(r"\frac{a}{b");

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == ParseDiagnosticKind::UnbalancedGroup && diagnostic.span.as_range() == (8..9)
        }));
    }

    #[test]
    fn duplicate_scripts_are_rejected() {
        let diagnostics = parse_err("x_i_j");

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == ParseDiagnosticKind::DuplicateSubscript)
        );
    }

    #[test]
    fn unmatched_environment_end_is_rejected() {
        let diagnostics = parse_err(r"\end{matrix}");

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == ParseDiagnosticKind::UnmatchedEnvironmentEnd)
        );
    }
}
