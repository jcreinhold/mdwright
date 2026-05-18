//! Heading attribute trailer support.
//!
//! Pulldown-cmark parses ATX heading attribute lists into structured
//! `id` / `classes` / `attrs` fields. mdwright also keeps the source
//! trailer bytes so preserve-default formatting can leave the trailer
//! untouched and the opt-in canonicalise pass can rewrite just that
//! byte range.

use std::ops::Range;

/// Heading attribute trailer (pulldown `Tag::Heading::{id, classes, attrs}`).
/// Carried on [`crate::tree::NodeKind::Heading`] when an
/// `{ #id .class key=val }` trailer was recognised on an ATX heading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadingAttrs {
    /// `{#id ...}`. Only the first id in source order is kept (pulldown
    /// drops subsequent ids).
    pub id: Option<String>,
    /// `.class` tokens in source order.
    pub classes: Vec<String>,
    /// `key=value` pairs in source order. The value is `None` for a
    /// bare `key` token with no `=`.
    pub attrs: Vec<(String, Option<String>)>,
    /// Source bytes of the `{...}` trailer (including braces). Empty
    /// only when the trailer scanner failed to relocate the braces in
    /// the heading source; the canonicalise pass then falls back to a
    /// structured render.
    pub source_trailer: String,
}

impl HeadingAttrs {
    /// Render the trailer in canonical order: `#id`, then classes in
    /// source order, then `key=value` pairs in source order. Values
    /// containing whitespace are double-quoted; values containing a
    /// double quote are double-quoted with embedded `\"`.
    pub(crate) fn canonical_trailer(&self) -> String {
        let mut tokens: Vec<String> = Vec::new();
        if let Some(id) = &self.id {
            tokens.push(format!("#{id}"));
        }
        for class in &self.classes {
            tokens.push(format!(".{class}"));
        }
        for (k, v) in &self.attrs {
            match v {
                Some(v) if v.chars().any(|c| c.is_ascii_whitespace() || c == '"') => {
                    let escaped: String = v
                        .chars()
                        .flat_map(|c| match c {
                            '"' => vec!['\\', '"'],
                            c => vec![c],
                        })
                        .collect();
                    tokens.push(format!("{k}=\"{escaped}\""));
                }
                Some(v) => tokens.push(format!("{k}={v}")),
                None => tokens.push(k.clone()),
            }
        }
        format!("{{{}}}", tokens.join(" "))
    }
}

/// Locate the `{...}` attribute trailer at the end of `raw`. Returns
/// the byte range of the trailer (braces included) relative to `raw`.
pub(crate) fn find_attr_trailer_range(raw: &str) -> Option<Range<usize>> {
    let bytes = raw.as_bytes();
    let mut end = bytes.len();
    while end > 0 && matches!(bytes.get(end.saturating_sub(1)), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        end = end.saturating_sub(1);
    }
    if end == 0 || bytes.get(end.saturating_sub(1)) != Some(&b'}') {
        return None;
    }
    let close = end.saturating_sub(1);
    let mut depth = 1i32;
    let mut i = close;
    while i > 0 {
        i = i.saturating_sub(1);
        match bytes.get(i) {
            Some(b'}') => depth = depth.saturating_add(1),
            Some(b'{') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i..end);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_trailer_id_then_classes_then_attrs() {
        let attrs = HeadingAttrs {
            id: Some("section".to_owned()),
            classes: vec!["warn".to_owned(), "imp".to_owned()],
            attrs: vec![
                ("data-x".to_owned(), Some("1".to_owned())),
                ("flag".to_owned(), None),
            ],
            source_trailer: "{.imp #section .warn data-x=1 flag}".to_owned(),
        };
        assert_eq!(attrs.canonical_trailer(), "{#section .warn .imp data-x=1 flag}");
    }

    #[test]
    fn canonical_trailer_quotes_value_with_whitespace() {
        let attrs = HeadingAttrs {
            id: None,
            classes: Vec::new(),
            attrs: vec![("title".to_owned(), Some("hello world".to_owned()))],
            source_trailer: "{title=\"hello world\"}".to_owned(),
        };
        assert_eq!(attrs.canonical_trailer(), "{title=\"hello world\"}");
    }

    #[test]
    fn canonical_trailer_omits_missing_id_and_empty_lists() {
        let attrs = HeadingAttrs {
            id: None,
            classes: vec!["only".to_owned()],
            attrs: Vec::new(),
            source_trailer: "{.only}".to_owned(),
        };
        assert_eq!(attrs.canonical_trailer(), "{.only}");
    }

    #[test]
    fn attr_trailer_range_finds_final_braces() {
        let raw = "## Heading {#id .class}\n";
        let found = find_attr_trailer_range(raw).map(|range| &raw[range]);
        assert_eq!(found, Some("{#id .class}"));
    }
}
