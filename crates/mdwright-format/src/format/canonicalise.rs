//! Style canonicalisation: opt-in byte-to-byte rewrites of structural
//! output.
//!
//! # Contract
//!
//! This module recognises opt-in style edits for one rewrite family at
//! a time and submits parse-owned edits to the rewrite-family pipeline. It does
//! not edit the formatter buffer directly.
//!
//! # Why a separate pass
//!
//! Structural emit ([`crate::format::document::format_document`]) is
//! pure source-byte preservation, so the structural pipeline is
//! idempotent and perturbation-free by construction. Style
//! canonicalisation is the opposite concern: deliberately rewrite
//! source bytes per user preference. Keeping it in its own pass
//! localises the perturbation: the rewrite-family pipeline owns family
//! order, local-overlap rejection, verification, and commit.
//!
//! # Performance
//!
//! Table-free default config triggers the early-out in
//! `format_document_with_report`, so structural callers pay zero. With
//! canonicalisation enabled, the rewrite-family pipeline reparses after
//! committed family plans so later families see fresh document facts. A
//! family that cannot produce a verified local plan skips instead of
//! committing partial progress.
//!
//! Family order lives in the rewrite-family pipeline. This module owns
//! only the style-specific policy for constructing a family's edits.

use crate::format::rewrite::{Candidate, OwnerKind, RewriteFamily, Snapshot, Verification};
use crate::{FmtOptions, HeadingAttrsStyle, LinkDefStyle, MathRender, OrderedListStyle, TableStyle, ThematicStyle};
use mdwright_document::{InlineDelimiterKind, TableAlign, TableSite};
use mdwright_math::MathRegion;
use mdwright_math::MathSpan;
use mdwright_math::normalise::{align_env_body, body_braces_balanced};
use mdwright_math::render::convert_for_dollar;

pub(crate) fn collect_family_candidates(
    snapshot: &Snapshot<'_>,
    opts: &FmtOptions,
    family: RewriteFamily,
    candidates: &mut Vec<Candidate>,
) {
    match family {
        RewriteFamily::Italic => {
            if let Some(target) = opts.italic_target_byte() {
                collect_emphasis_delim(snapshot, EmphasisKind::Italic, target, candidates);
            }
        }
        RewriteFamily::Strong => {
            if let Some(target) = opts.strong_target_byte() {
                collect_emphasis_delim(snapshot, EmphasisKind::Strong, target, candidates);
            }
        }
        RewriteFamily::UnorderedList => {
            if let Some(target) = opts.list_marker_target_byte() {
                collect_unordered_list_marker(snapshot, target, candidates);
            }
        }
        RewriteFamily::OrderedList => {
            if let Some(target) = opts.ordered_list_target() {
                collect_ordered_list_renumber(snapshot, target, candidates);
            }
        }
        RewriteFamily::ThematicBreak => {
            if let Some(target) = opts.thematic_target() {
                collect_thematic(snapshot, target, candidates);
            }
        }
        RewriteFamily::Table => {
            if opts.should_normalise_tables() {
                collect_table_normal_form(snapshot, opts.table(), candidates);
            }
        }
        RewriteFamily::LinkDestination => {
            if let Some(target) = opts.link_def_target() {
                collect_link_def_style(snapshot, target, candidates);
            }
        }
        RewriteFamily::HeadingAttrs => {
            if matches!(opts.heading_attrs(), HeadingAttrsStyle::Canonicalise) {
                collect_heading_attrs(snapshot, candidates);
            }
        }
        RewriteFamily::Math => {
            if needs_math_rewrite(opts) {
                collect_math(snapshot, opts, candidates);
            }
        }
        RewriteFamily::Frontmatter => {
            if !opts.preserve_frontmatter() {
                collect_strip_frontmatter(snapshot, candidates);
            }
        }
    }
}

fn needs_math_rewrite(opts: &FmtOptions) -> bool {
    matches!(opts.math().render, MathRender::Dollar) || opts.math().normalise
}

fn push_utf8_candidate(
    snapshot: &Snapshot<'_>,
    owner: OwnerKind,
    range: std::ops::Range<usize>,
    rewrite: Vec<u8>,
    verification: Verification,
    label: &'static str,
    candidates: &mut Vec<Candidate>,
) {
    let Ok(replacement) = String::from_utf8(rewrite) else {
        return;
    };
    if let Some(candidate) = snapshot.candidate(owner, range, replacement, verification, label) {
        candidates.push(candidate);
    }
}

// ----- Emphasis (italic + strong) -------------------------------

#[derive(Copy, Clone, Debug)]
enum EmphasisKind {
    Italic,
    Strong,
}

impl EmphasisKind {
    fn delim_len(self) -> usize {
        match self {
            Self::Italic => 1,
            Self::Strong => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Italic => "italic",
            Self::Strong => "strong",
        }
    }

    fn document_kind(self) -> InlineDelimiterKind {
        match self {
            Self::Italic => InlineDelimiterKind::Emphasis,
            Self::Strong => InlineDelimiterKind::Strong,
        }
    }
}

fn collect_emphasis_delim(snapshot: &Snapshot<'_>, kind: EmphasisKind, target: u8, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let slots = snapshot.document().inline_delimiter_slots(kind.document_kind());
    let delim_len = kind.delim_len();
    let replacement = std::iter::repeat_n(target, delim_len).collect::<Vec<_>>();

    for slot in slots {
        let bytes = out.as_bytes();
        let open_range = slot.open_range();
        let Some(open) = bytes.get(open_range.clone()) else {
            continue;
        };
        let close_range = slot.close_range();
        let Some(close) = bytes.get(close_range.clone()) else {
            continue;
        };

        if !open.iter().all(|&b| b == target) {
            push_utf8_candidate(
                snapshot,
                OwnerKind::InlineDelimiterSlot,
                open_range,
                replacement.clone(),
                Verification::PreserveMarkdownAndMath,
                kind.label(),
                candidates,
            );
        }
        if !close.iter().all(|&b| b == target) {
            push_utf8_candidate(
                snapshot,
                OwnerKind::InlineDelimiterSlot,
                close_range,
                replacement.clone(),
                Verification::PreserveMarkdownAndMath,
                kind.label(),
                candidates,
            );
        }
    }
}

// ----- Unordered list bullet rewrite ----------------------------

fn collect_unordered_list_marker(snapshot: &Snapshot<'_>, target: u8, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let bytes = out.as_bytes();
    for marker in snapshot.document().unordered_list_marker_sites() {
        let range = marker.marker_range();
        if bytes.get(range.start).copied() == Some(target) {
            continue;
        }
        if !matches!(bytes.get(range.start).copied(), Some(b'-' | b'*' | b'+')) {
            continue;
        }
        push_utf8_candidate(
            snapshot,
            OwnerKind::ListItem,
            range,
            vec![target],
            Verification::PreserveMarkdownAndMath,
            "unordered-list-marker",
            candidates,
        );
    }
}

// ----- Ordered list renumber ------------------------------------

fn collect_ordered_list_renumber(snapshot: &Snapshot<'_>, target: OrderedListStyle, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let bytes_view = out.as_bytes();

    for item in snapshot.document().ordered_list_marker_sites() {
        let marker = item.marker_range();
        let want = match target {
            OrderedListStyle::One => 1,
            OrderedListStyle::Consistent => item.start_number().saturating_add(item.ordinal() as u64),
            OrderedListStyle::Preserve => continue,
        };
        let cur = scan_ordered_marker_number(bytes_view, marker.start, marker.end);
        if cur == Some(want) {
            continue;
        }
        let want_bytes = want.to_string().into_bytes();
        push_utf8_candidate(
            snapshot,
            OwnerKind::ListItem,
            marker,
            want_bytes,
            Verification::PreserveMarkdownAndMath,
            "ordered-list-renumber",
            candidates,
        );
    }
}

fn scan_ordered_marker_number(bytes: &[u8], lo: usize, hi: usize) -> Option<u64> {
    let slice = bytes.get(lo..hi)?;
    let s = std::str::from_utf8(slice).ok()?;
    s.parse::<u64>().ok()
}

// ----- Thematic break -------------------------------------------

fn collect_thematic(snapshot: &Snapshot<'_>, target: ThematicStyle, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    for range in snapshot.document().thematic_break_ranges() {
        let lo = range.start;
        let hi = range.end;
        let bytes = out.as_bytes();
        let Some(line) = bytes.get(lo..hi) else { continue };
        if line.is_empty() {
            continue;
        }
        let rewrite = if matches!(target, ThematicStyle::Underscore70) {
            vec![b'_'; 70]
        } else {
            let Some(target_byte) = target.as_byte() else {
                continue;
            };
            let any_off_target = line
                .iter()
                .any(|&b| (b == b'-' || b == b'*' || b == b'_') && b != target_byte);
            if !any_off_target {
                continue;
            }
            let mut rewrite = line.to_vec();
            for byte in &mut rewrite {
                if *byte == b'-' || *byte == b'*' || *byte == b'_' {
                    *byte = target_byte;
                }
            }
            rewrite
        };
        if line == rewrite.as_slice() {
            continue;
        }
        push_utf8_candidate(
            snapshot,
            OwnerKind::ThematicBreak,
            lo..hi,
            rewrite,
            Verification::PreserveMarkdownAndMath,
            "thematic-break",
            candidates,
        );
    }
}

// ----- GFM table normal forms -----------------------------------

fn collect_table_normal_form(snapshot: &Snapshot<'_>, style: TableStyle, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    for table in snapshot.document().table_sites() {
        let Some(normal_form) = TableNormalForm::from_current_snapshot(out, table, style) else {
            continue;
        };
        let raw_range = table.raw_range();
        let Some(existing) = out.get(raw_range.clone()) else {
            continue;
        };
        if existing == normal_form.replacement {
            continue;
        }
        if let Some(candidate) = snapshot.candidate(
            OwnerKind::Table,
            raw_range,
            normal_form.replacement,
            Verification::PreserveMarkdownAndMath,
            "table-normal-form",
        ) {
            candidates.push(candidate);
        }
    }
}

struct TableNormalForm {
    replacement: String,
}

impl TableNormalForm {
    fn from_current_snapshot(source: &str, table: &TableSite, style: TableStyle) -> Option<Self> {
        let column_count = table_column_count(table)?;
        let rows = table_source_rows(source, table, column_count)?;
        let had_trailing_newline = source.get(table.raw_range()).is_some_and(|slice| slice.ends_with('\n'));

        // The table family runs after inline families. Cell contents
        // therefore come from the current snapshot, not the original
        // document, and the replacement owns only the table block.
        let replacement = match style {
            TableStyle::Compact => render_compact_table(&rows, table.alignments(), had_trailing_newline),
            TableStyle::Align => {
                let widths = table_column_widths(&rows, column_count);
                render_aligned_table(&rows, &widths, table.alignments(), had_trailing_newline)
            }
            TableStyle::Preserve => return None,
        };

        Some(Self { replacement })
    }
}

fn table_column_count(table: &TableSite) -> Option<usize> {
    if table.rows().len() < 2 {
        return None;
    }
    let column_count = table.alignments().len();
    if column_count == 0 {
        return None;
    }
    if table.rows().iter().any(|row| row.cells().len() > column_count) {
        return None;
    }
    Some(column_count)
}

fn table_source_rows(source: &str, table: &TableSite, column_count: usize) -> Option<Vec<Vec<String>>> {
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(table.rows().len());
    for row in table.rows() {
        let row_range = row.raw_range();
        if row_range.start < table.raw_range().start || row_range.end > table.raw_range().end {
            return None;
        }
        let mut cells = Vec::with_capacity(column_count);
        for cell in row.cells() {
            let cell_range = cell.raw_range();
            if cell_range.start < row_range.start || cell_range.end > row_range.end {
                return None;
            }
            let raw = source.get(cell_range)?;
            cells.push(raw.trim().to_owned());
        }
        while cells.len() < column_count {
            cells.push(String::new());
        }
        rows.push(cells);
    }
    Some(rows)
}

fn table_column_widths(rows: &[Vec<String>], column_count: usize) -> Vec<usize> {
    let mut widths = vec![3usize; column_count];
    for (row_idx, row) in rows.iter().enumerate() {
        if row_idx == 1 {
            continue;
        }
        for (col, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(col) {
                *width = (*width).max(display_width(cell));
            }
        }
    }
    widths
}

fn render_compact_table(rows: &[Vec<String>], alignments: &[TableAlign], had_trailing_newline: bool) -> String {
    let mut out = String::new();
    for (row_idx, row) in rows.iter().enumerate() {
        if row_idx == 1 {
            push_compact_table_delimiter(&mut out, alignments);
        } else {
            push_compact_table_row(&mut out, row);
        }
        if row_idx.saturating_add(1) < rows.len() || had_trailing_newline {
            out.push('\n');
        }
    }
    out
}

fn render_aligned_table(
    rows: &[Vec<String>],
    widths: &[usize],
    alignments: &[TableAlign],
    had_trailing_newline: bool,
) -> String {
    let mut out = String::new();
    for (row_idx, row) in rows.iter().enumerate() {
        if row_idx == 1 {
            push_aligned_table_delimiter(&mut out, widths, alignments);
        } else {
            push_aligned_table_row(&mut out, row, widths, alignments);
        }
        if row_idx.saturating_add(1) < rows.len() || had_trailing_newline {
            out.push('\n');
        }
    }
    out
}

fn push_compact_table_row(out: &mut String, row: &[String]) {
    out.push('|');
    for cell in row {
        out.push(' ');
        out.push_str(cell);
        out.push(' ');
        out.push('|');
    }
}

fn push_compact_table_delimiter(out: &mut String, alignments: &[TableAlign]) {
    out.push('|');
    for align in alignments {
        let delimiter = match align {
            TableAlign::None => "---",
            TableAlign::Left => ":---",
            TableAlign::Center => ":---:",
            TableAlign::Right => "---:",
        };
        out.push(' ');
        out.push_str(delimiter);
        out.push(' ');
        out.push('|');
    }
}

fn push_aligned_table_row(out: &mut String, row: &[String], widths: &[usize], alignments: &[TableAlign]) {
    out.push('|');
    for (col, width) in widths.iter().copied().enumerate() {
        let cell = row.get(col).map_or("", String::as_str);
        let pad = width.saturating_sub(display_width(cell));
        let align = alignments.get(col).copied().unwrap_or(TableAlign::None);
        let (left, right) = match align {
            TableAlign::Right => (pad, 0),
            TableAlign::Center => (pad / 2, pad.saturating_sub(pad / 2)),
            TableAlign::None | TableAlign::Left => (0, pad),
        };
        out.push(' ');
        push_chars(out, ' ', left);
        out.push_str(cell);
        push_chars(out, ' ', right);
        out.push(' ');
        out.push('|');
    }
}

fn push_aligned_table_delimiter(out: &mut String, widths: &[usize], alignments: &[TableAlign]) {
    out.push('|');
    for (col, width) in widths.iter().copied().enumerate() {
        out.push(' ');
        match alignments.get(col).copied().unwrap_or(TableAlign::None) {
            TableAlign::Left => {
                out.push(':');
                push_chars(out, '-', width.saturating_sub(1));
            }
            TableAlign::Right => {
                push_chars(out, '-', width.saturating_sub(1));
                out.push(':');
            }
            TableAlign::Center => {
                out.push(':');
                push_chars(out, '-', width.saturating_sub(2).max(1));
                out.push(':');
            }
            TableAlign::None => push_chars(out, '-', width),
        }
        out.push(' ');
        out.push('|');
    }
}

fn display_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

fn push_chars(out: &mut String, ch: char, n: usize) {
    for _ in 0..n {
        out.push(ch);
    }
}

// ----- Heading attribute trailer canonicalisation ---------------

/// Rewrite every ATX heading's `{...}` attribute trailer to canonical
/// order: `#id` first, then classes in source order, then `key=value`
/// pairs in source order. Skip the rewrite if the trailer is already
/// canonical, if the heading has no trailer, or if the document
/// reparse check would fail.
fn collect_heading_attrs(snapshot: &Snapshot<'_>, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let sites = snapshot.document().heading_attr_sites();
    for site in sites {
        let attrs = site.attrs();
        let trailer = site.trailer();
        let trailer_lo = trailer.start;
        let trailer_hi = trailer.end;
        let bytes = out.as_bytes();
        let Some(existing) = bytes.get(trailer_lo..trailer_hi) else {
            continue;
        };
        let canonical = attrs.canonical_trailer();
        if existing == canonical.as_bytes() {
            continue;
        }
        if let Some(candidate) = snapshot.candidate(
            OwnerKind::Heading,
            trailer_lo..trailer_hi,
            canonical,
            Verification::PreserveMarkdownAndMath,
            "heading-attrs",
        ) {
            candidates.push(candidate);
        }
    }
}

// ----- Link destination style -----------------------------------

fn collect_link_def_style(snapshot: &Snapshot<'_>, target: LinkDefStyle, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let sites = collect_link_destination_sites(snapshot);
    for site in sites {
        let lo = site.range.start;
        let hi = site.range.end;
        let bytes = out.as_bytes();
        let Some(slice) = bytes.get(lo..hi) else {
            continue;
        };
        let want_angle = match target {
            LinkDefStyle::Bare => false,
            LinkDefStyle::Angle => true,
            LinkDefStyle::Preserve => continue,
        };
        let is_angle = slice.starts_with(b"<") && slice.ends_with(b">") && slice.len() >= 2;
        if want_angle == is_angle {
            continue;
        }
        let Some(bare_slice) = safe_link_destination_body(slice, want_angle) else {
            tracing::debug!(
                target: "mdwright::rewrite",
                label = "link-destination-style",
                span_lo = lo,
                span_hi = hi,
                "skipped link destination: destination slot is not safe to rewrite",
            );
            continue;
        };
        let rewrite: Vec<u8> = if want_angle {
            let mut v = Vec::with_capacity(bare_slice.len().saturating_add(2));
            v.push(b'<');
            v.extend_from_slice(bare_slice);
            v.push(b'>');
            v
        } else {
            bare_slice.to_vec()
        };
        let Ok(replacement) = String::from_utf8(rewrite) else {
            continue;
        };
        if let Some(owner) = site.owner {
            if let Some(candidate) = snapshot.candidate_for_owner(
                owner,
                lo..hi,
                replacement,
                Verification::PreserveMarkdownAndMath,
                "link-destination-style",
            ) {
                candidates.push(candidate);
            }
        } else if let Some(candidate) = snapshot.candidate(
            OwnerKind::InlineLinkDestination,
            lo..hi,
            replacement,
            Verification::PreserveMarkdownAndMath,
            "link-destination-style",
        ) {
            candidates.push(candidate);
        }
    }
}

struct LinkDestinationSite {
    range: std::ops::Range<usize>,
    owner: Option<crate::format::rewrite::OwnerId>,
}

fn collect_link_destination_sites(snapshot: &Snapshot<'_>) -> Vec<LinkDestinationSite> {
    let mut sites: Vec<LinkDestinationSite> = Vec::new();
    for site in snapshot.document().inline_link_destination_slots() {
        sites.push(LinkDestinationSite {
            range: site.range(),
            owner: None,
        });
    }
    for site in snapshot.reference_destination_sites() {
        sites.push(LinkDestinationSite {
            range: site.range.clone(),
            owner: Some(site.owner),
        });
    }
    sites
}

fn safe_link_destination_body(slice: &[u8], want_angle: bool) -> Option<&[u8]> {
    let is_angle = slice.starts_with(b"<") && slice.ends_with(b">") && slice.len() >= 2;
    let body = if is_angle {
        let inner_hi = slice.len().saturating_sub(1);
        slice.get(1..inner_hi)?
    } else {
        slice
    };
    if body.iter().any(|&b| matches!(b, b'\\' | b'(' | b')' | b'\n' | b'\r')) {
        return None;
    }
    if !want_angle && body.iter().any(|&b| matches!(b, b' ' | b'\t')) {
        return None;
    }
    Some(body)
}

// ----- Frontmatter strip ---------------------------------------

/// Drop the document's frontmatter block (and the blank line that
/// usually follows it) when `preserve_frontmatter = false`. Detection
/// re-parses the buffer so the rewrite can be applied after other
/// canonicalisations have rewritten interior bytes.
fn collect_strip_frontmatter(snapshot: &Snapshot<'_>, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let Some(frontmatter) = snapshot.document().frontmatter() else {
        return;
    };
    let bytes = out.as_bytes();
    let mut cut = frontmatter.slice.raw_range.end;
    while bytes.get(cut).copied() == Some(b'\n') {
        cut = cut.saturating_add(1);
    }
    if let Some(candidate) = snapshot.candidate(
        OwnerKind::Frontmatter,
        0..cut,
        String::new(),
        Verification::RemoveFrontmatter,
        "frontmatter-strip",
    ) {
        candidates.push(candidate);
    }
}

// ----- Math regions --------------------------------------------

/// Apply the configured math rewrites to every recognised math
/// region in `out`. Two transformations are supported:
///
/// 1. `MathRender::Dollar`: rewrite `\[…\]` / `\(…\)` regions to
///    `$$…$$` / `$…$`. Environments (`\begin{env}…\end{env}`) are
///    passed through unchanged because there is no dollar form of a
///    LaTeX environment.
/// 2. `math.normalise = true`: pad columns of aligning environments.
fn collect_math(snapshot: &Snapshot<'_>, opts: &FmtOptions, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    for region in snapshot.document().math_regions() {
        let Some(replacement) = compute_math_replacement(out, region, opts) else {
            continue;
        };
        let Some(existing) = out.get(region.range.clone()) else {
            continue;
        };
        if replacement == existing {
            continue;
        }
        if let Some(candidate) = snapshot.candidate(
            OwnerKind::MathRegion,
            region.range.clone(),
            replacement,
            Verification::MathRewrite,
            "math-rewrite",
        ) {
            candidates.push(candidate);
        }
    }
}

fn compute_math_replacement(source: &str, region: &MathRegion, opts: &FmtOptions) -> Option<String> {
    let span = region.span();
    let render_mode = opts.math().render;

    // Dollar rewrite takes precedence and only applies to non-
    // environment math (there is no dollar form of an environment).
    if matches!(render_mode, MathRender::Dollar) && !matches!(span, MathSpan::Environment { .. }) {
        let cow = convert_for_dollar(source, &region.range, span);
        return Some(cow.into_owned());
    }

    if !opts.math().normalise {
        return None;
    }

    // Only aligning environments need a byte-level rewrite;
    // everything else already round-trips through the identity emit.
    let body = span.body().as_str(source);
    if body_braces_balanced(body.as_ref()).is_err() {
        return None;
    }
    let MathSpan::Environment { env, .. } = span else {
        return None;
    };
    if !env.is_aligning() {
        return None;
    }
    let name = env.name(source).to_owned();
    let body_rendered = align_env_body(body.as_ref());
    Some(format!("\\begin{{{name}}}\n{body_rendered}\n\\end{{{name}}}"))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::format::rewrite::Snapshot;
    use crate::{FmtOptions, ItalicStyle, ListMarkerStyle, OrderedListStyle, StrongStyle, ThematicStyle};
    use mdwright_document::ParseOptions;

    fn format_with(src: &str, opts: &FmtOptions) -> String {
        crate::format_document(&mdwright_document::Document::parse(src).expect("fixture parses"), opts)
    }

    #[test]
    fn italic_underscore_to_asterisk() {
        let out = format_with("_foo_\n", &FmtOptions::default().with_italic(ItalicStyle::Asterisk));
        assert_eq!(out, "*foo*\n");
    }

    #[test]
    fn italic_asterisk_already_target_is_noop() {
        let out = format_with("*foo*\n", &FmtOptions::default().with_italic(ItalicStyle::Asterisk));
        assert_eq!(out, "*foo*\n");
    }

    #[test]
    fn italic_intraword_underscore_skips() {
        let out = format_with(
            "foo_bar_baz\n",
            &FmtOptions::default().with_italic(ItalicStyle::Asterisk),
        );
        assert_eq!(out, "foo_bar_baz\n");
    }

    #[test]
    fn strong_double_underscore_to_asterisk() {
        let out = format_with("__foo__\n", &FmtOptions::default().with_strong(StrongStyle::Asterisk));
        assert_eq!(out, "**foo**\n");
    }

    #[test]
    fn italic_rewrite_edits_delimiter_slots_only() {
        let snapshot = Snapshot::parse_owned("_outer _inner__\n", ParseOptions::default()).expect("snapshot parses");
        let mut candidates = Vec::new();
        collect_family_candidates(
            &snapshot,
            &FmtOptions::default().with_italic(ItalicStyle::Asterisk),
            RewriteFamily::Italic,
            &mut candidates,
        );
        let mut ranges: Vec<_> = candidates.iter().map(|candidate| candidate.range().clone()).collect();
        ranges.sort_by_key(|range| range.start);
        assert_eq!(ranges, vec![0..1, 7..8, 13..14, 14..15]);
        assert!(ranges.iter().all(|range| range.len() == 1));
    }

    #[test]
    fn link_destination_rewrite_uses_inline_slots() {
        let snapshot =
            Snapshot::parse_owned("[x](https://example.com)\n", ParseOptions::default()).expect("snapshot parses");
        let mut candidates = Vec::new();
        collect_family_candidates(
            &snapshot,
            &FmtOptions::default().with_link_def_style(LinkDefStyle::Angle),
            RewriteFamily::LinkDestination,
            &mut candidates,
        );
        let ranges: Vec<_> = candidates.iter().map(|candidate| candidate.range().clone()).collect();
        assert_eq!(ranges, vec![4..23]);
    }

    #[test]
    fn link_destination_rewrite_skips_unproven_slots() {
        let snapshot = Snapshot::parse_owned("[x](https://example.com/a\\)b)\n", ParseOptions::default())
            .expect("snapshot parses");
        let mut candidates = Vec::new();
        collect_family_candidates(
            &snapshot,
            &FmtOptions::default().with_link_def_style(LinkDefStyle::Angle),
            RewriteFamily::LinkDestination,
            &mut candidates,
        );
        assert!(candidates.is_empty());
    }

    #[test]
    fn table_normal_form_skips_rows_with_unmodelled_extra_cells() {
        let snapshot = Snapshot::parse_owned("| a | b |\n| - | - |\n| x | y | z |\n", ParseOptions::default())
            .expect("snapshot parses");
        let mut candidates = Vec::new();
        collect_family_candidates(
            &snapshot,
            &FmtOptions::mdformat(),
            RewriteFamily::Table,
            &mut candidates,
        );
        assert!(candidates.is_empty());
    }

    #[test]
    fn table_normal_form_uses_current_inline_normal_form() {
        let src = "| item | value |\n| --- | --- |\n| _em_ | [x](https://example.com/a) |\n";
        let opts = FmtOptions::mdformat()
            .with_italic(ItalicStyle::Asterisk)
            .with_link_def_style(LinkDefStyle::Angle);
        let once = format_with(src, &opts);
        let twice = format_with(&once, &opts);

        assert_eq!(once, twice);
        assert!(once.contains("*em*"));
        assert!(once.contains("[x](<https://example.com/a>)"));
    }

    #[test]
    fn compact_table_normal_form_does_not_compute_column_padding() {
        let src = "| h1   | longer heading |\n| ---- | :------------- |\n| α    | beta           |\n| 你好 | world          |\n";
        let once = format_with(src, &FmtOptions::default());
        let twice = format_with(&once, &FmtOptions::default());

        assert_eq!(once, twice);
        assert_eq!(
            once,
            "| h1 | longer heading |\n| --- | :--- |\n| α | beta |\n| 你好 | world |\n"
        );
    }

    #[test]
    fn aligned_table_normal_form_uses_display_width() {
        let src = "| name | desc |\n| --- | --- |\n| α | beta |\n| 你好 | world |\n";
        let once = format_with(src, &FmtOptions::default().with_table(TableStyle::Align));

        assert_eq!(
            once,
            "| name | desc  |\n| ---- | ----- |\n| α    | beta  |\n| 你好 | world |\n"
        );
    }

    #[test]
    fn list_marker_dash_to_asterisk_atomic() {
        let out = format_with(
            "- a\n- b\n- c\n",
            &FmtOptions::default().with_list_marker(ListMarkerStyle::Asterisk),
        );
        assert_eq!(out, "* a\n* b\n* c\n");
    }

    #[test]
    fn list_marker_rewrite_skips_when_it_would_merge_adjacent_lists() {
        let out = format_with("+\n\n-", &FmtOptions::default().with_list_marker(ListMarkerStyle::Plus));
        assert_eq!(out, "+\n\n-");
    }

    #[test]
    fn list_marker_rewrite_skips_when_definition_list_context_would_merge() {
        let out = format_with(
            "M\n\n:\n-\n\n+",
            &FmtOptions::default().with_list_marker(ListMarkerStyle::Dash),
        );
        assert_eq!(out, "M\n\n:\n-\n\n+");
    }

    #[test]
    fn thematic_dash_to_asterisk() {
        let out = format_with(
            "before\n\n---\n\nafter\n",
            &FmtOptions::default().with_thematic_break(ThematicStyle::Asterisk),
        );
        assert_eq!(out, "before\n\n***\n\nafter\n");
    }

    #[test]
    fn ordered_list_renumber_consistent() {
        let out = format_with(
            "3. a\n5. b\n9. c\n",
            &FmtOptions::default().with_ordered_list(OrderedListStyle::Consistent),
        );
        assert_eq!(out, "3. a\n4. b\n5. c\n");
    }

    #[test]
    fn link_def_to_angle() {
        let out = format_with(
            "[ref]: https://example.com\n",
            &FmtOptions::default().with_link_def_style(LinkDefStyle::Angle),
        );
        assert_eq!(out, "[ref]: <https://example.com>\n");
    }

    #[test]
    fn link_def_style_skips_reference_like_html_block_line() {
        let out = format_with(
            "<?J\n\n[_]:#",
            &FmtOptions::default().with_link_def_style(LinkDefStyle::Angle),
        );
        assert_eq!(out, "<?J\n\n[_]:#");
    }

    #[test]
    fn link_def_to_bare() {
        let out = format_with(
            "[ref]: <https://example.com>\n",
            &FmtOptions::default().with_link_def_style(LinkDefStyle::Bare),
        );
        assert_eq!(out, "[ref]: https://example.com\n");
    }
}
