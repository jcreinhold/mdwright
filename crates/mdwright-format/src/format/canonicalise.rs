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

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::format::rewrite::{Candidate, OwnerKind, Phase, Snapshot, Verification};
use crate::{FmtOptions, HeadingAttrsStyle, LinkDefStyle, MathRender};
use mdwright_document::parse;
use mdwright_document::{CanonicalSource, Source};
use mdwright_document::{HeadingAttrs, find_attr_trailer_range};
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
    if opts.should_renumber_ordered_lists() {
        collect_ordered_list_renumber(snapshot, candidates);
    }
    if let Some(target) = opts.thematic_target_byte() {
        collect_thematic(snapshot, target, candidates);
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

    fn matches_start(self, ev: &Event<'_>) -> bool {
        match self {
            Self::Italic => matches!(ev, Event::Start(Tag::Emphasis)),
            Self::Strong => matches!(ev, Event::Start(Tag::Strong)),
        }
    }

    fn matches_end(self, ev: &Event<'_>) -> bool {
        match self {
            Self::Italic => matches!(ev, Event::End(TagEnd::Emphasis)),
            Self::Strong => matches!(ev, Event::End(TagEnd::Strong)),
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
}

fn collect_emphasis_delim(snapshot: &Snapshot<'_>, kind: EmphasisKind, target: u8, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let spans = collect_emphasis_spans(out, kind);
    let delim_len = kind.delim_len();

    for span in spans {
        let (open_lo, open_hi, close_lo, close_hi) = span;
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

/// Returns `(open_lo, open_hi, close_lo, close_hi)` for every span of
/// `kind` in source order. Indices reference `out`'s byte buffer.
fn collect_emphasis_spans(out: &str, kind: EmphasisKind) -> Vec<(usize, usize, usize, usize)> {
    let src = Source::new(out);
    let mut starts: Vec<usize> = Vec::new();
    let mut spans: Vec<(usize, usize, usize, usize)> = Vec::new();
    let delim_len = kind.delim_len();
    let bytes = out.as_bytes();
    for (ev, range) in parse::events_with_offsets(
        CanonicalSource::from_source(&src),
        parse::options(mdwright_document::ParseOptions::default()),
    ) {
        if kind.matches_start(&ev) {
            starts.push(range.start);
        } else if kind.matches_end(&ev) {
            let Some(open_lo) = starts.pop() else { continue };
            let close_hi = range.end;
            if close_hi < delim_len {
                continue;
            }
            let close_lo = close_hi.saturating_sub(delim_len);
            let open_hi = open_lo.saturating_add(delim_len);
            if open_hi > close_lo {
                continue;
            }
            let Some(open) = bytes.get(open_lo..open_hi) else {
                continue;
            };
            let Some(close) = bytes.get(close_lo..close_hi) else {
                continue;
            };
            if !is_emphasis_delim_run(open) || !is_emphasis_delim_run(close) {
                continue;
            }
            spans.push((open_lo, open_hi, close_lo, close_hi));
        }
    }
    spans
}

fn is_emphasis_delim_run(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|&b| b == b'*' || b == b'_')
}

// ----- Unordered list bullet rewrite ----------------------------

fn collect_unordered_list_marker(snapshot: &Snapshot<'_>, target: u8, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let lists = collect_unordered_lists(out);
    for list in lists {
        if list.bullets.is_empty() {
            continue;
        }
        let bytes = out.as_bytes();
        let already_target = list.bullets.iter().all(|p| bytes.get(*p).copied() == Some(target));
        if already_target {
            continue;
        }
        let (lo, hi) = list.range;
        let Some(slice) = bytes.get(lo..hi) else {
            continue;
        };
        let mut rewrite = slice.to_vec();
        for &p in &list.bullets {
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

struct ListSites {
    range: (usize, usize),
    bullets: Vec<usize>,
}

fn collect_unordered_lists(out: &str) -> Vec<ListSites> {
    let src = Source::new(out);
    let bytes = out.as_bytes();
    let mut stack: Vec<(bool, ListSites)> = Vec::new();
    let mut completed: Vec<ListSites> = Vec::new();

    for (ev, range) in parse::events_with_offsets(
        CanonicalSource::from_source(&src),
        parse::options(mdwright_document::ParseOptions::default()),
    ) {
        #[allow(clippy::wildcard_enum_match_arm, reason = "only list events drive this walk")]
        match ev {
            Event::Start(Tag::List(start)) => {
                stack.push((
                    start.is_none(),
                    ListSites {
                        range: (range.start, range.end),
                        bullets: Vec::new(),
                    },
                ));
            }
            Event::End(TagEnd::List(_)) => {
                if let Some((unordered, sites)) = stack.pop()
                    && unordered
                {
                    completed.push(sites);
                }
            }
            Event::Start(Tag::Item) => {
                let Some((unordered, sites)) = stack.last_mut() else {
                    continue;
                };
                if !*unordered {
                    continue;
                }
                if let Some(p) = find_unordered_bullet(bytes, range.start, range.end) {
                    sites.bullets.push(p);
                }
            }
            _ => {}
        }
    }
    completed
}

fn find_unordered_bullet(bytes: &[u8], start: usize, end: usize) -> Option<usize> {
    let end = end.min(bytes.len());
    let mut i = start;
    while i < end {
        let b = bytes.get(i).copied()?;
        if b == b'-' || b == b'*' || b == b'+' {
            return Some(i);
        }
        if b != b' ' && b != b'\t' {
            return None;
        }
        i = i.saturating_add(1);
    }
    None
}

// ----- Ordered list renumber ------------------------------------

fn collect_ordered_list_renumber(snapshot: &Snapshot<'_>, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let lists = collect_ordered_lists(out);

    for list in lists {
        let Some(first) = list.items.first() else {
            continue;
        };
        let bytes_view = out.as_bytes();
        let Some(start_num) = scan_ordered_marker_number(bytes_view, first.marker_lo, first.marker_hi) else {
            continue;
        };
        let (lo, hi) = list.range;
        let Some(slice) = bytes_view.get(lo..hi) else {
            continue;
        };
        let mut rewrite = slice.to_vec();
        let mut needs_change = false;
        // Renumber items in reverse so local offsets within `rewrite`
        // stay valid as marker widths grow or shrink.
        for (k, item) in list.items.iter().enumerate().rev() {
            let want = start_num.saturating_add(k as u64);
            if item.marker_lo < lo || item.marker_hi > hi {
                continue;
            }
            let cur = scan_ordered_marker_number(bytes_view, item.marker_lo, item.marker_hi);
            if cur == Some(want) {
                continue;
            }
            needs_change = true;
            let want_bytes = want.to_string().into_bytes();
            let local_lo = item.marker_lo.saturating_sub(lo);
            let local_hi = item.marker_hi.saturating_sub(lo);
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

struct OrderedListSites {
    range: (usize, usize),
    items: Vec<OrderedItemSite>,
}

struct OrderedItemSite {
    marker_lo: usize,
    marker_hi: usize,
}

fn collect_ordered_lists(out: &str) -> Vec<OrderedListSites> {
    let src = Source::new(out);
    let bytes = out.as_bytes();
    let mut stack: Vec<(bool, OrderedListSites)> = Vec::new();
    let mut completed: Vec<OrderedListSites> = Vec::new();

    for (ev, range) in parse::events_with_offsets(
        CanonicalSource::from_source(&src),
        parse::options(mdwright_document::ParseOptions::default()),
    ) {
        #[allow(clippy::wildcard_enum_match_arm, reason = "only list events drive this walk")]
        match ev {
            Event::Start(Tag::List(start)) => {
                stack.push((
                    start.is_some(),
                    OrderedListSites {
                        range: (range.start, range.end),
                        items: Vec::new(),
                    },
                ));
            }
            Event::End(TagEnd::List(_)) => {
                if let Some((ordered, sites)) = stack.pop()
                    && ordered
                {
                    completed.push(sites);
                }
            }
            Event::Start(Tag::Item) => {
                let Some((ordered, sites)) = stack.last_mut() else {
                    continue;
                };
                if !*ordered {
                    continue;
                }
                if let Some((mlo, mhi)) = find_ordered_marker_digits(bytes, range.start, range.end) {
                    sites.items.push(OrderedItemSite {
                        marker_lo: mlo,
                        marker_hi: mhi,
                    });
                }
            }
            _ => {}
        }
    }
    completed
}

fn find_ordered_marker_digits(bytes: &[u8], start: usize, end: usize) -> Option<(usize, usize)> {
    let end = end.min(bytes.len());
    let mut i = start;
    while i < end {
        let b = bytes.get(i).copied()?;
        if b == b' ' || b == b'\t' {
            i = i.saturating_add(1);
            continue;
        }
        if !b.is_ascii_digit() {
            return None;
        }
        let digit_lo = i;
        while i < end && bytes.get(i).copied().is_some_and(|c| c.is_ascii_digit()) {
            i = i.saturating_add(1);
        }
        return Some((digit_lo, i));
    }
    None
}

fn scan_ordered_marker_number(bytes: &[u8], lo: usize, hi: usize) -> Option<u64> {
    let slice = bytes.get(lo..hi)?;
    let s = std::str::from_utf8(slice).ok()?;
    s.parse::<u64>().ok()
}

// ----- Thematic break -------------------------------------------

fn collect_thematic(snapshot: &Snapshot<'_>, target: u8, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let sites = collect_thematic_breaks(out);
    for (lo, hi) in sites {
        let bytes = out.as_bytes();
        let Some(line) = bytes.get(lo..hi) else { continue };
        if line.is_empty() {
            continue;
        }
        let any_off_target = line
            .iter()
            .any(|&b| (b == b'-' || b == b'*' || b == b'_') && b != target);
        if !any_off_target {
            continue;
        }
        let mut rewrite = line.to_vec();
        for byte in &mut rewrite {
            if *byte == b'-' || *byte == b'*' || *byte == b'_' {
                *byte = target;
            }
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

fn collect_thematic_breaks(out: &str) -> Vec<(usize, usize)> {
    let src = Source::new(out);
    let mut sites: Vec<(usize, usize)> = Vec::new();
    for (ev, range) in parse::events_with_offsets(
        CanonicalSource::from_source(&src),
        parse::options(mdwright_document::ParseOptions::default()),
    ) {
        if matches!(ev, Event::Rule) {
            let bytes = out.as_bytes();
            let mut hi = range.end.min(bytes.len());
            while hi > range.start && matches!(bytes.get(hi.saturating_sub(1)).copied(), Some(b'\n' | b'\r')) {
                hi = hi.saturating_sub(1);
            }
            sites.push((range.start, hi));
        }
    }
    sites
}

// ----- Heading attribute trailer canonicalisation ---------------

/// Rewrite every ATX heading's `{...}` attribute trailer to canonical
/// order: `#id` first, then classes in source order, then `key=value`
/// pairs in source order. Skip the rewrite if the trailer is already
/// canonical, if the heading has no trailer, or if the document
/// reparse check would fail.
fn collect_heading_attrs(snapshot: &Snapshot<'_>, candidates: &mut Vec<Candidate>) {
    let out = snapshot.source();
    let sites = collect_heading_attr_sites(out);
    for site in sites {
        let HeadingAttrSite {
            attrs,
            trailer_lo,
            trailer_hi,
        } = site;
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

struct HeadingAttrSite {
    attrs: HeadingAttrs,
    trailer_lo: usize,
    trailer_hi: usize,
}

fn collect_heading_attr_sites(out: &str) -> Vec<HeadingAttrSite> {
    let src = Source::new(out);
    let mut sites: Vec<HeadingAttrSite> = Vec::new();
    for (ev, range) in parse::events_with_offsets(
        CanonicalSource::from_source(&src),
        parse::options(mdwright_document::ParseOptions::default()),
    ) {
        #[allow(clippy::wildcard_enum_match_arm, reason = "only heading start drives the walk")]
        if let Event::Start(Tag::Heading { id, classes, attrs, .. }) = ev
            && (id.is_some() || !classes.is_empty() || !attrs.is_empty())
        {
            let heading_attrs = HeadingAttrs {
                id: id.map(|s| s.to_string()),
                classes: classes.iter().map(|c| c.to_string()).collect(),
                attrs: attrs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.as_ref().map(std::string::ToString::to_string)))
                    .collect(),
                source_trailer: String::new(),
            };
            let bytes = out.as_bytes();
            let Some(slice_bytes) = bytes.get(range.clone()) else {
                continue;
            };
            let Ok(slice) = std::str::from_utf8(slice_bytes) else {
                continue;
            };
            if let Some(trailer_range) = find_attr_trailer_range(slice) {
                sites.push(HeadingAttrSite {
                    attrs: heading_attrs,
                    trailer_lo: range.start.saturating_add(trailer_range.start),
                    trailer_hi: range.start.saturating_add(trailer_range.end),
                });
            }
        }
    }
    sites
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
    let out = snapshot.source();
    let src = Source::new(out);
    let bytes = out.as_bytes();
    let mut sites: Vec<LinkDestinationSite> = Vec::new();
    let mut link_stack: Vec<usize> = Vec::new();
    for (ev, range) in parse::events_with_offsets(
        CanonicalSource::from_source(&src),
        parse::options(mdwright_document::ParseOptions::default()),
    ) {
        #[allow(clippy::wildcard_enum_match_arm, reason = "only link events drive this walk")]
        match ev {
            Event::Start(Tag::Link { .. }) => {
                link_stack.push(range.start);
            }
            Event::End(TagEnd::Link) => {
                let Some(open) = link_stack.pop() else { continue };
                if let Some(site) = find_inline_dest_range(bytes, open, range.end) {
                    sites.push(LinkDestinationSite {
                        range: site.0..site.1,
                        owner: None,
                    });
                }
            }
            _ => {}
        }
    }
    for site in snapshot.reference_destination_sites() {
        sites.push(LinkDestinationSite {
            range: site.range.clone(),
            owner: Some(site.owner),
        });
    }
    sites
}

fn find_inline_dest_range(bytes: &[u8], start: usize, end: usize) -> Option<(usize, usize)> {
    let end = end.min(bytes.len());
    if bytes.get(start).copied()? != b'[' {
        return None;
    }
    let mut depth: i32 = 1;
    let mut i = start.saturating_add(1);
    while i < end {
        let b = bytes.get(i).copied()?;
        match b {
            b'\\' => {
                i = i.saturating_add(2);
                continue;
            }
            b'[' => depth = depth.saturating_add(1),
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        i = i.saturating_add(1);
    }
    if depth != 0 || bytes.get(i).copied() != Some(b']') {
        return None;
    }
    let after_close = i.saturating_add(1);
    if bytes.get(after_close).copied() != Some(b'(') {
        return None;
    }
    let mut j = after_close.saturating_add(1);
    while j < end && matches!(bytes.get(j).copied(), Some(b' ' | b'\t' | b'\n')) {
        j = j.saturating_add(1);
    }
    let dest_lo = j;
    let dest_hi = if bytes.get(j).copied() == Some(b'<') {
        let mut k = j.saturating_add(1);
        while k < end && bytes.get(k).copied() != Some(b'>') {
            if bytes.get(k).copied() == Some(b'\n') {
                return None;
            }
            k = k.saturating_add(1);
        }
        if bytes.get(k).copied() != Some(b'>') {
            return None;
        }
        k.saturating_add(1)
    } else {
        let mut depth: i32 = 0;
        let mut k = j;
        while k < end {
            let b = bytes.get(k).copied()?;
            match b {
                b'\\' => {
                    k = k.saturating_add(2);
                    continue;
                }
                b'(' => depth = depth.saturating_add(1),
                b')' => {
                    if depth == 0 {
                        break;
                    }
                    depth = depth.saturating_sub(1);
                }
                b' ' | b'\t' | b'\n' => break,
                _ => {}
            }
            k = k.saturating_add(1);
        }
        k
    };
    if dest_hi <= dest_lo {
        return None;
    }
    Some((dest_lo, dest_hi))
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
mod tests {
    use super::*;
    use crate::{FmtOptions, ItalicStyle, ListMarkerStyle, OrderedListStyle, StrongStyle, ThematicStyle};

    fn format_with(src: &str, opts: &FmtOptions) -> String {
        crate::format_document(&mdwright_document::Document::parse(src), opts)
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
