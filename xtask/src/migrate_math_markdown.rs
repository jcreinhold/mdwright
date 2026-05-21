use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use ignore::WalkBuilder;
use mdwright_document::{CodeBlock, Document, InlineCode};
use mdwright_latex::{Translation, TranslationStatus, translate_unicode_to_latex};
use serde::Serialize;
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

// Evidence is collected during migration planning, not by grepping the
// rendered diff. Keeping the report at this layer preserves document
// facts, target kinds, translation outcomes, and skip decisions; parsing
// unified diffs would discard those facts, while documented grep commands
// would keep the classification work manual and lossy.
#[derive(Clone, Debug, Serialize)]
struct MigrationEvidenceReport {
    command: String,
    root: String,
    mode: String,
    generated_unix_seconds: u64,
    markdown_files: usize,
    changed_files: usize,
    edit_count: usize,
    category_counts: Vec<CategoryCount>,
    files: Vec<EvidenceFile>,
    blockers: Vec<EvidenceItem>,
}

#[derive(Clone, Debug, Serialize)]
struct CategoryCount {
    class: EvidenceClass,
    count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct EvidenceFile {
    path: String,
    items: Vec<EvidenceItem>,
}

#[derive(Clone, Debug, Serialize)]
struct EvidenceItem {
    class: EvidenceClass,
    line: usize,
    byte_range: EvidenceRange,
    excerpt: String,
    detail: String,
}

#[derive(Clone, Debug, Serialize)]
struct EvidenceRange {
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
enum EvidenceClass {
    MigratedExistingMath,
    MigratedInlineCode,
    MigratedCodeBlock,
    SkippedInlineProseLike,
    SkippedInlineDiagramLike,
    SkippedInlineUnsupportedMathVisible,
    SkippedBlockDiagramLayout,
    SkippedBlockMixedProse,
    SurroundingProseHit,
    ExistingLatexPassthrough,
    TrueBlocker,
}

impl EvidenceClass {
    fn label(self) -> &'static str {
        match self {
            Self::MigratedExistingMath => "migrated existing math",
            Self::MigratedInlineCode => "migrated inline code",
            Self::MigratedCodeBlock => "migrated code block",
            Self::SkippedInlineProseLike => "skipped inline code: prose-like",
            Self::SkippedInlineDiagramLike => "skipped inline code: diagram-like",
            Self::SkippedInlineUnsupportedMathVisible => "skipped inline code: unsupported math remains visible",
            Self::SkippedBlockDiagramLayout => "skipped block: diagram/layout",
            Self::SkippedBlockMixedProse => "skipped block: mixed prose",
            Self::SurroundingProseHit => "surrounding prose hit",
            Self::ExistingLatexPassthrough => "existing LaTeX passthrough",
            Self::TrueBlocker => "true blocker",
        }
    }

    fn is_blocker(self) -> bool {
        self == Self::TrueBlocker
    }
}

#[derive(Default)]
struct EvidenceCollector {
    counts: BTreeMap<EvidenceClass, usize>,
    sample_counts: BTreeMap<EvidenceClass, usize>,
    files: BTreeMap<String, EvidenceFile>,
    blockers: Vec<EvidenceItem>,
}

impl EvidenceCollector {
    const MAX_BLOCKERS: usize = 100;
    const MAX_SAMPLES_PER_CLASS: usize = 8;

    fn record(
        &mut self,
        path: &Path,
        source: &str,
        range: Range<usize>,
        class: EvidenceClass,
        detail: impl Into<String>,
    ) {
        self.counts
            .entry(class)
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);

        let item = EvidenceItem {
            class,
            line: line_number(source, range.start),
            byte_range: EvidenceRange {
                start: range.start,
                end: range.end,
            },
            excerpt: line_excerpt(source, range.start),
            detail: detail.into(),
        };

        if class.is_blocker() && self.blockers.len() < Self::MAX_BLOCKERS {
            self.blockers.push(item.clone());
        }

        let sample_count = self.sample_counts.entry(class).or_insert(0);
        if *sample_count >= Self::MAX_SAMPLES_PER_CLASS {
            return;
        }
        *sample_count = sample_count.saturating_add(1);

        let path_label = path.display().to_string();
        self.files
            .entry(path_label.clone())
            .or_insert_with(|| EvidenceFile {
                path: path_label,
                items: Vec::new(),
            })
            .items
            .push(item);
    }

    fn into_report(
        self,
        root: &Path,
        mode: MigrateMode,
        markdown_files: usize,
        summary: &MigrationSummary,
    ) -> MigrationEvidenceReport {
        MigrationEvidenceReport {
            command: "cargo xtask migrate-math-markdown".to_owned(),
            root: root.display().to_string(),
            mode: mode.label().to_owned(),
            generated_unix_seconds: generated_unix_seconds(),
            markdown_files,
            changed_files: summary.changed_files,
            edit_count: summary.edit_count,
            category_counts: self
                .counts
                .into_iter()
                .map(|(class, count)| CategoryCount { class, count })
                .collect(),
            files: self.files.into_values().collect(),
            blockers: self.blockers,
        }
    }
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
    run_with_report(root, mode, None)
}

pub fn run_with_report(root: &Path, mode: MigrateMode, report_path: Option<&Path>) -> Result<MigrationSummary> {
    let files = markdown_files(root)?;
    let files_scanned = files.len();
    let mut summary = MigrationSummary {
        changed_files: 0,
        edit_count: 0,
    };
    let mut evidence = report_path.is_some().then(EvidenceCollector::default);
    let mut stdout = io::stdout().lock();

    for path in files {
        let source = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let migration = migrate_source_collecting_evidence(&source, &path, evidence.as_mut())
            .with_context(|| format!("migrate {}", path.display()))?;
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

    if let (Some(path), Some(evidence)) = (report_path, evidence) {
        let report = evidence.into_report(root, mode, files_scanned, &summary);
        write_evidence_reports(path, &report)?;
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
    migrate_source_collecting_evidence(source, Path::new("<memory>"), None)
}

fn migrate_source_collecting_evidence(
    source: &str,
    path: &Path,
    mut evidence: Option<&mut EvidenceCollector>,
) -> Result<SourceMigration> {
    let document = Document::parse(source)?;
    let mut targets = Vec::new();
    let mut occupied = Vec::new();
    let mut source_fact_ranges = Vec::new();

    for region in document.math_regions() {
        let range = region.range.clone();
        targets.push(MathMigrationTarget::existing_math_body(
            source,
            region.span().body().source_range(),
        )?);
        occupied.push(range);
        source_fact_ranges.push(region.range.clone());
    }

    for code in document.inline_codes() {
        source_fact_ranges.push(code.raw_range.clone());
        if overlaps_any(&code.raw_range, &occupied) {
            continue;
        }
        if is_inline_math_like(&code.text) {
            targets.push(inline_code_target(source, code)?);
        } else if let Some(class) = skipped_inline_class(&code.text)
            && let Some(collector) = evidence.as_deref_mut()
        {
            collector.record(
                path,
                source,
                code.raw_range.clone(),
                class,
                "inline code was not converted",
            );
        }
    }

    for block in document.code_blocks() {
        source_fact_ranges.push(block.raw_range.clone());
        if overlaps_any(&block.raw_range, &occupied) {
            continue;
        }
        if let Some(body) = code_block_body_range(source, block) {
            let body_text = source.get(body.clone()).unwrap_or("");
            if is_convertible_math_block(block, body_text) {
                targets.push(MathMigrationTarget::code_block_to_display_math(
                    source,
                    block.raw_range.clone(),
                    body,
                )?);
            } else if let Some(class) = skipped_block_class(block, body_text)
                && let Some(collector) = evidence.as_deref_mut()
            {
                collector.record(
                    path,
                    source,
                    block.raw_range.clone(),
                    class,
                    "code block was not converted",
                );
            }
        }
    }

    if let Some(collector) = evidence.as_deref_mut() {
        record_surrounding_prose_hits(source, path, &source_fact_ranges, collector);
    }

    migrate_targets_collecting_evidence(source, path, targets, evidence)
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

#[cfg(test)]
fn migrate_targets(source: &str, targets: Vec<MathMigrationTarget>) -> Result<SourceMigration> {
    migrate_targets_collecting_evidence(source, Path::new("<memory>"), targets, None)
}

fn migrate_targets_collecting_evidence(
    source: &str,
    path: &Path,
    targets: Vec<MathMigrationTarget>,
    mut evidence: Option<&mut EvidenceCollector>,
) -> Result<SourceMigration> {
    let targets = sorted_non_overlapping_targets(targets)?;
    let mut edits = Vec::new();
    for target in targets {
        if let Some(edit) = target_to_edit_collecting_evidence(source, path, &target, evidence.as_deref_mut())? {
            edits.push(edit);
        }
    }
    Ok(SourceMigration {
        output: apply_edits(source, &edits),
        edit_count: edits.len(),
    })
}

fn target_to_edit_collecting_evidence(
    source: &str,
    path: &Path,
    target: &MathMigrationTarget,
    mut evidence: Option<&mut EvidenceCollector>,
) -> Result<Option<Edit>> {
    match target {
        MathMigrationTarget::ExistingMathBody { body } => {
            let Some(text) = source.get(body.clone()) else {
                return Ok(None);
            };
            let translated = translate_unicode_to_latex(text);
            if translated.diagnostics().is_empty() && translated.text() != text {
                if let Some(collector) = evidence.as_deref_mut() {
                    collector.record(
                        path,
                        source,
                        body.clone(),
                        EvidenceClass::MigratedExistingMath,
                        translation_detail(&translated),
                    );
                    if !translated.text().is_ascii() || !matches!(translated.status(), TranslationStatus::Lossless) {
                        collector.record(
                            path,
                            source,
                            body.clone(),
                            EvidenceClass::TrueBlocker,
                            "existing math migration produced non-ASCII or lossy output",
                        );
                    }
                }
                Ok(Some(Edit {
                    range: body.clone(),
                    replacement: translated.text().to_owned(),
                }))
            } else {
                if let Some(collector) = evidence.as_deref_mut() {
                    if has_tex_command(text) {
                        collector.record(
                            path,
                            source,
                            body.clone(),
                            EvidenceClass::ExistingLatexPassthrough,
                            translation_detail(&translated),
                        );
                    } else if has_strong_math_signal(text) && !translated.diagnostics().is_empty() {
                        collector.record(
                            path,
                            source,
                            body.clone(),
                            EvidenceClass::TrueBlocker,
                            "existing math body contains unsupported Unicode or diagnostics",
                        );
                    }
                }
                Ok(None)
            }
        }
        MathMigrationTarget::InlineCodeToMath { raw, body } => {
            let Some(text) = source.get(body.clone()) else {
                return Ok(None);
            };
            let translated = translate_unicode_to_latex(text);
            if !translation_usable_for_conversion(&translated) {
                if let Some(collector) = evidence.as_deref_mut() {
                    let class = if has_diagram_layout(text) {
                        EvidenceClass::SkippedInlineDiagramLike
                    } else {
                        EvidenceClass::SkippedInlineUnsupportedMathVisible
                    };
                    collector.record(path, source, raw.clone(), class, translation_detail(&translated));
                }
                return Ok(None);
            }
            if let Some(collector) = evidence {
                collector.record(
                    path,
                    source,
                    raw.clone(),
                    EvidenceClass::MigratedInlineCode,
                    translation_detail(&translated),
                );
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
                if let Some(collector) = evidence.as_deref_mut() {
                    collector.record(
                        path,
                        source,
                        raw.clone(),
                        EvidenceClass::TrueBlocker,
                        translation_detail(&translated),
                    );
                }
                return Ok(None);
            }
            if let Some(collector) = evidence {
                collector.record(
                    path,
                    source,
                    raw.clone(),
                    EvidenceClass::MigratedCodeBlock,
                    translation_detail(&translated),
                );
            }
            Ok(Some(Edit {
                range: raw.clone(),
                replacement: display_math_replacement(raw_ends_with_newline(source, raw), translated.text()),
            }))
        }
    }
}

fn skipped_inline_class(text: &str) -> Option<EvidenceClass> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if has_diagram_layout(trimmed) {
        return Some(EvidenceClass::SkippedInlineDiagramLike);
    }
    if has_strong_math_signal(trimmed) || has_script_syntax(trimmed) || has_tex_command(trimmed) {
        return Some(EvidenceClass::SkippedInlineUnsupportedMathVisible);
    }
    if looks_prose_like(trimmed) || !trimmed.is_ascii() {
        return Some(EvidenceClass::SkippedInlineProseLike);
    }
    None
}

fn skipped_block_class(block: &CodeBlock, body: &str) -> Option<EvidenceClass> {
    if has_diagram_layout(body) {
        return Some(EvidenceClass::SkippedBlockDiagramLayout);
    }
    if block.fenced && !is_plain_code_info(&block.info) {
        return None;
    }
    if has_nonblank_line(body) && nonblank_lines(body).any(looks_prose_like) {
        return Some(EvidenceClass::SkippedBlockMixedProse);
    }
    None
}

fn translation_detail(translation: &Translation) -> String {
    format!(
        "status={:?}, diagnostics={}, output_ascii={}",
        translation.status(),
        translation.diagnostics().len(),
        translation.text().is_ascii()
    )
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
    if has_diagram_layout(body) {
        return false;
    }
    if block.fenced {
        if !is_plain_code_info(&block.info) {
            return false;
        }
        return has_nonblank_line(body) && nonblank_lines(body).all(is_math_like_line);
    }
    has_nonblank_line(body) && nonblank_lines(body).all(is_high_confidence_math_line)
}

fn has_diagram_layout(text: &str) -> bool {
    text.chars().any(is_diagram_layout_char) || has_long_ruler_run(text)
}

fn is_diagram_layout_char(ch: char) -> bool {
    matches!(ch, '\u{2500}'..='\u{257f}' | '↙' | '↘' | '↖' | '↗')
}

fn has_long_ruler_run(text: &str) -> bool {
    let mut run = 0usize;
    for ch in text.chars() {
        if matches!(ch, '-' | '=' | '─' | '━') {
            run = run.saturating_add(1);
            if run >= 4 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
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
        && translation.text().is_ascii()
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
        '′' | '″' | '‴' | '⁗' | '·' | '⥲' | '─' | '━' | '▸'
            | '\u{0370}'..='\u{03ff}'
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

fn record_surrounding_prose_hits(
    source: &str,
    path: &Path,
    excluded_ranges: &[Range<usize>],
    collector: &mut EvidenceCollector,
) {
    let mut ranges = excluded_ranges.to_vec();
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut range_index = 0usize;
    let mut lines_recorded = BTreeSet::new();

    for (byte, ch) in source.char_indices() {
        while ranges.get(range_index).is_some_and(|range| byte >= range.end) {
            range_index = range_index.saturating_add(1);
        }
        if ranges
            .get(range_index)
            .is_some_and(|range| byte >= range.start && byte < range.end)
        {
            continue;
        }
        if !is_strong_math_char(ch) {
            continue;
        }
        let line = line_number(source, byte);
        if lines_recorded.insert(line) {
            collector.record(
                path,
                source,
                byte..byte.saturating_add(ch.len_utf8()),
                EvidenceClass::SurroundingProseHit,
                "raw math-like Unicode remains outside math/code facts",
            );
        }
    }
}

fn write_evidence_reports(path: &Path, report: &MigrationEvidenceReport) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(path, markdown_report(report)).with_context(|| format!("write {}", path.display()))?;
    let json_path = if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        path.with_extension("md.json")
    } else {
        path.with_extension("json")
    };
    fs::write(
        &json_path,
        serde_json::to_string_pretty(report).context("serialize migration evidence report")?,
    )
    .with_context(|| format!("write {}", json_path.display()))?;
    Ok(())
}

fn markdown_report(report: &MigrationEvidenceReport) -> String {
    let mut out = String::from("# Math Markdown Migration Evidence\n\n");
    out.push_str(&format!("- Command: `{}`\n", report.command));
    out.push_str(&format!("- Root: `{}`\n", report.root));
    out.push_str(&format!("- Mode: `{}`\n", report.mode));
    out.push_str(&format!(
        "- Generated unix seconds: `{}`\n",
        report.generated_unix_seconds
    ));
    out.push_str(&format!("- Markdown files scanned: `{}`\n", report.markdown_files));
    out.push_str(&format!("- Changed files: `{}`\n", report.changed_files));
    out.push_str(&format!("- Edit count: `{}`\n\n", report.edit_count));

    out.push_str("## Category Counts\n\n");
    out.push_str("| Class | Count |\n| --- | ---: |\n");
    for count in &report.category_counts {
        out.push_str(&format!("| {} | {} |\n", count.class.label(), count.count));
    }
    out.push('\n');

    out.push_str("## True Blockers\n\n");
    if report.blockers.is_empty() {
        out.push_str("No true blockers were classified.\n\n");
    } else {
        for item in &report.blockers {
            out.push_str(&format!(
                "- line `{}` bytes `{}..{}`: {} — `{}`\n",
                item.line,
                item.byte_range.start,
                item.byte_range.end,
                item.detail,
                escaped_inline_code(&item.excerpt)
            ));
        }
        out.push('\n');
    }

    out.push_str("## Samples\n\n");
    if report.files.is_empty() {
        out.push_str("No evidence samples were collected.\n");
    }
    for file in &report.files {
        out.push_str(&format!("### `{}`\n\n", file.path));
        for item in &file.items {
            out.push_str(&format!(
                "- {} at line `{}` bytes `{}..{}`: {} — `{}`\n",
                item.class.label(),
                item.line,
                item.byte_range.start,
                item.byte_range.end,
                item.detail,
                escaped_inline_code(&item.excerpt)
            ));
        }
        out.push('\n');
    }
    out
}

fn escaped_inline_code(text: &str) -> String {
    text.replace('`', "\\`")
}

fn generated_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn line_number(source: &str, byte: usize) -> usize {
    source
        .get(..byte.min(source.len()))
        .unwrap_or("")
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        .saturating_add(1)
}

fn line_excerpt(source: &str, byte: usize) -> String {
    let byte = byte.min(source.len());
    let start = source
        .get(..byte)
        .and_then(|prefix| prefix.rfind('\n').map(|idx| idx.saturating_add(1)))
        .unwrap_or(0);
    let end = source
        .get(byte..)
        .and_then(|suffix| suffix.find('\n').map(|idx| byte.saturating_add(idx)))
        .unwrap_or(source.len());
    source.get(start..end).unwrap_or("").trim().chars().take(160).collect()
}

impl MigrateMode {
    fn label(self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::Write => "write",
            Self::Check => "check",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence_report_for_source(source: &str) -> Result<MigrationEvidenceReport> {
        let mut collector = EvidenceCollector::default();
        let migration = migrate_source_collecting_evidence(source, Path::new("sample.md"), Some(&mut collector))?;
        let summary = MigrationSummary {
            changed_files: usize::from(migration.output != source),
            edit_count: migration.edit_count,
        };
        Ok(collector.into_report(Path::new("sample.md"), MigrateMode::Check, 1, &summary))
    }

    fn category_count(report: &MigrationEvidenceReport, class: EvidenceClass) -> usize {
        report
            .category_counts
            .iter()
            .find(|count| count.class == class)
            .map_or(0, |count| count.count)
    }

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
    fn inline_code_math_uses_canonical_operator_words() -> Result<()> {
        let source = "Use `Hom(A, ℤ)` and `log(q) ≤ 0`.\n";
        let migrated = migrate_source(source)?;
        assert_eq!(
            migrated.output,
            r"Use \(\operatorname{Hom}(A, \mathbb{Z})\) and \(\log(q) \leq 0\).".to_owned() + "\n"
        );
        assert_eq!(migrated.edit_count, 2);
        Ok(())
    }

    #[test]
    fn existing_math_body_translation_preserves_delimiters() -> Result<()> {
        let source = r"Let \[αᵢ P^∨/S\] hold.".to_owned() + "\n";
        let migrated = migrate_source(&source)?;
        assert_eq!(
            migrated.output,
            r"Let \[\alpha_{i} P^{\vee}/S\] hold.".to_owned() + "\n"
        );
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
        let source = "Before\n```text\nαᵢ ≤ x²\nA ⥲ B\n```\nAfter\n";
        let migrated = migrate_source(source)?;
        assert_eq!(
            migrated.output,
            "Before\n\\[\n\\alpha_{i} \\leq x^{2}\nA \\xrightarrow{\\sim} B\n\\]\nAfter\n"
        );
        assert_eq!(migrated.edit_count, 1);
        Ok(())
    }

    #[test]
    fn diagram_like_plain_code_blocks_are_skipped() -> Result<()> {
        let source =
            "```text\n                  S_α⁻¹A\n                ρ_α ↙ ↘ φ_α\n              A′ ────φ──── S⁻¹A\n```\n";
        let migrated = migrate_source(source)?;
        assert_eq!(migrated.output, source);
        assert_eq!(migrated.edit_count, 0);
        Ok(())
    }

    #[test]
    fn code_blocks_with_unknown_unicode_math_are_skipped() -> Result<()> {
        let source = "```text\nA ⥪ B\n```\n";
        let migrated = migrate_source(source)?;
        assert_eq!(migrated.output, source);
        assert_eq!(migrated.edit_count, 0);
        Ok(())
    }

    #[test]
    fn inline_conversion_uses_canonical_latex_output() -> Result<()> {
        let source = "Use `𝒪_X`, `A′`, and `C ──g──▸ E ──▸ E/L`.\n";
        let migrated = migrate_source(source)?;
        assert_eq!(
            migrated.output,
            r"Use \(\mathcal{O}_{X}\), \(A'\), and \(C \xrightarrow{g} E \to E/L\).".to_owned() + "\n"
        );
        assert_eq!(migrated.edit_count, 3);
        Ok(())
    }

    #[test]
    fn inline_conversion_uses_extended_unicode_normalization() -> Result<()> {
        let source = "Use `lim⃗ Mₙ`, `𝚪_*`, `𝐟𝐠`, and `f♯ ⊠ g♭`.\n";
        let migrated = migrate_source(source)?;
        assert_eq!(
            migrated.output,
            r"Use \(\varinjlim M_{n}\), \(\Gamma_{*}\), \(\mathbf{fg}\), and \(f\sharp \boxtimes g\flat\).".to_owned()
                + "\n"
        );
        assert_eq!(migrated.edit_count, 4);
        Ok(())
    }

    #[test]
    fn formula_only_blocks_with_extended_unicode_convert_to_latex() -> Result<()> {
        let source = concat!(
            "```text\nM̃ ⟺ 𝓗𝓸𝓶(A, B)\nA ↔ B\nD₊ ⨁ □\n𝒟ℯ𝓇(X) ",
            "\u{227A}\u{0338}",
            " Y ",
            "\u{2A7D}\u{0338}",
            " Z\n```\n"
        );
        let migrated = migrate_source(source)?;
        assert_eq!(
            migrated.output,
            "\\[\n\\tilde{M} \\Longleftrightarrow \\operatorname{Hom}(A, B)\nA \\leftrightarrow B\nD_{+} \\bigoplus \\square\n\\operatorname{Der}(X) \\nprec Y \\nleqslant Z\n\\]\n"
        );
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
    fn evidence_classifies_converted_inline_code() -> Result<()> {
        let report = evidence_report_for_source("Let `αᵢ ≤ x²` hold.\n")?;
        assert_eq!(category_count(&report, EvidenceClass::MigratedInlineCode), 1);
        assert_eq!(category_count(&report, EvidenceClass::TrueBlocker), 0);
        Ok(())
    }

    #[test]
    fn evidence_classifies_skipped_prose_inline_code() -> Result<()> {
        let report = evidence_report_for_source("Use `préschéma` here.\n")?;
        assert_eq!(category_count(&report, EvidenceClass::SkippedInlineProseLike), 1);
        assert_eq!(report.changed_files, 0);
        Ok(())
    }

    #[test]
    fn evidence_classifies_skipped_diagram_inline_code() -> Result<()> {
        let report = evidence_report_for_source("Map `A ↙ B` here.\n")?;
        assert_eq!(category_count(&report, EvidenceClass::SkippedInlineDiagramLike), 1);
        assert_eq!(report.changed_files, 0);
        Ok(())
    }

    #[test]
    fn evidence_classifies_converted_plain_code_block() -> Result<()> {
        let report = evidence_report_for_source("```text\nαᵢ ≤ x²\n```\n")?;
        assert_eq!(category_count(&report, EvidenceClass::MigratedCodeBlock), 1);
        assert_eq!(report.edit_count, 1);
        Ok(())
    }

    #[test]
    fn evidence_classifies_mixed_prose_block() -> Result<()> {
        let report = evidence_report_for_source("```text\nαᵢ ≤ x²\nthis is a prose sentence about math\n```\n")?;
        assert_eq!(category_count(&report, EvidenceClass::SkippedBlockMixedProse), 1);
        assert_eq!(report.changed_files, 0);
        Ok(())
    }

    #[test]
    fn evidence_keeps_surrounding_unicode_out_of_blockers() -> Result<()> {
        let report = evidence_report_for_source("The symbol α appears in prose.\n")?;
        assert_eq!(category_count(&report, EvidenceClass::SurroundingProseHit), 1);
        assert_eq!(category_count(&report, EvidenceClass::TrueBlocker), 0);
        Ok(())
    }

    #[test]
    fn evidence_classifies_unsupported_visible_inline_math() -> Result<()> {
        let report = evidence_report_for_source("Use `A ⧟ B` here.\n")?;
        assert_eq!(
            category_count(&report, EvidenceClass::SkippedInlineUnsupportedMathVisible),
            1
        );
        assert_eq!(category_count(&report, EvidenceClass::TrueBlocker), 0);
        Ok(())
    }

    #[test]
    fn evidence_report_writes_markdown_and_json() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("corpus");
        fs::create_dir(&root)?;
        fs::write(root.join("a.md"), "Let `αᵢ ≤ x²` hold.\n")?;
        let report_path = dir.path().join("evidence.md");

        let summary = run_with_report(&root, MigrateMode::Check, Some(&report_path))?;

        assert_eq!(summary.changed_files, 1);
        let markdown = fs::read_to_string(&report_path)?;
        assert!(markdown.contains("migrated inline code"));
        let json = fs::read_to_string(dir.path().join("evidence.json"))?;
        assert!(json.contains("migrated-inline-code"));
        Ok(())
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
