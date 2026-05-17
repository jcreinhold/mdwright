//! Strikethrough runs (GFM §6.5).
//!
//! Always emitted as `~~body~~`. The single-tilde variant is not
//! supported by GFM and pulldown-cmark never produces it. No
//! per-instance state to carry; the unit struct is the value-level
//! witness that the surrounding node is a strikethrough.
//!
//! ## Round-trip invariant
//!
//! [`Strikethrough::pretty`] is the single emission point for
//! strikethrough output (callers: `src/format/inline.rs`). Its
//! construction-time invariant is:
//!
//! > The emitted bytes, when reparsed by pulldown in their inline
//! > position, recover one `Strikethrough` event whose flattened
//! > text equals `body`'s flattened text.
//!
//! Pulldown decides where a `~~` wrapper closes by scanning the
//! body for the **next** `~~` it finds. If `body` itself contains
//! a literal `~~` (or even a single `~` adjacent to other tildes),
//! the inner tilde run closes the wrapper early, and the recovered
//! event structure differs from the input. To make the invariant
//! hold by construction we escape every `~` byte in `body`'s
//! [`Doc::Text`] leaves before wrapping (`\~`). The escape is
//! HTML-transparent (`\~` and `~` render identically) and
//! idempotent (pulldown collapses `\~` back to a `Text("~")` event;
//! the next format re-escapes, producing the same bytes).
//!
//! Atomic Doc children (e.g. inline code spans, which fence their
//! own bytes) are left alone: their `~` characters cannot
//! participate in the surrounding strikethrough's delimiter
//! detection, and rewriting them would break the round-trip
//! invariants those constructs already enforce.

use std::borrow::Cow;

use crate::format::doc::Doc;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Strikethrough;

impl Strikethrough {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self
    }

    /// Wrap `body` in `~~…~~`, escaping any `~` byte that appears
    /// in a text leaf of `body` so the wrapper's `~~` cannot be
    /// closed early on reparse. See the module-level invariant.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'a>(body: Doc<'a>) -> Doc<'a> {
        use crate::format::doc::{concat, text};
        let safe = escape_body_tildes(body);
        debug_assert!(
            body_has_no_unescaped_tilde(&safe),
            "Strikethrough::pretty: escape pass left an unescaped `~` in body — \
             the wrapping `~~` would close early on reparse",
        );
        concat([text("~~"), safe, text("~~")])
    }
}

/// Iteratively walk `body` and rewrite every `Doc::Text(s)` leaf to
/// have its `~` bytes escaped as `\~`. `Atomic` / `Prefix` children
/// are passed through verbatim (their bytes are already fenced or
/// drained, so their interior `~` cannot reach the surrounding
/// delimiter detector). `Concat` is splayed.
fn escape_body_tildes<'a>(body: Doc<'a>) -> Doc<'a> {
    use crate::format::doc::concat;
    let mut parts: Vec<Doc<'a>> = Vec::new();
    let mut stack: Vec<Doc<'a>> = vec![body];
    while let Some(node) = stack.pop() {
        match node {
            Doc::Concat(items) => {
                for item in items.into_vec().into_iter().rev() {
                    stack.push(item);
                }
            }
            Doc::Text(s) => parts.push(Doc::Text(escape_tildes_in(s))),
            leaf @ (Doc::Line | Doc::SoftSpace | Doc::HardLine | Doc::Atomic(_) | Doc::Prefix(_, _)) => {
                parts.push(leaf);
            }
        }
    }
    if parts.len() == 1 {
        parts.into_iter().next().unwrap_or_else(|| concat([]))
    } else {
        concat(parts)
    }
}

/// Replace every unescaped `~` byte in `s` with `\~`. A `~`
/// immediately preceded by `\` (an escape the IR builder already
/// inserted to preserve source fidelity — see
/// `cm::inline::run::forced_escapes_from_source`) is left alone:
/// re-escaping would double the backslashes on each format pass.
/// Returns the input untouched (zero allocs) when no escape is
/// needed — the common case for non-tilde-bearing bodies.
fn escape_tildes_in(s: Cow<'_, str>) -> Cow<'_, str> {
    if !needs_tilde_escape(s.as_ref()) {
        return s;
    }
    let mut out = String::with_capacity(s.len().saturating_add(s.len() / 4));
    let mut prev: Option<char> = None;
    for ch in s.chars() {
        if ch == '~' && prev != Some('\\') {
            out.push('\\');
        }
        out.push(ch);
        prev = Some(ch);
    }
    Cow::Owned(out)
}

/// True iff `s` contains at least one `~` byte not immediately
/// preceded by `\`. Cheap pre-scan to avoid allocating in the
/// common all-clean case.
fn needs_tilde_escape(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes
        .iter()
        .enumerate()
        .any(|(i, &b)| b == b'~' && i.checked_sub(1).and_then(|j| bytes.get(j)).copied() != Some(b'\\'))
}

/// Debug-only invariant check: walk `body` and confirm no
/// `Doc::Text` leaf has a `~` byte that is not immediately preceded
/// by a `\` (the escape we just inserted). Used in the
/// `debug_assert!` inside [`Strikethrough::pretty`].
#[cfg(debug_assertions)]
fn body_has_no_unescaped_tilde(body: &Doc<'_>) -> bool {
    let mut stack: Vec<&Doc<'_>> = vec![body];
    while let Some(node) = stack.pop() {
        match node {
            Doc::Text(s) => {
                let bytes = s.as_bytes();
                for (i, &b) in bytes.iter().enumerate() {
                    if b == b'~' {
                        let prev = i.checked_sub(1).and_then(|j| bytes.get(j)).copied();
                        if prev != Some(b'\\') {
                            return false;
                        }
                    }
                }
            }
            Doc::Concat(items) => {
                for item in items.iter().rev() {
                    stack.push(item);
                }
            }
            Doc::Line | Doc::SoftSpace | Doc::HardLine | Doc::Atomic(_) | Doc::Prefix(_, _) => {}
        }
    }
    true
}
#[cfg(not(debug_assertions))]
fn body_has_no_unescaped_tilde(_: &Doc<'_>) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::doc::{RenderOptions, concat, render, text, unbreakable};

    fn render_pretty(body: Doc<'_>) -> String {
        render(&Strikethrough::pretty(body), &RenderOptions)
    }

    #[test]
    fn strikethrough_is_uniquely_inhabited() {
        assert_eq!(Strikethrough::new(), Strikethrough);
    }

    #[test]
    fn plain_text_body_is_unchanged() {
        assert_eq!(render_pretty(text("hi")), "~~hi~~");
    }

    /// Body containing a literal `~~`: the inner tildes must be
    /// escaped so the wrapper does not close early on reparse.
    #[test]
    fn body_with_double_tilde_escapes_each() {
        assert_eq!(render_pretty(text("a~~b")), "~~a\\~\\~b~~");
    }

    /// Single interior `~` is also a clash candidate (`~` then
    /// wrapper `~~` → 3-tilde run); escape it.
    #[test]
    fn body_with_single_tilde_escapes() {
        assert_eq!(render_pretty(text("a~b")), "~~a\\~b~~");
    }

    #[test]
    fn body_with_run_of_three_tildes_escapes_all() {
        assert_eq!(render_pretty(text("a~~~b")), "~~a\\~\\~\\~b~~");
    }

    /// Atomic children (inline code spans) carry their own fences
    /// and round-trip invariants; the strikethrough escape pass
    /// must NOT recurse into them.
    #[test]
    fn atomic_children_are_passed_through() {
        let code = unbreakable(text("`~~`"));
        let body = concat([text("a"), code, text("b")]);
        assert_eq!(render_pretty(body), "~~a`~~`b~~");
    }
}
