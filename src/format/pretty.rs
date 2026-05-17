//! Immutable context threaded through every typed-construct `pretty()`
//! method.
//!
//! `PrettyCtx` carries the source, the resolved formatter options, the
//! tree IR, the reference table, and the overlay-region inputs
//! (frontmatter, admonitions). It is a `Copy` value object — no
//! mutable state. Pass by `&PrettyCtx<'a>`.
//!
//! Math is **not** here: math regions are materialised into
//! [`NodeKind::Math`](crate::tree::NodeKind::Math) leaves during tree
//! construction, so the formatter dispatches on them through the
//! normal per-node path rather than consulting an overlay.
//!
//! Indent state is **not** here: container constructs that need to
//! prefix their body's emitted lines (blockquote, list item, footnote
//! definition) own the prefix decision and apply it inside their own
//! `pretty()` method.

use crate::cm::refs::ReferenceTable;
use crate::config::FmtOptions;
use crate::ir::{AdmonitionRegion, Frontmatter};
use crate::tree::Tree;

#[derive(Clone, Copy)]
pub(crate) struct PrettyCtx<'a> {
    pub source: &'a str,
    pub opts: &'a FmtOptions,
    pub tree: &'a Tree,
    pub frontmatter: Option<&'a Frontmatter>,
    pub admonitions: &'a [AdmonitionRegion],
    /// Resolved link reference definitions in insertion order.
    /// `LinkReferenceDefinition` is not a tree node — the table is
    /// the single source of truth.
    pub refs: &'a ReferenceTable,
}
