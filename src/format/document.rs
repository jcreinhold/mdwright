//! Top-level document printer — the iterative-draft formatter.
//!
//! Every render uses [`FlankSource::Draft`]: the safety ladder reads
//! flank bytes from a *previously-rendered draft*, sliced at the
//! position of each emphasis / strong / link / image site via a
//! lockstep correspondence map from source-tree `NodeId`s to draft-
//! tree `NodeId`s. The very first iteration uses the *source itself*
//! as the initial draft — source bytes are the best available flank
//! approximation before any IR-driven emit has run, and the existing
//! source-tree gives node positions for free (identity correspondence).
//! Subsequent iterations swap the draft for the previous iteration's
//! output. The loop returns when two consecutive iterations produce
//! the same bytes (a fixed point of "render source IR with draft-
//! derived flank").
//!
//! Trailing-newline and end-of-line policies run *after* convergence,
//! so the convergence comparison is on the structural payload only and
//! per-iteration cost stays close to one full IR render. See
//! `docs/architecture/two-pass.md` for the full design and bench
//! numbers.

use crate::cm::refs::ReferenceTable;
use crate::config::{FmtOptions, FormatMode};
use crate::format::block;
use crate::format::doc::{self, RenderOptions};
use crate::format::emit_safety::{DraftView, FlankSource};
use crate::format::pretty::PrettyCtx;
use crate::format::wrap::wrap_doc;
use crate::format::{ConvergenceError, apply_end_of_line, normalize_line_endings_lf, normalize_trailing_newline};
use crate::ir::{AdmonitionRegion, Frontmatter, Ir};
use crate::source::{CanonicalSource, Source};
use crate::tree::{NodeId, Tree};

/// Maximum number of render iterations before [`format_document`]
/// gives up with [`ConvergenceError::DidNotConverge`]. The first
/// iteration uses source as the draft (i.e. flank is read from
/// source bytes); subsequent iterations use the previous iteration's
/// output. The loop returns on the first pair of consecutive equal
/// outputs.
///
/// `2` is the principled bound: one render that establishes the
/// candidate output and one confirming render to verify it is a fixed
/// point under draft-flank substitution. If the second render
/// disagrees with the first, the safety ladder is in a flank-
/// decision cycle — a real design flaw to surface, not a transient
/// to absorb with more iterations. Treating the cycle as an error
/// and falling back to verbatim source emission keeps the convergence
/// invariant load-bearing.
const MAX_PASSES: u32 = 2;

/// Front-end used by `Document::format`. Renders the tree IR into a
/// Markdown string via the two-pass convergence loop.
///
/// # Errors
///
/// Returns [`ConvergenceError::DidNotConverge`] when the loop cannot
/// reach a fixed point within `MAX_PASSES` rectifying rounds. The
/// returned `last_draft` is the most recent pass-2 output and is
/// useful for diagnostics; the calling layer chooses between
/// verbatim-source fallback (`Document::format`) and surfacing the
/// error to the user (`Document::format_validated`).
pub(crate) fn format_document<'a>(
    source: &'a str,
    opts: &'a FmtOptions,
    tree: &'a Tree,
    frontmatter: Option<&'a Frontmatter>,
    admonitions: &'a [AdmonitionRegion],
    refs: &'a ReferenceTable,
) -> Result<String, ConvergenceError> {
    // Verbatim mode bypasses the IR render and is a fixed point by
    // construction — source bytes in, source bytes out.
    if opts.mode() == FormatMode::Verbatim {
        let mut out = render_with_flank(
            source,
            opts,
            tree,
            frontmatter,
            admonitions,
            refs,
            FlankSource::Isolated,
        );
        normalize_line_endings_lf(&mut out);
        return Ok(apply_tail_policies(out, opts, source));
    }

    // Initial render: use source as the draft. Identity map keeps
    // each source NodeId pointing at itself in the source tree, so
    // the safety ladder reads flank bytes from the actual bytes
    // around each source-tree node — the best approximation
    // available before any IR-driven emit has run.
    let identity_map: Vec<Option<NodeId>> = (0..tree.len())
        .map(|i| u32::try_from(i).ok().map(NodeId::from_index))
        .collect();
    let source_view = DraftView {
        bytes: source,
        tree,
        source_to_draft: &identity_map,
    };
    let mut current = render_with_flank(
        source,
        opts,
        tree,
        frontmatter,
        admonitions,
        refs,
        FlankSource::Draft(&source_view),
    );
    normalize_line_endings_lf(&mut current);

    for _ in 0..MAX_PASSES {
        // Re-parse the current draft so the next iteration can slice
        // its bytes at each emphasis / strong / link / image site.
        // `Source::new` canonicalisation is idempotent on a draft
        // that already went through normalize_line_endings_lf above
        // — no double cost.
        let draft_source = Source::new(&current);
        let draft_canonical = CanonicalSource::from_source(&draft_source);
        let draft_ir = Ir::parse(draft_canonical);
        let draft_tree = &draft_ir.tree;
        let draft_bytes = draft_source.canonical();
        let map = tree.corresponding_node_map(draft_tree);
        let view = DraftView {
            bytes: draft_bytes,
            tree: draft_tree,
            source_to_draft: &map,
        };
        let mut next = render_with_flank(
            source,
            opts,
            tree,
            frontmatter,
            admonitions,
            refs,
            FlankSource::Draft(&view),
        );
        normalize_line_endings_lf(&mut next);
        if next == current {
            return Ok(apply_tail_policies(current, opts, source));
        }
        current = next;
    }

    Err(ConvergenceError::DidNotConverge { last_draft: current })
}

/// Render the source IR with the supplied `flank`, returning the
/// structural payload — no trailing-newline normalisation, no EOL
/// substitution. The convergence loop calls this for pass 1 and each
/// pass-2 iteration; tail policies apply once at the end so the
/// fixed-point comparison sees only the structural shape.
fn render_with_flank<'a>(
    source: &'a str,
    opts: &'a FmtOptions,
    tree: &'a Tree,
    frontmatter: Option<&'a Frontmatter>,
    admonitions: &'a [AdmonitionRegion],
    refs: &'a ReferenceTable,
    flank: FlankSource<'a>,
) -> String {
    let ctx = PrettyCtx {
        source,
        opts,
        tree,
        frontmatter,
        admonitions,
        refs,
        flank,
    };
    let doc = if opts.mode() == FormatMode::Verbatim {
        doc::unbreakable(doc::text(source))
    } else {
        block::pretty_block_sequence(&ctx, tree.root())
    };
    let wrapped = wrap_doc(doc, opts.wrap());
    doc::render(&wrapped, &RenderOptions)
}

/// Apply post-convergence policies (trailing-newline, end-of-line).
/// These were inside the old single-pass `format_document`; moving
/// them out of the convergence loop keeps the fixed-point comparison
/// on the structural payload only — a trailing-newline policy change
/// must not count as "non-convergent".
fn apply_tail_policies(mut out: String, opts: &FmtOptions, source: &str) -> String {
    normalize_trailing_newline(&mut out, opts.trailing_newline(), source);
    apply_end_of_line(&mut out, opts.end_of_line(), source);
    out
}
