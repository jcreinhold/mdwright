use std::ops::Range;

use crate::Document;
use crate::cm::math::span::MathSpan;
use crate::config::{FmtOptions, MathRender};
use crate::format::rewrite::candidate::{Candidate, Verification};
use crate::format::semantic::{canonical_events, semantically_equivalent};
use crate::source::{CanonicalSource, Source};

pub(crate) fn verify_batch(before: &str, after: &str, candidates: &[Candidate], opts: &FmtOptions) -> bool {
    if candidates.is_empty() {
        return before == after;
    }
    if candidates
        .iter()
        .all(|c| matches!(c.verification(), Verification::PreserveMarkdownAndMath))
    {
        return markdown_and_math_signature(before) == markdown_and_math_signature(after);
    }
    candidates
        .first()
        .is_some_and(|candidate| candidates.len() == 1 && verify_one(before, after, candidate, opts))
}

pub(crate) fn verify_one(before: &str, after: &str, candidate: &Candidate, opts: &FmtOptions) -> bool {
    match candidate.verification() {
        Verification::PreserveMarkdownAndMath => {
            markdown_and_math_signature(before) == markdown_and_math_signature(after)
        }
        Verification::MathRewrite => verify_math_rewrite(before, after, candidate.range(), opts),
        Verification::RemoveFrontmatter => verify_frontmatter_removal(before, after, candidate.range()),
    }
}

fn markdown_and_math_signature(source: &str) -> (Vec<crate::format::semantic::CanonicalEvent>, Vec<MathSig>) {
    let src = Source::new(source);
    let events = canonical_events(CanonicalSource::from_source(&src));
    let math = math_signature(source);
    (events, math)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MathSig {
    kind: MathKindSig,
    body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MathKindSig {
    Inline,
    Display,
    Environment(String),
}

fn math_signature(source: &str) -> Vec<MathSig> {
    let doc = Document::parse(source);
    doc.math_regions()
        .iter()
        .map(|region| {
            let span = &region.span;
            let kind = match span {
                MathSpan::Inline { .. } => MathKindSig::Inline,
                MathSpan::Display { .. } => MathKindSig::Display,
                MathSpan::Environment { env, .. } => MathKindSig::Environment(env.name(source).to_owned()),
            };
            MathSig {
                kind,
                body: span.body().as_str(source).into_owned(),
            }
        })
        .collect()
}

fn verify_math_rewrite(before: &str, after: &str, range: &Range<usize>, opts: &FmtOptions) -> bool {
    if !changed_only_at(before, after, range) {
        return false;
    }
    let before_doc = Document::parse(before);
    let Some(before_region) = before_doc
        .math_regions()
        .iter()
        .find(|region| region.range.start <= range.start && region.range.end >= range.end)
    else {
        return false;
    };
    if matches!(opts.math().render, MathRender::Dollar)
        && let MathSpan::Inline { .. } | MathSpan::Display { .. } = &before_region.span
    {
        return verify_dollar_math_replacement(before, after, range, &before_region.span);
    }

    let after_doc = Document::parse(after);
    let Some(after_region) = after_doc
        .math_regions()
        .iter()
        .find(|region| region.range.start == before_region.range.start)
    else {
        return false;
    };
    match (&before_region.span, &after_region.span) {
        (MathSpan::Inline { body: before_body, .. }, MathSpan::Inline { body: after_body, .. })
        | (MathSpan::Display { body: before_body, .. }, MathSpan::Display { body: after_body, .. }) => {
            before_body.as_str(before) == after_body.as_str(after)
        }
        (MathSpan::Environment { env: before_env, .. }, MathSpan::Environment { env: after_env, .. }) => {
            before_env.name(before) == after_env.name(after)
        }
        _ => false,
    }
}

fn verify_dollar_math_replacement(before: &str, after: &str, range: &Range<usize>, span: &MathSpan) -> bool {
    let before_suffix = before.get(range.end..).unwrap_or("");
    let replacement_hi = after.len().saturating_sub(before_suffix.len());
    let Some(replacement) = after.get(range.start..replacement_hi) else {
        return false;
    };
    match span {
        MathSpan::Inline { body, .. } => replacement == format!("${}$", body.as_str(before).trim()),
        MathSpan::Display { body, .. } => replacement == format!("$$ {} $$", body.as_str(before).trim()),
        MathSpan::Environment { .. } => false,
    }
}

fn changed_only_at(before: &str, after: &str, range: &Range<usize>) -> bool {
    let Some(before_prefix) = before.get(..range.start) else {
        return false;
    };
    let Some(after_prefix) = after.get(..range.start) else {
        return false;
    };
    if before_prefix != after_prefix {
        return false;
    }
    let before_suffix = before.get(range.end..).unwrap_or("");
    after.ends_with(before_suffix)
}

fn verify_frontmatter_removal(before: &str, after: &str, range: &Range<usize>) -> bool {
    if range.start != 0 {
        return false;
    }
    let doc = Document::parse(before);
    let Some(frontmatter) = doc.frontmatter() else {
        return false;
    };
    if range.start != frontmatter.slice.raw_range.start || range.end < frontmatter.slice.raw_range.end {
        return false;
    }
    before.get(range.end..) == Some(after) && semantically_equivalent(after, after)
}
