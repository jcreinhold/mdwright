//! Immutable context threaded through every typed-construct `pretty()`
//! method.
//!
//! `PrettyCtx` carries the source, the resolved formatter options, the
//! tree IR, the reference table, and the overlay-region inputs
//! (frontmatter, admonitions, math regions). It is a `Copy` value
//! object — no mutable state. Pass by `&PrettyCtx<'a>`.
//!
//! Indent state is **not** here: container constructs that need to
//! prefix their body's emitted lines (blockquote, list item, footnote
//! definition) own the prefix decision and apply it inside their own
//! `pretty()` method.

use crate::cm::math::MathRegion;
use crate::cm::refs::ReferenceTable;
use crate::config::FmtOptions;
use crate::ir::{AdmonitionRegion, Frontmatter};
use crate::tree::Tree;

#[derive(Clone, Copy)]
pub(crate) struct PrettyCtx<'a> {
    pub source: &'a str,
    pub opts: &'a FmtOptions,
    pub tree: &'a Tree,
    pub frontmatter: Option<&'a Frontmatter<'a>>,
    pub admonitions: &'a [AdmonitionRegion<'a>],
    /// Math regions in source order. Any block whose `raw_range`
    /// overlaps a region is emitted byte-verbatim from `source`,
    /// short-circuiting normal IR-driven emission. This keeps
    /// `\[ ... \]` (and the prose around it within the same block)
    /// pulldown-byte-identical between source and formatted output.
    pub math_regions: &'a [MathRegion],
    /// Resolved link reference definitions in insertion order.
    /// `LinkReferenceDefinition` is not a tree node — the table is
    /// the single source of truth.
    pub refs: &'a ReferenceTable,
}
