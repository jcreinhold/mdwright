use anyhow::{Context, Result};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};

pub(crate) fn highlight_html(html: &str) -> Result<String> {
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let syntax = syntax_set
        .find_syntax_by_extension("html")
        .context("load HTML syntax")?;
    let theme = theme_set
        .themes
        .get("base16-ocean.dark")
        .or_else(|| theme_set.themes.values().next())
        .context("load syntax highlighting theme")?;
    highlight_with(html, syntax, theme, &syntax_set)
}

pub(crate) fn highlight_code(code: &str, language: &str) -> Option<String> {
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let syntax = syntax_set
        .find_syntax_by_token(language)
        .or_else(|| syntax_set.find_syntax_by_extension(language))?;
    let theme = theme_set
        .themes
        .get("base16-ocean.dark")
        .or_else(|| theme_set.themes.values().next())?;
    highlight_with(code, syntax, theme, &syntax_set).ok()
}

fn highlight_with(
    source: &str,
    syntax: &syntect::parsing::SyntaxReference,
    theme: &Theme,
    syntax_set: &SyntaxSet,
) -> Result<String> {
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut out = String::with_capacity(source.len());
    for line in LinesWithEndings::from(source) {
        let ranges = highlighter
            .highlight_line(line, syntax_set)
            .context("highlight terminal output")?;
        out.push_str(&as_24_bit_terminal_escaped(&ranges, false));
    }
    Ok(out)
}
