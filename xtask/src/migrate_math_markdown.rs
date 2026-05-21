use std::fs;
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ignore::WalkBuilder;
use mdwright_document::{CodeBlock, Document, InlineCode};
use mdwright_latex::{Translation, TranslationStatus, translate_unicode_to_latex};
use similar::TextDiff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrateMode {
    Diff,
    Write,
    Check,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationSummary {
    pub changed_files: usize,
    pub edit_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMigration {
    pub output: String,
    pub edit_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    range: Range<usize>,
    replacement: String,
}

enum MathMigrationTarget {
    ExistingMathBody { body: Range<usize> },
    InlineCodeToMath { raw: Range<usize>, body: Range<usize> },
    CodeBlockToDisplayMath { raw: Range<usize>, body: Range<usize> },
}

impl MathMigrationTarget {
    fn existing_math_body(source: &str, body: Range<usize>) -> Result<Self> {
        ensure_valid_range(source, &body)?;
        Ok(Self::ExistingMathBody { body })
    }

    fn inline_code_to_math(source: &str, raw: Range<usize>, body: Range<usize>) -> Result<Self> {
        ensure_valid_range(source, &raw)?;
        ensure_valid_range(source, &body)?;
        ensure_contained(&raw, &body)?;
        Ok(Self::InlineCodeToMath { raw, body })
    }

    fn code_block_to_display_math(source: &str, raw: Range<usize>, body: Range<usize>) -> Result<Self> {
        ensure_valid_range(source, &raw)?;
        ensure_valid_range(source, &body)?;
        ensure_contained(&raw, &body)?;
        Ok(Self::CodeBlockToDisplayMath { raw, body })
    }

    fn edit_range(&self) -> Range<usize> {
        match self {
            Self::ExistingMathBody { body } => body.clone(),
            Self::InlineCodeToMath { raw, .. } | Self::CodeBlockToDisplayMath { raw, .. } => raw.clone(),
        }
    }
}

pub fn run(root: &Path, mode: MigrateMode) -> Result<MigrationSummary> {
    let files = markdown_files(root)?;
    let mut summary = MigrationSummary {
        changed_files: 0,
        edit_count: 0,
    };
    let mut stdout = io::stdout().lock();

    for path in files {
        let source = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let migration = migrate_source(&source).with_context(|| format!("migrate {}", path.display()))?;
        if migration.output == source {
            continue;
        }
        summary.changed_files = summary.changed_files.saturating_add(1);
        summary.edit_count = summary.edit_count.saturating_add(migration.edit_count);
        match mode {
            MigrateMode::Diff => {
                write_unified_diff(&mut stdout, &path.display().to_string(), &source, &migration.output)?;
            }
            MigrateMode::Write => {
                fs::write(&path, migration.output).with_context(|| format!("write {}", path.display()))?;
            }
            MigrateMode::Check => {}
        }
    }

    match mode {
        MigrateMode::Write if summary.changed_files > 0 => {
            eprintln!(
                "xtask: migrated {} file(s), {} edit(s)",
                summary.changed_files, summary.edit_count
            );
        }
        MigrateMode::Diff | MigrateMode::Check if summary.changed_files > 0 => {
            eprintln!(
                "xtask: {} file(s) would be migrated ({} edit(s))",
                summary.changed_files, summary.edit_count
            );
        }
        _ => {}
    }
    Ok(summary)
}

pub fn migrate_source(source: &str) -> Result<SourceMigration> {
    let document = Document::parse(source)?;
    let mut targets = Vec::new();
    let mut occupied = Vec::new();

    for region in document.math_regions() {
        let range = region.range.clone();
        targets.push(MathMigrationTarget::existing_math_body(
            source,
            region.span().body().source_range(),
        )?);
        occupied.push(range);
    }

    for code in document.inline_codes() {
        if is_inline_math_like(&code.text) && !overlaps_any(&code.raw_range, &occupied) {
            targets.push(inline_code_target(source, code)?);
        }
    }

    for block in document.code_blocks() {
        if let Some(body) = code_block_body_range(source, block)
            && is_convertible_math_block(block, source.get(body.clone()).unwrap_or(""))
            && !overlaps_any(&block.raw_range, &occupied)
        {
            targets.push(MathMigrationTarget::code_block_to_display_math(
                source,
                block.raw_range.clone(),
                body,
            )?);
        }
    }

    migrate_targets(source, targets)
}

fn overlaps_any(range: &Range<usize>, occupied: &[Range<usize>]) -> bool {
    occupied
        .iter()
        .any(|other| range.start < other.end && other.start < range.end)
}

fn inline_code_target(source: &str, code: &InlineCode) -> Result<MathMigrationTarget> {
    let body = code.byte_offset..code.byte_offset.saturating_add(code.text.len());
    MathMigrationTarget::inline_code_to_math(source, code.raw_range.clone(), body)
}

fn migrate_targets(source: &str, targets: Vec<MathMigrationTarget>) -> Result<SourceMigration> {
    let targets = sorted_non_overlapping_targets(targets)?;
    let mut edits = Vec::new();
    for target in targets {
        if let Some(edit) = target_to_edit(source, &target)? {
            edits.push(edit);
        }
    }
    Ok(SourceMigration {
        output: apply_edits(source, &edits),
        edit_count: edits.len(),
    })
}

fn target_to_edit(source: &str, target: &MathMigrationTarget) -> Result<Option<Edit>> {
    match target {
        MathMigrationTarget::ExistingMathBody { body } => {
            let Some(text) = source.get(body.clone()) else {
                return Ok(None);
            };
            let translated = translate_unicode_to_latex(text);
            if translated.diagnostics().is_empty() && translated.text() != text {
                Ok(Some(Edit {
                    range: body.clone(),
                    replacement: translated.text().to_owned(),
                }))
            } else {
                Ok(None)
            }
        }
        MathMigrationTarget::InlineCodeToMath { raw, body } => {
            let Some(text) = source.get(body.clone()) else {
                return Ok(None);
            };
            let translated = translate_unicode_to_latex(text);
            if !translation_usable_for_conversion(&translated) {
                return Ok(None);
            }
            Ok(Some(Edit {
                range: raw.clone(),
                replacement: format!(r"\({}\)", translated.text()),
            }))
        }
        MathMigrationTarget::CodeBlockToDisplayMath { raw, body } => {
            let Some(text) = source.get(body.clone()) else {
                return Ok(None);
            };
            let translated = translate_unicode_to_latex(text);
            if !translation_usable_for_conversion(&translated) {
                return Ok(None);
            }
            Ok(Some(Edit {
                range: raw.clone(),
                replacement: display_math_replacement(raw_ends_with_newline(source, raw), translated.text()),
            }))
        }
    }
}

fn sorted_non_overlapping_targets(mut targets: Vec<MathMigrationTarget>) -> Result<Vec<MathMigrationTarget>> {
    targets.sort_by_key(|target| {
        let range = target.edit_range();
        (range.start, range.end)
    });
    let mut previous: Option<Range<usize>> = None;
    for target in &targets {
        let range = target.edit_range();
        if let Some(prev) = &previous
            && range.start < prev.end
        {
            bail!(
                "math migration targets overlap: {}..{} overlaps {}..{}",
                range.start,
                range.end,
                prev.start,
                prev.end
            );
        }
        previous = Some(range);
    }
    Ok(targets)
}

fn ensure_valid_range(source: &str, range: &Range<usize>) -> Result<()> {
    if range.start > range.end
        || range.end > source.len()
        || !source.is_char_boundary(range.start)
        || !source.is_char_boundary(range.end)
    {
        bail!("invalid UTF-8 range {}..{} for source", range.start, range.end);
    }
    Ok(())
}

fn ensure_contained(outer: &Range<usize>, inner: &Range<usize>) -> Result<()> {
    if inner.start < outer.start || inner.end > outer.end {
        bail!(
            "math migration body {}..{} is not contained in raw range {}..{}",
            inner.start,
            inner.end,
            outer.start,
            outer.end
        );
    }
    Ok(())
}

fn apply_edits(source: &str, edits: &[Edit]) -> String {
    if edits.is_empty() {
        return source.to_owned();
    }
    let mut out = source.to_owned();
    for edit in edits.iter().rev() {
        out.replace_range(edit.range.clone(), &edit.replacement);
    }
    out
}

fn is_convertible_math_block(block: &CodeBlock, body: &str) -> bool {
    if block.fenced {
        if !is_plain_code_info(&block.info) {
            return false;
        }
        return has_nonblank_line(body) && nonblank_lines(body).all(is_math_like_line);
    }
    has_nonblank_line(body) && nonblank_lines(body).all(is_high_confidence_math_line)
}

fn code_block_body_range(source: &str, block: &CodeBlock) -> Option<Range<usize>> {
    if !block.fenced {
        return Some(block.raw_range.clone());
    }
    let raw = source.get(block.raw_range.clone())?;
    let body_start_in_raw = raw.find('\n')?.saturating_add(1);
    let closing_start_in_raw = raw.rfind("\n```").or_else(|| raw.rfind("\n~~~"))?.saturating_add(1);
    if body_start_in_raw > closing_start_in_raw {
        return None;
    }
    Some(
        block.raw_range.start.saturating_add(body_start_in_raw)
            ..block.raw_range.start.saturating_add(closing_start_in_raw),
    )
}

fn is_plain_code_info(info: &str) -> bool {
    let language = info.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    matches!(language.as_str(), "" | "text" | "txt" | "plain" | "plaintext")
}

fn nonblank_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|line| !line.is_empty())
}

fn has_nonblank_line(text: &str) -> bool {
    nonblank_lines(text).next().is_some()
}

fn is_inline_math_like(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || looks_like_short_label(trimmed) {
        return false;
    }
    if has_strong_math_signal(trimmed) {
        return true;
    }
    if has_tex_command(trimmed) {
        return translation_changes_without_diagnostics(trimmed);
    }
    has_script_syntax(trimmed) && !looks_prose_like(trimmed)
}

fn is_math_like_line(line: &str) -> bool {
    is_high_confidence_math_line(line) || (has_script_syntax(line) && !looks_prose_like(line))
}

fn is_high_confidence_math_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && !looks_like_short_label(trimmed)
        && (has_strong_math_signal(trimmed)
            || has_tex_command(trimmed)
            || translation_changes_without_diagnostics(trimmed))
        && !looks_prose_like(trimmed)
}

fn translation_changes_without_diagnostics(text: &str) -> bool {
    let translated = translate_unicode_to_latex(text);
    translated.diagnostics().is_empty() && translated.text() != text
}

fn translation_usable_for_conversion(translation: &Translation) -> bool {
    translation.diagnostics().is_empty()
        && matches!(
            translation.status(),
            TranslationStatus::Lossless | TranslationStatus::Unchanged
        )
}

fn has_tex_command(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek().is_some_and(char::is_ascii_alphabetic) {
            return true;
        }
    }
    false
}

fn has_script_syntax(text: &str) -> bool {
    (text.contains('_') || text.contains('^')) && text.chars().any(|ch| ch.is_alphabetic())
}

fn has_strong_math_signal(text: &str) -> bool {
    text.chars().any(is_strong_math_char)
}

fn is_strong_math_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{0370}'..='\u{03ff}'
            | '\u{2070}'..='\u{209f}'
            | '\u{2100}'..='\u{214f}'
            | '\u{2190}'..='\u{22ff}'
            | '\u{27c0}'..='\u{27ff}'
            | '\u{2980}'..='\u{29ff}'
            | '\u{2a00}'..='\u{2aff}'
            | '\u{1d400}'..='\u{1d7ff}'
    )
}

fn looks_like_short_label(text: &str) -> bool {
    let bare = text.trim_matches(|ch: char| matches!(ch, '(' | ')' | '[' | ']'));
    !bare.is_empty()
        && bare.len() <= 4
        && bare
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
}

fn looks_prose_like(text: &str) -> bool {
    let word_count = text
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphabetic))
        .count();
    if word_count >= 4 {
        return true;
    }
    let mut run = 0usize;
    for ch in text.chars() {
        if ch.is_alphabetic() && !is_strong_math_char(ch) {
            run = run.saturating_add(1);
            if run >= 9 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn display_math_replacement(raw_had_final_newline: bool, body: &str) -> String {
    let trimmed = body.trim_matches(|ch| matches!(ch, '\n' | '\r'));
    let mut out = String::from("\\[\n");
    out.push_str(trimmed);
    out.push_str("\n\\]");
    if raw_had_final_newline {
        out.push('\n');
    }
    out
}

fn raw_ends_with_newline(source: &str, range: &Range<usize>) -> bool {
    source
        .get(range.clone())
        .is_some_and(|raw| raw.ends_with('\n') || raw.ends_with("\r\n"))
}

fn markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if root.is_file() {
        if is_markdown_path(root) {
            paths.push(root.to_path_buf());
        }
        return Ok(paths);
    }
    for entry in WalkBuilder::new(root).build() {
        let entry = entry.with_context(|| format!("walk {}", root.display()))?;
        if entry.file_type().is_some_and(|ty| ty.is_file()) && is_markdown_path(entry.path()) {
            paths.push(entry.path().to_path_buf());
        }
    }
    paths.sort();
    Ok(paths)
}

fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown" | "mdown"))
}

fn write_unified_diff<W: Write>(out: &mut W, path: &str, old: &str, new: &str) -> Result<()> {
    let diff = TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string();
    out.write_all(diff.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_code_math_converts_to_inline_latex_math() -> Result<()> {
        let source = "Let `αᵢ ≤ x²` hold.\n";
        let migrated = migrate_source(source)?;
        assert_eq!(
            migrated.output,
            r"Let \(\alpha_{i} \leq x^{2}\) hold.".to_owned() + "\n"
        );
        assert_eq!(migrated.edit_count, 1);
        Ok(())
    }

    #[test]
    fn existing_math_body_translation_preserves_delimiters() -> Result<()> {
        let source = r"Let \[αᵢ P^∨/S\] hold.".to_owned() + "\n";
        let migrated = migrate_source(&source)?;
        assert_eq!(migrated.output, r"Let \[\alpha_{i} P^\vee/S\] hold.".to_owned() + "\n");
        assert_eq!(migrated.edit_count, 1);
        Ok(())
    }

    #[test]
    fn non_math_inline_code_remains_unchanged() -> Result<()> {
        let source = "Use `préschéma` and `(TF)` here.\n";
        let migrated = migrate_source(source)?;
        assert_eq!(migrated.output, source);
        assert_eq!(migrated.edit_count, 0);
        Ok(())
    }

    #[test]
    fn plain_fenced_math_code_block_becomes_display_math() -> Result<()> {
        let source = "Before\n```text\nαᵢ ≤ x²\n```\nAfter\n";
        let migrated = migrate_source(source)?;
        assert_eq!(migrated.output, "Before\n\\[\n\\alpha_{i} \\leq x^{2}\n\\]\nAfter\n");
        assert_eq!(migrated.edit_count, 1);
        Ok(())
    }

    #[test]
    fn code_blocks_with_language_tags_are_skipped() -> Result<()> {
        let source = "```rust\nlet x = 1;\n```\n```lean\n#check Nat\n```\n```python\nx = 1\n```\n";
        let migrated = migrate_source(source)?;
        assert_eq!(migrated.output, source);
        assert_eq!(migrated.edit_count, 0);
        Ok(())
    }

    #[test]
    fn mixed_prose_code_blocks_are_skipped() -> Result<()> {
        let source = "```text\nαᵢ ≤ x²\nthis is a prose sentence about math\n```\n";
        let migrated = migrate_source(source)?;
        assert_eq!(migrated.output, source);
        assert_eq!(migrated.edit_count, 0);
        Ok(())
    }

    #[test]
    fn overlapping_targets_are_rejected_before_edits_apply() {
        let source = "`αᵢ ≤ x²`\n";
        let targets = vec![
            MathMigrationTarget::inline_code_to_math(source, 0..12, 1..11).expect("valid first target"),
            MathMigrationTarget::existing_math_body(source, 3..6).expect("valid second target"),
        ];
        let err = migrate_targets(source, targets).expect_err("overlap should be rejected");
        assert!(err.to_string().contains("overlap"));
    }

    #[test]
    fn indented_code_requires_high_confidence_math_lines() -> Result<()> {
        let source = "    αᵢ ≤ x²\n\n    ordinary prose here\n";
        let migrated = migrate_source(source)?;
        assert_eq!(migrated.output, source);
        assert_eq!(migrated.edit_count, 0);
        Ok(())
    }
}
