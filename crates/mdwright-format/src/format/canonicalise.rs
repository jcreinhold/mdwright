//! Style canonicalisation — opt-in byte-to-byte rewrites of structural
//! output.
//!
//! # Contract
//!
//! This module recognises opt-in style edits and submits them as
//! parse-owned candidates to the rewrite engine. It does not edit the
//! formatter buffer directly.
//!
//! # Why a separate pass
//!
//! Structural emit ([`crate::format::document::format_document`]) is
//! pure source-byte preservation, so the structural pipeline is
//! idempotent and perturbation-free by construction. Style
//! canonicalisation is the opposite concern: deliberately rewrite
//! source bytes per user preference. Keeping it in its own pass
//! localises the perturbation — the rewrite engine owns ordering,
//! overlap handling, verification, and commit.
//!
//! # Performance
//!
//! Default config (every knob `Preserve`) triggers the early-out in
//! [`FmtOptions::has_any_canonicalisation`], so structural callers
//! pay zero. With any knob set the pass reparses `out` per knob to
//! collect rewrite sites; the convergence loop caps iterations at
//! [`MAX_CANONICALISE_ITERS`], which is enough for every input in
//! the property sweep.
//!
//! # Order of rewrites
//!
//! Inline knobs first (most flank-sensitive), block-level after
//! (insensitive to inline changes):
//!
//! 1. italic — flanking-sensitive `_`/`*` swap.
//! 2. strong — flanking-sensitive `**`/`__` swap.
//! 3. unordered list marker — atomic per list; partial bullet
//!    rewrites would split the list at the parse layer.
//! 4. ordered list renumber — atomic per list.
//! 5. thematic break — trivial; any well-formed break survives all
//!    three byte choices.
//! 6. link destination style — angle/bare toggle, per definition.

use crate::format::rewrite::{Candidate, OwnerKind, Phase, Snapshot, Verification};
use crate::{FmtOptions, HeadingAttrsStyle, LinkDefStyle, MathRender, OrderedListStyle, ThematicStyle};
use mdwright_document::{InlineDelimiterKind, TableAlign, TableSite};
use mdwright_math::MathRegion;
use mdwright_math::MathSpan;
use mdwright_math::normalise::{align_env_body, body_braces_balanced};
use mdwright_math::render::convert_for_dollar;

pub(crate) fn collect_candidates(snapshot: &Snapshot<'_>, opts: &FmtOptions, candidates: &mut Vec<Candidate>) {
    if let Some(target) = opts.italic_target_byte() {
        collect_emphasis_delim(snapshot, EmphasisKind::Italic, target, candidates);
    }
    if let Some(target) = opts.strong_target_byte() {
        collect_emphasis_delim(snapshot, EmphasisKind::Strong, target, candidates);
    }
    if let Some(target) = opts.list_marker_target_byte() {
        collect_unordered_list_marker(snapshot, target, candidates);
    }
    if let Some(target) = opts.ordered_list_target() {
        collect_ordered_list_renumber(snapshot, target, candidates);
    }
    if let Some(target) = opts.thematic_target() {
        collect_thematic(snapshot, target, candidates);
    }
    if opts.should_pad_tables() {
        collect_table_padding(snapshot, candidates);
    }
    if let Some(target) = opts.link_def_target() {
        collect_link_def_style(snapshot, target, candidates);
    }
    if matches!(opts.heading_attrs(), HeadingAttrsStyle::Canonicalise) {
        collect_heading_attrs(snapshot, candidates);
    }
    if needs_math_rewrite(opts) {
        collect_math(snapshot, opts, candidates);
    }
    if !opts.preserve_frontmatter() {
        collect_strip_frontmatter(snapshot, candidates);
    }
}

fn needs_math_rewrite(opts: &FmtOptions) -> bool {
    matches!(opts.math().render, MathRender::Dollar) || opts.math().normalise
}

fn push_utf8_candidate(
    snapshot: &Snapshot<'_>,
    phase: Phase,
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
    if let Some(candidate) = snapshot.candidate(phase, owner, range, replacement, verification, label) {
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

    fn phase(self) -> Phase {
        match self {
            Self::Italic => Phase::Italic,
            Self::Strong => Phase::Strong,
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
    let spans = snapshot.document().inline_delimiter_spans(kind.document_kind());
    let delim_len = kind.delim_len();

    for span in spans {
        let open_range = span.open_range();
        let close_range = span.close_range();
        let open_lo = open_range.start;
        let open_hi = open_range.end;
        let close_lo = close_range.start;
        let close_hi = close_range.end;
        let bytes = out.as_bytes();
        let Some(open) = bytes.get(open_lo..open_hi) else {
            continue;
        };
        let Some(close) = bytes.get(close_lo..close_hi) else {
            continue;
        };
        let Some(inner) = bytes.get(open_hi..close_lo) else {
            continue;
        };
        let already_target = open.iter().all(|&b| b == target) && close.iter().all(|&b| b == target);
        if already_target {
            continue;
        }
        let total = delim_len
            .saturating_mul(2)
            .saturating_add(close_lo.saturating_sub(open_hi));
        let mut rewrite: Vec<u8> = Vec::with_capacity(total);
        for _ in 0..delim_len {
            rewrite.push(target);
        }
        rewrite.extend_from_slice(inner);
        for _ in 0..delim_len {
            rewrite.push(target);
        }
        push_utf8_candidate(
            snapshot,
            kind.phase(),
            OwnerKind::Paragraph,
            open_lo..close_hi,
            rewrite,
            Verification::PreserveMarkdownAndMath,
            kind.label(),
            candidates,
        );
    }
}

// ----- Unordered list bullet rewrite ----------------------------

fn collect_unordered_list_marker(snapshot: &Snapshot<'_>, target: u8, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let lists = snapshot.document().unordered_list_sites();
    for list in lists {
        if list.bullets().is_empty() {
            continue;
        }
        let bytes = out.as_bytes();
        let already_target = list.bullets().iter().all(|p| bytes.get(*p).copied() == Some(target));
        if already_target {
            continue;
        }
        let raw_range = list.raw_range();
        let lo = raw_range.start;
        let hi = raw_range.end;
        let Some(slice) = bytes.get(lo..hi) else {
            continue;
        };
        let mut rewrite = slice.to_vec();
        for &p in list.bullets() {
            if p < lo {
                continue;
            }
            let local = p.saturating_sub(lo);
            if let Some(byte) = rewrite.get_mut(local) {
                *byte = target;
            }
        }
        push_utf8_candidate(
            snapshot,
            Phase::UnorderedList,
            OwnerKind::List,
            lo..hi,
            rewrite,
            Verification::PreserveMarkdownAndMath,
            "unordered-list-marker",
            candidates,
        );
    }
}

// ----- Ordered list renumber ------------------------------------

fn collect_ordered_list_renumber(snapshot: &Snapshot<'_>, target: OrderedListStyle, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let lists = snapshot.document().ordered_list_sites();

    for list in lists {
        let Some(first) = list.items().first() else {
            continue;
        };
        let bytes_view = out.as_bytes();
        let first_marker = first.marker_range();
        let Some(start_num) = scan_ordered_marker_number(bytes_view, first_marker.start, first_marker.end) else {
            continue;
        };
        let raw_range = list.raw_range();
        let lo = raw_range.start;
        let hi = raw_range.end;
        let Some(slice) = bytes_view.get(lo..hi) else {
            continue;
        };
        let mut rewrite = slice.to_vec();
        let mut needs_change = false;
        // Renumber items in reverse so local offsets within `rewrite`
        // stay valid as marker widths grow or shrink.
        for (k, item) in list.items().iter().enumerate().rev() {
            let want = match target {
                OrderedListStyle::One => 1,
                OrderedListStyle::Consistent => start_num.saturating_add(k as u64),
                OrderedListStyle::Preserve => continue,
            };
            let marker = item.marker_range();
            if marker.start < lo || marker.end > hi {
                continue;
            }
            let cur = scan_ordered_marker_number(bytes_view, marker.start, marker.end);
            if cur == Some(want) {
                continue;
            }
            needs_change = true;
            let want_bytes = want.to_string().into_bytes();
            let local_lo = marker.start.saturating_sub(lo);
            let local_hi = marker.end.saturating_sub(lo);
            if local_hi <= rewrite.len() && local_lo <= local_hi {
                rewrite.splice(local_lo..local_hi, want_bytes);
            }
        }
        if !needs_change {
            continue;
        }
        push_utf8_candidate(
            snapshot,
            Phase::OrderedList,
            OwnerKind::List,
            lo..hi,
            rewrite,
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
            Phase::ThematicBreak,
            OwnerKind::ThematicBreak,
            lo..hi,
            rewrite,
            Verification::PreserveMarkdownAndMath,
            "thematic-break",
            candidates,
        );
    }
}

// ----- GFM table padding ----------------------------------------

fn collect_table_padding(snapshot: &Snapshot<'_>, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    for table in snapshot.document().table_sites() {
        let Some(replacement) = padded_table(out, table) else {
            continue;
        };
        let raw_range = table.raw_range();
        let Some(existing) = out.get(raw_range.clone()) else {
            continue;
        };
        if existing == replacement {
            continue;
        }
        if let Some(candidate) = snapshot.candidate(
            Phase::Table,
            OwnerKind::Table,
            raw_range,
            replacement,
            Verification::PreserveMarkdownAndMath,
            "table-pad",
        ) {
            candidates.push(candidate);
        }
    }
}

fn padded_table(source: &str, table: &TableSite) -> Option<String> {
    if table.rows().len() < 2 {
        return None;
    }
    let column_count = table
        .rows()
        .iter()
        .map(|row| row.cells().len())
        .max()
        .unwrap_or(0)
        .min(table.alignments().len().max(1));
    if column_count == 0 {
        return None;
    }

    let mut rows: Vec<Vec<String>> = Vec::with_capacity(table.rows().len());
    for row in table.rows() {
        let mut cells = Vec::with_capacity(column_count);
        for cell in row.cells().iter().take(column_count) {
            let raw = source.get(cell.raw_range())?;
            cells.push(raw.trim().to_owned());
        }
        while cells.len() < column_count {
            cells.push(String::new());
        }
        rows.push(cells);
    }

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

    let had_trailing_newline = source.get(table.raw_range()).is_some_and(|slice| slice.ends_with('\n'));
    let mut out = String::new();
    for (row_idx, row) in rows.iter().enumerate() {
        if row_idx == 1 {
            push_table_delimiter(&mut out, &widths, table.alignments());
        } else {
            push_table_row(&mut out, row, &widths, table.alignments());
        }
        if row_idx.saturating_add(1) < rows.len() || had_trailing_newline {
            out.push('\n');
        }
    }
    Some(out)
}

fn push_table_row(out: &mut String, row: &[String], widths: &[usize], alignments: &[TableAlign]) {
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

fn push_table_delimiter(out: &mut String, widths: &[usize], alignments: &[TableAlign]) {
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
            Phase::HeadingAttrs,
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
        let is_angle = slice.starts_with(b"<") && slice.ends_with(b">") && slice.len() >= 2;
        let bare_slice: &[u8] = if is_angle {
            let inner_hi = slice.len().saturating_sub(1);
            slice.get(1..inner_hi).unwrap_or_default()
        } else {
            slice
        };
        let want_angle = match target {
            LinkDefStyle::Bare => false,
            LinkDefStyle::Angle => true,
            LinkDefStyle::Preserve => continue,
        };
        if want_angle == is_angle {
            continue;
        }
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
                Phase::LinkDestination,
                lo..hi,
                replacement,
                Verification::PreserveMarkdownAndMath,
                "link-destination-style",
            ) {
                candidates.push(candidate);
            }
        } else if let Some(candidate) = snapshot.candidate(
            Phase::LinkDestination,
            OwnerKind::Paragraph,
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
    for site in snapshot.document().inline_link_destination_sites() {
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
        Phase::Frontmatter,
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
/// 1. `MathRender::Dollar` — rewrite `\[…\]` / `\(…\)` regions to
///    `$$…$$` / `$…$`. Environments (`\begin{env}…\end{env}`) are
///    passed through unchanged because there is no dollar form of a
///    LaTeX environment.
/// 2. `math.normalise = true` — pad columns of aligning environments.
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
            Phase::Math,
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

    // Dollar rewrite takes precedence — and only applies to non-
    // environment math (there is no dollar form of an environment).
    if matches!(render_mode, MathRender::Dollar) && !matches!(span, MathSpan::Environment { .. }) {
        let cow = convert_for_dollar(source, &region.range, span);
        return Some(cow.into_owned());
    }

    if !opts.math().normalise {
        return None;
    }

    // Only aligning environments need a byte-level rewrite —
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
    use crate::{FmtOptions, ItalicStyle, ListMarkerStyle, OrderedListStyle, StrongStyle, ThematicStyle};

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
