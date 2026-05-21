//! Recursive-descent parser for Unicode mathematical source.
//!
//! Design comparison:
//!
//! - Keeping the token stream plus a direct emitter would be smaller, but it
//!   would leave ownership rules in formatting branches. That is the current
//!   failure mode for bars, primes, scripts, and labelled arrows.
//! - A generic node enum with optional fields would make the parser easy to
//!   extend, but it would permit impossible states such as scripts without
//!   explicit bases, accents without targets, and arrows without direction.
//! - This module uses typed nodes with required children. The implementation is
//!   larger, but the later emitter will consume named source structures rather
//!   than visual adjacency.

#![allow(
    clippy::wildcard_enum_match_arm,
    reason = "parser recovery groups token and node variants that share the same fallback"
)]

use crate::SourceSpan;
use crate::registry::MathAlphabetStyle;
use crate::unicode_lexer::{
    ArrowDirection as TokenArrowDirection, CombiningAccent, ScriptPosition, UnicodeLexDiagnostic,
    UnicodeLexDiagnosticKind, UnicodeToken, UnicodeTokenCursor, UnicodeTokenKind, UnicodeTokenStream,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnicodeMathBody<'src> {
    pub(crate) elements: Vec<UnicodeNode<'src>>,
    pub(crate) span: SourceSpan,
}

impl<'src> UnicodeMathBody<'src> {
    fn new(elements: Vec<UnicodeNode<'src>>, span: SourceSpan) -> Self {
        Self { elements, span }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnicodeNode<'src> {
    pub(crate) kind: UnicodeNodeKind<'src>,
    pub(crate) span: SourceSpan,
}

impl<'src> UnicodeNode<'src> {
    fn new(kind: UnicodeNodeKind<'src>, span: SourceSpan) -> Self {
        Self { kind, span }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnicodeNodeKind<'src> {
    Plain(&'src str),
    Number(&'src str),
    Punctuation(&'src str),
    DirectSymbol(&'src str),
    ExistingLatex(&'src str),
    StyledRun(StyledRun),
    Script(Script<'src>),
    Accent(Accent<'src>),
    Group(Group<'src>),
    Root(Root<'src>),
    LinearArrow(LinearArrow<'src>),
    Unknown(&'src str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StyledRun {
    pub(crate) style: MathAlphabetStyle,
    pub(crate) base: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Script<'src> {
    pub(crate) base: ScriptBase<'src>,
    pub(crate) subscript: Option<ScriptArgument<'src>>,
    pub(crate) superscript: Option<ScriptArgument<'src>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScriptBase<'src> {
    Node(Box<UnicodeNode<'src>>),
    Empty(SourceSpan),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScriptArgument<'src> {
    Node(Box<UnicodeNode<'src>>),
    Group(Group<'src>),
    ScriptRun { source: String, span: SourceSpan },
}

impl ScriptArgument<'_> {
    const fn span(&self) -> SourceSpan {
        match self {
            Self::Node(node) => node.span,
            Self::Group(group) => group.span,
            Self::ScriptRun { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Accent<'src> {
    pub(crate) accent: CombiningAccent,
    pub(crate) target: AccentTarget<'src>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AccentTarget<'src> {
    Node(Box<UnicodeNode<'src>>),
    Group(Group<'src>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Group<'src> {
    pub(crate) delimiter: GroupDelimiter,
    pub(crate) body: UnicodeMathBody<'src>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupDelimiter {
    Brace,
    Bracket,
    Parenthesis,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Root<'src> {
    pub(crate) degree: Option<ScriptArgument<'src>>,
    pub(crate) body: Box<UnicodeNode<'src>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinearArrow<'src> {
    pub(crate) direction: ArrowDirection,
    pub(crate) label: Option<UnicodeMathBody<'src>>,
    pub(crate) raw_label: Option<&'src str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArrowDirection {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnicodeParseDiagnostic {
    kind: UnicodeParseDiagnosticKind,
    span: SourceSpan,
    message: String,
}

impl UnicodeParseDiagnostic {
    fn new(kind: UnicodeParseDiagnosticKind, span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
        }
    }

    pub(crate) const fn kind(&self) -> &UnicodeParseDiagnosticKind {
        &self.kind
    }

    pub(crate) const fn span(&self) -> SourceSpan {
        self.span
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnicodeParseDiagnosticKind {
    Lexical,
    UnexpectedToken,
    DetachedCombiningMark,
    UnsupportedAccentTarget,
    ScriptWithoutRepresentableBase,
    DuplicateScript,
    MalformedGroupedScript,
    UnclosedGroup,
    UnprovenLinearArrow,
    UnknownUnicodeSourceShape,
}

pub(crate) fn parse_unicode_math_body(source: &str) -> Result<UnicodeMathBody<'_>, Vec<UnicodeParseDiagnostic>> {
    let stream = UnicodeTokenStream::new(source);
    let mut diagnostics = stream
        .diagnostics()
        .iter()
        .map(convert_lex_diagnostic)
        .collect::<Vec<_>>();
    let mut parser = UnicodeParser::new(stream.source(), stream.tokens());
    let body = parser.parse_sequence(Stop::TopLevel);
    diagnostics.extend(parser.diagnostics);
    if diagnostics.is_empty() {
        Ok(body)
    } else {
        Err(diagnostics)
    }
}

fn convert_lex_diagnostic(diagnostic: &UnicodeLexDiagnostic) -> UnicodeParseDiagnostic {
    let kind = match diagnostic.kind() {
        UnicodeLexDiagnosticKind::DetachedCombiningMark => UnicodeParseDiagnosticKind::DetachedCombiningMark,
        UnicodeLexDiagnosticKind::UnknownSourceShape => UnicodeParseDiagnosticKind::UnknownUnicodeSourceShape,
        UnicodeLexDiagnosticKind::MalformedLatexPassthrough | UnicodeLexDiagnosticKind::ControlCharacter => {
            UnicodeParseDiagnosticKind::Lexical
        }
    };
    UnicodeParseDiagnostic::new(kind, diagnostic.span(), diagnostic.message())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    TopLevel,
    Group(GroupDelimiter),
    ArrowLabel,
}

struct UnicodeParser<'stream, 'src> {
    source: &'src str,
    cursor: UnicodeTokenCursor<'stream, 'src>,
    diagnostics: Vec<UnicodeParseDiagnostic>,
}

impl<'stream, 'src> UnicodeParser<'stream, 'src> {
    fn new(source: &'src str, tokens: &'stream [UnicodeToken<'src>]) -> Self {
        Self {
            source,
            cursor: UnicodeTokenCursor::new(tokens),
            diagnostics: Vec::new(),
        }
    }

    fn parse_sequence(&mut self, stop: Stop) -> UnicodeMathBody<'src> {
        let start = self.cursor.peek().span();
        let mut elements = Vec::new();
        loop {
            self.skip_whitespace();
            if self.cursor.is_eof() || self.at_stop(stop) {
                break;
            }
            if let Some(node) = self.parse_element() {
                elements.push(node);
            }
        }
        let end = elements.last().map_or(start, |node| node.span);
        UnicodeMathBody::new(elements, join_spans(start, end))
    }

    fn parse_element(&mut self) -> Option<UnicodeNode<'src>> {
        let primary = self.parse_primary()?;
        self.parse_postfix(primary)
    }

    fn parse_primary(&mut self) -> Option<UnicodeNode<'src>> {
        self.skip_whitespace();
        let token = self.cursor.peek();
        match token.kind() {
            UnicodeTokenKind::PlainWord(text) => {
                self.cursor.advance();
                Some(UnicodeNode::new(UnicodeNodeKind::Plain(text), token.span()))
            }
            UnicodeTokenKind::Number(text) => {
                self.cursor.advance();
                Some(UnicodeNode::new(UnicodeNodeKind::Number(text), token.span()))
            }
            UnicodeTokenKind::Punctuation(text) => {
                self.cursor.advance();
                Some(UnicodeNode::new(UnicodeNodeKind::Punctuation(text), token.span()))
            }
            UnicodeTokenKind::DirectSymbol(text) => {
                self.cursor.advance();
                Some(UnicodeNode::new(UnicodeNodeKind::DirectSymbol(text), token.span()))
            }
            UnicodeTokenKind::SquareRoot => self.parse_root(None),
            UnicodeTokenKind::ExistingLatex(text) => {
                self.cursor.advance();
                Some(UnicodeNode::new(UnicodeNodeKind::ExistingLatex(text), token.span()))
            }
            UnicodeTokenKind::StyledRun { style, base } => {
                self.cursor.advance();
                Some(UnicodeNode::new(
                    UnicodeNodeKind::StyledRun(StyledRun {
                        style: *style,
                        base: base.clone(),
                    }),
                    token.span(),
                ))
            }
            UnicodeTokenKind::CombiningAccentCluster { base, accents } => {
                self.cursor.advance();
                Some(self.accent_cluster_node(base, accents, token.span()))
            }
            UnicodeTokenKind::UnicodeScriptRun { position, source } => {
                if *position == ScriptPosition::Superscript && self.next_non_whitespace_is_square_root() {
                    let degree = ScriptArgument::ScriptRun {
                        source: source.clone(),
                        span: token.span(),
                    };
                    self.cursor.advance();
                    return self.parse_root(Some(degree));
                }
                self.cursor.advance();
                Some(Self::leading_script_node(*position, source.clone(), token.span()))
            }
            UnicodeTokenKind::SuperscriptMarker | UnicodeTokenKind::SubscriptMarker => {
                let position = marker_position(token.kind())?;
                self.parse_leading_ascii_script(position, token.span())
            }
            UnicodeTokenKind::LeftBrace => self.parse_group_node(GroupDelimiter::Brace),
            UnicodeTokenKind::LeftBracket => self.parse_group_node(GroupDelimiter::Bracket),
            UnicodeTokenKind::LeftParen => self.parse_group_node(GroupDelimiter::Parenthesis),
            UnicodeTokenKind::CombiningAccentMark(_accent) => {
                self.diagnostics.push(UnicodeParseDiagnostic::new(
                    UnicodeParseDiagnosticKind::DetachedCombiningMark,
                    token.span(),
                    "combining accent has no parsed target",
                ));
                self.cursor.advance();
                Some(UnicodeNode::new(
                    UnicodeNodeKind::Unknown(&self.source[token.span().as_range()]),
                    token.span(),
                ))
            }
            UnicodeTokenKind::ArrowShaft(_) => self.parse_right_arrow(),
            UnicodeTokenKind::ArrowHead(TokenArrowDirection::Left) => self.parse_left_arrow(),
            UnicodeTokenKind::ArrowHead(TokenArrowDirection::Right) => {
                self.unproven_arrow(token.span(), "right arrow head has no shaft");
                self.cursor.advance();
                Some(UnicodeNode::new(
                    UnicodeNodeKind::Unknown(&self.source[token.span().as_range()]),
                    token.span(),
                ))
            }
            UnicodeTokenKind::UnknownScalar(text) => {
                self.cursor.advance();
                Some(UnicodeNode::new(UnicodeNodeKind::Unknown(text), token.span()))
            }
            UnicodeTokenKind::RightBrace
            | UnicodeTokenKind::RightBracket
            | UnicodeTokenKind::RightParen
            | UnicodeTokenKind::Whitespace(_)
            | UnicodeTokenKind::Error
            | UnicodeTokenKind::Eof => {
                self.diagnostics.push(UnicodeParseDiagnostic::new(
                    UnicodeParseDiagnosticKind::UnexpectedToken,
                    token.span(),
                    "unexpected token in Unicode math source",
                ));
                self.cursor.advance();
                None
            }
        }
    }

    fn parse_postfix(&mut self, mut base: UnicodeNode<'src>) -> Option<UnicodeNode<'src>> {
        loop {
            self.skip_whitespace();
            let token = self.cursor.peek();
            match token.kind() {
                UnicodeTokenKind::UnicodeScriptRun { position, source } => {
                    self.cursor.advance();
                    base = self.attach_script_run(base, *position, source.clone(), token.span());
                }
                UnicodeTokenKind::SubscriptMarker | UnicodeTokenKind::SuperscriptMarker => {
                    let position = marker_position(token.kind())?;
                    self.cursor.advance();
                    let Some(argument) = self.parse_script_argument() else {
                        self.diagnostics.push(UnicodeParseDiagnostic::new(
                            UnicodeParseDiagnosticKind::MalformedGroupedScript,
                            token.span(),
                            "script marker has no argument",
                        ));
                        continue;
                    };
                    base = self.attach_script_argument(base, position, argument, token.span());
                }
                UnicodeTokenKind::CombiningAccentMark(accent) => {
                    self.cursor.advance();
                    base = Self::attach_accent(base, *accent, token.span());
                }
                _ => break,
            }
        }
        Some(base)
    }

    fn parse_leading_ascii_script(
        &mut self,
        position: ScriptPosition,
        marker_span: SourceSpan,
    ) -> Option<UnicodeNode<'src>> {
        self.cursor.advance();
        let Some(argument) = self.parse_script_argument() else {
            self.diagnostics.push(UnicodeParseDiagnostic::new(
                UnicodeParseDiagnosticKind::ScriptWithoutRepresentableBase,
                marker_span,
                "leading script marker has no representable argument",
            ));
            return Some(UnicodeNode::new(
                UnicodeNodeKind::Unknown(&self.source[marker_span.as_range()]),
                marker_span,
            ));
        };
        let span = join_spans(marker_span, argument.span());
        Some(UnicodeNode::new(
            UnicodeNodeKind::Script(Script {
                base: ScriptBase::Empty(marker_span),
                subscript: (position == ScriptPosition::Subscript).then_some(argument.clone()),
                superscript: (position == ScriptPosition::Superscript).then_some(argument),
            }),
            span,
        ))
    }

    fn parse_script_argument(&mut self) -> Option<ScriptArgument<'src>> {
        self.skip_whitespace();
        let token = self.cursor.peek();
        match token.kind() {
            UnicodeTokenKind::LeftBrace => self.parse_group_node(GroupDelimiter::Brace).and_then(group_argument),
            UnicodeTokenKind::LeftBracket => self.parse_group_node(GroupDelimiter::Bracket).and_then(group_argument),
            UnicodeTokenKind::LeftParen => self
                .parse_group_node(GroupDelimiter::Parenthesis)
                .and_then(group_argument),
            UnicodeTokenKind::UnicodeScriptRun { source, .. } => {
                self.cursor.advance();
                Some(ScriptArgument::ScriptRun {
                    source: source.clone(),
                    span: token.span(),
                })
            }
            UnicodeTokenKind::RightBrace
            | UnicodeTokenKind::RightBracket
            | UnicodeTokenKind::RightParen
            | UnicodeTokenKind::ArrowShaft(_)
            | UnicodeTokenKind::ArrowHead(_)
            | UnicodeTokenKind::CombiningAccentMark(_)
            | UnicodeTokenKind::Whitespace(_)
            | UnicodeTokenKind::Error
            | UnicodeTokenKind::Eof => None,
            _ => self.parse_primary().map(|node| ScriptArgument::Node(Box::new(node))),
        }
    }

    fn parse_group_node(&mut self, delimiter: GroupDelimiter) -> Option<UnicodeNode<'src>> {
        let open = self.cursor.advance();
        let body = self.parse_sequence(Stop::Group(delimiter));
        let close = self.cursor.peek();
        if !matches_group_close(close.kind(), delimiter) {
            self.diagnostics.push(UnicodeParseDiagnostic::new(
                UnicodeParseDiagnosticKind::UnclosedGroup,
                open.span(),
                "unclosed Unicode math group",
            ));
            return Some(UnicodeNode::new(
                UnicodeNodeKind::Group(Group {
                    delimiter,
                    body,
                    span: join_spans(open.span(), close.span()),
                }),
                join_spans(open.span(), close.span()),
            ));
        }
        self.cursor.advance();
        let span = join_spans(open.span(), close.span());
        Some(UnicodeNode::new(
            UnicodeNodeKind::Group(Group { delimiter, body, span }),
            span,
        ))
    }

    fn parse_root(&mut self, degree: Option<ScriptArgument<'src>>) -> Option<UnicodeNode<'src>> {
        self.skip_whitespace();
        let root = self.cursor.peek();
        if !matches!(root.kind(), UnicodeTokenKind::SquareRoot) {
            self.diagnostics.push(UnicodeParseDiagnostic::new(
                UnicodeParseDiagnosticKind::UnexpectedToken,
                root.span(),
                "expected square-root symbol",
            ));
            return None;
        }
        self.cursor.advance();
        let Some(body) = self.parse_element() else {
            self.diagnostics.push(UnicodeParseDiagnostic::new(
                UnicodeParseDiagnosticKind::UnexpectedToken,
                root.span(),
                "square-root symbol has no body",
            ));
            return Some(UnicodeNode::new(
                UnicodeNodeKind::Unknown(&self.source[root.span().as_range()]),
                root.span(),
            ));
        };
        let span = if let Some(degree) = &degree {
            join_spans(degree.span(), body.span)
        } else {
            join_spans(root.span(), body.span)
        };
        Some(UnicodeNode::new(
            UnicodeNodeKind::Root(Root {
                degree,
                body: Box::new(body),
            }),
            span,
        ))
    }

    fn accent_cluster_node(&mut self, base: &str, accents: &[CombiningAccent], span: SourceSpan) -> UnicodeNode<'src> {
        let mut node = UnicodeNode::new(base_node_kind(base, &self.source[span.as_range()]), span);
        for accent in accents {
            if base == "'" {
                self.diagnostics.push(UnicodeParseDiagnostic::new(
                    UnicodeParseDiagnosticKind::UnsupportedAccentTarget,
                    span,
                    "combining accent on a prime is preserved as source",
                ));
                return UnicodeNode::new(UnicodeNodeKind::Unknown(&self.source[span.as_range()]), span);
            }
            node = Self::attach_accent(node, *accent, span);
        }
        node
    }

    fn attach_accent(base: UnicodeNode<'src>, accent: CombiningAccent, accent_span: SourceSpan) -> UnicodeNode<'src> {
        match base.kind {
            UnicodeNodeKind::Group(group) => {
                let span = join_spans(group.span, accent_span);
                UnicodeNode::new(
                    UnicodeNodeKind::Accent(Accent {
                        accent,
                        target: AccentTarget::Group(group),
                    }),
                    span,
                )
            }
            UnicodeNodeKind::Unknown(text) => UnicodeNode::new(UnicodeNodeKind::Unknown(text), base.span),
            _ => {
                let base_span = base.span;
                let span = join_spans(base_span, accent_span);
                UnicodeNode::new(
                    UnicodeNodeKind::Accent(Accent {
                        accent,
                        target: AccentTarget::Node(Box::new(base)),
                    }),
                    span,
                )
            }
        }
    }

    fn leading_script_node(position: ScriptPosition, source: String, span: SourceSpan) -> UnicodeNode<'src> {
        UnicodeNode::new(
            UnicodeNodeKind::Script(Script {
                base: ScriptBase::Empty(SourceSpan::new(span.start(), span.start())),
                subscript: (position == ScriptPosition::Subscript).then_some(ScriptArgument::ScriptRun {
                    source: source.clone(),
                    span,
                }),
                superscript: (position == ScriptPosition::Superscript)
                    .then_some(ScriptArgument::ScriptRun { source, span }),
            }),
            span,
        )
    }

    fn attach_script_run(
        &mut self,
        base: UnicodeNode<'src>,
        position: ScriptPosition,
        source: String,
        span: SourceSpan,
    ) -> UnicodeNode<'src> {
        self.attach_script_argument(base, position, ScriptArgument::ScriptRun { source, span }, span)
    }

    fn attach_script_argument(
        &mut self,
        base: UnicodeNode<'src>,
        position: ScriptPosition,
        argument: ScriptArgument<'src>,
        marker_span: SourceSpan,
    ) -> UnicodeNode<'src> {
        let end = argument.span();
        match base.kind {
            UnicodeNodeKind::Script(mut script) => {
                let duplicate = (position == ScriptPosition::Subscript && script.subscript.is_some())
                    || (position == ScriptPosition::Superscript && script.superscript.is_some());
                if duplicate {
                    self.diagnostics.push(UnicodeParseDiagnostic::new(
                        UnicodeParseDiagnosticKind::DuplicateScript,
                        marker_span,
                        "duplicate script on the same Unicode math base",
                    ));
                } else if position == ScriptPosition::Subscript {
                    script.subscript = Some(argument);
                } else {
                    script.superscript = Some(argument);
                }
                let span = join_spans(base.span, end);
                UnicodeNode::new(UnicodeNodeKind::Script(script), span)
            }
            _ => {
                let base_span = base.span;
                UnicodeNode::new(
                    UnicodeNodeKind::Script(Script {
                        base: ScriptBase::Node(Box::new(base)),
                        subscript: (position == ScriptPosition::Subscript).then_some(argument.clone()),
                        superscript: (position == ScriptPosition::Superscript).then_some(argument),
                    }),
                    join_spans(base_span, end),
                )
            }
        }
    }

    fn parse_right_arrow(&mut self) -> Option<UnicodeNode<'src>> {
        let start = self.cursor.advance();
        let label_start = self.cursor.peek().span();
        let label = self.parse_sequence(Stop::ArrowLabel);
        let label_end = self.cursor.peek().span();
        while matches!(self.cursor.peek().kind(), UnicodeTokenKind::ArrowShaft(_)) {
            self.cursor.advance();
        }
        let head = self.cursor.peek();
        if !matches!(head.kind(), UnicodeTokenKind::ArrowHead(TokenArrowDirection::Right)) {
            self.unproven_arrow(start.span(), "arrow shaft is not closed by a right arrow head");
            return Some(UnicodeNode::new(
                UnicodeNodeKind::Unknown(&self.source[start.span().as_range()]),
                start.span(),
            ));
        }
        self.cursor.advance();
        let span = join_spans(start.span(), head.span());
        let raw_label = label_raw_source(self.source, label_start, label_end);
        Some(UnicodeNode::new(
            UnicodeNodeKind::LinearArrow(LinearArrow {
                direction: ArrowDirection::Right,
                label: (!label.elements.is_empty()).then_some(label),
                raw_label,
            }),
            span,
        ))
    }

    fn parse_left_arrow(&mut self) -> Option<UnicodeNode<'src>> {
        let head = self.cursor.advance();
        let label_start = self.cursor.peek().span();
        let label = self.parse_sequence(Stop::ArrowLabel);
        let shaft = self.cursor.peek();
        if !matches!(shaft.kind(), UnicodeTokenKind::ArrowShaft(_)) {
            self.unproven_arrow(head.span(), "left arrow head is not closed by a shaft");
            return Some(UnicodeNode::new(
                UnicodeNodeKind::Unknown(&self.source[head.span().as_range()]),
                head.span(),
            ));
        }
        self.cursor.advance();
        let span = join_spans(head.span(), shaft.span());
        let raw_label = label_raw_source(self.source, label_start, shaft.span());
        Some(UnicodeNode::new(
            UnicodeNodeKind::LinearArrow(LinearArrow {
                direction: ArrowDirection::Left,
                label: (!label.elements.is_empty()).then_some(label),
                raw_label,
            }),
            span,
        ))
    }

    fn unproven_arrow(&mut self, span: SourceSpan, message: &'static str) {
        self.diagnostics.push(UnicodeParseDiagnostic::new(
            UnicodeParseDiagnosticKind::UnprovenLinearArrow,
            span,
            message,
        ));
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.cursor.peek().kind(), UnicodeTokenKind::Whitespace(_)) {
            self.cursor.advance();
        }
    }

    fn next_non_whitespace_is_square_root(&self) -> bool {
        let mut cursor = self.cursor;
        cursor.advance();
        while matches!(cursor.peek().kind(), UnicodeTokenKind::Whitespace(_)) {
            cursor.advance();
        }
        matches!(cursor.peek().kind(), UnicodeTokenKind::SquareRoot)
    }

    fn at_stop(&self, stop: Stop) -> bool {
        match stop {
            Stop::TopLevel => false,
            Stop::Group(delimiter) => matches_group_close(self.cursor.peek().kind(), delimiter),
            Stop::ArrowLabel => matches!(
                self.cursor.peek().kind(),
                UnicodeTokenKind::ArrowShaft(_) | UnicodeTokenKind::ArrowHead(_)
            ),
        }
    }
}

fn group_argument(node: UnicodeNode<'_>) -> Option<ScriptArgument<'_>> {
    match node.kind {
        UnicodeNodeKind::Group(group) => Some(ScriptArgument::Group(group)),
        _ => None,
    }
}

fn marker_position(kind: &UnicodeTokenKind<'_>) -> Option<ScriptPosition> {
    match kind {
        UnicodeTokenKind::SubscriptMarker => Some(ScriptPosition::Subscript),
        UnicodeTokenKind::SuperscriptMarker => Some(ScriptPosition::Superscript),
        _ => None,
    }
}

fn matches_group_close(kind: &UnicodeTokenKind<'_>, delimiter: GroupDelimiter) -> bool {
    matches!(
        (kind, delimiter),
        (UnicodeTokenKind::RightBrace, GroupDelimiter::Brace)
            | (UnicodeTokenKind::RightBracket, GroupDelimiter::Bracket)
            | (UnicodeTokenKind::RightParen, GroupDelimiter::Parenthesis)
    )
}

fn base_node_kind<'src>(base: &str, original: &'src str) -> UnicodeNodeKind<'src> {
    if base.chars().all(|ch| ch.is_ascii_alphabetic()) {
        UnicodeNodeKind::Plain(original)
    } else if base.chars().all(|ch| ch.is_ascii_digit()) {
        UnicodeNodeKind::Number(original)
    } else {
        UnicodeNodeKind::Punctuation(original)
    }
}

fn label_raw_source(source: &str, start: SourceSpan, end: SourceSpan) -> Option<&str> {
    if start.start() >= end.start() {
        return None;
    }
    let raw = source.get(start.start()..end.start())?.trim();
    (!raw.is_empty()).then_some(raw)
}

fn join_spans(first: SourceSpan, second: SourceSpan) -> SourceSpan {
    SourceSpan::new(first.start().min(second.start()), first.end().max(second.end()))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unicode_not_nfc,
        reason = "unicode parser tests assert exact private AST shapes"
    )]

    use super::*;

    fn parse_ok(source: &str) -> UnicodeMathBody<'_> {
        parse_unicode_math_body(source).unwrap_or_else(|diagnostics| {
            panic!("expected parse success for {source:?}, got {diagnostics:?}");
        })
    }

    fn parse_err(source: &str) -> Vec<UnicodeParseDiagnostic> {
        parse_unicode_math_body(source).expect_err("expected Unicode parse diagnostics")
    }

    fn first(source: &str) -> UnicodeNode<'_> {
        parse_ok(source)
            .elements
            .first()
            .cloned()
            .unwrap_or_else(|| panic!("no first node for {source:?}"))
    }

    #[test]
    fn ascii_grouped_scripts_have_typed_arguments() {
        let node = first("M_[φ]");
        let UnicodeNodeKind::Script(script) = node.kind else {
            panic!("expected script node");
        };
        assert!(matches!(*script.base_node(), UnicodeNodeKind::Plain("M")));
        assert!(matches!(
            script.subscript,
            Some(ScriptArgument::Group(Group {
                delimiter: GroupDelimiter::Bracket,
                ..
            }))
        ));

        let node = first("x^(n)");
        let UnicodeNodeKind::Script(script) = node.kind else {
            panic!("expected script node");
        };
        assert!(script.subscript.is_none());
        assert!(matches!(
            script.superscript,
            Some(ScriptArgument::Group(Group {
                delimiter: GroupDelimiter::Parenthesis,
                ..
            }))
        ));
    }

    #[test]
    fn unicode_and_leading_scripts_have_explicit_bases() {
        let node = first("xᵐ");
        let UnicodeNodeKind::Script(script) = node.kind else {
            panic!("expected script node");
        };
        assert!(matches!(*script.base_node(), UnicodeNodeKind::Plain("x")));
        assert!(matches!(
            script.superscript,
            Some(ScriptArgument::ScriptRun { ref source, .. }) if source == "m"
        ));

        let node = first("ᵃ\\phi");
        let UnicodeNodeKind::Script(script) = node.kind else {
            panic!("expected leading script node");
        };
        assert!(matches!(script.base, ScriptBase::Empty(_)));
        assert!(matches!(
            script.superscript,
            Some(ScriptArgument::ScriptRun { ref source, .. }) if source == "a"
        ));
    }

    #[test]
    fn subscript_and_superscript_can_attach_to_same_base() {
        let node = first("iˢ_A");
        let UnicodeNodeKind::Script(script) = node.kind else {
            panic!("expected script node");
        };
        assert!(script.subscript.is_some());
        assert!(script.superscript.is_some());

        let node = first("D₊");
        let UnicodeNodeKind::Script(script) = node.kind else {
            panic!("expected script node");
        };
        assert!(script.subscript.is_some());
        assert!(script.superscript.is_none());
    }

    #[test]
    fn duplicate_scripts_are_diagnostic() {
        let diagnostics = parse_err("xᵐ^n");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind() == &UnicodeParseDiagnosticKind::DuplicateScript)
        );
    }

    #[test]
    fn accent_ownership_distinguishes_bar_and_prime_cases() {
        let body = parse_ok("Ȳ'");
        assert_eq!(body.elements.len(), 2);
        assert!(matches!(body.elements[0].kind, UnicodeNodeKind::Accent(_)));
        assert!(matches!(body.elements[1].kind, UnicodeNodeKind::Punctuation("'")));

        let body = parse_ok("{Y'}̄");
        assert_eq!(body.elements.len(), 1);
        let UnicodeNodeKind::Accent(accent) = &body.elements[0].kind else {
            panic!("expected grouped accent");
        };
        assert!(matches!(accent.target, AccentTarget::Group(_)));

        let diagnostics = parse_err("Y'̄");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind() == &UnicodeParseDiagnosticKind::UnsupportedAccentTarget)
        );
    }

    #[test]
    fn styled_runs_existing_latex_and_symbols_are_nodes() {
        let body = parse_ok("𝓗𝓸𝓶 𝓟𝓻𝓸𝓳 𝚪_* 𝐟𝐠 \\leqslant̸ ⩾ ⋯ ⨁ □");
        assert!(
            body.elements
                .iter()
                .any(|node| matches!(node.kind, UnicodeNodeKind::StyledRun(_)))
        );
        assert!(
            body.elements
                .iter()
                .any(|node| matches!(node.kind, UnicodeNodeKind::ExistingLatex("\\leqslant̸")))
        );
        assert!(
            body.elements
                .iter()
                .any(|node| matches!(node.kind, UnicodeNodeKind::DirectSymbol("⨁")))
        );
    }

    #[test]
    fn roots_parse_with_optional_degrees_and_scripted_bodies() {
        let body = parse_ok("√x ⁿ√x √x²");
        assert_eq!(
            body.elements
                .iter()
                .filter(|node| matches!(node.kind, UnicodeNodeKind::Root(_)))
                .count(),
            3
        );
    }

    #[test]
    fn linear_arrows_own_direction_and_label_when_proven() {
        let body = parse_ok("A ─u→ B A ←u─ B A ——→ B A ─^{u}→ B A ←^{u}─ B");
        let arrows = body
            .elements
            .iter()
            .filter_map(|node| match &node.kind {
                UnicodeNodeKind::LinearArrow(arrow) => Some(arrow),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(arrows.len(), 5);
        assert_eq!(arrows[0].direction, ArrowDirection::Right);
        assert_eq!(arrows[0].raw_label, Some("u"));
        assert_eq!(arrows[1].direction, ArrowDirection::Left);
        assert_eq!(arrows[2].raw_label, None);
    }

    #[test]
    fn complex_arrow_label_stays_owned_by_arrow() {
        let body = parse_ok("A ──φ^{S′}──→ B");
        let arrow = body
            .elements
            .iter()
            .find_map(|node| match &node.kind {
                UnicodeNodeKind::LinearArrow(arrow) => Some(arrow),
                _ => None,
            })
            .expect("expected arrow node");
        assert_eq!(arrow.direction, ArrowDirection::Right);
        assert_eq!(arrow.raw_label, Some("φ^{S′}"));
        assert!(arrow.label.as_ref().is_some_and(|label| !label.elements.is_empty()));
    }

    #[test]
    fn unproven_arrow_sequences_are_diagnostic_and_visible() {
        let diagnostics = parse_err("A ─u B");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind() == &UnicodeParseDiagnosticKind::UnprovenLinearArrow)
        );
    }

    #[test]
    fn unknown_unicode_preserves_source_text() {
        let diagnostics = parse_err("🙂");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind() == &UnicodeParseDiagnosticKind::UnknownUnicodeSourceShape)
        );
    }

    impl<'src> Script<'src> {
        fn base_node(&self) -> &UnicodeNodeKind<'src> {
            match &self.base {
                ScriptBase::Node(node) => &node.kind,
                ScriptBase::Empty(_) => panic!("expected node base"),
            }
        }
    }
}
