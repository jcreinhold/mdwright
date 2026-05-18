//! CM §4.7 link reference definitions and the link-resolution algorithm.
//!
//! The [`ReferenceTable`] is built once per document in
//! [`crate::ir::Ir::parse`] and is immutable thereafter. The tree
//! builder uses it to downgrade unresolved reference links and images
//! to raw-source text per CM §4.7's "leave as text" rule.
//!
//! This module is the *sole* site of CM §4.7 label normalisation
//! ([`NormalisedLabel::from_raw`]). Every lookup goes through that type.
//! See `docs/architecture/pulldown-model.md` §4 for the rule and the
//! drift test that pins down what pulldown surfaces in `Tag::Link::id`.
//!
//! ## Building the table from pulldown events
//!
//! `pulldown-cmark` 0.13 does not emit a `LinkReferenceDefinition`
//! event, but the parser already implements CM §4.7 correctly: it
//! resolves reference-style links during parsing and exposes the
//! resolved `dest_url` and `title` on each `Tag::Link` / `Tag::Image`
//! event, with the original raw label in the `id` field.
//!
//! Harvesting those resolved events is the single source of truth for
//! "which labels are defined and what they point to" — and cheaper
//! than re-implementing the CM §4.7 grammar (multi-line defs, escaped
//! brackets, blockquote-nested defs, defs interrupted by paragraph
//! text). A regex scanner gets the simple shapes right and the hard
//! ones wrong; pulldown gets both right by construction.
//!
//! Canonicalisation passes read the table when they need reference
//! definitions in normalised label order.
use std::ops::Range;
use std::sync::OnceLock;

use pulldown_cmark::{Event, LinkType, Tag};
use regex::Regex;
use rustc_hash::FxHashMap;

use crate::util::regex::compile_static;

/// CM §4.7 caps a label at 999 characters. Labels exceeding this are
/// rejected per the spec.
const MAX_LABEL_CHARS: usize = 999;

/// A label after CM §4.7 normalisation: ASCII-trimmed, internal
/// whitespace collapsed to a single U+0020, case-folded.
///
/// The *only* constructor is [`Self::from_raw`]. Every label comparison
/// in the codebase routes through this type, so we cannot accidentally
/// match on differently-normalised inputs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NormalisedLabel(String);

impl NormalisedLabel {
    /// Apply CM §4.7 normalisation. Returns `None` for labels that the
    /// spec rejects: zero-length after trimming, or longer than 999
    /// characters before normalisation.
    ///
    /// Steps, in order:
    ///
    /// 1. Reject if `raw.chars().count() > 999`.
    /// 2. Strip leading and trailing Unicode whitespace.
    /// 3. Collapse internal whitespace runs to a single U+0020.
    /// 4. Lowercase (CM defers to Unicode case folding; the standard
    ///    `to_lowercase` is `String::to_lowercase` which performs
    ///    Unicode-aware case folding — sufficient for the labels CM
    ///    normalises in practice).
    /// 5. Reject if the result is empty.
    pub fn from_raw(raw: &str) -> Option<Self> {
        if raw.chars().count() > MAX_LABEL_CHARS {
            return None;
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut out = String::with_capacity(trimmed.len());
        let mut in_ws = false;
        for ch in trimmed.chars() {
            if ch.is_whitespace() {
                in_ws = true;
                continue;
            }
            if in_ws && !out.is_empty() {
                out.push(' ');
            }
            in_ws = false;
            for low in ch.to_lowercase() {
                out.push(low);
            }
        }
        if out.is_empty() {
            return None;
        }
        Some(Self(out))
    }

    #[cfg(test)]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque index into a [`ReferenceTable`]. Returned by
/// [`ReferenceTable::resolve`]; consumed by
/// [`ReferenceTable::target`]. `u32` is enough for ~4 billion defs;
/// real documents stay well below that.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ReferenceHandle(u32);

/// One resolved reference target.
///
/// Strings are owned because pulldown's reference resolution may
/// return a `CowStr::Boxed` for destinations / titles that contain
/// escapes; we can't safely re-borrow from source bytes for those.
/// The label is the original raw label as pulldown saw it on the
/// first link event that resolved to this target.
#[derive(Clone, Debug)]
pub(crate) struct LinkTarget {
    /// Raw label as first seen in source. Carries the casing the
    /// source chose; used for `[label]` rendering of full-reference
    /// links. CM §4.7 lets the same target be reached via several
    /// labels that normalise equally; we keep the first.
    pub(crate) label_raw: String,
    pub(crate) dest: String,
    pub(crate) title: Option<String>,
    /// Synthetic source range. Pulldown does not surface a real range
    /// for the definition block, so we use `usize::MAX..usize::MAX`
    /// to keep the trailing-def emitter's source-order sort stable
    /// (synthesised defs sort to the end). Callers should treat this
    /// as opaque.
    pub(crate) raw_range: Range<usize>,
}

/// Per-document reference table. Built once in
/// [`build_reference_table`]; immutable thereafter.
#[derive(Debug, Default)]
pub(crate) struct ReferenceTable {
    // FxHash chosen over std SipHash because link labels are short
    // strings looked up on every reference link; SipHash's collision
    // resistance is not needed for a per-document table that never
    // crosses a trust boundary.
    by_label: FxHashMap<NormalisedLabel, ReferenceHandle>,
    targets: Vec<LinkTarget>,
    duplicate_count: u32,
}

impl ReferenceTable {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    /// Resolve a raw (un-normalised) label to a handle. Performs CM
    /// §4.7 normalisation on the way in; returns `None` for labels
    /// that fail normalisation or are not defined.
    pub(crate) fn resolve(&self, raw: &str) -> Option<ReferenceHandle> {
        let norm = NormalisedLabel::from_raw(raw)?;
        self.by_label.get(&norm).copied()
    }

    /// Look up a target by handle. Test-only; production paths
    /// consume the table via [`Self::iter`] (the format walker) or
    /// [`Self::resolve`] (the link resolver).
    #[cfg(test)]
    pub(crate) fn target(&self, h: ReferenceHandle) -> Option<&LinkTarget> {
        self.targets.get(h.0 as usize)
    }

    /// Iterate all targets in insertion order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &LinkTarget> {
        self.targets.iter()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    /// Insert a candidate target observed on a reference-style Link /
    /// Image event. CM §4.7: first definition wins; duplicates are
    /// counted (for observability) but do not displace the winner.
    fn insert(&mut self, label_raw: String, dest: String, title: Option<String>) {
        let Some(norm) = NormalisedLabel::from_raw(&label_raw) else {
            return;
        };
        if self.by_label.contains_key(&norm) {
            self.duplicate_count = self.duplicate_count.saturating_add(1);
            return;
        }
        let idx = u32::try_from(self.targets.len()).unwrap_or(u32::MAX);
        let handle = ReferenceHandle(idx);
        self.targets.push(LinkTarget {
            label_raw,
            dest,
            title,
            raw_range: usize::MAX..usize::MAX,
        });
        self.by_label.insert(norm, handle);
    }
}

/// Build the document's reference table from a pulldown event stream
/// and the original source.
///
/// Pulldown is the source of truth for *which* labels are resolved
/// (correct under multi-line defs, escaped brackets, blockquote nesting,
/// and the "no def after paragraph text" rule), but its `id` field on
/// a `Tag::Link` carries the *link's* bracketed text — not the
/// definition's source casing. To preserve user casing (`[Foo]: …`
/// emits as `[Foo]: …` rather than `[foo]: …` even when the link says
/// `[FOO]`), we scan the source for definition-label prefixes and
/// override the casing for every label pulldown resolved.
#[tracing::instrument(level = "debug", skip(events, source), fields(events = events.len()))]
pub(crate) fn build_reference_table(events: &[Event<'_>], source: &str) -> ReferenceTable {
    let casings = scan_def_label_casings(source);
    let mut table = ReferenceTable::empty();
    for ev in events {
        let Event::Start(tag) = ev else { continue };
        let (lt, dest, title, id) = match tag {
            Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }
            | Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            } => (*link_type, dest_url, title, id),
            Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::MetadataBlock(_) => continue,
        };
        if !matches!(lt, LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut) {
            continue;
        }
        if id.is_empty() {
            continue;
        }
        let Some(norm) = NormalisedLabel::from_raw(id) else {
            continue;
        };
        let label_raw = casings.get(&norm).cloned().unwrap_or_else(|| id.to_string());
        let title_str = title.to_string();
        let title_opt = if title_str.is_empty() { None } else { Some(title_str) };
        table.insert(label_raw, dest.to_string(), title_opt);
    }
    tracing::debug!(defs = table.targets.len(), dupes = table.duplicate_count, "refs built");
    table
}

/// Scan source for `[label]:` prefixes and return a normalised→raw
/// label map. Captures only the label, not the dest/title — those
/// come from pulldown's resolution. The first occurrence wins so the
/// behaviour matches CM §4.7's "first definition" tie-breaker even
/// for the casing.
///
/// Multi-line definition bodies are not a concern because the label
/// always lives on a single line. Escaped `]` inside labels is
/// supported via the `\\.` alternative in the label group.
fn scan_def_label_casings(source: &str) -> FxHashMap<NormalisedLabel, String> {
    let mut out: FxHashMap<NormalisedLabel, String> = FxHashMap::default();
    let re = label_prefix_regex();
    for line in source.lines() {
        let Some(caps) = re.captures(line) else {
            continue;
        };
        let Some(lab) = caps.get(1) else { continue };
        let Some(norm) = NormalisedLabel::from_raw(lab.as_str()) else {
            continue;
        };
        out.entry(norm).or_insert_with(|| lab.as_str().to_string());
    }
    out
}

fn label_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Group 1: label text. Supports `\]` and other escaped chars
    // inside the label. We do not validate dest / title here —
    // pulldown does that.
    RE.get_or_init(|| compile_static(r"^ {0,3}\[((?:[^\]\\\n]|\\.)+)\]:"))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::parse;
    use crate::source::{CanonicalSource, Source};

    fn build(src: &str) -> ReferenceTable {
        let source = Source::new(src);
        let events: Vec<Event<'_>> = parse::events(
            CanonicalSource::from_source(&source),
            parse::options(crate::ParseOptions::default()),
        )
        .collect();
        build_reference_table(&events, src)
    }

    #[test]
    fn normalisation_trims_and_collapses() {
        let n = NormalisedLabel::from_raw("  Foo   Bar  ").expect("valid");
        assert_eq!(n.as_str(), "foo bar");
    }

    #[test]
    fn normalisation_case_folds() {
        let a = NormalisedLabel::from_raw("HELLO").expect("a");
        let b = NormalisedLabel::from_raw("hello").expect("b");
        assert_eq!(a, b);
    }

    #[test]
    fn normalisation_rejects_empty() {
        assert!(NormalisedLabel::from_raw("").is_none());
        assert!(NormalisedLabel::from_raw("   ").is_none());
    }

    #[test]
    fn normalisation_rejects_overlong() {
        let big = "a".repeat(1000);
        assert!(NormalisedLabel::from_raw(&big).is_none());
        let ok = "a".repeat(999);
        assert!(NormalisedLabel::from_raw(&ok).is_some());
    }

    #[test]
    fn table_resolves_case_insensitively() {
        let table = build("[Foo]: https://example.com\n\n[Foo]\n");
        let h = table.resolve("FOO").expect("resolves");
        let t = table.target(h).expect("target");
        assert_eq!(t.dest, "https://example.com");
        assert_eq!(t.label_raw, "Foo");
    }

    #[test]
    fn table_unresolved_label_returns_none() {
        let table = build("");
        assert!(table.resolve("missing").is_none());
    }

    #[test]
    fn table_skips_unknown_reference_links() {
        let table = build("[missing]\n");
        assert!(table.is_empty());
    }

    #[test]
    fn table_handles_multiline_definition() {
        let table = build("[foo]:\n/url\n\n[foo]\n");
        let t = table.target(table.resolve("foo").expect("resolves")).expect("target");
        assert_eq!(t.dest, "/url");
    }

    #[test]
    fn label_casing_preserved_from_def_not_link() {
        // The def writes `[Foo]:`; the link writes `[FOO]`. The
        // emitted label should match the def's casing.
        let table = build("[Foo]: https://example.com\n\n[FOO]\n");
        let t = table.target(table.resolve("foo").expect("resolves")).expect("target");
        assert_eq!(t.label_raw, "Foo");
    }
}
