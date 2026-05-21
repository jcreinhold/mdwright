/// Unicode layout output for a TeX math body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedLatex {
    lines: Vec<String>,
    baseline: usize,
    width: usize,
}

impl RenderedLatex {
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
