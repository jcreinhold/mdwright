//! Unicode terminal layout for parsed TeX math bodies.
//!
//! This is not a TeX box model. It is a conservative grid renderer:
//! each intermediate value has a display-cell width, a positive
//! height, a baseline row inside that height, and padded rows. The
//! constructors maintain those invariants so callers cannot observe
//! ragged multi-line output.

use std::fmt;

use unicode_width::UnicodeWidthStr;

use crate::error::{LatexError, LatexErrorKind, SourceSpan};
use crate::parser::{
    Accent, AccentKind, Atom, Delimited, Delimiter, Environment, Fraction, Group, MathBody, Node, NodeKind,
    ParseDiagnostic, ParseDiagnosticKind, Row, Script, ScriptArgument, ScriptBase, Sqrt, parse_math_body,
};
use crate::registry::{latex_symbol, unicode_sub_str, unicode_super_str};

/// Unicode layout output for a TeX math body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedLatex {
    lines: Vec<String>,
    baseline: usize,
    width: usize,
}

impl RenderedLatex {
    fn from_grid(grid: Grid) -> Self {
        Self {
            lines: grid.lines,
            baseline: grid.baseline,
            width: grid.width,
        }
    }

    /// Rendered terminal lines.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Baseline line index.
    #[must_use]
    pub const fn baseline(&self) -> usize {
        self.baseline
    }

    /// Display-cell width.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Materialise the rendered block as newline-separated text.
    #[must_use]
    pub fn as_text(&self) -> String {
        self.lines.join("\n")
    }
}

impl fmt::Display for RenderedLatex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_text())
    }
}

/// Render one TeX math body as terminal-friendly Unicode.
///
/// # Errors
///
/// Returns [`LatexError`] when the body is malformed or contains a
/// parsed construct that mdwright cannot render honestly as Unicode.
pub fn render_unicode_math(source: &str) -> Result<RenderedLatex, LatexError> {
    let body = parse_math_body(source).map_err(first_parse_error)?;
    render_body(&body).map(RenderedLatex::from_grid)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Grid {
    lines: Vec<String>,
    width: usize,
    baseline: usize,
}

impl Grid {
    fn new(lines: Vec<String>, baseline: usize) -> Self {
        let width = lines
            .iter()
            .map(|line| UnicodeWidthStr::width(line.as_str()))
            .max()
            .unwrap_or(0);
        let mut padded = if lines.is_empty() { vec![String::new()] } else { lines };
        for line in &mut padded {
            *line = pad_to_width(line, width);
        }
        let baseline = baseline.min(padded.len().saturating_sub(1));
        Self {
            lines: padded,
            width,
            baseline,
        }
    }

    fn text(text: impl Into<String>) -> Self {
        Self::new(vec![text.into()], 0)
    }

    fn empty() -> Self {
        Self::text("")
    }

    fn height(&self) -> usize {
        self.lines.len()
    }

    fn hcat(&self, rhs: &Self) -> Self {
        let baseline = self.baseline.max(rhs.baseline);
        let self_below = self.height().saturating_sub(self.baseline.saturating_add(1));
        let rhs_below = rhs.height().saturating_sub(rhs.baseline.saturating_add(1));
        let below = self_below.max(rhs_below);
        let height = baseline.saturating_add(1).saturating_add(below);
        let mut lines = Vec::with_capacity(height);
        for row in 0..height {
            let lhs_line = self.line_at(row, baseline);
            let rhs_line = rhs.line_at(row, baseline);
            lines.push(format!("{lhs_line}{rhs_line}"));
        }
        Self::new(lines, baseline)
    }

    fn append_baseline_suffix(mut self, suffix: &str) -> Self {
        if let Some(line) = self.lines.get_mut(self.baseline) {
            line.push_str(suffix);
        }
        Self::new(self.lines, self.baseline)
    }

    fn line_at(&self, row: usize, target_baseline: usize) -> String {
        let source_row = if row >= target_baseline {
            self.baseline.checked_add(row.saturating_sub(target_baseline))
        } else {
            self.baseline.checked_sub(target_baseline.saturating_sub(row))
        };
        source_row
            .and_then(|idx| self.lines.get(idx))
            .map_or_else(|| " ".repeat(self.width), ToOwned::to_owned)
    }

    fn single_line_text(&self) -> Option<&str> {
        (self.lines.len() == 1).then(|| self.lines.first().map(String::as_str))?
    }
}

fn render_body(body: &MathBody<'_>) -> Result<Grid, LatexError> {
    let parts = body.elements.iter().map(render_node).collect::<Result<Vec<_>, _>>()?;
    Ok(hcat_all(&parts))
}

fn render_node(node: &Node<'_>) -> Result<Grid, LatexError> {
    match &node.kind {
        NodeKind::Atom(atom) => render_atom(*atom, node.span),
        NodeKind::Group(group) => render_group(group),
        NodeKind::Fraction(fraction) => render_fraction(fraction, node.span),
        NodeKind::Sqrt(sqrt) => render_sqrt(sqrt, node.span),
        NodeKind::Accent(accent) => render_accent(accent, node.span),
        NodeKind::Script(script) => render_script(script, node.span),
        NodeKind::Delimited(delimited) => render_delimited(delimited),
        NodeKind::Environment(environment) => render_environment(environment, node.span),
    }
}

fn render_atom(atom: Atom<'_>, span: SourceSpan) -> Result<Grid, LatexError> {
    match atom {
        Atom::Identifier(text) | Atom::Number(text) | Atom::Punctuation(text) | Atom::UnicodeSymbol(text) => {
            Ok(Grid::text(text))
        }
        Atom::ControlSymbol(text) => Ok(Grid::text(control_symbol_text(text))),
        Atom::CommandSymbol(name) => latex_symbol(name)
            .map(Grid::text)
            .ok_or_else(|| unsupported(span, format!("unsupported TeX command `\\{name}`"))),
        Atom::Delimiter(delimiter) => Ok(Grid::text(delimiter_text(delimiter))),
    }
}

fn render_group(group: &Group<'_>) -> Result<Grid, LatexError> {
    render_body(&group.body)
}

fn render_fraction(fraction: &Fraction<'_>, span: SourceSpan) -> Result<Grid, LatexError> {
    let numerator = render_group(&fraction.numerator)?;
    let denominator = render_group(&fraction.denominator)?;
    let width = numerator.width.max(denominator.width).max(1);
    let mut lines = Vec::new();
    lines.extend(numerator.lines.iter().map(|line| center(line, width)));
    let baseline = lines.len();
    lines.push("─".repeat(width));
    lines.extend(denominator.lines.iter().map(|line| center(line, width)));
    let grid = Grid::new(lines, baseline);
    if grid.width == 0 {
        Err(unsupported(span, "empty fraction cannot be rendered"))
    } else {
        Ok(grid)
    }
}

fn render_sqrt(sqrt: &Sqrt<'_>, span: SourceSpan) -> Result<Grid, LatexError> {
    let radicand = render_group(&sqrt.body)?;
    let root = match &sqrt.degree {
        Some(degree) => {
            let degree = render_group(degree)?;
            let Some(text) = degree.single_line_text() else {
                return Err(unsupported(span, "multi-line root degree cannot be rendered"));
            };
            let rendered = unicode_super_str(text)
                .ok_or_else(|| unsupported(span, "root degree has no Unicode superscript form"))?;
            format!("{rendered}√")
        }
        None => "√".to_owned(),
    };
    Ok(prefix_baseline(&root, &radicand))
}

fn render_accent(accent: &Accent<'_>, span: SourceSpan) -> Result<Grid, LatexError> {
    let body = render_group(&accent.body)?;
    let Some(text) = body.single_line_text() else {
        return Err(unsupported(span, "multi-line accent body cannot be rendered"));
    };
    let mark = match accent.accent {
        AccentKind::Hat => '\u{302}',
        AccentKind::Bar => '\u{305}',
        AccentKind::Tilde => '\u{303}',
        AccentKind::Vec => '\u{20d7}',
    };
    let mut out = String::new();
    for ch in text.chars() {
        out.push(ch);
        if !ch.is_whitespace() {
            out.push(mark);
        }
    }
    Ok(Grid::text(out))
}

fn render_script(script: &Script<'_>, span: SourceSpan) -> Result<Grid, LatexError> {
    let mut base = render_script_base(&script.base)?;
    if let Some(superscript) = &script.superscript {
        let text = render_script_argument(superscript)?;
        let rendered =
            unicode_super_str(&text).ok_or_else(|| unsupported(span, format!("unsupported superscript {text:?}")))?;
        base = base.append_baseline_suffix(&rendered);
    }
    if let Some(subscript) = &script.subscript {
        let text = render_script_argument(subscript)?;
        let rendered =
            unicode_sub_str(&text).ok_or_else(|| unsupported(span, format!("unsupported subscript {text:?}")))?;
        base = base.append_baseline_suffix(&rendered);
    }
    Ok(base)
}

fn render_script_base(base: &ScriptBase<'_>) -> Result<Grid, LatexError> {
    match base {
        ScriptBase::Atom(atom) => render_atom(*atom, SourceSpan::new(0, 0)),
        ScriptBase::Group(group) => render_group(group),
        ScriptBase::Fraction(fraction) => render_fraction(fraction, SourceSpan::new(0, 0)),
        ScriptBase::Sqrt(sqrt) => render_sqrt(sqrt, SourceSpan::new(0, 0)),
        ScriptBase::Accent(accent) => render_accent(accent, SourceSpan::new(0, 0)),
        ScriptBase::Delimited(delimited) => render_delimited(delimited),
    }
}

fn render_script_argument(argument: &ScriptArgument<'_>) -> Result<String, LatexError> {
    let rendered = match argument {
        ScriptArgument::Atom { atom, span } => render_atom(*atom, *span)?,
        ScriptArgument::Group(group) => render_group(group)?,
    };
    rendered
        .single_line_text()
        .map(ToOwned::to_owned)
        .ok_or_else(|| unsupported(argument.span(), "multi-line script cannot be rendered"))
}

fn render_delimited(delimited: &Delimited<'_>) -> Result<Grid, LatexError> {
    let opener = Grid::text(delimiter_text(delimited.opener));
    let body = render_body(&delimited.body)?;
    let closer = Grid::text(delimiter_text(delimited.closer));
    Ok(opener.hcat(&body).hcat(&closer))
}

fn render_environment(environment: &Environment<'_>, span: SourceSpan) -> Result<Grid, LatexError> {
    match environment.name {
        "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" | "cases" | "array" => {
            render_matrix_like(environment, span)
        }
        "aligned" | "split" => render_matrix_rows(&environment.rows, span),
        name => Err(unsupported(span, format!("unsupported environment `{name}`"))),
    }
}

fn render_matrix_like(environment: &Environment<'_>, span: SourceSpan) -> Result<Grid, LatexError> {
    let rows = render_matrix_rows(&environment.rows, span)?;
    let (left, right) = match environment.name {
        "pmatrix" => ("(", ")"),
        "bmatrix" => ("[", "]"),
        "Bmatrix" => ("{", "}"),
        "vmatrix" => ("|", "|"),
        "Vmatrix" => ("‖", "‖"),
        "cases" => ("{", ""),
        _ => ("", ""),
    };
    Ok(wrap_rows(left, rows, right))
}

fn render_matrix_rows(rows: &[Row<'_>], span: SourceSpan) -> Result<Grid, LatexError> {
    if rows.is_empty() {
        return Ok(Grid::empty());
    }
    let rendered = rows.iter().map(render_row).collect::<Result<Vec<_>, _>>()?;
    if rendered
        .iter()
        .flat_map(|row| row.iter())
        .any(|cell| cell.height() != 1)
    {
        return Err(unsupported(span, "multi-line matrix cells cannot be rendered"));
    }
    let columns = rendered.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0usize; columns];
    for row in &rendered {
        for (idx, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(idx) {
                *width = (*width).max(cell.width);
            }
        }
    }
    let mut lines = Vec::with_capacity(rendered.len());
    for row in rendered {
        let mut parts = Vec::with_capacity(columns);
        for idx in 0..columns {
            let cell = row.get(idx).map_or_else(Grid::empty, Clone::clone);
            let width = widths.get(idx).copied().unwrap_or(0);
            parts.push(center(cell.single_line_text().unwrap_or(""), width));
        }
        lines.push(parts.join("  "));
    }
    let baseline = lines.len() / 2;
    Ok(Grid::new(lines, baseline))
}

fn render_row(row: &Row<'_>) -> Result<Vec<Grid>, LatexError> {
    row.cells
        .iter()
        .map(|cell| render_body(&cell.body))
        .collect::<Result<Vec<_>, _>>()
}

fn hcat_all(parts: &[Grid]) -> Grid {
    parts.iter().fold(Grid::empty(), |acc, part| acc.hcat(part))
}

fn prefix_baseline(prefix: &str, rhs: &Grid) -> Grid {
    let prefix_width = UnicodeWidthStr::width(prefix);
    let mut lines = Vec::with_capacity(rhs.lines.len());
    for (idx, line) in rhs.lines.iter().enumerate() {
        if idx == rhs.baseline {
            lines.push(format!("{prefix}{line}"));
        } else {
            lines.push(format!("{}{line}", " ".repeat(prefix_width)));
        }
    }
    Grid::new(lines, rhs.baseline)
}

fn wrap_rows(left: &str, body: Grid, right: &str) -> Grid {
    if left.is_empty() && right.is_empty() {
        return body;
    }
    let lines = body
        .lines
        .iter()
        .map(|line| format!("{left}{line}{right}"))
        .collect::<Vec<_>>();
    Grid::new(lines, body.baseline)
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

fn first_parse_error(diagnostics: Vec<ParseDiagnostic>) -> LatexError {
    diagnostics.into_iter().next().map_or_else(
        || LatexError::new(LatexErrorKind::Syntax, SourceSpan::new(0, 0), "invalid TeX math"),
        |diagnostic| parse_error(&diagnostic),
    )
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

fn unsupported(span: SourceSpan, message: impl Into<String>) -> LatexError {
    LatexError::new(LatexErrorKind::Unsupported, span, message)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::literal_string_with_formatting_args,
        reason = "layout tests inspect concrete grid output"
    )]

    use super::*;

    fn text(source: &str) -> String {
        render_unicode_math(source).expect("math renders").as_text()
    }

    #[test]
    fn grid_constructor_normalises_ragged_lines() {
        let grid = Grid::new(vec!["x".to_owned(), "alpha".to_owned()], 9);

        assert_eq!(grid.width, 5);
        assert_eq!(grid.baseline, 1);
        assert_eq!(UnicodeWidthStr::width(grid.lines[0].as_str()), 5);
        assert_eq!(UnicodeWidthStr::width(grid.lines[1].as_str()), 5);
    }

    #[test]
    fn simple_symbols_and_scripts_render() {
        assert_eq!(text(r"\alpha_i"), "αᵢ");
        assert_eq!(text("x^{2}"), "x²");
        assert_eq!(text("x^{-1}"), "x⁻¹");
    }

    #[test]
    fn fractions_and_nested_fractions_render_with_stable_baselines() {
        let rendered = render_unicode_math(r"\frac{a}{b}").expect("fraction renders");
        assert_eq!(rendered.lines(), &["a".to_owned(), "─".to_owned(), "b".to_owned()]);
        assert_eq!(rendered.baseline(), 1);
        assert_eq!(rendered.width(), 1);

        let nested = render_unicode_math(r"\frac{\frac{a}{b}}{c}").expect("nested fraction renders");
        assert_eq!(
            nested.lines(),
            &[
                "a".to_owned(),
                "─".to_owned(),
                "b".to_owned(),
                "─".to_owned(),
                "c".to_owned()
            ]
        );
        assert_eq!(nested.baseline(), 3);
    }

    #[test]
    fn square_roots_and_root_degrees_render() {
        assert_eq!(text(r"\sqrt{x}"), "√x");
        assert_eq!(text(r"\sqrt[n]{x}"), "ⁿ√x");
    }

    #[test]
    fn accents_render_with_combining_marks() {
        assert_eq!(text(r"\hat{x}"), "x\u{302}");
        assert_eq!(text(r"\vec{v}"), "v\u{20d7}");
    }

    #[test]
    fn delimiters_render_around_body() {
        assert_eq!(text(r"\left( x \right)"), "(x)");
        assert_eq!(text(r"\left. x \right|"), "x|");
    }

    #[test]
    fn matrices_and_cases_render_as_grid_text() {
        let matrix = render_unicode_math(r"\begin{pmatrix}a & bb \\ c & d\end{pmatrix}").expect("matrix renders");
        assert_eq!(matrix.lines(), &["(a  bb)".to_owned(), "(c  d )".to_owned()]);
        assert_eq!(matrix.baseline(), 1);

        let cases = render_unicode_math(r"\begin{cases}x & y \\ z & w\end{cases}").expect("cases renders");
        assert_eq!(cases.lines(), &["{x  y".to_owned(), "{z  w".to_owned()]);
    }

    #[test]
    fn unsupported_constructs_return_typed_errors() {
        let err = render_unicode_math(r"\color{red}{x}").expect_err("color is unsupported");
        assert_eq!(err.kind(), &LatexErrorKind::Unsupported);

        let err = render_unicode_math(r"\frac{a}").expect_err("fraction is malformed");
        assert_eq!(err.kind(), &LatexErrorKind::Syntax);
    }
}
