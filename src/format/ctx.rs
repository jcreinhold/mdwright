//! Immutable context threaded through the block/inline serializers.
//!
//! `Ctx` carries the source, the resolved formatter options, and the
//! tree IR — everything the per-node renderers read but never write.
//! Indent state is **not** here: it lives in the `Doc` IR via
//! `Nest`, so recursion through the serializer stays pure.

use crate::config::FmtOptions;
use crate::format::math::MathRegion;
use crate::ir::{AdmonitionRegion, Frontmatter};
use crate::tree::Tree;

#[derive(Clone, Copy)]
pub(crate) struct Ctx<'a> {
    pub source: &'a str,
    pub opts: &'a FmtOptions,
    pub tree: &'a Tree<'a>,
    pub frontmatter: Option<&'a Frontmatter<'a>>,
    pub admonitions: &'a [AdmonitionRegion<'a>],
    /// Math regions in source order. Any block whose `raw_range`
    /// overlaps a region is emitted byte-verbatim from `source`,
    /// short-circuiting normal IR-driven emission. This keeps
    /// `\[ ... \]` (and the prose around it within the same block)
    /// pulldown-byte-identical between source and formatted output.
    pub math_regions: &'a [MathRegion],
}
