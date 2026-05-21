//! Conservative LaTeX-math to Unicode terminal rendering.
//!
//! This is a display adapter, not a TeX engine. It accepts the small
//! subset mdwright can render honestly in a cell grid and reports an
//! error for shapes that need real TeX layout.

use std::fmt;

use unicode_width::UnicodeWidthStr;

use mdwright_latex::{latex_symbol, unicode_sub_str, unicode_super_str};

/// A rendered Unicode math block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedMath {
    lines: Vec<String>,
    baseline: usize,
    width: usize,
}

impl RenderedMath {
    fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        let width = UnicodeWidthStr::width(text.as_str());
        Self {
            lines: vec![text],
            baseline: 0,
            width,
        }
    }

    fn empty() -> Self {
        Self::text("")
    }

    /// Rendered lines.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Baseline line index.
    #[must_use]
    pub fn baseline(&self) -> usize {
        self.baseline
    }

    /// Display-cell width.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Materialise the block as newline-separated text.
    #[must_use]
    pub fn as_text(&self) -> String {
        self.lines.join("\n")
    }

    fn hcat(&self, rhs: &Self) -> Self {
        let baseline = self.baseline.max(rhs.baseline);
        let below = self
            .lines
            .len()
            .saturating_sub(self.baseline.saturating_add(1))
            .max(rhs.lines.len().saturating_sub(rhs.baseline.saturating_add(1)));
        let height = baseline.saturating_add(1).saturating_add(below);
        let mut lines = Vec::with_capacity(height);
        for row in 0..height {
            let lhs_line = block_line(self, row, baseline);
            let rhs_line = block_line(rhs, row, baseline);
            lines.push(format!("{lhs_line}{rhs_line}"));
        }
        Self {
            lines,
            baseline,
            width: self.width.saturating_add(rhs.width),
        }
    }

    fn append_baseline_suffix(mut self, suffix: &str) -> Self {
        if let Some(line) = self.lines.get_mut(self.baseline) {
            line.push_str(suffix);
        }
        self.width = self.width.saturating_add(UnicodeWidthStr::width(suffix));
        self
    }

    fn single_line_text(&self) -> Option<&str> {
        (self.lines.len() == 1).then(|| self.lines.first().map(String::as_str))?
    }
}

impl fmt::Display for RenderedMath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_text())
    }
}

/// Why Unicode math rendering could not represent the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnicodeMathError {
    UnsupportedCommand(String),
    UnbalancedGroup,
    UnexpectedEnd,
    UnsupportedScript(String),
}

impl fmt::Display for UnicodeMathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCommand(name) => write!(f, "unsupported LaTeX command \\{name}"),
            Self::UnbalancedGroup => f.write_str("unbalanced LaTeX group"),
            Self::UnexpectedEnd => f.write_str("unexpected end of math input"),
            Self::UnsupportedScript(script) => write!(f, "unsupported script {script:?}"),
        }
    }
}

impl std::error::Error for UnicodeMathError {}

/// Render a conservative subset of LaTeX math as Unicode terminal text.
///
/// # Errors
///
/// Returns [`UnicodeMathError`] for unsupported commands, malformed
/// groups, or script bodies that do not have a Unicode representation.
pub fn render_unicode_math(source: &str) -> Result<RenderedMath, UnicodeMathError> {
    Parser::new(source).parse()
}

fn block_line(block: &RenderedMath, row: usize, baseline: usize) -> String {
    let source_row = if row >= baseline {
        block.baseline.checked_add(row.saturating_sub(baseline))
    } else {
        block.baseline.checked_sub(baseline.saturating_sub(row))
    };
    let Some(line) = source_row.and_then(|idx| block.lines.get(idx)) else {
        return " ".repeat(block.width);
    };
    pad_to_width(line, block.width)
}

fn pad_to_width(line: &str, width: usize) -> String {
    let current = UnicodeWidthStr::width(line);
    if current >= width {
        line.to_owned()
    } else {
        format!("{line}{}", " ".repeat(width.saturating_sub(current)))
    }
}

fn center(line: &str, width: usize) -> String {
    let current = UnicodeWidthStr::width(line);
    if current >= width {
        return line.to_owned();
    }
    let pad = width.saturating_sub(current);
    let left = pad / 2;
    let right = pad.saturating_sub(left);
    format!("{}{}{}", " ".repeat(left), line, " ".repeat(right))
}

fn hcat_all(parts: &[RenderedMath]) -> RenderedMath {
    parts.iter().fold(RenderedMath::empty(), |acc, part| acc.hcat(part))
}

fn fraction(numerator: &RenderedMath, denominator: &RenderedMath) -> RenderedMath {
    let width = numerator.width.max(denominator.width).max(1);
    let mut lines = Vec::new();
    lines.extend(numerator.lines.iter().map(|line| center(line, width)));
    let baseline = lines.len();
    lines.push("─".repeat(width));
    lines.extend(denominator.lines.iter().map(|line| center(line, width)));
    RenderedMath { lines, baseline, width }
}

fn sqrt(radicand: &RenderedMath) -> RenderedMath {
    let mut lines = Vec::with_capacity(radicand.lines.len());
    for (idx, line) in radicand.lines.iter().enumerate() {
        if idx == radicand.baseline {
            lines.push(format!("√{line}"));
        } else {
            lines.push(format!(" {line}"));
        }
    }
    RenderedMath {
        lines,
        baseline: radicand.baseline,
        width: radicand.width.saturating_add(1),
    }
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn parse(mut self) -> Result<RenderedMath, UnicodeMathError> {
        let out = self.parse_expr(false)?;
        self.skip_ws();
        if self.peek() == Some('}') {
            return Err(UnicodeMathError::UnbalancedGroup);
        }
        Ok(out)
    }

    fn parse_expr(&mut self, stop_on_rbrace: bool) -> Result<RenderedMath, UnicodeMathError> {
        let mut parts = Vec::new();
        while let Some(ch) = self.peek() {
            if ch == '}' {
                if stop_on_rbrace {
                    break;
                }
                return Err(UnicodeMathError::UnbalancedGroup);
            }
            if ch.is_whitespace() {
                self.skip_ws();
                if !parts.is_empty() && !matches!(self.peek(), None | Some('}')) {
                    parts.push(RenderedMath::text(" "));
                }
                continue;
            }
            let atom = self.parse_atom()?;
            parts.push(self.parse_scripts(atom)?);
        }
        Ok(hcat_all(&parts))
    }

    fn parse_atom(&mut self) -> Result<RenderedMath, UnicodeMathError> {
        match self.bump() {
            Some('{') => {
                let inner = self.parse_expr(true)?;
                if self.bump() != Some('}') {
                    return Err(UnicodeMathError::UnbalancedGroup);
                }
                Ok(inner)
            }
            Some('\\') => self.parse_command(),
            Some(ch) => Ok(RenderedMath::text(ch.to_string())),
            None => Err(UnicodeMathError::UnexpectedEnd),
        }
    }

    fn parse_command(&mut self) -> Result<RenderedMath, UnicodeMathError> {
        let name = self.take_ascii_letters();
        if name.is_empty() {
            return self
                .bump()
                .map(|ch| RenderedMath::text(ch.to_string()))
                .ok_or(UnicodeMathError::UnexpectedEnd);
        }
        match name.as_str() {
            "frac" => {
                let numerator = self.parse_required_group()?;
                let denominator = self.parse_required_group()?;
                Ok(fraction(&numerator, &denominator))
            }
            "sqrt" => {
                let radicand = self.parse_required_group()?;
                Ok(sqrt(&radicand))
            }
            "begin" | "end" => Err(UnicodeMathError::UnsupportedCommand(name)),
            _ => latex_symbol(&name)
                .map(RenderedMath::text)
                .ok_or(UnicodeMathError::UnsupportedCommand(name)),
        }
    }

    fn parse_required_group(&mut self) -> Result<RenderedMath, UnicodeMathError> {
        self.skip_ws();
        if self.bump() != Some('{') {
            return Err(UnicodeMathError::UnbalancedGroup);
        }
        let inner = self.parse_expr(true)?;
        if self.bump() != Some('}') {
            return Err(UnicodeMathError::UnbalancedGroup);
        }
        Ok(inner)
    }

    fn parse_scripts(&mut self, mut base: RenderedMath) -> Result<RenderedMath, UnicodeMathError> {
        loop {
            match self.peek() {
                Some('^') => {
                    self.bump();
                    let script = self.parse_script_body()?;
                    let rendered = unicode_super_str(&script)
                        .ok_or_else(|| UnicodeMathError::UnsupportedScript(script.clone()))?;
                    base = base.append_baseline_suffix(&rendered);
                }
                Some('_') => {
                    self.bump();
                    let script = self.parse_script_body()?;
                    let rendered =
                        unicode_sub_str(&script).ok_or_else(|| UnicodeMathError::UnsupportedScript(script.clone()))?;
                    base = base.append_baseline_suffix(&rendered);
                }
                _ => return Ok(base),
            }
        }
    }

    fn parse_script_body(&mut self) -> Result<String, UnicodeMathError> {
        let body = if self.peek() == Some('{') {
            self.bump();
            let inner = self.parse_expr(true)?;
            if self.bump() != Some('}') {
                return Err(UnicodeMathError::UnbalancedGroup);
            }
            inner
        } else {
            self.parse_atom()?
        };
        body.single_line_text()
            .map(ToOwned::to_owned)
            .ok_or_else(|| UnicodeMathError::UnsupportedScript(body.as_text()))
    }

    fn take_ascii_letters(&mut self) -> String {
        let start = self.pos;
        while matches!(self.peek(), Some(ch) if ch.is_ascii_alphabetic()) {
            self.bump();
        }
        self.input.get(start..self.pos).unwrap_or("").to_owned()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(ch) if ch.is_whitespace()) {
            self.bump();
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos..).and_then(|tail| tail.chars().next())
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos = self.pos.saturating_add(ch.len_utf8());
        Some(ch)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "unit tests use expect to surface setup failures")]
mod tests {
    use super::*;

    fn text(source: &str) -> String {
        render_unicode_math(source).expect("math renders").as_text()
    }

    #[test]
    fn simple_symbols_and_scripts_render() {
        assert_eq!(text(r"\alpha_i"), "αᵢ");
        assert_eq!(text("x^{2}"), "x²");
        assert_eq!(text("f^{-1}"), "f⁻¹");
    }

    #[test]
    fn fraction_renders_as_grid() {
        let rendered = render_unicode_math(r"\frac{a}{b}").expect("fraction renders");
        assert_eq!(rendered.lines(), &["a".to_owned(), "─".to_owned(), "b".to_owned()]);
        assert_eq!(rendered.baseline(), 1);
        assert_eq!(rendered.width(), 1);
    }

    #[test]
    fn sqrt_renders_inline() {
        assert_eq!(text(r"\sqrt{x}"), "√x");
    }

    #[test]
    fn nested_simple_expression_renders() {
        assert_eq!(text(r"\frac{\alpha_i}{x^{2}}"), "αᵢ\n──\nx²");
    }

    #[test]
    fn unsupported_shapes_return_errors() {
        assert!(matches!(
            render_unicode_math(r"\bar{x}"),
            Err(UnicodeMathError::UnsupportedCommand(name)) if name == "bar"
        ));
        assert!(matches!(
            render_unicode_math(r"\frac{a}"),
            Err(UnicodeMathError::UnbalancedGroup | UnicodeMathError::UnexpectedEnd)
        ));
    }
}
