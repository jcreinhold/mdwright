//! Editable source translation for TeX math bodies.
//!
//! This module translates source text, not terminal layout. It uses the
//! private parser and command registry so command names, scripts, and
//! grouped arguments are recognised structurally instead of by substring
//! replacement.

use std::ops::Range;

use crate::error::{LatexError, LatexErrorKind, SourceSpan};
use crate::parser::{
    Accent, AccentKind, Atom, Delimited, Delimiter, Fraction, Group, MathBody, Node, NodeKind, ParseDiagnostic,
    ParseDiagnosticKind, Script, ScriptArgument, ScriptBase, Sqrt, parse_math_body,
};
use crate::registry::{
    LatexSourceFragment, latex_symbol, lookup_command, math_alphabet_latex_command, styled_word_latex,
    unicode_sub_latex, unicode_sub_str, unicode_super_latex, unicode_super_str, unicode_symbol_latex_source,
};
use crate::unicode_lexer::CombiningAccent;
use crate::unicode_parser::{
    Accent as UnicodeAccent, AccentTarget as UnicodeAccentTarget, ArrowDirection as UnicodeArrowDirection,
    Group as UnicodeGroup, GroupDelimiter as UnicodeGroupDelimiter, LinearArrow as UnicodeLinearArrow,
    Root as UnicodeRoot, Script as UnicodeScript, ScriptArgument as UnicodeScriptArgument,
    ScriptBase as UnicodeScriptBase, UnicodeMathBody, UnicodeNode, UnicodeNodeKind, UnicodeParseDiagnostic,
    UnicodeParseDiagnosticKind, parse_unicode_math_body_with_diagnostics,
};

/// Loss marker recorded when a source translation cannot be exact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationLoss {
    span: SourceSpan,
    reason: String,
}

impl TranslationLoss {
    fn new(span: SourceSpan, reason: impl Into<String>) -> Self {
        Self {
            span,
            reason: reason.into(),
        }
    }

    /// Source span where the loss occurred.
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Why translation was lossy.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Coarse translation outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranslationStatus {
    /// The output is byte-identical to the input and no diagnostics or
    /// losses were recorded.
    Unchanged,
    /// The output changed without known loss.
    Lossless,
    /// Translation changed source spelling, skipped unsupported input,
    /// or recorded diagnostics.
    Lossy,
}

/// Source translation output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translation {
    text: String,
    edit_count: usize,
    losses: Vec<TranslationLoss>,
    diagnostics: Vec<LatexError>,
}

impl Translation {
    fn with_diagnostics(source: &str, diagnostics: Vec<LatexError>) -> Self {
        Self {
            text: source.to_owned(),
            edit_count: 0,
            losses: Vec::new(),
            diagnostics,
        }
    }

    fn from_parts(source: &str, text: String, losses: Vec<TranslationLoss>, diagnostics: Vec<LatexError>) -> Self {
        Self {
            edit_count: usize::from(text != source),
            text,
            losses,
            diagnostics,
        }
    }

    /// Translated source text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Number of source ranges replaced by the translation.
    #[must_use]
    pub const fn edit_count(&self) -> usize {
        self.edit_count
    }

    /// Losses recorded during translation.
    #[must_use]
    pub fn losses(&self) -> &[TranslationLoss] {
        &self.losses
    }

    /// Typed diagnostics recorded while translating.
    #[must_use]
    pub fn diagnostics(&self) -> &[LatexError] {
        &self.diagnostics
    }

    /// Whether translation was exact.
    #[must_use]
    pub fn is_lossless(&self) -> bool {
        self.losses.is_empty() && self.diagnostics.is_empty()
    }

    /// Coarse translation outcome.
    #[must_use]
    pub fn status(&self) -> TranslationStatus {
        if !self.is_lossless() {
            TranslationStatus::Lossy
        } else if self.edit_count == 0 {
            TranslationStatus::Unchanged
        } else {
            TranslationStatus::Lossless
        }
    }
}

/// Translate one LaTeX math body to editable Unicode math source.
#[must_use]
pub fn translate_latex_to_unicode(source: &str) -> Translation {
    let body = match parse_math_body(source) {
        Ok(body) => body,
        Err(diagnostics) => return Translation::with_diagnostics(source, parse_errors(&diagnostics)),
    };
    let mut ctx = TranslateContext::new(source);
    let text = ctx.translate_body_preserving_gaps(&body, 0, source.len());
    Translation::from_parts(source, text, ctx.losses, ctx.diagnostics)
}

/// Translate one Unicode math body to preferred LaTeX source.
#[must_use]
pub fn translate_unicode_to_latex(source: &str) -> Translation {
    let parsed = parse_unicode_math_body_with_diagnostics(source);
    let mut ctx = UnicodeEmitContext::new(source);
    ctx.diagnostics
        .extend(parsed.diagnostics.iter().map(unicode_parse_error));
    let text = ctx.emit_body_preserving_gaps(&parsed.body, 0, source.len());
    Translation::from_parts(source, text, ctx.losses, ctx.diagnostics)
}

struct UnicodeEmitContext<'src> {
    source: &'src str,
    losses: Vec<TranslationLoss>,
    diagnostics: Vec<LatexError>,
}

impl<'src> UnicodeEmitContext<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            source,
            losses: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn emit_body_preserving_gaps(&mut self, body: &UnicodeMathBody<'_>, start: usize, end: usize) -> String {
        let mut out = String::new();
        let mut cursor = start;
        for node in &body.elements {
            if cursor < node.span.start() {
                out.push_str(slice_or_empty(self.source, cursor..node.span.start()));
            }
            out.push_str(&self.emit_node(node));
            cursor = node.span.end();
        }
        if cursor < end {
            out.push_str(slice_or_empty(self.source, cursor..end));
        }
        out
    }

    fn emit_node(&mut self, node: &UnicodeNode<'_>) -> String {
        match self.try_emit_node(node) {
            Ok(text) => text,
            Err(reason) => {
                self.losses.push(TranslationLoss::new(node.span, reason));
                slice_or_empty(self.source, node.span.as_range()).to_owned()
            }
        }
    }

    fn try_emit_node(&mut self, node: &UnicodeNode<'_>) -> Result<String, String> {
        match &node.kind {
            UnicodeNodeKind::Plain(text) | UnicodeNodeKind::Number(text) | UnicodeNodeKind::Punctuation(text) => {
                Ok(prime_source(text).unwrap_or(text).to_owned())
            }
            UnicodeNodeKind::CanonicalSource(text) => Ok(translate_unicode_to_latex(text).text().to_owned()),
            UnicodeNodeKind::DirectSymbol(text) => self.emit_direct_symbol(text, node.span),
            UnicodeNodeKind::ExistingLatex(text) => Ok(canonical_latex_passthrough(text).to_owned()),
            UnicodeNodeKind::StyledRun(run) => Ok(Self::emit_styled_run(run.style, &run.base)),
            UnicodeNodeKind::Script(script) => self.emit_script(script),
            UnicodeNodeKind::Accent(accent) => self.emit_accent(accent),
            UnicodeNodeKind::Group(group) => Ok(self.emit_group(group)),
            UnicodeNodeKind::Root(root) => self.emit_root(root),
            UnicodeNodeKind::LinearArrow(arrow) => Ok(self.emit_linear_arrow(arrow)),
            UnicodeNodeKind::Unknown(text) => Err(format!("unknown Unicode math source {text:?}")),
        }
    }

    fn emit_direct_symbol(&self, text: &str, span: SourceSpan) -> Result<String, String> {
        if let Some(source) = prime_source(text) {
            return Ok(source.to_owned());
        }
        let Some(fragment) = unicode_symbol_latex_source(text) else {
            return Err(format!("Unicode symbol {text:?} has no canonical LaTeX source"));
        };
        Ok(self.latex_fragment(fragment, span.end()))
    }

    fn emit_styled_run(style: crate::registry::MathAlphabetStyle, base: &str) -> String {
        if let Some(source) = styled_word_latex(style, base) {
            return source.to_owned();
        }
        if base.chars().any(|ch| !ch.is_ascii_alphanumeric()) {
            return translate_unicode_to_latex(base).text().to_owned();
        }
        if let Some(command) = math_alphabet_latex_command(style) {
            return format!(r"\{command}{{{base}}}");
        }
        base.to_owned()
    }

    fn emit_script(&mut self, script: &UnicodeScript<'_>) -> Result<String, String> {
        let mut out = self.emit_script_base(&script.base);
        if let Some(superscript) = &script.superscript {
            out.push_str("^{");
            out.push_str(&self.emit_script_argument(superscript)?);
            out.push('}');
        }
        if let Some(subscript) = &script.subscript {
            out.push_str("_{");
            out.push_str(&self.emit_script_argument(subscript)?);
            out.push('}');
        }
        Ok(out)
    }

    fn emit_script_base(&mut self, base: &UnicodeScriptBase<'_>) -> String {
        match base {
            UnicodeScriptBase::Node(node) => self.emit_node(node),
            UnicodeScriptBase::Empty(_) => "{}".to_owned(),
        }
    }

    fn emit_script_argument(&mut self, argument: &UnicodeScriptArgument<'_>) -> Result<String, String> {
        match argument {
            UnicodeScriptArgument::Node(node) => Ok(self.emit_node(node)),
            UnicodeScriptArgument::Group(group) => {
                Ok(self.emit_body_preserving_gaps(&group.body, group.body.span.start(), group.body.span.end()))
            }
            UnicodeScriptArgument::ScriptRun { source, .. } => Ok(source.clone()),
        }
    }

    fn emit_accent(&mut self, accent: &UnicodeAccent<'_>) -> Result<String, String> {
        if let Some(command) = directional_limit_command(accent) {
            return Ok(command.to_owned());
        }
        let command = accent_command(accent.accent);
        let body = match &accent.target {
            UnicodeAccentTarget::Node(node) => self.emit_node(node),
            UnicodeAccentTarget::Group(group) => {
                self.emit_body_preserving_gaps(&group.body, group.body.span.start(), group.body.span.end())
            }
        };
        Ok(format!(r"\{command}{{{body}}}"))
    }

    fn emit_group(&mut self, group: &UnicodeGroup<'_>) -> String {
        let mut out = String::new();
        let (open, close) = group_delimiters(group.delimiter);
        out.push(open);
        out.push_str(&self.emit_body_preserving_gaps(&group.body, group.body.span.start(), group.body.span.end()));
        out.push(close);
        out
    }

    fn emit_root(&mut self, root: &UnicodeRoot<'_>) -> Result<String, String> {
        let mut out = String::from(r"\sqrt");
        if let Some(degree) = &root.degree {
            out.push('[');
            out.push_str(&self.emit_script_argument(degree)?);
            out.push(']');
        }
        out.push('{');
        out.push_str(&self.emit_node(&root.body));
        out.push('}');
        Ok(out)
    }

    fn emit_linear_arrow(&mut self, arrow: &UnicodeLinearArrow<'_>) -> String {
        let unlabelled = match arrow.direction {
            UnicodeArrowDirection::Left => r"\leftarrow",
            UnicodeArrowDirection::Right => r"\to",
        };
        let labelled = match arrow.direction {
            UnicodeArrowDirection::Left => r"\xleftarrow",
            UnicodeArrowDirection::Right => r"\xrightarrow",
        };
        let Some(label) = &arrow.label else {
            return unlabelled.to_owned();
        };
        let label_text = self.emit_arrow_label(label);
        if label_text.is_empty() {
            unlabelled.to_owned()
        } else {
            format!("{labelled}{{{label_text}}}")
        }
    }

    fn emit_arrow_label(&mut self, label: &UnicodeMathBody<'_>) -> String {
        if let [node] = label.elements.as_slice()
            && let UnicodeNodeKind::Script(script) = &node.kind
            && matches!(script.base, UnicodeScriptBase::Empty(_))
            && script.subscript.is_none()
            && let Some(superscript) = &script.superscript
        {
            return self
                .emit_script_argument(superscript)
                .unwrap_or_else(|_| self.emit_body_preserving_gaps(label, label.span.start(), label.span.end()));
        }
        self.emit_body_preserving_gaps(label, label.span.start(), label.span.end())
    }

    fn latex_fragment(&self, fragment: LatexSourceFragment, span_end: usize) -> String {
        match fragment {
            LatexSourceFragment::Command(command) => {
                let mut out = String::from('\\');
                out.push_str(command);
                if next_char(self.source, span_end).is_some_and(needs_command_separator) {
                    out.push(' ');
                }
                out
            }
            LatexSourceFragment::Raw(source) => source.to_owned(),
        }
    }
}

fn prime_source(text: &str) -> Option<&'static str> {
    match text {
        "′" => Some("'"),
        "″" => Some("''"),
        "‴" => Some("'''"),
        "⁗" => Some("''''"),
        _ => None,
    }
}

fn canonical_latex_passthrough(source: &str) -> &str {
    let Some(command) = source.strip_prefix('\\') else {
        return source;
    };
    match command {
        "supset\u{0338}" => r"\nsupset",
        "subset\u{0338}" => r"\nsubset",
        "supseteq\u{0338}" => r"\nsupseteq",
        "subseteq\u{0338}" => r"\nsubseteq",
        "leq\u{0338}" | "le\u{0338}" | "leqslant\u{0338}" => r"\nleq",
        "geq\u{0338}" | "ge\u{0338}" | "geqslant\u{0338}" => r"\ngeq",
        "in\u{0338}" => r"\notin",
        _ => source,
    }
}

fn directional_limit_command(accent: &UnicodeAccent<'_>) -> Option<&'static str> {
    let UnicodeAccentTarget::Node(node) = &accent.target else {
        return None;
    };
    if !matches!(&node.kind, UnicodeNodeKind::Plain("lim")) {
        return None;
    }
    match accent.accent {
        CombiningAccent::Vec => Some(r"\varinjlim"),
        CombiningAccent::Overleftarrow => Some(r"\varprojlim"),
        CombiningAccent::Tilde
        | CombiningAccent::Hat
        | CombiningAccent::Check
        | CombiningAccent::Bar
        | CombiningAccent::Breve
        | CombiningAccent::Dot
        | CombiningAccent::Ddot
        | CombiningAccent::Acute
        | CombiningAccent::Grave
        | CombiningAccent::Overleftrightarrow
        | CombiningAccent::Overline => None,
    }
}

fn accent_command(accent: CombiningAccent) -> &'static str {
    match accent {
        CombiningAccent::Tilde => "tilde",
        CombiningAccent::Hat => "hat",
        CombiningAccent::Check => "check",
        CombiningAccent::Bar => "bar",
        CombiningAccent::Breve => "breve",
        CombiningAccent::Dot => "dot",
        CombiningAccent::Ddot => "ddot",
        CombiningAccent::Acute => "acute",
        CombiningAccent::Grave => "grave",
        CombiningAccent::Vec => "vec",
        CombiningAccent::Overleftarrow => "overleftarrow",
        CombiningAccent::Overleftrightarrow => "overleftrightarrow",
        CombiningAccent::Overline => "overline",
    }
}

fn group_delimiters(delimiter: UnicodeGroupDelimiter) -> (char, char) {
    match delimiter {
        UnicodeGroupDelimiter::Brace => ('{', '}'),
        UnicodeGroupDelimiter::Bracket => ('[', ']'),
        UnicodeGroupDelimiter::Parenthesis => ('(', ')'),
    }
}

fn unicode_parse_error(diagnostic: &UnicodeParseDiagnostic) -> LatexError {
    let kind = match diagnostic.kind() {
        UnicodeParseDiagnosticKind::Lexical
        | UnicodeParseDiagnosticKind::DetachedCombiningMark
        | UnicodeParseDiagnosticKind::UnknownUnicodeSourceShape => LatexErrorKind::Lexical,
        UnicodeParseDiagnosticKind::UnsupportedAccentTarget | UnicodeParseDiagnosticKind::UnprovenLinearArrow => {
            LatexErrorKind::Unsupported
        }
        UnicodeParseDiagnosticKind::UnexpectedToken
        | UnicodeParseDiagnosticKind::ScriptWithoutRepresentableBase
        | UnicodeParseDiagnosticKind::DuplicateScript
        | UnicodeParseDiagnosticKind::MalformedGroupedScript
        | UnicodeParseDiagnosticKind::UnclosedGroup => LatexErrorKind::Syntax,
    };
    LatexError::new(kind, diagnostic.span(), diagnostic.message())
}

fn next_char(source: &str, index: usize) -> Option<char> {
    source.get(index..)?.chars().next()
}

/// Translate delimiter-excluded LaTeX math body ranges inside a larger source.
///
/// Ranges must be sorted, non-overlapping UTF-8 byte ranges into `source`.
/// Invalid range sets leave the source unchanged and report diagnostics.
#[must_use]
pub fn translate_latex_ranges_to_unicode(source: &str, ranges: &[Range<usize>]) -> Translation {
    translate_ranges(source, ranges, translate_latex_to_unicode)
}

/// Translate delimiter-excluded Unicode math body ranges inside a larger source.
///
/// Ranges must be sorted, non-overlapping UTF-8 byte ranges into `source`.
/// Invalid range sets leave the source unchanged and report diagnostics.
#[must_use]
pub fn translate_unicode_ranges_to_latex(source: &str, ranges: &[Range<usize>]) -> Translation {
    translate_ranges(source, ranges, translate_unicode_to_latex)
}

struct TranslateContext<'src> {
    source: &'src str,
    losses: Vec<TranslationLoss>,
    diagnostics: Vec<LatexError>,
}

impl<'src> TranslateContext<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            source,
            losses: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn translate_body_preserving_gaps(&mut self, body: &MathBody<'_>, start: usize, end: usize) -> String {
        let mut out = String::new();
        let mut cursor = start;
        for node in &body.elements {
            if cursor < node.span.start() {
                out.push_str(slice_or_empty(self.source, cursor..node.span.start()));
            }
            out.push_str(&self.translate_node(node));
            cursor = node.span.end();
        }
        if cursor < end {
            out.push_str(slice_or_empty(self.source, cursor..end));
        }
        out
    }

    fn translate_node(&mut self, node: &Node<'_>) -> String {
        match self.try_translate_node(node) {
            Ok(text) => text,
            Err(reason) => {
                self.losses.push(TranslationLoss::new(node.span, reason));
                slice_or_empty(self.source, node.span.as_range()).to_owned()
            }
        }
    }

    fn try_translate_node(&mut self, node: &Node<'_>) -> Result<String, String> {
        match &node.kind {
            NodeKind::Atom(atom) => self.translate_atom(*atom, node.span),
            NodeKind::Group(group) => Ok(self.translate_group_preserving_delimiters(group)),
            NodeKind::Fraction(fraction) => Self::translate_fraction(fraction),
            NodeKind::Sqrt(sqrt) => self.translate_sqrt(sqrt),
            NodeKind::Accent(accent) => self.translate_accent(accent),
            NodeKind::Script(script) => self.translate_script(script),
            NodeKind::Delimited(delimited) => Ok(self.translate_delimited(delimited)),
            NodeKind::Environment(_) => Err("environment has no editable Unicode source form".to_owned()),
        }
    }

    fn translate_atom(&mut self, atom: Atom<'_>, span: SourceSpan) -> Result<String, String> {
        match atom {
            Atom::Identifier(text) | Atom::Number(text) | Atom::Punctuation(text) | Atom::UnicodeSymbol(text) => {
                Ok(text.to_owned())
            }
            Atom::ControlSymbol(text) => Ok(control_symbol_text(text).to_owned()),
            Atom::Delimiter(delimiter) => Ok(delimiter_text(delimiter).to_owned()),
            Atom::CommandSymbol(name) => {
                let Some(symbol) = latex_symbol(name) else {
                    return Ok(String::new());
                };
                if let Some(command) = lookup_command(name)
                    && command.preferred() != name
                {
                    self.losses.push(TranslationLoss::new(
                        span,
                        format!(
                            "alias `\\{name}` canonicalises to `\\{}` in reverse translation",
                            command.preferred()
                        ),
                    ));
                }
                Ok(symbol.to_owned())
            }
        }
    }

    fn translate_group_preserving_delimiters(&mut self, group: &Group<'_>) -> String {
        let mut out = String::new();
        out.push_str(slice_or_empty(self.source, group.span.start()..group.body.span.start()));
        out.push_str(&self.translate_body_preserving_gaps(&group.body, group.body.span.start(), group.body.span.end()));
        out.push_str(slice_or_empty(self.source, group.body.span.end()..group.span.end()));
        out
    }

    fn translate_fraction(_fraction: &Fraction<'_>) -> Result<String, String> {
        Err("fraction has no unambiguous editable Unicode source form".to_owned())
    }

    fn translate_sqrt(&mut self, sqrt: &Sqrt<'_>) -> Result<String, String> {
        let mut out = String::new();
        if let Some(degree) = &sqrt.degree {
            let degree = self.translate_body_plain(&degree.body)?;
            let Some(script) = unicode_super_str(&degree) else {
                return Err("root degree has no Unicode superscript form".to_owned());
            };
            out.push_str(&script);
        }
        out.push('√');
        out.push_str(&self.translate_body_plain(&sqrt.body.body)?);
        Ok(out)
    }

    fn translate_accent(&mut self, accent: &Accent<'_>) -> Result<String, String> {
        let body = self.translate_body_plain(&accent.body.body)?;
        let mark = match accent.accent {
            AccentKind::Hat => '\u{302}',
            AccentKind::Bar => '\u{305}',
            AccentKind::Tilde => '\u{303}',
            AccentKind::Vec => '\u{20d7}',
        };
        let mut out = String::new();
        for ch in body.chars() {
            out.push(ch);
            if !ch.is_whitespace() {
                out.push(mark);
            }
        }
        Ok(out)
    }

    fn translate_script(&mut self, script: &Script<'_>) -> Result<String, String> {
        let mut out = self.translate_script_base(&script.base)?;
        if let Some(subscript) = &script.subscript {
            let text = self.translate_script_argument(subscript)?;
            let Some(rendered) = unicode_sub_str(&text) else {
                return Err(format!("subscript {text:?} has no Unicode source form"));
            };
            out.push_str(&rendered);
        }
        if let Some(superscript) = &script.superscript {
            let text = self.translate_script_argument(superscript)?;
            let Some(rendered) = unicode_super_str(&text) else {
                return Err(format!("superscript {text:?} has no Unicode source form"));
            };
            out.push_str(&rendered);
        }
        Ok(out)
    }

    fn translate_script_base(&mut self, base: &ScriptBase<'_>) -> Result<String, String> {
        match base {
            ScriptBase::Atom(atom) => self.translate_atom(*atom, SourceSpan::new(0, 0)),
            ScriptBase::Group(group) => self.translate_body_plain(&group.body),
            ScriptBase::Sqrt(sqrt) => self.translate_sqrt(sqrt),
            ScriptBase::Accent(accent) => self.translate_accent(accent),
            ScriptBase::Delimited(delimited) => Ok(self.translate_delimited(delimited)),
            ScriptBase::Fraction(_) => Err("scripted fraction has no Unicode source form".to_owned()),
        }
    }

    fn translate_script_argument(&mut self, argument: &ScriptArgument<'_>) -> Result<String, String> {
        match argument {
            ScriptArgument::Atom { atom, span } => self.translate_atom(*atom, *span),
            ScriptArgument::Group(group) => self.translate_body_plain(&group.body),
        }
    }

    fn translate_delimited(&mut self, delimited: &Delimited<'_>) -> String {
        let mut out = String::new();
        out.push_str(delimiter_text(delimited.opener));
        out.push_str(&self.translate_body_preserving_gaps(
            &delimited.body,
            delimited.body.span.start(),
            delimited.body.span.end(),
        ));
        out.push_str(delimiter_text(delimited.closer));
        out
    }

    fn translate_body_plain(&mut self, body: &MathBody<'_>) -> Result<String, String> {
        let translated = self.translate_body_preserving_gaps(body, body.span.start(), body.span.end());
        if translated.contains('\n') {
            Err("multi-line math body has no compact Unicode source form".to_owned())
        } else {
            Ok(translated)
        }
    }
}

fn translate_ranges(source: &str, ranges: &[Range<usize>], translate_body: fn(&str) -> Translation) -> Translation {
    if let Some(error) = validate_ranges(source, ranges) {
        return Translation::with_diagnostics(source, vec![error]);
    }

    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    let mut edit_count = 0usize;
    let mut losses = Vec::new();
    let mut diagnostics = Vec::new();
    for range in ranges {
        out.push_str(slice_or_empty(source, cursor..range.start));
        let body = slice_or_empty(source, range.clone());
        let translated = translate_body(body);
        if translated.text() != body {
            edit_count = edit_count.saturating_add(1);
        }
        losses.extend(shift_losses(translated.losses(), range.start));
        diagnostics.extend(shift_diagnostics(translated.diagnostics(), range.start));
        out.push_str(translated.text());
        cursor = range.end;
    }
    out.push_str(slice_or_empty(source, cursor..source.len()));
    Translation {
        text: out,
        edit_count,
        losses,
        diagnostics,
    }
}

fn validate_ranges(source: &str, ranges: &[Range<usize>]) -> Option<LatexError> {
    let mut end = 0usize;
    for range in ranges {
        if range.start < end {
            return Some(LatexError::new(
                LatexErrorKind::Syntax,
                SourceSpan::new(range.start, range.end.min(source.len())),
                "math body ranges must be sorted and non-overlapping",
            ));
        }
        if range.start > range.end
            || range.end > source.len()
            || !source.is_char_boundary(range.start)
            || !source.is_char_boundary(range.end)
        {
            return Some(LatexError::new(
                LatexErrorKind::Syntax,
                SourceSpan::new(range.start.min(source.len()), range.end.min(source.len())),
                "math body range is not a valid UTF-8 source range",
            ));
        }
        end = range.end;
    }
    None
}

fn shift_losses(losses: &[TranslationLoss], base: usize) -> impl Iterator<Item = TranslationLoss> + '_ {
    losses.iter().map(move |loss| {
        TranslationLoss::new(
            SourceSpan::new(
                loss.span.start().saturating_add(base),
                loss.span.end().saturating_add(base),
            ),
            loss.reason.clone(),
        )
    })
}

fn shift_diagnostics(diagnostics: &[LatexError], base: usize) -> impl Iterator<Item = LatexError> + '_ {
    diagnostics.iter().map(move |diagnostic| {
        LatexError::new(
            diagnostic.kind().clone(),
            SourceSpan::new(
                diagnostic.span().start().saturating_add(base),
                diagnostic.span().end().saturating_add(base),
            ),
            diagnostic.message(),
        )
    })
}

fn parse_errors(diagnostics: &[ParseDiagnostic]) -> Vec<LatexError> {
    diagnostics.iter().map(parse_error).collect()
}

fn parse_error(diagnostic: &ParseDiagnostic) -> LatexError {
    let kind = match diagnostic.kind() {
        ParseDiagnosticKind::Lexical => LatexErrorKind::Lexical,
        ParseDiagnosticKind::UnsupportedCommand | ParseDiagnosticKind::UnsupportedEnvironment => {
            LatexErrorKind::Unsupported
        }
        ParseDiagnosticKind::UnexpectedToken
        | ParseDiagnosticKind::MissingRequiredArgument
        | ParseDiagnosticKind::UnbalancedGroup
        | ParseDiagnosticKind::UnmatchedEnvironmentEnd
        | ParseDiagnosticKind::ScriptWithoutBase
        | ParseDiagnosticKind::DuplicateSubscript
        | ParseDiagnosticKind::DuplicateSuperscript => LatexErrorKind::Syntax,
    };
    LatexError::new(kind, diagnostic.span(), diagnostic.message())
}

fn delimiter_text(delimiter: Delimiter<'_>) -> &str {
    match delimiter {
        Delimiter::Source(".") => "",
        Delimiter::Source(source) => source,
    }
}

fn control_symbol_text(source: &str) -> &str {
    source.strip_prefix('\\').unwrap_or(source)
}

fn needs_command_separator(next: char) -> bool {
    next.is_alphanumeric() && unicode_super_latex(next).is_none() && unicode_sub_latex(next).is_none()
}

fn slice_or_empty(source: &str, range: Range<usize>) -> &str {
    source.get(range).unwrap_or("")
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::literal_string_with_formatting_args,
        clippy::unicode_not_nfc,
        reason = "translation tests inspect exact source output"
    )]

    use super::*;

    #[test]
    fn latex_to_unicode_translates_direct_commands_and_scripts() {
        let translated = translate_latex_to_unicode(r"\alpha_i + x^{2} \to \beta");

        assert_eq!(translated.text(), "αᵢ + x² → β");
        assert_eq!(translated.status(), TranslationStatus::Lossless);
        assert_eq!(translated.edit_count(), 1);
    }

    #[test]
    fn unicode_to_latex_uses_preferred_spellings_and_script_groups() {
        let translated = translate_unicode_to_latex("αᵢ ≤ x² → β");

        assert_eq!(translated.text(), r"\alpha_{i} \leq x^{2} \to \beta");
        assert_eq!(translated.status(), TranslationStatus::Lossless);
    }

    #[test]
    fn script_translation_laws_hold_for_supported_vocabulary() {
        let unicode = translate_latex_to_unicode(r"\alpha_i");
        assert_eq!(unicode.text(), "αᵢ");

        let latex = translate_unicode_to_latex(unicode.text());
        assert_eq!(latex.text(), r"\alpha_{i}");

        let unicode_again = translate_latex_to_unicode(latex.text());
        assert_eq!(unicode_again.text(), "αᵢ");
    }

    #[test]
    fn aliases_translate_to_unicode_with_reverse_canonicalisation_loss() {
        let translated = translate_latex_to_unicode(r"\le");

        assert_eq!(translated.text(), "≤");
        assert_eq!(translated.status(), TranslationStatus::Lossy);
        assert_eq!(
            translated.losses()[0].reason(),
            r"alias `\le` canonicalises to `\leq` in reverse translation"
        );
    }

    #[test]
    fn unsupported_commands_return_diagnostics_without_regex_replacement() {
        let translated = translate_latex_to_unicode(r"\alphabeta + \color{red}{x}");

        assert_eq!(translated.text(), r"\alphabeta + \color{red}{x}");
        assert_eq!(translated.status(), TranslationStatus::Lossy);
        assert_eq!(translated.diagnostics().len(), 2);
        assert!(
            translated
                .diagnostics()
                .iter()
                .all(|diagnostic| diagnostic.kind() == &LatexErrorKind::Unsupported)
        );
    }

    #[test]
    fn structural_forms_without_honest_source_shape_remain_visible() {
        let translated = translate_latex_to_unicode(r"\frac{a}{b} + \sqrt[n]{x}");

        assert_eq!(translated.text(), r"\frac{a}{b} + ⁿ√x");
        assert_eq!(translated.status(), TranslationStatus::Lossy);
        assert_eq!(
            translated.losses()[0].reason(),
            "fraction has no unambiguous editable Unicode source form"
        );
    }

    #[test]
    fn unicode_roots_translate_to_preferred_latex_source() {
        assert_eq!(translate_unicode_to_latex("√x").text(), r"\sqrt{x}");
        assert_eq!(translate_unicode_to_latex("ⁿ√x").text(), r"\sqrt[n]{x}");
        assert_eq!(translate_unicode_to_latex("√x²").text(), r"\sqrt{x^{2}}");
    }

    #[test]
    fn unicode_to_latex_preserves_visible_latex_fallback_syntax() {
        assert_eq!(translate_unicode_to_latex(r"\frac{a}{b}").text(), r"\frac{a}{b}");
        assert_eq!(
            translate_unicode_to_latex(r"x_{n} + \color{red}{x}").text(),
            r"x_{n} + \color{red}{x}"
        );
    }

    #[test]
    fn unicode_to_latex_normalizes_ascii_style_bracket_scripts() {
        assert_eq!(translate_unicode_to_latex("M_[φ]").text(), r"M_{\phi}");
        assert_eq!(translate_unicode_to_latex("x^(n)").text(), r"x^{n}");
    }

    #[test]
    fn unicode_to_latex_normalizes_prime_suffixes() {
        assert_eq!(translate_unicode_to_latex("A′").text(), "A'");
        assert_eq!(translate_unicode_to_latex("𝔭′").text(), r"\mathfrak{p}'");
        assert_eq!(translate_unicode_to_latex("A″").text(), "A''");
    }

    #[test]
    fn unicode_to_latex_normalizes_math_alphabets() {
        assert_eq!(translate_unicode_to_latex("𝒪_X").text(), r"\mathcal{O}_{X}");
        assert_eq!(translate_unicode_to_latex("ℱ(U)").text(), r"\mathcal{F}(U)");
        assert_eq!(translate_unicode_to_latex("𝔭").text(), r"\mathfrak{p}");
        assert_eq!(translate_unicode_to_latex("ℤ").text(), r"\mathbb{Z}");
        assert_eq!(translate_unicode_to_latex("𝓗𝓸𝓶").text(), r"\mathcal{H}om");
        assert_eq!(translate_unicode_to_latex("𝓟𝓻𝓸𝓳").text(), r"\mathcal{P}roj");
        assert_eq!(translate_unicode_to_latex("𝒟ℯ𝓇").text(), r"\mathcal{D}er");
        assert_eq!(translate_unicode_to_latex("𝚪_*").text(), r"\Gamma_{*}");
        assert_eq!(translate_unicode_to_latex("𝐒").text(), r"\mathbf{S}");
        assert_eq!(translate_unicode_to_latex("𝐕").text(), r"\mathbf{V}");
        assert_eq!(translate_unicode_to_latex("𝐗").text(), r"\mathbf{X}");
        assert_eq!(translate_unicode_to_latex("𝐟𝐠").text(), r"\mathbf{fg}");
    }

    #[test]
    fn unicode_to_latex_normalizes_combining_accents() {
        assert_eq!(translate_unicode_to_latex("M̃").text(), r"\tilde{M}");
        assert_eq!(translate_unicode_to_latex("Ω̂").text(), r"\hat{\Omega}");
        assert_eq!(translate_unicode_to_latex("Ĉ").text(), r"\hat{C}");
        assert_eq!(translate_unicode_to_latex("{x}̅").text(), r"\bar{x}");
        assert_eq!(translate_unicode_to_latex("Ȳ'").text(), r"\bar{Y}'");
        assert_eq!(translate_unicode_to_latex("{Y'}̄").text(), r"\bar{Y'}");
        let prime_bar = translate_unicode_to_latex("Y'̄");
        assert_eq!(prime_bar.text(), "Y'̄");
        assert_eq!(prime_bar.status(), TranslationStatus::Lossy);
        assert_eq!(prime_bar.diagnostics()[0].kind(), &LatexErrorKind::Unsupported);
        assert_eq!(translate_unicode_to_latex("ũ").text(), r"\tilde{u}");
        assert_eq!(translate_unicode_to_latex("ẑ").text(), r"\hat{z}");
        assert_eq!(translate_unicode_to_latex("c̄").text(), r"\bar{c}");
        assert_eq!(translate_unicode_to_latex("lim⃗").text(), r"\varinjlim");
        assert_eq!(translate_unicode_to_latex("lim⃖").text(), r"\varprojlim");
    }

    #[test]
    fn unicode_to_latex_normalizes_extended_scripts() {
        assert_eq!(translate_unicode_to_latex("iˢ_A").text(), r"i^{s}_{A}");
        assert_eq!(translate_unicode_to_latex("iᵀ_M").text(), r"i^{T}_{M}");
        assert_eq!(translate_unicode_to_latex(r"ᵃ\phi").text(), r"{}^{a}\phi");
        assert_eq!(translate_unicode_to_latex("D₊").text(), r"D_{+}");
        assert_eq!(translate_unicode_to_latex("xᵐ").text(), r"x^{m}");
        assert_eq!(translate_unicode_to_latex("Aᵖ").text(), r"A^{p}");
    }

    #[test]
    fn unicode_to_latex_normalizes_common_operator_fragments() {
        assert_eq!(translate_unicode_to_latex("a · m").text(), r"a \cdot m");
        assert_eq!(translate_unicode_to_latex("A − 𝔭").text(), r"A - \mathfrak{p}");
        assert_eq!(translate_unicode_to_latex("A ⥲ B").text(), r"A \xrightarrow{\sim} B");
        assert_eq!(translate_unicode_to_latex("A ⟺ B").text(), r"A \Longleftrightarrow B");
        assert_eq!(
            translate_unicode_to_latex("a ⩾ b ⩽ c").text(),
            r"a \geqslant b \leqslant c"
        );
        assert_eq!(translate_unicode_to_latex("a ⋯ b … c").text(), r"a \cdots b \cdots c");
        assert_eq!(
            translate_unicode_to_latex("⨁ A □ ∁B").text(),
            r"\bigoplus A \square \complement B"
        );
        assert_eq!(translate_unicode_to_latex("A ↔ B").text(), r"A \leftrightarrow B");
        assert_eq!(
            translate_unicode_to_latex("f♯ ⊠ g♭ ⊔ h♮").text(),
            r"f\sharp \boxtimes g\flat \sqcup h\natural"
        );
        assert_eq!(
            translate_unicode_to_latex("A ↠ B ≀ C").text(),
            r"A \twoheadrightarrow B \wr C"
        );
        assert_eq!(
            translate_unicode_to_latex("A ⊉ B ⊄ C ⊀ D").text(),
            r"A \nsupseteq B \nsubset C \nprec D"
        );
        assert_eq!(
            translate_unicode_to_latex(concat!(
                "a ",
                "\u{227A}\u{0338}",
                " b ",
                "\u{2A7D}\u{0338}",
                " c ≽ d ≼ e ≫ f"
            ))
            .text(),
            r"a \nprec b \nleqslant c \succeq d \preceq e \gg f"
        );
        assert_eq!(
            translate_unicode_to_latex("codim(‾{x}, S)").text(),
            r"codim(\overline{x}, S)"
        );
        assert_eq!(translate_unicode_to_latex("‾K").text(), r"\overline{K}");
        assert_eq!(translate_unicode_to_latex("ℎ^{q} ℴ").text(), r"h^{q} o");
        assert_eq!(translate_unicode_to_latex("X°_{y}").text(), r"X^{\circ}_{y}");
        assert_eq!(
            translate_unicode_to_latex("A ⨂ B ↝ C").text(),
            r"A \bigotimes B \rightsquigarrow C"
        );
        assert_eq!(translate_unicode_to_latex("⋂ A").text(), r"\bigcap A");
        assert_eq!(
            translate_unicode_to_latex(concat!("\\supset", "\u{0338}", " S")).text(),
            r"\nsupset S"
        );
        assert_eq!(
            translate_unicode_to_latex(concat!("\\leqslant", "\u{0338}", r" \lambda")).text(),
            r"\nleq \lambda"
        );
    }

    #[test]
    fn unicode_to_latex_normalizes_linear_arrow_notation() {
        assert_eq!(translate_unicode_to_latex("A ─u→ B").text(), r"A \xrightarrow{u} B");
        assert_eq!(translate_unicode_to_latex("A ←u─ B").text(), r"A \xleftarrow{u} B");
        assert_eq!(translate_unicode_to_latex("A ——→ B").text(), r"A \to B");
        assert_eq!(translate_unicode_to_latex("A ─^{u}→ B").text(), r"A \xrightarrow{u} B");
        assert_eq!(translate_unicode_to_latex("A ←^{u}─ B").text(), r"A \xleftarrow{u} B");
        assert_eq!(
            translate_unicode_to_latex("A ──φ^{S′}──→ B").text(),
            r"A \xrightarrow{\phi^{S'}} B"
        );
        assert_eq!(translate_unicode_to_latex("A ─────→ B").text(), r"A \to B");
        assert_eq!(
            translate_unicode_to_latex("Ω ──u──▸ Ω'").text(),
            r"\Omega \xrightarrow{u} \Omega'"
        );
        assert_eq!(
            translate_unicode_to_latex("U ─j × 1→ X").text(),
            r"U \xrightarrow{j \times 1} X"
        );
        assert_eq!(translate_unicode_to_latex("A ⤏ B").text(), r"A \dashrightarrow B");
    }

    #[test]
    fn unsupported_unicode_remains_visible_and_lossy() {
        let translated = translate_unicode_to_latex("A ⥪ B");

        assert_eq!(translated.text(), "A ⥪ B");
        assert_eq!(translated.status(), TranslationStatus::Lossy);
        assert_eq!(translated.losses().len(), 1);
        assert_eq!(translated.diagnostics()[0].kind(), &LatexErrorKind::Lexical);
    }

    #[test]
    fn unicode_to_latex_corpus_forms_reach_latex_fixed_point() {
        for source in [
            "M_[φ]",
            "A′",
            "S⁻¹A",
            "𝒪_X",
            "a · m",
            "A ⥲ B",
            "U ─j × 1→ X",
            "f♯ ⊠ g♭",
            "codim(‾{x}, S)",
        ] {
            let latex = translate_unicode_to_latex(source);
            let unicode = translate_latex_to_unicode(latex.text());
            let latex_again = translate_unicode_to_latex(unicode.text());
            assert_eq!(latex_again.text(), latex.text(), "source {source:?}");
        }
    }

    #[test]
    fn span_translation_preserves_markdown_delimiters() {
        let source = r"Inline \( \alpha_i \) and \( x^{2} \).";
        let first = r"Inline \( ".len();
        let second = r"Inline \( \alpha_i \) and \[ ".len();
        let translated = translate_latex_ranges_to_unicode(
            source,
            &[first..first + r"\alpha_i".len(), second..second + "x^{2}".len()],
        );

        assert_eq!(translated.text(), r"Inline \( αᵢ \) and \( x² \).");
        assert_eq!(translated.edit_count(), 2);
        assert_eq!(translated.status(), TranslationStatus::Lossless);
    }

    #[test]
    fn invalid_span_sets_do_not_rewrite_source() {
        let range = 1..2;
        let translated = translate_latex_ranges_to_unicode("αβ", std::slice::from_ref(&range));

        assert_eq!(translated.text(), "αβ");
        assert_eq!(translated.status(), TranslationStatus::Lossy);
        assert_eq!(translated.diagnostics()[0].kind(), &LatexErrorKind::Syntax);
    }

    #[test]
    fn scanner_backed_unicode_normalizer_is_deleted() {
        let deleted_type = ["Unicode", "Latex", "Normalizer"].concat();
        assert!(!include_str!("translation.rs").contains(&deleted_type));
    }
}
