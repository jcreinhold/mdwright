use std::cmp::Reverse;
use std::ops::Range;

use crate::format::canonicalise;
use crate::format::rewrite::candidate::{Candidate, RewriteFamily};
use crate::format::rewrite::signature::verify_batch;
use crate::format::rewrite::snapshot::Snapshot;
use crate::format::wrap_pass;
use crate::{FmtOptions, FormatReport, Wrap};
use mdwright_document::{Document, ParseError, ParseOptions};

const MAX_REWRITE_PASSES: u32 = 8;

const FAMILY_ORDER: [RewriteFamily; 11] = [
    RewriteFamily::Italic,
    RewriteFamily::Strong,
    RewriteFamily::UnorderedList,
    RewriteFamily::OrderedList,
    RewriteFamily::ThematicBreak,
    RewriteFamily::LinkDestination,
    RewriteFamily::HeadingAttrs,
    RewriteFamily::Table,
    RewriteFamily::Math,
    RewriteFamily::Frontmatter,
    RewriteFamily::Wrap,
];

pub(crate) fn apply_rewrites(doc: &Document, opts: &FmtOptions) -> Result<(String, FormatReport), ParseError> {
    let parse_options = doc.parse_options();
    let original = doc.source().to_owned();
    let mut out = original.clone();
    let mut report = FormatReport::default();

    for _ in 0..MAX_REWRITE_PASSES {
        let mut changed_in_pass = false;
        let mut family_idx = 0usize;

        while family_idx < FAMILY_ORDER.len() {
            let committed = {
                let snapshot = snapshot_for(doc, &out, parse_options)?;
                let mut committed = None;

                while family_idx < FAMILY_ORDER.len() {
                    let Some(&family) = FAMILY_ORDER.get(family_idx) else {
                        break;
                    };
                    family_idx = family_idx.saturating_add(1);
                    let mut candidates = collect_family(&snapshot, opts, family);
                    candidates.retain(|c| snapshot.source().get(c.range().clone()) != Some(c.replacement()));
                    report.rewrite_candidates = report.rewrite_candidates.saturating_add(candidates.len());

                    let outcome = FamilyPlan::build(family, candidates);
                    let FamilyPlanBuild::Ready(plan) = outcome else {
                        if let FamilyPlanBuild::RejectedOverlap { rejected } = outcome {
                            report.rewrite_rejected_overlap = report.rewrite_rejected_overlap.saturating_add(rejected);
                        }
                        continue;
                    };

                    let before = out.clone();
                    let candidate = apply_plan(&before, &plan);
                    if candidate == before {
                        continue;
                    }
                    if verify_batch(&before, &candidate, plan.edits(), opts, parse_options) {
                        report.rewrite_committed = report.rewrite_committed.saturating_add(plan.len());
                        committed = Some(candidate);
                        break;
                    }

                    let first = plan.edits().first();
                    report.rewrite_rejected_verification =
                        report.rewrite_rejected_verification.saturating_add(plan.len());
                    tracing::warn!(
                        target: "mdwright::rewrite",
                        family = ?plan.family(),
                        edits = plan.len(),
                        first_label = first.map_or("", Candidate::label),
                        first_owner = ?first.map(Candidate::owner),
                        "skipped rewrite family: verification failed",
                    );
                }

                committed
            };

            if let Some(candidate) = committed {
                out = candidate;
                changed_in_pass = true;
            }
        }

        if !changed_in_pass {
            return Ok((out, report));
        }
    }

    report.rewrite_committed = 0;
    report.rewrite_rejected_convergence = report.rewrite_rejected_convergence.saturating_add(1);
    tracing::warn!(
        target: "mdwright::rewrite",
        passes = MAX_REWRITE_PASSES,
        "rewrite engine did not reach a fixed point; leaving original source bytes unchanged",
    );
    Ok((original, report))
}

fn snapshot_for<'a>(doc: &'a Document, out: &'a str, parse_options: ParseOptions) -> Result<Snapshot<'a>, ParseError> {
    if out == doc.source() {
        Ok(Snapshot::from_document(doc))
    } else {
        Snapshot::parse_owned(out, parse_options)
    }
}

fn collect_family(snapshot: &Snapshot<'_>, opts: &FmtOptions, family: RewriteFamily) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    if matches!(family, RewriteFamily::Wrap) {
        if !matches!(opts.wrap(), Wrap::Keep) {
            wrap_pass::collect_wrap_candidates(snapshot, opts.wrap(), &mut candidates);
        }
    } else if opts.has_any_canonicalisation() {
        canonicalise::collect_family_candidates(snapshot, opts, family, &mut candidates);
    }
    candidates
}

#[derive(Clone, Debug)]
struct FamilyPlan {
    family: RewriteFamily,
    edits: Vec<Candidate>,
}

enum FamilyPlanBuild {
    Noop,
    Ready(FamilyPlan),
    RejectedOverlap { rejected: usize },
}

impl FamilyPlan {
    fn build(family: RewriteFamily, mut edits: Vec<Candidate>) -> FamilyPlanBuild {
        if edits.is_empty() {
            return FamilyPlanBuild::Noop;
        }
        edits.sort_by(|a, b| {
            a.range()
                .start
                .cmp(&b.range().start)
                .then_with(|| a.range().end.cmp(&b.range().end))
        });
        let (rejected, first_overlap) = count_local_overlaps(&edits);
        if rejected > 0 {
            tracing::debug!(
                target: "mdwright::rewrite",
                family = ?family,
                rejected,
                first_label = first_overlap.map_or("", Candidate::label),
                first_owner = ?first_overlap.map(Candidate::owner),
                "skipped rewrite family: local edits overlap",
            );
            return FamilyPlanBuild::RejectedOverlap { rejected };
        }
        FamilyPlanBuild::Ready(Self { family, edits })
    }

    fn family(&self) -> RewriteFamily {
        self.family
    }

    fn edits(&self) -> &[Candidate] {
        &self.edits
    }

    fn len(&self) -> usize {
        self.edits.len()
    }
}

fn count_local_overlaps(edits: &[Candidate]) -> (usize, Option<&Candidate>) {
    let mut rejected = 0usize;
    let mut first_overlap = None;
    for pair in edits.windows(2) {
        if let [left, right] = pair
            && ranges_overlap(left.range(), right.range())
        {
            if first_overlap.is_none() {
                first_overlap = Some(right);
            }
            rejected = rejected.saturating_add(1);
        }
    }
    (rejected, first_overlap)
}

fn ranges_overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn apply_plan(before: &str, plan: &FamilyPlan) -> String {
    let mut out = before.to_owned();
    let mut ordered: Vec<&Candidate> = plan.edits().iter().collect();
    ordered.sort_by_key(|candidate| Reverse(candidate.range().start));
    for candidate in ordered {
        out.replace_range(candidate.range().clone(), candidate.replacement());
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use crate::format::rewrite::snapshot::{OwnerKind, Snapshot};
    use crate::format::rewrite::{RewriteFamily, Verification};
    use crate::{FmtOptions, FormatReport};
    use mdwright_document::ParseOptions;

    use super::*;

    #[test]
    fn family_plan_rejects_overlapping_local_edits() {
        let snapshot = Snapshot::parse_owned("*x*", ParseOptions::default()).expect("snapshot parses");
        let a = snapshot
            .candidate(
                OwnerKind::Document,
                0..2,
                "_x".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "a",
            )
            .expect("candidate");
        let b = snapshot
            .candidate(
                OwnerKind::Document,
                1..3,
                "x_".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "b",
            )
            .expect("candidate");

        assert!(matches!(
            FamilyPlan::build(RewriteFamily::Italic, vec![a, b]),
            FamilyPlanBuild::RejectedOverlap { rejected: 1 }
        ));
    }

    #[test]
    fn non_overlapping_family_plan_applies_all_edits() {
        let snapshot = Snapshot::parse_owned("- a\n- b\n", ParseOptions::default()).expect("snapshot parses");
        let a = snapshot
            .candidate(
                OwnerKind::ListItem,
                0..1,
                "+".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "a",
            )
            .expect("candidate");
        let b = snapshot
            .candidate(
                OwnerKind::ListItem,
                4..5,
                "+".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "b",
            )
            .expect("candidate");
        let FamilyPlanBuild::Ready(plan) = FamilyPlan::build(RewriteFamily::UnorderedList, vec![a, b]) else {
            panic!("plan should be ready");
        };

        assert_eq!(apply_plan(snapshot.source(), &plan), "+ a\n+ b\n");
    }

    #[test]
    fn convergence_cap_returns_original_bytes() {
        let doc = Document::parse("*x*").expect("fixture parses");
        let report = FormatReport {
            rewrite_rejected_convergence: 1,
            ..FormatReport::default()
        };
        let (out, _) = (doc.source().to_owned(), report);
        assert_eq!(out, "*x*");
    }

    #[test]
    fn isolated_failed_candidate_leaves_source_unchanged() {
        let snapshot = Snapshot::parse_owned("- a\n+ b\n", ParseOptions::default()).expect("snapshot parses");
        let candidate = snapshot
            .candidate(
                OwnerKind::Document,
                0..7,
                "- a\n- b\n".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "merge",
            )
            .expect("candidate");
        let FamilyPlanBuild::Ready(plan) = FamilyPlan::build(RewriteFamily::UnorderedList, vec![candidate]) else {
            panic!("plan should be ready");
        };
        let before = snapshot.source();
        let after = apply_plan(before, &plan);
        assert!(!verify_batch(
            before,
            &after,
            plan.edits(),
            &FmtOptions::default(),
            ParseOptions::default(),
        ));
    }
}
