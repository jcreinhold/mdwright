use std::cmp::Reverse;
use std::ops::Range;

use crate::format::canonicalise;
use crate::format::rewrite::candidate::{Candidate, Verification};
use crate::format::rewrite::signature::{verify_batch, verify_one};
use crate::format::rewrite::snapshot::Snapshot;
use crate::format::wrap_pass;
use crate::{FmtOptions, Wrap};

const MAX_REWRITE_ITERS: u32 = 8;

pub(crate) fn apply_rewrites(source: &str, opts: &FmtOptions) -> String {
    let mut out = source.to_owned();
    for iter in 0..MAX_REWRITE_ITERS {
        let snapshot = Snapshot::new(&out);
        let mut candidates = Vec::new();
        if opts.has_any_canonicalisation() {
            canonicalise::collect_candidates(&snapshot, opts, &mut candidates);
        }
        if !matches!(opts.wrap(), Wrap::Keep) {
            wrap_pass::collect_wrap_candidates(&snapshot, opts.wrap(), &mut candidates);
        }

        candidates.retain(|c| snapshot.source().get(c.range().clone()) != Some(c.replacement()));
        if candidates.is_empty() {
            return out;
        }

        candidates.sort_by(|a, b| {
            a.phase()
                .cmp(&b.phase())
                .then_with(|| a.range().start.cmp(&b.range().start))
                .then_with(|| a.range().end.cmp(&b.range().end))
        });
        let selected = select_non_overlapping(candidates);
        let before = out.clone();

        if selected
            .iter()
            .all(|c| matches!(c.verification(), Verification::PreserveMarkdownAndMath))
        {
            let candidate = apply_batch(&before, &selected);
            if verify_batch(&before, &candidate, &selected, opts) {
                out = candidate;
            } else {
                out = apply_isolated(&before, selected, opts);
            }
        } else {
            out = apply_isolated(&before, selected, opts);
        }

        if out == before {
            return out;
        }
        if iter.saturating_add(1) == MAX_REWRITE_ITERS {
            tracing::warn!(
                target: "mdwright::rewrite",
                iters = MAX_REWRITE_ITERS,
                "rewrite engine did not converge within iteration cap; leaving last verified bytes in place",
            );
        }
    }
    out
}

fn select_non_overlapping(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut selected: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        if selected
            .iter()
            .any(|prior| ranges_overlap(prior.range(), candidate.range()))
        {
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
    selected
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

fn apply_isolated(before: &str, mut candidates: Vec<Candidate>, opts: &FmtOptions) -> String {
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
        if verify_one(&out, &scratch, &candidate, opts) {
            out = scratch;
        } else {
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
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use crate::FmtOptions;
    use crate::format::rewrite::snapshot::{OwnerKind, Snapshot};
    use crate::format::rewrite::{Phase, Verification};

    use super::*;

    #[test]
    fn overlapping_candidates_keep_earlier_phase() {
        let snapshot = Snapshot::new("*x*");
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
        let selected = select_non_overlapping(vec![a, b]);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].label(), "a");
    }

    #[test]
    fn invalid_byte_boundary_candidate_is_rejected() {
        let snapshot = Snapshot::new("é");
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
        let snapshot = Snapshot::new("- a\n+ b\n");
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
        let out = apply_isolated(snapshot.source(), vec![candidate], &FmtOptions::default());
        assert_eq!(out, snapshot.source());
    }
}
