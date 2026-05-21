#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "parity fixtures should fail loudly with the exact fixture output"
)]

use mdwright_document::{Document, MathDelimiterSet, MathParseOptions, ParseOptions};
use mdwright_format::{FmtOptions, Wrap};

fn format_with_wrap(source: &str, wrap: u32) -> String {
    format_with_wrap_and_parse(source, wrap, ParseOptions::default())
}

fn format_with_wrap_and_parse(source: &str, wrap: u32, parse_options: ParseOptions) -> String {
    let doc = Document::parse_with_options(source, parse_options).expect("parity fixture parses");
    mdwright_format::format_document(&doc, &FmtOptions::default().with_wrap(Wrap::At(wrap)))
}

fn assert_wrap(source: &str, wrap: u32, expected: &str) {
    let got = format_with_wrap(source, wrap);
    assert_eq!(got, expected, "input:\n{source}\n--- got ---\n{got}");
}

fn assert_wrap_with_parse(source: &str, wrap: u32, parse_options: ParseOptions, expected: &str) {
    let got = format_with_wrap_and_parse(source, wrap, parse_options);
    assert_eq!(got, expected, "input:\n{source}\n--- got ---\n{got}");
}

#[test]
fn summary_nested_list_indentation_preserves_mdbook_style() {
    let source = "- [Catalogue](rules/index.md)\n  - [adjacent-code-no-space](rules/adjacent-code-no-space.md)\n  - [math/unbalanced-env](rules/math/unbalanced-env.md)\n";
    assert_wrap(source, 120, source);
}

#[test]
fn frontmatter_and_footnote_wrapping_stay_in_their_constructs() {
    let source = "---\ntitle: Parity fixture\n---\n\nThis paragraph has enough ordinary prose to wrap after the frontmatter without changing the frontmatter bytes.\n\n[^note]: This footnote definition has enough prose to wrap while staying inside the footnote definition body.\n";
    let expected = "---\ntitle: Parity fixture\n---\n\nThis paragraph has enough ordinary\nprose to wrap after the frontmatter\nwithout changing the frontmatter\nbytes.\n\n[^note]: This footnote definition\n    has enough prose to wrap while\n    staying inside the footnote\n    definition body.\n";
    assert_wrap(source, 36, expected);
}

#[test]
fn gfm_table_and_task_list_wrapping_preserve_structural_rows() {
    let source = "| Task | Done |\n| --- | --- |\n| keep table row intact | yes |\n\n- [x] This task list item contains enough prose to wrap without losing the task marker.\n";
    assert_wrap(source, 38, source);
}

#[test]
fn mkdocs_admonition_like_block_is_preserved_under_wrap() {
    let source = "!!! note\n    This admonition body is indented for the downstream renderer and should not be rewrapped as ordinary prose.\n";
    assert_wrap(source, 40, source);
}

#[test]
fn math_adjacent_prose_wrap_keeps_math_atomic() {
    let source = "This paragraph mentions \\(\\alpha_{i + 1}\\) right next to ordinary prose that should wrap without opening the math span.\n";
    let expected = "This paragraph mentions \\(\\alpha_{i + 1}\\)\nright next to ordinary prose that should\nwrap without opening the math span.\n";
    assert_wrap(source, 44, expected);
}

#[test]
fn dollar_math_adjacent_prose_wrap_keeps_math_atomic() {
    let source = "This paragraph mentions $(A_{\\alpha \\beta})_{(\\alpha,\\beta) \\in I \\times I}$ right next to ordinary prose that should wrap without opening the math span.\n";
    let expected = "This paragraph mentions\n$(A_{\\alpha \\beta})_{(\\alpha,\\beta) \\in I \\times I}$\nright next to ordinary prose that should\nwrap without opening the math span.\n";
    let parse_options = ParseOptions::default().with_math(MathParseOptions {
        delimiters: MathDelimiterSet::Github,
    });
    assert_wrap_with_parse(source, 44, parse_options, expected);
}

#[test]
fn dollar_math_is_plain_text_without_github_parse_policy() {
    let source = "This paragraph mentions $(A_{\\alpha \\beta})_{(\\alpha,\\beta) \\in I \\times I}$ right next to ordinary prose that should wrap without opening the math span.\n";
    let expected = "This paragraph mentions $(A_{\\alpha\n\\beta})_{(\\alpha,\\beta) \\in I \\times I}$\nright next to ordinary prose that should\nwrap without opening the math span.\n";
    assert_wrap(source, 44, expected);
}
