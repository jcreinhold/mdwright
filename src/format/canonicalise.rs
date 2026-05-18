//! Style canonicalisation — opt-in byte-to-byte rewrites of structural
//! output.
//!
//! # Contract
//!
//! [`canonicalise`] rewrites `out` in place per the style knobs in
//! `opts`. Each rewrite is local and self-verifying: rewrite a byte
//! sequence, reparse the affected paragraph window, confirm the
//! event stream is unchanged. If verification fails, the rewrite is
//! skipped (the source-preserved bytes stay) and a `tracing::warn!`
//! records the skip with span context.
//!
//! # Why a separate pass
//!
//! Structural emit ([`crate::format::document::format_document`]) is
//! pure source-byte preservation: every emit site picks the source's
//! own representation, which by construction re-parses to the
//! source's IR. That makes the structural pipeline idempotent and
//! perturbation-free.
//!
//! Style canonicalisation is the *opposite* concern: deliberately
//! rewrite source bytes per user preference. Doing it during
//! structural emit (the pre-prompt-51 design) meant every emit site's
//! decision could perturb its neighbour's flank class, which required
//! the safety ladder + convergence loop to detect and recover.
//! Separating canonicalisation into its own pass localises the
//! perturbation: each rewrite verifies itself before committing.
//!
//! # Per-rewrite verification
//!
//! For each candidate rewrite at byte range `[lo, hi)`:
//!
//! 1. Compute the pre-rewrite canonical event stream over a window
//!    enclosing `[lo, hi)` (the previous blank line, inclusive, to
//!    the next blank line, exclusive).
//! 2. Apply the rewrite to a scratch buffer.
//! 3. Compute the post-rewrite canonical event stream over the same
//!    window in the scratch buffer.
//! 4. If the two streams compare equal, commit; otherwise skip.
//!
//! Both parses route through [`crate::parse::events`] (the prompt-46
//! chokepoint). Event canonicalisation matches the gate at
//! [`crate::format::semantic::semantically_equivalent`].
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

use crate::config::{FmtOptions, LinkDefStyle};
use crate::format::semantic::{CanonicalEvent, canonical_events};
use crate::parse::{self, FORMATTER_OPTIONS};
use crate::source::{CanonicalSource, Source};

/// Apply every opted-in style rewrite to `out`. No-op when every
/// style knob is `Preserve` — callers should gate on
/// [`FmtOptions::has_any_canonicalisation`] so the chokepoint reparse
/// only happens when at least one rewrite is configured.
pub(crate) fn canonicalise(out: &mut String, opts: &FmtOptions) {
    if let Some(target) = opts.italic_target_byte() {
        rewrite_emphasis_delim(out, EmphasisKind::Italic, target);
    }
    if let Some(target) = opts.strong_target_byte() {
        rewrite_emphasis_delim(out, EmphasisKind::Strong, target);
    }
    if let Some(target) = opts.list_marker_target_byte() {
        rewrite_unordered_list_marker(out, target);
    }
    if opts.should_renumber_ordered_lists() {
        rewrite_ordered_list_renumber(out);
    }
    if let Some(target) = opts.thematic_target_byte() {
        rewrite_thematic(out, target);
    }
    if let Some(target) = opts.link_def_target() {
        rewrite_link_def_style(out, target);
    }
}

// ----- Verification primitive -----------------------------------

/// True iff replacing `out[lo..hi]` with `rewrite` produces a paragraph
/// window that reparses to the same canonical event stream.
fn rewrite_preserves_parse(out: &str, rewrite: &[u8], lo: usize, hi: usize) -> bool {
    let win_lo = previous_blank_line_or_start(out, lo);
    let win_hi = next_blank_line_or_end(out, hi);
    let Some(before_window) = out.get(win_lo..win_hi) else {
        return false;
    };
    let Some(prefix) = out.get(win_lo..lo) else {
        return false;
    };
    let Some(suffix) = out.get(hi..win_hi) else {
        return false;
    };

    let total = prefix.len().saturating_add(rewrite.len()).saturating_add(suffix.len());
    let mut after_window: Vec<u8> = Vec::with_capacity(total);
    after_window.extend_from_slice(prefix.as_bytes());
    after_window.extend_from_slice(rewrite);
    after_window.extend_from_slice(suffix.as_bytes());
    let Ok(after_str) = std::str::from_utf8(&after_window) else {
        return false;
    };

    events_for(before_window) == events_for(after_str)
}

fn events_for(text: &str) -> Vec<CanonicalEvent> {
    let src = Source::new(text);
    canonical_events(CanonicalSource::from_source(&src))
}

/// Replace `out[lo..hi]` with `rewrite` if `rewrite` is valid UTF-8.
fn commit_rewrite(out: &mut String, lo: usize, hi: usize, rewrite: &[u8]) {
    if let Ok(s) = std::str::from_utf8(rewrite) {
        out.replace_range(lo..hi, s);
    }
}

/// Walk backward from `lo` to find the byte position immediately after
/// the most recent blank-line break (`\n\n`, with any horizontal
/// whitespace allowed inside). Returns 0 if no blank line precedes
/// `lo`.
fn previous_blank_line_or_start(s: &str, lo: usize) -> usize {
    let bytes = s.as_bytes();
    let lo = lo.min(bytes.len());
    let mut i = lo;
    while i > 0 {
        let prefix = bytes.get(..i).unwrap_or(&[]);
        let Some(nl) = prefix.iter().rposition(|&b| b == b'\n') else {
            break;
        };
        // Find the start of the line that ends at `nl`.
        let prev_prefix = bytes.get(..nl).unwrap_or(&[]);
        let line_start = prev_prefix
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |p| p.saturating_add(1));
        let line = bytes.get(line_start..nl).unwrap_or(&[]);
        let line_is_blank = line.iter().all(|&b| b == b' ' || b == b'\t');
        if line_is_blank {
            return nl.saturating_add(1);
        }
        i = line_start;
    }
    0
}

/// Walk forward from `hi` to find the byte position of the next
/// blank-line break (`\n\n`, with any horizontal whitespace allowed
/// inside). Returns the document length if no blank line follows.
fn next_blank_line_or_end(s: &str, hi: usize) -> usize {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let hi = hi.min(len);
    let mut i = hi;
    while i < len {
        let tail = bytes.get(i..).unwrap_or(&[]);
        let Some(rel) = tail.iter().position(|&b| b == b'\n') else {
            return len;
        };
        let nl = i.saturating_add(rel);
        let next_line_start = nl.saturating_add(1);
        let next_tail = bytes.get(next_line_start..).unwrap_or(&[]);
        let next_nl_rel = next_tail.iter().position(|&b| b == b'\n');
        let next_nl = match next_nl_rel {
            Some(p) => next_line_start.saturating_add(p),
            None => len,
        };
        let next_line = bytes.get(next_line_start..next_nl).unwrap_or(&[]);
        let blank = next_line.iter().all(|&b| b == b' ' || b == b'\t');
        if blank {
            return nl;
        }
        i = next_nl;
    }
    len
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
}

fn rewrite_emphasis_delim(out: &mut String, kind: EmphasisKind, target: u8) {
    let candidates = collect_emphasis_spans(out, kind);
    let delim_len = kind.delim_len();

    for span in candidates.into_iter().rev() {
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
        if rewrite_preserves_parse(out, &rewrite, open_lo, close_hi) {
            commit_rewrite(out, open_lo, close_hi, &rewrite);
        } else {
            tracing::warn!(
                target: "mdwright::canonicalise",
                kind = kind.label(),
                span_lo = open_lo,
                span_hi = close_hi,
                "skipped emphasis rewrite: parse would diverge",
            );
        }
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
    for (ev, range) in parse::events_with_offsets(CanonicalSource::from_source(&src), FORMATTER_OPTIONS) {
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

fn rewrite_unordered_list_marker(out: &mut String, target: u8) {
    let lists = collect_unordered_lists(out);
    for list in lists.into_iter().rev() {
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
        if rewrite_preserves_parse(out, &rewrite, lo, hi) {
            commit_rewrite(out, lo, hi, &rewrite);
        } else {
            tracing::warn!(
                target: "mdwright::canonicalise",
                span_lo = lo,
                span_hi = hi,
                bullets = list.bullets.len(),
                "skipped unordered-list marker rewrite: parse would diverge",
            );
        }
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

    for (ev, range) in parse::events_with_offsets(CanonicalSource::from_source(&src), FORMATTER_OPTIONS) {
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

fn rewrite_ordered_list_renumber(out: &mut String) {
    let lists = collect_ordered_lists(out);

    for list in lists.into_iter().rev() {
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
        if rewrite_preserves_parse(out, &rewrite, lo, hi) {
            commit_rewrite(out, lo, hi, &rewrite);
        } else {
            tracing::warn!(
                target: "mdwright::canonicalise",
                span_lo = lo,
                span_hi = hi,
                items = list.items.len(),
                "skipped ordered-list renumber: parse would diverge",
            );
        }
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

    for (ev, range) in parse::events_with_offsets(CanonicalSource::from_source(&src), FORMATTER_OPTIONS) {
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

fn rewrite_thematic(out: &mut String, target: u8) {
    let sites = collect_thematic_breaks(out);
    for (lo, hi) in sites.into_iter().rev() {
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
        if rewrite_preserves_parse(out, &rewrite, lo, hi) {
            commit_rewrite(out, lo, hi, &rewrite);
        } else {
            tracing::warn!(
                target: "mdwright::canonicalise",
                span_lo = lo,
                span_hi = hi,
                "skipped thematic-break rewrite: parse would diverge",
            );
        }
    }
}

fn collect_thematic_breaks(out: &str) -> Vec<(usize, usize)> {
    let src = Source::new(out);
    let mut sites: Vec<(usize, usize)> = Vec::new();
    for (ev, range) in parse::events_with_offsets(CanonicalSource::from_source(&src), FORMATTER_OPTIONS) {
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

// ----- Link destination style -----------------------------------

fn rewrite_link_def_style(out: &mut String, target: LinkDefStyle) {
    let sites = collect_link_destination_sites(out);
    for (lo, hi) in sites.into_iter().rev() {
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
        if rewrite_preserves_parse(out, &rewrite, lo, hi) {
            commit_rewrite(out, lo, hi, &rewrite);
        } else {
            tracing::warn!(
                target: "mdwright::canonicalise",
                span_lo = lo,
                span_hi = hi,
                "skipped link-destination style rewrite: parse would diverge",
            );
        }
    }
}

fn collect_link_destination_sites(out: &str) -> Vec<(usize, usize)> {
    let src = Source::new(out);
    let bytes = out.as_bytes();
    let mut sites: Vec<(usize, usize)> = Vec::new();
    let mut link_stack: Vec<usize> = Vec::new();
    for (ev, range) in parse::events_with_offsets(CanonicalSource::from_source(&src), FORMATTER_OPTIONS) {
        #[allow(clippy::wildcard_enum_match_arm, reason = "only link events drive this walk")]
        match ev {
            Event::Start(Tag::Link { .. }) => {
                link_stack.push(range.start);
            }
            Event::End(TagEnd::Link) => {
                let Some(open) = link_stack.pop() else { continue };
                if let Some(site) = find_inline_dest_range(bytes, open, range.end) {
                    sites.push(site);
                }
            }
            _ => {}
        }
    }
    for site in scan_reference_definitions(out) {
        sites.push(site);
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

fn scan_reference_definitions(out: &str) -> Vec<(usize, usize)> {
    let mut sites: Vec<(usize, usize)> = Vec::new();
    let bytes = out.as_bytes();
    let len = bytes.len();
    let mut line_start = 0usize;
    while line_start <= len {
        let tail = bytes.get(line_start..).unwrap_or(&[]);
        let line_end = tail
            .iter()
            .position(|&b| b == b'\n')
            .map_or(len, |p| line_start.saturating_add(p));
        if let Some(site) = parse_ref_def_line(bytes, line_start, line_end) {
            sites.push(site);
        }
        if line_end == len {
            break;
        }
        line_start = line_end.saturating_add(1);
    }
    sites
}

fn parse_ref_def_line(bytes: &[u8], lo: usize, hi: usize) -> Option<(usize, usize)> {
    let mut i = lo;
    let mut spaces = 0usize;
    while i < hi && bytes.get(i).copied() == Some(b' ') && spaces < 3 {
        i = i.saturating_add(1);
        spaces = spaces.saturating_add(1);
    }
    if bytes.get(i).copied() != Some(b'[') {
        return None;
    }
    i = i.saturating_add(1);
    while i < hi {
        let b = bytes.get(i).copied()?;
        match b {
            b'\\' => i = i.saturating_add(2),
            b']' => break,
            b'\n' => return None,
            _ => i = i.saturating_add(1),
        }
    }
    if bytes.get(i).copied() != Some(b']') {
        return None;
    }
    i = i.saturating_add(1);
    if bytes.get(i).copied() != Some(b':') {
        return None;
    }
    i = i.saturating_add(1);
    while i < hi && matches!(bytes.get(i).copied(), Some(b' ' | b'\t')) {
        i = i.saturating_add(1);
    }
    if i >= hi {
        return None;
    }
    let dest_lo = i;
    let dest_hi = if bytes.get(i).copied() == Some(b'<') {
        let mut k = i.saturating_add(1);
        while k < hi && bytes.get(k).copied() != Some(b'>') {
            k = k.saturating_add(1);
        }
        if bytes.get(k).copied() != Some(b'>') {
            return None;
        }
        k.saturating_add(1)
    } else {
        let mut k = i;
        while k < hi && !matches!(bytes.get(k).copied(), Some(b' ' | b'\t')) {
            k = k.saturating_add(1);
        }
        k
    };
    if dest_hi <= dest_lo {
        return None;
    }
    Some((dest_lo, dest_hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previous_blank_line_at_document_start() {
        assert_eq!(previous_blank_line_or_start("foo\nbar", 4), 0);
    }

    #[test]
    fn previous_blank_line_after_blank() {
        let s = "alpha\n\nbeta gamma\n";
        assert_eq!(previous_blank_line_or_start(s, 7), 7);
    }

    #[test]
    fn next_blank_line_at_eof() {
        assert_eq!(next_blank_line_or_end("foo bar", 0), 7);
    }

    #[test]
    fn next_blank_line_at_blank() {
        let s = "alpha\n\nbeta\n";
        assert_eq!(next_blank_line_or_end(s, 0), 5);
    }

    #[test]
    fn italic_underscore_to_asterisk() {
        let mut out = String::from("_foo_\n");
        rewrite_emphasis_delim(&mut out, EmphasisKind::Italic, b'*');
        assert_eq!(out, "*foo*\n");
    }

    #[test]
    fn italic_asterisk_already_target_is_noop() {
        let mut out = String::from("*foo*\n");
        rewrite_emphasis_delim(&mut out, EmphasisKind::Italic, b'*');
        assert_eq!(out, "*foo*\n");
    }

    #[test]
    fn italic_intraword_underscore_skips() {
        let mut out = String::from("foo_bar_baz\n");
        rewrite_emphasis_delim(&mut out, EmphasisKind::Italic, b'*');
        assert_eq!(out, "foo_bar_baz\n");
    }

    #[test]
    fn strong_double_underscore_to_asterisk() {
        let mut out = String::from("__foo__\n");
        rewrite_emphasis_delim(&mut out, EmphasisKind::Strong, b'*');
        assert_eq!(out, "**foo**\n");
    }

    #[test]
    fn list_marker_dash_to_asterisk_atomic() {
        let mut out = String::from("- a\n- b\n- c\n");
        rewrite_unordered_list_marker(&mut out, b'*');
        assert_eq!(out, "* a\n* b\n* c\n");
    }

    #[test]
    fn thematic_dash_to_asterisk() {
        let mut out = String::from("before\n\n---\n\nafter\n");
        rewrite_thematic(&mut out, b'*');
        assert_eq!(out, "before\n\n***\n\nafter\n");
    }

    #[test]
    fn ordered_list_renumber_consistent() {
        let mut out = String::from("3. a\n5. b\n9. c\n");
        rewrite_ordered_list_renumber(&mut out);
        assert_eq!(out, "3. a\n4. b\n5. c\n");
    }

    #[test]
    fn link_def_to_angle() {
        let mut out = String::from("[ref]: https://example.com\n");
        rewrite_link_def_style(&mut out, LinkDefStyle::Angle);
        assert_eq!(out, "[ref]: <https://example.com>\n");
    }

    #[test]
    fn link_def_to_bare() {
        let mut out = String::from("[ref]: <https://example.com>\n");
        rewrite_link_def_style(&mut out, LinkDefStyle::Bare);
        assert_eq!(out, "[ref]: https://example.com\n");
    }
}
