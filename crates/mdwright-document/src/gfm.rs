//! GitHub Flavored Markdown extension overlay.
//!
//! Pulldown-cmark owns `CommonMark` parsing, but it does not expose every
//! GFM behaviour mdwright needs as document facts. This module keeps the
//! GFM scanner and render/signature event transform together so callers
//! consume recognised facts instead of extension mechanics.

use std::ops::Range;
use std::sync::OnceLock;

use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};
use regex::Regex;

use crate::{GfmAutolinkPolicy, GfmOptions};

/// Where an autolink came from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AutolinkOrigin {
    CommonMark,
    GfmUrl,
    GfmEmail,
}

/// One autolink recognised in source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutolinkFact {
    raw_range: Range<usize>,
    text: String,
    href: String,
    origin: AutolinkOrigin,
}

impl AutolinkFact {
    fn new(raw_range: Range<usize>, text: String, href: String, origin: AutolinkOrigin) -> Self {
        Self {
            raw_range,
            text,
            href,
            origin,
        }
    }

    /// Source byte range of the autolink.
    #[must_use]
    pub fn raw_range(&self) -> Range<usize> {
        self.raw_range.clone()
    }

    /// Text displayed by the autolink.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Link destination after GFM / `CommonMark` href normalisation.
    #[must_use]
    pub fn href(&self) -> &str {
        &self.href
    }

    /// Recognition origin.
    #[must_use]
    pub fn origin(&self) -> AutolinkOrigin {
        self.origin
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AutolinkMatch {
    range: Range<usize>,
    text: String,
    href: String,
    origin: AutolinkOrigin,
}

pub(crate) fn collect_autolinks(
    source: &str,
    events: &[(Event<'_>, Range<usize>)],
    opts: GfmOptions,
) -> Vec<AutolinkFact> {
    let mut out = Vec::new();
    let mut link_depth = 0u32;
    let mut code_block_depth = 0u32;
    for (event, range) in events {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                code_block_depth = code_block_depth.saturating_add(1);
            }
            Event::End(TagEnd::CodeBlock) => {
                code_block_depth = code_block_depth.saturating_sub(1);
            }
            Event::Start(Tag::Link {
                link_type, dest_url, ..
            }) if code_block_depth == 0 && matches!(link_type, LinkType::Autolink | LinkType::Email) => {
                out.push(commonmark_autolink_fact(
                    source,
                    range.clone(),
                    *link_type,
                    dest_url.as_ref(),
                ));
                link_depth = link_depth.saturating_add(1);
            }
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                link_depth = link_depth.saturating_add(1);
            }
            Event::End(TagEnd::Link | TagEnd::Image) => {
                link_depth = link_depth.saturating_sub(1);
            }
            Event::Text(text) if link_depth == 0 && code_block_depth == 0 => {
                out.extend(scan_gfm_autolinks_in_source(
                    text.as_ref(),
                    range.start,
                    source,
                    opts.autolinks,
                ));
            }
            Event::Start(_)
            | Event::End(_)
            | Event::Text(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::TaskListMarker(_) => {}
        }
    }
    out
}

pub(crate) fn apply_gfm_render_policy<'a>(
    source: &str,
    events: Vec<(Event<'a>, Range<usize>)>,
    opts: GfmOptions,
) -> Vec<Event<'a>> {
    let mut out = Vec::with_capacity(events.len());
    let mut link_depth = 0u32;
    let mut code_block_depth = 0u32;
    let mut skip_until = 0usize;
    for (event, range) in events {
        if range.end <= skip_until {
            continue;
        }
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                code_block_depth = code_block_depth.saturating_add(1);
                out.push(event);
            }
            Event::End(TagEnd::CodeBlock) => {
                code_block_depth = code_block_depth.saturating_sub(1);
                out.push(event);
            }
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                link_depth = link_depth.saturating_add(1);
                out.push(event);
            }
            Event::End(TagEnd::Link | TagEnd::Image) => {
                link_depth = link_depth.saturating_sub(1);
                out.push(event);
            }
            Event::Text(text) if link_depth == 0 && code_block_depth == 0 => {
                let text = text.as_ref();
                let local_skip = skip_until.saturating_sub(range.start).min(text.len());
                if let Some(text) = text.get(local_skip..) {
                    skip_until = push_text_with_gfm_autolinks(
                        text,
                        range.start.saturating_add(local_skip),
                        source,
                        opts.autolinks,
                        &mut out,
                    )
                    .max(skip_until);
                }
            }
            Event::Html(html) if opts.tagfilter => {
                out.push(Event::Html(CowStr::from(tagfilter_html(html.as_ref()))));
            }
            Event::InlineHtml(html) if opts.tagfilter => {
                out.push(Event::InlineHtml(CowStr::from(tagfilter_html(html.as_ref()))));
            }
            Event::Start(_)
            | Event::End(_)
            | Event::Text(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::TaskListMarker(_) => out.push(event),
        }
    }
    out
}

fn commonmark_autolink_fact(source: &str, range: Range<usize>, link_type: LinkType, href: &str) -> AutolinkFact {
    let text = source
        .get(range.clone())
        .and_then(|raw| raw.strip_prefix('<').and_then(|s| s.strip_suffix('>')))
        .unwrap_or(href)
        .to_owned();
    let href = match link_type {
        LinkType::Email if href.starts_with("mailto:") => href.to_owned(),
        LinkType::Email => format!("mailto:{href}"),
        LinkType::Inline
        | LinkType::Reference
        | LinkType::ReferenceUnknown
        | LinkType::Collapsed
        | LinkType::CollapsedUnknown
        | LinkType::Shortcut
        | LinkType::ShortcutUnknown
        | LinkType::Autolink
        | LinkType::WikiLink { .. } => href.to_owned(),
    };
    AutolinkFact::new(range, text, href, AutolinkOrigin::CommonMark)
}

fn scan_gfm_autolinks_in_source(text: &str, base: usize, source: &str, policy: GfmAutolinkPolicy) -> Vec<AutolinkFact> {
    scan_gfm_autolink_matches(text, base, source, policy)
        .into_iter()
        .map(|m| {
            AutolinkFact::new(
                base.saturating_add(m.range.start)..base.saturating_add(m.range.end),
                m.text,
                m.href,
                m.origin,
            )
        })
        .collect()
}

fn push_text_with_gfm_autolinks(
    text: &str,
    base: usize,
    source: &str,
    policy: GfmAutolinkPolicy,
    out: &mut Vec<Event<'_>>,
) -> usize {
    let matches = scan_gfm_autolink_matches(text, base, source, policy);
    if matches.is_empty() {
        out.push(Event::Text(CowStr::from(text.to_owned())));
        return base.saturating_add(text.len());
    }
    let mut cursor = 0usize;
    let mut skip_until = base;
    for m in matches {
        if m.range.start > cursor
            && let Some(prefix) = text.get(cursor..m.range.start)
        {
            out.push(Event::Text(CowStr::from(prefix.to_owned())));
        }
        out.push(Event::Start(Tag::Link {
            link_type: LinkType::Autolink,
            dest_url: CowStr::from(m.href.clone()),
            title: CowStr::from(String::new()),
            id: CowStr::from(String::new()),
        }));
        out.push(Event::Text(CowStr::from(m.text)));
        out.push(Event::End(TagEnd::Link));
        cursor = m.range.end;
        skip_until = skip_until.max(base.saturating_add(m.range.end));
    }
    if cursor < text.len()
        && let Some(suffix) = text.get(cursor..)
    {
        out.push(Event::Text(CowStr::from(suffix.to_owned())));
    }
    skip_until
}

fn scan_gfm_autolink_matches(text: &str, base: usize, source: &str, policy: GfmAutolinkPolicy) -> Vec<AutolinkMatch> {
    if policy == GfmAutolinkPolicy::Disabled {
        return Vec::new();
    }
    let mut matches = scan_gfm_url_matches(text, base, source);
    if policy == GfmAutolinkPolicy::UrlsAndEmails {
        matches.extend(scan_gfm_email_matches(text, base, source));
    }
    matches.sort_by_key(|m| (m.range.start, m.range.end));
    let mut out = Vec::new();
    let mut consumed_until = 0usize;
    for m in matches {
        if m.range.start < consumed_until {
            continue;
        }
        consumed_until = m.range.end;
        out.push(m);
    }
    out
}

fn scan_gfm_url_matches(text: &str, base: usize, source: &str) -> Vec<AutolinkMatch> {
    let mut out = Vec::new();
    let mut consumed_until = 0usize;
    for caps in bare_autolink_regex().captures_iter(text) {
        let Some(candidate) = caps.get(2) else {
            continue;
        };
        if candidate.start() < consumed_until {
            continue;
        }
        let Some(m) = classify_url_candidate(text, candidate.start(), candidate.end(), base, source) else {
            continue;
        };
        consumed_until = m.range.end;
        out.push(m);
    }
    out
}

#[allow(clippy::expect_used, reason = "static GFM autolink regex is validated by unit tests")]
fn bare_autolink_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(^|[\s*_~(])((?:https?|ftp)://[^\s<]+|www\.[^\s<]+)")
            .expect("GFM bare autolink regex compiles")
    })
}

fn classify_url_candidate(text: &str, start: usize, end: usize, base: usize, source: &str) -> Option<AutolinkMatch> {
    let raw = url_source_candidate(text, start, end, base, source)?;
    if raw.starts_with("www.") || raw.starts_with("WWW.") {
        classify_www(raw, start)
    } else if raw.contains("://") {
        classify_url(raw, start)
    } else {
        None
    }
}

fn url_source_candidate<'a>(text: &'a str, start: usize, end: usize, base: usize, source: &'a str) -> Option<&'a str> {
    if end < text.len() {
        return text.get(start..end);
    }
    let abs_start = base.saturating_add(start);
    let rest = source.get(abs_start..)?;
    let rel_end = rest
        .char_indices()
        .find_map(|(i, ch)| (ch.is_whitespace() || ch == '<').then_some(i))
        .unwrap_or(rest.len());
    rest.get(..rel_end)
}

fn classify_www(raw: &str, start: usize) -> Option<AutolinkMatch> {
    let rest = raw.get(4..)?;
    let host_len = valid_domain_prefix(rest)?;
    let candidate_end = extend_path_and_trim(raw, 4usize.saturating_add(host_len));
    let text = raw.get(..candidate_end)?.to_owned();
    Some(AutolinkMatch {
        range: start..start.saturating_add(candidate_end),
        href: format!("http://{text}"),
        text,
        origin: AutolinkOrigin::GfmUrl,
    })
}

fn classify_url(raw: &str, start: usize) -> Option<AutolinkMatch> {
    let scheme_end = raw.find("://")?;
    let scheme = raw.get(..scheme_end)?.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https" | "ftp") {
        return None;
    }
    let host_start = scheme_end.saturating_add(3);
    let host = raw.get(host_start..)?;
    let host_len = valid_domain_prefix(host)?;
    let candidate_end = extend_path_and_trim(raw, host_start.saturating_add(host_len));
    let text = raw.get(..candidate_end)?.to_owned();
    Some(AutolinkMatch {
        range: start..start.saturating_add(candidate_end),
        href: text.clone(),
        text,
        origin: AutolinkOrigin::GfmUrl,
    })
}

fn scan_gfm_email_matches(text: &str, base: usize, source: &str) -> Vec<AutolinkMatch> {
    email_regex()
        .find_iter(text)
        .filter_map(|m| classify_email_candidate(m.as_str(), m.start(), base, source))
        .collect()
}

#[allow(clippy::expect_used, reason = "static GFM email regex is validated by unit tests")]
fn email_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[A-Za-z0-9._+-]+@[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)+\.?")
            .expect("GFM email autolink regex compiles")
    })
}

fn classify_email_candidate(raw: &str, start: usize, base: usize, source: &str) -> Option<AutolinkMatch> {
    let trimmed = raw.strip_suffix('.').unwrap_or(raw);
    let domain = trimmed.split_once('@')?.1;
    let last = domain.as_bytes().last().copied()?;
    if matches!(last, b'-' | b'_') {
        return None;
    }
    let absolute_end = base.saturating_add(start).saturating_add(trimmed.len());
    if source
        .as_bytes()
        .get(absolute_end)
        .is_some_and(|b| matches!(b, b'-' | b'_'))
    {
        return None;
    }
    let text = trimmed.to_owned();
    Some(AutolinkMatch {
        range: start..start.saturating_add(trimmed.len()),
        href: format!("mailto:{text}"),
        text,
        origin: AutolinkOrigin::GfmEmail,
    })
}

fn valid_domain_prefix(data: &str) -> Option<usize> {
    let mut last_end = 0usize;
    let mut labels = Vec::new();
    for (i, ch) in data.char_indices() {
        if ch == '.' || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric() {
            last_end = i.saturating_add(ch.len_utf8());
            continue;
        }
        break;
    }
    while last_end > 0 && data.as_bytes().get(last_end.saturating_sub(1)) == Some(&b'.') {
        last_end = last_end.saturating_sub(1);
    }
    let domain = data.get(..last_end)?;
    if domain.is_empty() || !domain.contains('.') {
        return None;
    }
    for label in domain.split('.') {
        if label.is_empty() || label.starts_with('-') || label.ends_with('-') {
            return None;
        }
        labels.push(label);
    }
    let len = labels.len();
    if len < 2 {
        return None;
    }
    if labels
        .iter()
        .skip(len.saturating_sub(2))
        .any(|label| label.contains('_'))
    {
        return None;
    }
    Some(last_end)
}

fn extend_path_and_trim(raw: &str, min_end: usize) -> usize {
    let mut end = raw.len();
    while end > min_end {
        let Some(&b) = raw.as_bytes().get(end.saturating_sub(1)) else {
            break;
        };
        if matches!(b, b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"') {
            end = end.saturating_sub(1);
        } else if b == b';' && looks_like_entity_suffix(raw, end) {
            end = trim_entity_suffix(raw, end);
        } else if b == b')' && has_unbalanced_trailing_paren(raw, end) {
            end = end.saturating_sub(1);
        } else {
            break;
        }
    }
    end
}

fn looks_like_entity_suffix(raw: &str, end: usize) -> bool {
    trim_entity_suffix(raw, end) < end
}

fn trim_entity_suffix(raw: &str, end: usize) -> usize {
    let bytes = raw.as_bytes();
    let mut i = end.saturating_sub(1);
    while i > 0 && bytes.get(i.saturating_sub(1)).is_some_and(u8::is_ascii_alphanumeric) {
        i = i.saturating_sub(1);
    }
    if i > 0 && bytes.get(i.saturating_sub(1)) == Some(&b'&') {
        i.saturating_sub(1)
    } else {
        end.saturating_sub(1)
    }
}

fn has_unbalanced_trailing_paren(raw: &str, end: usize) -> bool {
    let Some(slice) = raw.get(..end) else {
        return false;
    };
    let open = slice.bytes().filter(|&b| b == b'(').count();
    let close = slice.bytes().filter(|&b| b == b')').count();
    close > open
}

fn tagfilter_html(html: &str) -> String {
    tagfilter_regex().replace_all(html, "&lt;$rest").into_owned()
}

#[allow(
    clippy::expect_used,
    reason = "static GFM tagfilter regex is validated by unit tests"
)]
fn tagfilter_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?ix)<(?P<rest>/?(?:title|textarea|style|xmp|iframe|noembed|noframes|script|plaintext)(?:\s|>|/))")
            .expect("GFM tagfilter regex compiles")
    })
}

#[cfg(test)]
mod tests {
    use super::{AutolinkFact, AutolinkOrigin, GfmAutolinkPolicy, scan_gfm_autolinks_in_source, tagfilter_html};

    #[test]
    fn scans_gfm_www_url_and_email_autolinks() {
        let matches = scan_gfm_autolinks_in_source(
            "www.commonmark.org http://commonmark.org ftp://foo.bar.baz foo@bar.baz",
            10,
            "www.commonmark.org http://commonmark.org ftp://foo.bar.baz foo@bar.baz",
            GfmAutolinkPolicy::UrlsAndEmails,
        );
        let hrefs: Vec<&str> = matches.iter().map(|m| m.href()).collect();
        assert_eq!(
            hrefs,
            [
                "http://www.commonmark.org",
                "http://commonmark.org",
                "ftp://foo.bar.baz",
                "mailto:foo@bar.baz",
            ]
        );
        assert_eq!(matches.first().map(|m| m.raw_range()), Some(10..28));
    }

    #[test]
    fn trims_gfm_trailing_punctuation_and_balances_parentheses() {
        let matches = scan_gfm_autolinks_in_source(
            "Visit www.commonmark.org/a.b. (www.google.com/q=(x)))",
            0,
            "Visit www.commonmark.org/a.b. (www.google.com/q=(x)))",
            GfmAutolinkPolicy::Urls,
        );
        let texts: Vec<&str> = matches.iter().map(|m| m.text()).collect();
        assert_eq!(texts, ["www.commonmark.org/a.b", "www.google.com/q=(x)"]);
    }

    #[test]
    fn rejects_invalid_domains_and_email_tails() {
        assert!(
            scan_gfm_autolinks_in_source("foo www. foo", 0, "foo www. foo", GfmAutolinkPolicy::UrlsAndEmails)
                .is_empty()
        );
        assert!(
            scan_gfm_autolinks_in_source(
                "foo http:// foo",
                0,
                "foo http:// foo",
                GfmAutolinkPolicy::UrlsAndEmails
            )
            .is_empty()
        );
        assert!(
            scan_gfm_autolinks_in_source(
                "www.xxx.yyy._zzz",
                0,
                "www.xxx.yyy._zzz",
                GfmAutolinkPolicy::UrlsAndEmails
            )
            .is_empty()
        );
        assert!(
            scan_gfm_autolinks_in_source("a.b-c_d@a.b-", 0, "a.b-c_d@a.b-", GfmAutolinkPolicy::UrlsAndEmails)
                .is_empty()
        );
        assert!(
            scan_gfm_autolinks_in_source("a.b-c_d@a.b_", 0, "a.b-c_d@a.b_", GfmAutolinkPolicy::UrlsAndEmails)
                .is_empty()
        );
    }

    #[test]
    fn email_autolink_policy_can_be_url_only() {
        let matches = scan_gfm_autolinks_in_source(
            "https://example.com foo@bar.baz",
            0,
            "https://example.com foo@bar.baz",
            GfmAutolinkPolicy::Urls,
        );
        assert_eq!(matches.len(), 1);
        assert_eq!(matches.first().map(AutolinkFact::origin), Some(AutolinkOrigin::GfmUrl));
    }

    #[test]
    fn tagfilter_escapes_disallowed_tags() {
        assert_eq!(
            tagfilter_html("<script>alert(1)</script>"),
            "&lt;script>alert(1)&lt;/script>"
        );
        assert_eq!(tagfilter_html("<custom>ok</custom>"), "<custom>ok</custom>");
    }
}
