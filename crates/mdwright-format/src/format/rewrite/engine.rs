use std::cmp::Reverse;
use std::ops::Range;

use crate::format::canonicalise;
use crate::format::rewrite::candidate::{Candidate, Verification};
use crate::format::rewrite::signature::{verify_batch, verify_one};
use crate::format::rewrite::snapshot::Snapshot;
use crate::format::wrap_pass;
use crate::{FmtOptions, FormatReport, Wrap};
use mdwright_document::{Document, ParseError, ParseOptions};

const MAX_REWRITE_ITERS: u32 = 8;

pub(crate) fn apply_rewrites(doc: &Document, opts: &FmtOptions) -> Result<(String, FormatReport), ParseError> {
    let parse_options = doc.parse_options();
    let mut out = doc.source().to_owned();
    let mut report = FormatReport::default();
    for iter in 0..MAX_REWRITE_ITERS {
        let snapshot = if iter == 0 {
            Snapshot::from_document(doc)
        } else {
            Snapshot::parse_owned(&out, parse_options)?
        };
        let mut candidates = Vec::new();
        if opts.has_any_canonicalisation() {
            canonicalise::collect_candidates(&snapshot, opts, &mut candidates);
        }
        if !matches!(opts.wrap(), Wrap::Keep) {
            wrap_pass::collect_wrap_candidates(&snapshot, opts.wrap(), &mut candidates);
        }

        candidates.retain(|c| snapshot.source().get(c.range().clone()) != Some(c.replacement()));
        report.rewrite_candidates = report.rewrite_candidates.saturating_add(candidates.len());
        if candidates.is_empty() {
            return Ok((out, report));
        }

        candidates.sort_by(|a, b| {
            a.phase()
                .cmp(&b.phase())
                .then_with(|| a.range().start.cmp(&b.range().start))
                .then_with(|| a.range().end.cmp(&b.range().end))
        });
        let (selected, rejected_overlap) = select_non_overlapping(candidates);
        report.rewrite_rejected_overlap = report.rewrite_rejected_overlap.saturating_add(rejected_overlap);
        let before = out.clone();

        if selected
            .iter()
            .all(|c| matches!(c.verification(), Verification::PreserveMarkdownAndMath))
        {
            let candidate = apply_batch(&before, &selected);
            if verify_batch(&before, &candidate, &selected, opts, parse_options) {
                report.rewrite_committed = report.rewrite_committed.saturating_add(selected.len());
                out = candidate;
            } else {
                let (isolated, isolated_report) = apply_isolated(&before, selected, opts, parse_options);
                merge_report(&mut report, &isolated_report);
                out = isolated;
            }
        } else {
            let (isolated, isolated_report) = apply_isolated(&before, selected, opts, parse_options);
            merge_report(&mut report, &isolated_report);
            out = isolated;
        }

        if out == before {
            return Ok((out, report));
        }
        if iter.saturating_add(1) == MAX_REWRITE_ITERS {
            tracing::warn!(
                target: "mdwright::rewrite",
                iters = MAX_REWRITE_ITERS,
                "rewrite engine did not converge within iteration cap; leaving last verified bytes in place",
            );
        }
    }
    Ok((out, report))
}

fn merge_report(report: &mut FormatReport, other: &FormatReport) {
    report.rewrite_candidates = report.rewrite_candidates.saturating_add(other.rewrite_candidates);
    report.rewrite_committed = report.rewrite_committed.saturating_add(other.rewrite_committed);
    report.rewrite_rejected_overlap = report
        .rewrite_rejected_overlap
        .saturating_add(other.rewrite_rejected_overlap);
    report.rewrite_rejected_verification = report
        .rewrite_rejected_verification
        .saturating_add(other.rewrite_rejected_verification);
}

fn select_non_overlapping(candidates: Vec<Candidate>) -> (Vec<Candidate>, usize) {
    let mut selected: Vec<Candidate> = Vec::new();
    let mut rejected = 0usize;
    for candidate in candidates {
        if selected
            .iter()
            .any(|prior| ranges_overlap(prior.range(), candidate.range()))
        {
            rejected = rejected.saturating_add(1);
            tracing::debug!(
                target: "mdwright::rewrite",
                label = candidate.label(),
                span_lo = candidate.range().start,
                span_hi = candidate.range().end,
                owner = ?candidate.owner(),
                "skipped rewrite candidate: range overlaps an earlier phase",
            );
            continue;
        }
        selected.push(candidate);
    }
    (selected, rejected)
}

fn ranges_overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn apply_batch(before: &str, candidates: &[Candidate]) -> String {
    let mut out = before.to_owned();
    let mut ordered: Vec<&Candidate> = candidates.iter().collect();
    ordered.sort_by_key(|candidate| Reverse(candidate.range().start));
    for candidate in ordered {
        out.replace_range(candidate.range().clone(), candidate.replacement());
    }
    out
}

fn apply_isolated(
    before: &str,
    mut candidates: Vec<Candidate>,
    opts: &FmtOptions,
    parse_options: ParseOptions,
) -> (String, FormatReport) {
    let mut report = FormatReport::default();
    candidates.sort_by_key(|candidate| Reverse(candidate.range().start));
    let mut out = before.to_owned();
    for candidate in candidates {
        let Some(existing) = out.get(candidate.range().clone()) else {
            continue;
        };
        if existing == candidate.replacement() {
            continue;
        }
        let mut scratch = out.clone();
        scratch.replace_range(candidate.range().clone(), candidate.replacement());
        if verify_one(&out, &scratch, &candidate, opts, parse_options) {
            report.rewrite_committed = report.rewrite_committed.saturating_add(1);
            out = scratch;
        } else {
            report.rewrite_rejected_verification = report.rewrite_rejected_verification.saturating_add(1);
            tracing::warn!(
                target: "mdwright::rewrite",
                label = candidate.label(),
                span_lo = candidate.range().start,
                span_hi = candidate.range().end,
                owner = ?candidate.owner(),
                "skipped rewrite candidate: verification failed",
            );
        }
    }
    (out, report)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use crate::FmtOptions;
    use crate::format::rewrite::snapshot::{OwnerKind, Snapshot};
    use crate::format::rewrite::{Phase, Verification};
    use mdwright_document::ParseOptions;

    use super::*;

    #[test]
    fn overlapping_candidates_keep_earlier_phase() {
        let snapshot = Snapshot::parse_owned("*x*", ParseOptions::default()).expect("snapshot parses");
        let a = snapshot
            .candidate(
                Phase::Italic,
                OwnerKind::Document,
                0..3,
                "_x_".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "a",
            )
            .expect("candidate");
        let b = snapshot
            .candidate(
                Phase::Strong,
                OwnerKind::Document,
                1..2,
                "y".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "b",
            )
            .expect("candidate");
        let (selected, rejected_overlap) = select_non_overlapping(vec![a, b]);
        assert_eq!(selected.len(), 1);
        assert_eq!(rejected_overlap, 1);
        assert_eq!(selected[0].label(), "a");
    }

    #[test]
    fn invalid_byte_boundary_candidate_is_rejected() {
        let snapshot = Snapshot::parse_owned("é", ParseOptions::default()).expect("snapshot parses");
        assert!(
            snapshot
                .candidate(
                    Phase::Italic,
                    OwnerKind::Document,
                    1..2,
                    "x".to_owned(),
                    Verification::PreserveMarkdownAndMath,
                    "bad",
                )
                .is_none()
        );
    }

    #[test]
    fn isolated_failed_candidate_leaves_source_unchanged() {
        let snapshot = Snapshot::parse_owned("- a\n+ b\n", ParseOptions::default()).expect("snapshot parses");
        let candidate = snapshot
            .candidate(
                Phase::UnorderedList,
                OwnerKind::Document,
                0..7,
                "- a\n- b\n".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "merge",
            )
            .expect("candidate");
        let (out, report) = apply_isolated(
            snapshot.source(),
            vec![candidate],
            &FmtOptions::default(),
            ParseOptions::default(),
        );
        assert_eq!(out, snapshot.source());
        assert_eq!(report.rewrite_rejected_verification, 1);
    }
}
