//! Inline text runs.
//!
//! A run is the maximal stretch of `Event::Text` / `Event::SoftBreak`
//! / `Event::HardBreak` between two structural inline events (or
//! container boundaries). The CM §6.2 flanking decision for a delimiter
//! byte is paragraph-scoped, so the run is the smallest unit that can
//! own a CM round-trip invariant.
//!
//! The IR builder buffers events into a `Vec<RunInput>` until it sees
//! a non-text event, then flushes via [`InlineRun::new`]. The
//! constructor coalesces the inputs into one buffer, computes which
//! bytes came from a source `\X` (so they must be re-escaped on
//! output even when the standard policy wouldn't), runs the escape
//! policy across the combined buffer, and segments the result back at
//! the break positions.

use crate::cm::inline::escape_policy::{EscapeScope, any_byte_needs_escape, escape_buffer};

/// Input event handed to [`InlineRun::new`]. The IR builder produces
/// one of these per pulldown event during a run.
#[derive(Debug)]
pub(crate) enum RunInput<'a> {
    /// Decoded text from `Event::Text`. `source` is the raw source
    /// slice the event was derived from (used to detect bytes that
    /// were originally backslash-escaped); `None` for synthesised
    /// chunks with no source correspondence.
    Text {
        payload: std::borrow::Cow<'a, str>,
        source: Option<&'a str>,
    },
    SoftBreak,
    HardBreak,
}

/// One emission-ready piece of a coalesced text run.
#[derive(Clone, Debug)]
pub(crate) enum RunPart {
    Text(String),
    SoftBreak,
    /// Paragraph-context hard break: emits as `\` + newline.
    HardLineBreak,
    /// Heading-context hard break: emits as `<br/>`.
    HardBreakTag,
}

/// A coalesced, escape-applied, segmented inline text run. Once
/// constructed, the bytes inside [`parts`](Self::parts) round-trip
/// through the `CommonMark` tokenizer.
#[derive(Clone, Debug)]
pub struct InlineRun {
    parts: Vec<RunPart>,
}

impl InlineRun {
    /// Coalesce `inputs` into a run and apply the escape policy under
    /// `scope`. Hard breaks are resolved to [`RunPart::HardBreakTag`]
    /// inside a heading and to [`RunPart::HardLineBreak`] otherwise;
    /// the resulting parts no longer depend on `scope`.
    #[tracing::instrument(level = "trace", skip(inputs))]
    pub(crate) fn new(inputs: Vec<RunInput<'_>>, scope: EscapeScope) -> Self {
        if inputs.is_empty() {
            return Self { parts: Vec::new() };
        }

        // Singleton fast path: one input means no cross-event flanking
        // to worry about. A lone Text takes the allocation-free borrow
        // path; a lone break maps to its corresponding part directly.
        if inputs.len() == 1 {
            let part = match inputs.into_iter().next() {
                Some(RunInput::Text { payload, source }) => {
                    RunPart::Text(escape_singleton(&payload, source, scope))
                }
                Some(RunInput::SoftBreak) => RunPart::SoftBreak,
                Some(RunInput::HardBreak) => hard_break_part(scope),
                None => return Self { parts: Vec::new() },
            };
            return Self { parts: vec![part] };
        }

        // Multi-input run: build a contiguous buffer with `\n`
        // placeholders at each break, a parallel forced-escape bitmap,
        // and a break-kind table indexed by occurrence.
        let total: usize = inputs
            .iter()
            .map(|c| match c {
                RunInput::Text { payload, .. } => payload.len(),
                RunInput::SoftBreak | RunInput::HardBreak => 1,
            })
            .sum();
        let mut buf = String::with_capacity(total);
        let mut forced: Vec<bool> = Vec::with_capacity(total);
        // One tag per `\n` byte that the buffer ends up containing.
        // Tagged `\n`s are real breaks (segment boundaries); un-tagged
        // `\n`s are text-internal newlines (typically from an `&#10;`
        // character reference pulldown already decoded) and stay as
        // bytes inside their surrounding text segment.
        let mut newline_tags: Vec<Option<BreakKind>> = Vec::new();
        for input in inputs {
            match input {
                RunInput::Text { payload, source } => {
                    let chunk_forced = match source {
                        Some(src) if payload_has_source_escape(payload.as_ref(), src) => {
                            forced_escapes_from_source(payload.as_ref(), src)
                        }
                        _ => vec![false; payload.len()],
                    };
                    for &b in payload.as_bytes() {
                        if b == b'\n' {
                            newline_tags.push(None);
                        }
                    }
                    buf.push_str(payload.as_ref());
                    forced.extend(chunk_forced);
                }
                RunInput::SoftBreak => {
                    newline_tags.push(Some(BreakKind::Soft));
                    buf.push('\n');
                    forced.push(false);
                }
                RunInput::HardBreak => {
                    newline_tags.push(Some(BreakKind::Hard));
                    buf.push('\n');
                    forced.push(false);
                }
            }
        }

        let escaped = escape_buffer(&buf, &forced, scope);

        // Walk the escaped buffer, splitting at `\n` byte boundaries.
        // `\n` is ASCII so byte indices are also char boundaries; the
        // segments between are proper UTF-8 slices. Each `\n` byte
        // corresponds to one entry in `newline_tags` (the policy
        // never inserts before `\n` and never removes one, so the
        // count is preserved).
        let mut parts: Vec<RunPart> = Vec::new();
        let mut tag_iter = newline_tags.into_iter();
        let bytes = escaped.as_bytes();
        let mut segment_start = 0usize;
        let mut segment = String::new();
        for (i, &b) in bytes.iter().enumerate() {
            if b != b'\n' {
                continue;
            }
            // Flush bytes [segment_start..i) into `segment`.
            segment.push_str(&escaped[segment_start..i]);
            segment_start = i.saturating_add(1);
            match tag_iter.next().unwrap_or(None) {
                Some(BreakKind::Soft) => {
                    if !segment.is_empty() {
                        parts.push(RunPart::Text(std::mem::take(&mut segment)));
                    }
                    parts.push(RunPart::SoftBreak);
                }
                Some(BreakKind::Hard) => {
                    if !segment.is_empty() {
                        parts.push(RunPart::Text(std::mem::take(&mut segment)));
                    }
                    parts.push(hard_break_part(scope));
                }
                None => segment.push('\n'),
            }
        }
        segment.push_str(&escaped[segment_start..]);
        if !segment.is_empty() {
            parts.push(RunPart::Text(segment));
        }
        Self { parts }
    }

    pub(crate) fn parts(&self) -> &[RunPart] {
        &self.parts
    }

    /// Emit the run's parts as a `Doc`. Text segments map to
    /// [`crate::format::doc::Doc::Text`]; soft breaks to
    /// [`crate::format::doc::Doc::Line`]; hard breaks to either
    /// `"\\" + HardLine` or `<br/>` depending on the form the run
    /// committed to at construction.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self) -> crate::format::doc::Doc<'b> {
        use crate::format::doc::{Doc, concat, hard_line, line, text};
        let mut parts: Vec<Doc<'b>> = Vec::with_capacity(self.parts.len());
        for part in &self.parts {
            match part {
                RunPart::Text(s) => parts.push(text(s.clone())),
                RunPart::SoftBreak => parts.push(line()),
                RunPart::HardLineBreak => parts.push(concat([text("\\"), hard_line()])),
                RunPart::HardBreakTag => parts.push(text("<br/>")),
            }
        }
        concat(parts)
    }

    /// `true` iff the run has no parts. The IR builder uses this to
    /// avoid materialising empty `NodeKind::Run` leaves.
    pub(crate) fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

#[derive(Clone, Copy, Debug)]
enum BreakKind {
    Soft,
    Hard,
}

fn hard_break_part(scope: EscapeScope) -> RunPart {
    if scope.in_heading {
        RunPart::HardBreakTag
    } else {
        RunPart::HardLineBreak
    }
}

/// Escape a singleton text fragment. Forces escapes derived from
/// source `\X` even on the singleton path, so the round-trip invariant
/// holds independent of how pulldown split the run.
fn escape_singleton(payload: &str, source: Option<&str>, scope: EscapeScope) -> String {
    let forced = match source {
        Some(src) if payload_has_source_escape(payload, src) => {
            forced_escapes_from_source(payload, src)
        }
        _ => {
            if !any_byte_needs_escape(payload, scope) {
                return payload.to_owned();
            }
            vec![false; payload.len()]
        }
    };
    escape_buffer(payload, &forced, scope)
}

/// True iff `source` contains at least one CM `§2.4` backslash escape
/// (`\X` for some CM-punct byte `X`). Fast path: when false, the
/// chunk has no source-escape semantics to preserve.
fn payload_has_source_escape(payload: &str, source: &str) -> bool {
    source.len() > payload.len() && source.contains('\\')
}

/// Walk `source` (the raw slice the chunk's `payload` was derived
/// from) and return a bitmap, one entry per payload byte, marking
/// which payload bytes came from a `\X` escape in source. Each `\X`
/// (where `X` is CM punctuation) yields one payload byte `X` consumed
/// from two source bytes; every other source byte maps 1:1 to the
/// payload.
fn forced_escapes_from_source(payload: &str, source: &str) -> Vec<bool> {
    let mut forced = vec![false; payload.len()];
    let s = source.as_bytes();
    let p = payload.as_bytes();
    let mut si = 0usize;
    let mut pi = 0usize;
    while si < s.len() && pi < p.len() {
        let sb = s.get(si).copied();
        let pb = p.get(pi).copied();
        if sb == Some(b'\\')
            && si.saturating_add(1) < s.len()
            && let Some(next) = s.get(si.saturating_add(1)).copied()
            && pb == Some(next)
        {
            if let Some(slot) = forced.get_mut(pi) {
                *slot = true;
            }
            si = si.saturating_add(2);
            pi = pi.saturating_add(1);
        } else if sb == pb {
            si = si.saturating_add(1);
            pi = pi.saturating_add(1);
        } else {
            // Mismatch — pulldown decoded a non-trivial source span
            // (e.g., an entity reference). Bail conservatively.
            break;
        }
    }
    forced
}

#[cfg(test)]
#[allow(clippy::panic, clippy::wildcard_enum_match_arm)]
mod tests {
    use super::*;

    fn paragraph_scope() -> EscapeScope {
        EscapeScope::default()
    }

    fn heading_scope() -> EscapeScope {
        EscapeScope {
            in_heading: true,
            ..EscapeScope::default()
        }
    }

    #[test]
    fn empty_inputs_yield_empty_run() {
        let run = InlineRun::new(vec![], paragraph_scope());
        assert!(run.is_empty());
    }

    #[test]
    fn singleton_plain_text_borrows() {
        let run = InlineRun::new(
            vec![RunInput::Text {
                payload: std::borrow::Cow::Borrowed("hello"),
                source: None,
            }],
            paragraph_scope(),
        );
        assert_eq!(run.parts().len(), 1);
        match run.parts().first() {
            Some(RunPart::Text(s)) if s == "hello" => {}
            other => panic!("expected borrowed text, got {other:?}"),
        }
    }

    #[test]
    fn singleton_escapes_emphasis() {
        let run = InlineRun::new(
            vec![RunInput::Text {
                payload: std::borrow::Cow::Borrowed("*foo*"),
                source: None,
            }],
            paragraph_scope(),
        );
        match run.parts().first() {
            Some(RunPart::Text(s)) => assert_eq!(s.as_str(), r"\*foo\*"),
            other => panic!("expected escaped text, got {other:?}"),
        }
    }

    #[test]
    fn split_run_cross_event_flanking() {
        // Source `a \*b\* c`: pulldown splits at each `\X` into
        // ["a ", "*b", "* c"] with raw_ranges that include the
        // backslash. The escape policy must see the run as a whole
        // and force-escape the `*`s that came from source escapes.
        let run = InlineRun::new(
            vec![
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("a "),
                    source: Some("a "),
                },
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("*b"),
                    source: Some(r"\*b"),
                },
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("* c"),
                    source: Some(r"\* c"),
                },
            ],
            paragraph_scope(),
        );
        let joined: String = run
            .parts()
            .iter()
            .filter_map(|p| match p {
                RunPart::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(joined, r"a \*b\* c");
    }

    #[test]
    fn soft_break_emits_soft_break_part() {
        let run = InlineRun::new(
            vec![
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("a"),
                    source: None,
                },
                RunInput::SoftBreak,
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("b"),
                    source: None,
                },
            ],
            paragraph_scope(),
        );
        let kinds: Vec<&str> = run
            .parts()
            .iter()
            .map(|p| match p {
                RunPart::Text(_) => "text",
                RunPart::SoftBreak => "soft",
                RunPart::HardLineBreak => "hard",
                RunPart::HardBreakTag => "tag",
            })
            .collect();
        assert_eq!(kinds, vec!["text", "soft", "text"]);
    }

    #[test]
    fn hard_break_in_paragraph_is_hard_line_break() {
        let run = InlineRun::new(
            vec![
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("a"),
                    source: None,
                },
                RunInput::HardBreak,
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("b"),
                    source: None,
                },
            ],
            paragraph_scope(),
        );
        assert!(matches!(run.parts().get(1), Some(RunPart::HardLineBreak)));
    }

    #[test]
    fn hard_break_in_heading_is_br_tag() {
        let run = InlineRun::new(
            vec![
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("a"),
                    source: None,
                },
                RunInput::HardBreak,
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("b"),
                    source: None,
                },
            ],
            heading_scope(),
        );
        assert!(matches!(run.parts().get(1), Some(RunPart::HardBreakTag)));
    }

    #[test]
    fn cross_break_emphasis_pair_is_escaped() {
        // CM treats line endings within a paragraph as whitespace
        // for flanking, so `*foo` + soft break + `bar*` is a pair.
        let run = InlineRun::new(
            vec![
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("*foo"),
                    source: None,
                },
                RunInput::SoftBreak,
                RunInput::Text {
                    payload: std::borrow::Cow::Borrowed("bar*"),
                    source: None,
                },
            ],
            paragraph_scope(),
        );
        let texts: Vec<String> = run
            .parts()
            .iter()
            .filter_map(|p| match p {
                RunPart::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec![r"\*foo".to_owned(), r"bar\*".to_owned()]);
    }
}
