use std::ops::Range;

use crate::format::rewrite::candidate::{Candidate, Verification};
use crate::format::semantic::semantically_equivalent_with_options;
use crate::{FmtOptions, MathRender};
use mdwright_document::{Document, ParseOptions, markdown_signature};
use mdwright_math::MathSpan;

pub(crate) fn verify_batch(
    before: &str,
    after: &str,
    candidates: &[Candidate],
    opts: &FmtOptions,
    parse_options: ParseOptions,
) -> bool {
    if candidates.is_empty() {
        return before == after;
    }
    if candidates
        .iter()
        .all(|c| matches!(c.verification(), Verification::PreserveMarkdownAndMath))
    {
        return match (
            markdown_and_math_signature(before, parse_options),
            markdown_and_math_signature(after, parse_options),
        ) {
            (Ok(before_sig), Ok(after_sig)) => before_sig == after_sig,
            _ => false,
        };
    }
    if candidates
        .iter()
        .all(|c| matches!(c.verification(), Verification::MathRewrite))
    {
        return verify_math_family_rewrite(before, after, candidates, opts, parse_options);
    }
    candidates
        .first()
        .is_some_and(|candidate| candidates.len() == 1 && verify_one(before, after, candidate, opts, parse_options))
}

pub(crate) fn verify_one(
    before: &str,
    after: &str,
    candidate: &Candidate,
    opts: &FmtOptions,
    parse_options: ParseOptions,
) -> bool {
    match candidate.verification() {
        Verification::PreserveMarkdownAndMath => {
            match (
                markdown_and_math_signature(before, parse_options),
                markdown_and_math_signature(after, parse_options),
            ) {
                (Ok(before_sig), Ok(after_sig)) => before_sig == after_sig,
                _ => false,
            }
        }
        Verification::MathRewrite => verify_math_rewrite(before, after, candidate.range(), opts, parse_options),
        Verification::RemoveFrontmatter => verify_frontmatter_removal(before, after, candidate.range(), parse_options),
    }
}

fn markdown_and_math_signature(
    source: &str,
    parse_options: ParseOptions,
) -> Result<(mdwright_document::MarkdownSignature, Vec<MathSig>), mdwright_document::ParseError> {
    let events = markdown_signature(source, parse_options)?;
    let math = math_signature(source, parse_options)?;
    Ok((events, math))
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

fn math_signature(source: &str, parse_options: ParseOptions) -> Result<Vec<MathSig>, mdwright_document::ParseError> {
    let doc = Document::parse_with_options(source, parse_options)?;
    Ok(doc
        .math_regions()
        .iter()
        .map(|region| {
            let span = region.span();
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
        .collect())
}

fn verify_math_family_rewrite(
    before: &str,
    after: &str,
    candidates: &[Candidate],
    opts: &FmtOptions,
    parse_options: ParseOptions,
) -> bool {
    if !changed_only_at_candidate_ranges(before, after, candidates) {
        return false;
    }
    if !candidates
        .iter()
        .all(|candidate| verify_math_candidate_replacement(before, candidate, opts, parse_options))
    {
        return false;
    }
    true
}

fn verify_math_candidate_replacement(
    before: &str,
    candidate: &Candidate,
    opts: &FmtOptions,
    parse_options: ParseOptions,
) -> bool {
    let Ok(before_doc) = Document::parse_with_options(before, parse_options) else {
        return false;
    };
    let Some(before_region) = before_doc
        .math_regions()
        .iter()
        .find(|region| region.range.start <= candidate.range().start && region.range.end >= candidate.range().end)
    else {
        return false;
    };
    if matches!(opts.math().render, MathRender::Dollar)
        && let MathSpan::Inline { .. } | MathSpan::Display { .. } = before_region.span()
    {
        return expected_dollar_math_replacement(before, before_region.span())
            .is_some_and(|expected| expected == candidate.replacement());
    }
    let MathSpan::Environment { env, .. } = before_region.span() else {
        return false;
    };
    if !opts.math().normalise {
        return false;
    }
    let name = env.name(before);
    candidate.replacement().contains(&format!("\\begin{{{name}}}"))
        && candidate.replacement().contains(&format!("\\end{{{name}}}"))
}

fn verify_math_rewrite(
    before: &str,
    after: &str,
    range: &Range<usize>,
    opts: &FmtOptions,
    parse_options: ParseOptions,
) -> bool {
    if !changed_only_at(before, after, range) {
        return false;
    }
    let Ok(before_doc) = Document::parse_with_options(before, parse_options) else {
        return false;
    };
    let Some(before_region) = before_doc
        .math_regions()
        .iter()
        .find(|region| region.range.start <= range.start && region.range.end >= range.end)
    else {
        return false;
    };
    if matches!(opts.math().render, MathRender::Dollar)
        && let MathSpan::Inline { .. } | MathSpan::Display { .. } = before_region.span()
    {
        return verify_dollar_math_replacement(before, after, range, before_region.span());
    }

    let Ok(after_doc) = Document::parse_with_options(after, parse_options) else {
        return false;
    };
    let Some(after_region) = after_doc
        .math_regions()
        .iter()
        .find(|region| region.range.start == before_region.range.start)
    else {
        return false;
    };
    match (before_region.span(), after_region.span()) {
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
    expected_dollar_math_replacement(before, span).is_some_and(|expected| replacement == expected)
}

fn expected_dollar_math_replacement(before: &str, span: &MathSpan) -> Option<String> {
    match span {
        MathSpan::Inline { body, .. } => Some(format!("${}$", body.as_str(before).trim())),
        MathSpan::Display { body, .. } => Some(format!("$$ {} $$", body.as_str(before).trim())),
        MathSpan::Environment { .. } => None,
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

fn changed_only_at_candidate_ranges(before: &str, after: &str, candidates: &[Candidate]) -> bool {
    let mut sorted: Vec<&Candidate> = candidates.iter().collect();
    sorted.sort_by_key(|candidate| candidate.range().start);
    let mut expected = String::with_capacity(after.len());
    let mut cursor = 0usize;
    for candidate in sorted {
        let range = candidate.range();
        if range.start < cursor {
            return false;
        }
        let Some(prefix) = before.get(cursor..range.start) else {
            return false;
        };
        expected.push_str(prefix);
        expected.push_str(candidate.replacement());
        cursor = range.end;
    }
    let Some(suffix) = before.get(cursor..) else {
        return false;
    };
    expected.push_str(suffix);
    expected == after
}

fn verify_frontmatter_removal(before: &str, after: &str, range: &Range<usize>, parse_options: ParseOptions) -> bool {
    if range.start != 0 {
        return false;
    }
    let Ok(doc) = Document::parse_with_options(before, parse_options) else {
        return false;
    };
    let Some(frontmatter) = doc.frontmatter() else {
        return false;
    };
    if range.start != frontmatter.slice.raw_range.start || range.end < frontmatter.slice.raw_range.end {
        return false;
    }
    before.get(range.end..) == Some(after)
        && semantically_equivalent_with_options(after, after, parse_options).unwrap_or(false)
}
