//! GFM extension facts that pulldown-cmark does not surface as parser
//! events.

use std::ops::Range;
use std::sync::OnceLock;

use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};
use regex::Regex;

/// One bare autolink recognised by GFM's extended autolink rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GfmAutolink {
    pub raw_range: Range<usize>,
    pub text: String,
    pub href: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AutolinkMatch {
    range: Range<usize>,
    text: String,
    href: String,
}

pub(crate) fn scan_bare_autolinks(text: &str, base: usize) -> Vec<GfmAutolink> {
    scan_bare_autolink_matches(text)
        .into_iter()
        .map(|m| GfmAutolink {
            raw_range: base.saturating_add(m.range.start)..base.saturating_add(m.range.end),
            text: m.text,
            href: m.href,
        })
        .collect()
}

pub(crate) fn render_autolink_events(events: Vec<Event<'_>>, bare_url_autolinks: bool) -> Vec<Event<'_>> {
    if !bare_url_autolinks {
        return events;
    }
    let mut out = Vec::with_capacity(events.len());
    let mut link_depth = 0u32;
    for event in events {
        match event {
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                link_depth = link_depth.saturating_add(1);
                out.push(event);
            }
            Event::End(TagEnd::Link | TagEnd::Image) => {
                link_depth = link_depth.saturating_sub(1);
                out.push(event);
            }
            Event::Text(text) if link_depth == 0 => push_text_with_autolinks(text.as_ref(), &mut out),
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

fn push_text_with_autolinks(text: &str, out: &mut Vec<Event<'_>>) {
    let matches = scan_bare_autolink_matches(text);
    if matches.is_empty() {
        out.push(Event::Text(CowStr::from(text.to_owned())));
        return;
    }
    let mut cursor = 0usize;
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
    }
    if cursor < text.len()
        && let Some(suffix) = text.get(cursor..)
    {
        out.push(Event::Text(CowStr::from(suffix.to_owned())));
    }
}

fn scan_bare_autolink_matches(text: &str) -> Vec<AutolinkMatch> {
    let mut out = Vec::new();
    let mut consumed_until = 0usize;
    for caps in bare_autolink_regex().captures_iter(text) {
        let Some(candidate) = caps.get(2) else {
            continue;
        };
        if candidate.start() < consumed_until {
            continue;
        }
        let Some(m) = classify_candidate(text, candidate.start(), candidate.end()) else {
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

fn classify_candidate(text: &str, start: usize, end: usize) -> Option<AutolinkMatch> {
    let raw = text.get(start..end)?;
    if raw.starts_with("www.") || raw.starts_with("WWW.") {
        classify_www(raw, start)
    } else if raw.contains("://") {
        classify_url(raw, start)
    } else {
        let _ = (text, end);
        None
    }
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

#[cfg(test)]
mod tests {
    use super::scan_bare_autolinks;

    #[test]
    fn scans_gfm_www_and_url_autolinks() {
        let matches = scan_bare_autolinks(
            "www.commonmark.org http://commonmark.org ftp://foo.bar.baz foo@bar.baz",
            10,
        );
        let hrefs: Vec<&str> = matches.iter().map(|m| m.href.as_str()).collect();
        assert_eq!(
            hrefs,
            [
                "http://www.commonmark.org",
                "http://commonmark.org",
                "ftp://foo.bar.baz",
            ]
        );
        assert_eq!(matches.first().map(|m| m.raw_range.clone()), Some(10..28));
    }

    #[test]
    fn trims_gfm_trailing_punctuation_and_balances_parentheses() {
        let matches = scan_bare_autolinks("Visit www.commonmark.org/a.b. (www.google.com/q=(x)))", 0);
        let texts: Vec<&str> = matches.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["www.commonmark.org/a.b", "www.google.com/q=(x)"]);
    }

    #[test]
    fn rejects_invalid_domains() {
        assert!(scan_bare_autolinks("foo www. foo", 0).is_empty());
        assert!(scan_bare_autolinks("foo http:// foo", 0).is_empty());
        assert!(scan_bare_autolinks("www.xxx.yyy._zzz", 0).is_empty());
    }
}
