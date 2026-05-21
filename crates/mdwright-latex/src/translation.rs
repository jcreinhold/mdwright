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
    latex_symbol, lookup_command, unicode_sub_latex, unicode_sub_str, unicode_super_latex, unicode_super_str,
    unicode_symbol_latex,
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
    let mut out = String::with_capacity(source.len());
    let mut cursor = source.char_indices().peekable();
    while let Some((_, ch)) = cursor.next() {
        if let Some(script) = unicode_super_latex(ch) {
            let mut body = String::from(script);
            while let Some((_, next)) = cursor.peek().copied() {
                let Some(piece) = unicode_super_latex(next) else {
                    break;
                };
                cursor.next();
                body.push_str(piece);
            }
            out.push_str("^{");
            out.push_str(&body);
            out.push('}');
            continue;
        }
        if let Some(script) = unicode_sub_latex(ch) {
            let mut body = String::from(script);
            while let Some((_, next)) = cursor.peek().copied() {
                let Some(piece) = unicode_sub_latex(next) else {
                    break;
                };
                cursor.next();
                body.push_str(piece);
            }
            out.push_str("_{");
            out.push_str(&body);
            out.push('}');
            continue;
        }

        let symbol = ch.to_string();
        if let Some(command) = unicode_symbol_latex(&symbol) {
            out.push('\\');
            out.push_str(command);
            if cursor.peek().is_some_and(|(_, next)| needs_command_separator(*next)) {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    Translation::from_parts(source, out, Vec::new(), Vec::new())
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
}
