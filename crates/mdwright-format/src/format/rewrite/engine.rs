use std::cmp::Reverse;
use std::ops::Range;

use crate::format::canonicalise;
use crate::format::rewrite::candidate::{Candidate, RewriteFamily};
use crate::format::rewrite::signature::{verify_batch, verify_one};
use crate::format::rewrite::snapshot::Snapshot;
use crate::format::wrap_pass;
use crate::{FmtOptions, FormatReport, Wrap};
use mdwright_document::{Document, ParseError, ParseOptions};

const MAX_REWRITE_STEPS: u32 = 64;

const CANONICAL_FAMILY_ORDER: [RewriteFamily; 10] = [
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
];

pub(crate) fn apply_rewrites(doc: &Document, opts: &FmtOptions) -> Result<(String, FormatReport), ParseError> {
    let parse_options = doc.parse_options();
    let original = doc.source().to_owned();
    let mut out = original.clone();
    let mut report = FormatReport::default();

    for _ in 0..MAX_REWRITE_STEPS {
        let snapshot = snapshot_for(doc, &out, parse_options)?;
        if let Some(candidate) = commit_first_canonical_family(&snapshot, opts, parse_options, &mut report) {
            out = candidate;
            continue;
        }

        if !matches!(opts.wrap(), Wrap::Keep) {
            let snapshot = snapshot_for(doc, &out, parse_options)?;
            if let Some(candidate) = commit_terminal_wrap(&snapshot, opts, parse_options, &mut report) {
                out = candidate;
                continue;
            }
        }

        return Ok((out, report));
    }

    report.rewrite_committed = 0;
    report.rewrite_rejected_convergence = report.rewrite_rejected_convergence.saturating_add(1);
    tracing::warn!(
        target: "mdwright::rewrite",
        steps = MAX_REWRITE_STEPS,
        "rewrite-family pipeline did not reach a no-commit pass; leaving original source bytes unchanged",
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

fn commit_first_canonical_family(
    snapshot: &Snapshot<'_>,
    opts: &FmtOptions,
    parse_options: ParseOptions,
    report: &mut FormatReport,
) -> Option<String> {
    if !opts.has_any_canonicalisation() {
        return None;
    }
    for family in CANONICAL_FAMILY_ORDER {
        let mut candidates = collect_family(snapshot, opts, family);
        candidates.retain(|c| snapshot.source().get(c.range().clone()) != Some(c.replacement()));
        if candidates.is_empty() {
            continue;
        }
        if let Some(committed) =
            commit_canonical_family(snapshot.source(), opts, parse_options, family, candidates, report)
        {
            return Some(committed);
        }
    }
    None
}

/// Commit one canonical family, preferring a single atomic batch and
/// falling back to per-candidate verification when the batch fails.
///
/// The atomic batch is the fast path and the only viable path for
/// families whose candidates are interdependent: emphasis emits
/// separate open and close delimiter candidates, and list-marker
/// rewrites must land together or the list splits, so each such
/// candidate fails verification on its own. Per-candidate salvage is
/// therefore a fallback after a batch failure, never a pre-filter — for
/// those families the survivor set is empty and behaviour is unchanged,
/// while a family of independent candidates (one self-contained table
/// per candidate) keeps the candidates that verify on their own rather
/// than letting one unpreservable candidate veto the rest.
fn commit_canonical_family(
    before: &str,
    opts: &FmtOptions,
    parse_options: ParseOptions,
    family: RewriteFamily,
    candidates: Vec<Candidate>,
    report: &mut FormatReport,
) -> Option<String> {
    let kind = PlanKind::Family(family);
    report.rewrite_candidates = report.rewrite_candidates.saturating_add(candidates.len());
    let multi = candidates.len() > 1;

    match attempt_plan(before, opts, parse_options, kind, candidates.clone()) {
        PlanAttempt::Committed { result, edits } => {
            record_commit(report, kind, edits);
            return Some(result);
        }
        PlanAttempt::RejectedOverlap { rejected } => {
            report.rewrite_rejected_overlap = report.rewrite_rejected_overlap.saturating_add(rejected);
            return None;
        }
        PlanAttempt::Noop => return None,
        PlanAttempt::RejectedVerification { edits } => {
            if !multi {
                report.rewrite_rejected_verification = report.rewrite_rejected_verification.saturating_add(edits);
                return None;
            }
        }
    }

    let survivors = verify_candidates_individually(before, opts, parse_options, candidates, report);
    if survivors.is_empty() {
        return None;
    }
    let salvaged = survivors.len();
    match attempt_plan(before, opts, parse_options, kind, survivors) {
        PlanAttempt::Committed { result, edits } => {
            tracing::debug!(
                target: "mdwright::rewrite",
                family = ?kind,
                salvaged,
                "committed independent candidates after batch verification failed",
            );
            record_commit(report, kind, edits);
            Some(result)
        }
        PlanAttempt::RejectedOverlap { rejected } => {
            report.rewrite_rejected_overlap = report.rewrite_rejected_overlap.saturating_add(rejected);
            None
        }
        PlanAttempt::RejectedVerification { edits } => {
            report.rewrite_rejected_verification = report.rewrite_rejected_verification.saturating_add(edits);
            None
        }
        PlanAttempt::Noop => None,
    }
}

fn commit_terminal_wrap(
    snapshot: &Snapshot<'_>,
    opts: &FmtOptions,
    parse_options: ParseOptions,
    report: &mut FormatReport,
) -> Option<String> {
    let outcome = wrap_pass::collect_terminal_wrap_edits(snapshot, opts);
    report.rewrite_skipped_wrap = report.rewrite_skipped_wrap.saturating_add(outcome.skipped_unsupported);
    let mut edits = outcome.edits;
    edits.retain(|c| snapshot.source().get(c.range().clone()) != Some(c.replacement()));
    let edits = verify_candidates_individually(snapshot.source(), opts, parse_options, edits, report);
    verify_plan(
        snapshot.source(),
        opts,
        parse_options,
        PlanKind::TerminalWrap,
        edits,
        report,
    )
}

/// Keep only the candidates that preserve the document signature when
/// applied on their own, recording the rest as verification rejections.
///
/// Callers use this to salvage independent edits when an atomic batch
/// fails; a single candidate is returned unverified because the caller
/// re-checks it as a plan of one.
fn verify_candidates_individually(
    before: &str,
    opts: &FmtOptions,
    parse_options: ParseOptions,
    edits: Vec<Candidate>,
    report: &mut FormatReport,
) -> Vec<Candidate> {
    if edits.len() <= 1 {
        return edits;
    }
    let mut verified = Vec::with_capacity(edits.len());
    let mut rejected = 0usize;
    for candidate in edits {
        let mut after = before.to_owned();
        after.replace_range(candidate.range().clone(), candidate.replacement());
        if verify_one(before, &after, &candidate, opts, parse_options) {
            verified.push(candidate);
        } else {
            rejected = rejected.saturating_add(1);
        }
    }
    report.rewrite_rejected_verification = report.rewrite_rejected_verification.saturating_add(rejected);
    verified
}

fn verify_plan(
    before: &str,
    opts: &FmtOptions,
    parse_options: ParseOptions,
    kind: PlanKind,
    candidates: Vec<Candidate>,
    report: &mut FormatReport,
) -> Option<String> {
    report.rewrite_candidates = report.rewrite_candidates.saturating_add(candidates.len());
    match attempt_plan(before, opts, parse_options, kind, candidates) {
        PlanAttempt::Committed { result, edits } => {
            record_commit(report, kind, edits);
            Some(result)
        }
        PlanAttempt::RejectedOverlap { rejected } => {
            report.rewrite_rejected_overlap = report.rewrite_rejected_overlap.saturating_add(rejected);
            None
        }
        PlanAttempt::RejectedVerification { edits } => {
            report.rewrite_rejected_verification = report.rewrite_rejected_verification.saturating_add(edits);
            None
        }
        PlanAttempt::Noop => None,
    }
}

/// Outcome of building and verifying one plan, free of report
/// bookkeeping so callers can combine attempts (batch then salvage)
/// and account for the result exactly once.
enum PlanAttempt {
    Committed { result: String, edits: usize },
    Noop,
    RejectedOverlap { rejected: usize },
    RejectedVerification { edits: usize },
}

/// Build the plan, apply it, and run batch verification. Pure with
/// respect to the report; the caller decides how to record the result.
fn attempt_plan(
    before: &str,
    opts: &FmtOptions,
    parse_options: ParseOptions,
    kind: PlanKind,
    candidates: Vec<Candidate>,
) -> PlanAttempt {
    let plan = match FamilyPlan::build(kind, candidates) {
        FamilyPlanBuild::Ready(plan) => plan,
        FamilyPlanBuild::RejectedOverlap { rejected } => return PlanAttempt::RejectedOverlap { rejected },
        FamilyPlanBuild::Noop => return PlanAttempt::Noop,
    };

    let candidate = apply_plan(before, &plan);
    if candidate == before {
        return PlanAttempt::Noop;
    }
    if verify_batch(before, &candidate, plan.edits(), opts, parse_options) {
        return PlanAttempt::Committed {
            result: candidate,
            edits: plan.len(),
        };
    }

    let first = plan.edits().first();
    tracing::debug!(
        target: "mdwright::rewrite",
        family = ?plan.kind(),
        edits = plan.len(),
        first_label = first.map_or("", Candidate::label),
        first_owner = ?first.map(Candidate::owner),
        "rewrite plan failed batch verification",
    );
    PlanAttempt::RejectedVerification { edits: plan.len() }
}

fn record_commit(report: &mut FormatReport, kind: PlanKind, edits: usize) {
    report.rewrite_committed = report.rewrite_committed.saturating_add(edits);
    match kind {
        PlanKind::TerminalWrap => {
            report.rewrite_committed_wrap = report.rewrite_committed_wrap.saturating_add(edits);
        }
        PlanKind::Family(_) => {
            report.rewrite_committed_style = report.rewrite_committed_style.saturating_add(edits);
        }
    }
}

fn collect_family(snapshot: &Snapshot<'_>, opts: &FmtOptions, family: RewriteFamily) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    canonicalise::collect_family_candidates(snapshot, opts, family, &mut candidates);
    candidates
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanKind {
    Family(RewriteFamily),
    TerminalWrap,
}

#[derive(Clone, Debug)]
struct FamilyPlan {
    kind: PlanKind,
    edits: Vec<Candidate>,
}

enum FamilyPlanBuild {
    Noop,
    Ready(FamilyPlan),
    RejectedOverlap { rejected: usize },
}

impl FamilyPlan {
    fn build(kind: PlanKind, mut edits: Vec<Candidate>) -> FamilyPlanBuild {
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
                family = ?kind,
                rejected,
                first_label = first_overlap.map_or("", Candidate::label),
                first_owner = ?first_overlap.map(Candidate::owner),
                "skipped rewrite family: local edits overlap",
            );
            return FamilyPlanBuild::RejectedOverlap { rejected };
        }
        FamilyPlanBuild::Ready(Self { kind, edits })
    }

    fn kind(&self) -> PlanKind {
        self.kind
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
            FamilyPlan::build(PlanKind::Family(RewriteFamily::Italic), vec![a, b]),
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
        let FamilyPlanBuild::Ready(plan) =
            FamilyPlan::build(PlanKind::Family(RewriteFamily::UnorderedList), vec![a, b])
        else {
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
        let FamilyPlanBuild::Ready(plan) =
            FamilyPlan::build(PlanKind::Family(RewriteFamily::UnorderedList), vec![candidate])
        else {
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

    #[test]
    fn terminal_wrap_filters_individually_invalid_candidates() {
        let snapshot =
            Snapshot::parse_owned("alpha beta gamma\n\nkeep me\n", ParseOptions::default()).expect("snapshot parses");
        let good = snapshot
            .candidate(
                OwnerKind::Document,
                0..17,
                "alpha beta\ngamma\n".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "wrap-good",
            )
            .expect("candidate");
        let bad = snapshot
            .candidate(
                OwnerKind::Document,
                18..25,
                "drop me".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "wrap-bad",
            )
            .expect("candidate");
        let mut report = FormatReport::default();

        let verified = verify_candidates_individually(
            snapshot.source(),
            &FmtOptions::default(),
            ParseOptions::default(),
            vec![good, bad],
            &mut report,
        );

        assert_eq!(verified.len(), 1);
        assert_eq!(verified.first().map(Candidate::label), Some("wrap-good"));
        assert_eq!(report.rewrite_rejected_verification, 1);
    }

    #[test]
    fn family_salvages_independent_candidate_when_batch_fails() {
        // `***` -> `---` keeps the thematic-break event, so it verifies
        // alone; rewriting the paragraph text changes the signature and
        // must not veto the preservable sibling.
        let source = "***\n\nhello\n";
        let snapshot = Snapshot::parse_owned(source, ParseOptions::default()).expect("snapshot parses");
        let preserving = snapshot
            .candidate(
                OwnerKind::Document,
                0..3,
                "---".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "good",
            )
            .expect("candidate");
        let breaking = snapshot
            .candidate(
                OwnerKind::Document,
                5..10,
                "world".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "bad",
            )
            .expect("candidate");
        let mut report = FormatReport::default();

        let committed = commit_canonical_family(
            source,
            &FmtOptions::default(),
            ParseOptions::default(),
            RewriteFamily::Table,
            vec![preserving, breaking],
            &mut report,
        );

        assert_eq!(committed.as_deref(), Some("---\n\nhello\n"));
        assert_eq!(report.rewrite_candidates, 2);
        assert_eq!(report.rewrite_committed, 1);
        assert_eq!(report.rewrite_committed_style, 1);
        assert_eq!(report.rewrite_rejected_verification, 1);
    }

    #[test]
    fn family_salvage_commits_nothing_when_no_candidate_verifies() {
        let source = "hello world\n";
        let snapshot = Snapshot::parse_owned(source, ParseOptions::default()).expect("snapshot parses");
        let first = snapshot
            .candidate(
                OwnerKind::Document,
                0..5,
                "HELLO".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "a",
            )
            .expect("candidate");
        let second = snapshot
            .candidate(
                OwnerKind::Document,
                6..11,
                "WORLD".to_owned(),
                Verification::PreserveMarkdownAndMath,
                "b",
            )
            .expect("candidate");
        let mut report = FormatReport::default();

        let committed = commit_canonical_family(
            source,
            &FmtOptions::default(),
            ParseOptions::default(),
            RewriteFamily::Table,
            vec![first, second],
            &mut report,
        );

        assert_eq!(committed, None);
        assert_eq!(report.rewrite_candidates, 2);
        assert_eq!(report.rewrite_committed, 0);
        assert_eq!(report.rewrite_rejected_verification, 2);
    }
}
