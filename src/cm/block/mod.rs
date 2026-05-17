//! Typed block values: heading, code block, block quote, thematic break.
//!
//! Each submodule owns one `CommonMark` §4 block kind. Constructors
//! refuse the impossible state — a [`heading::Heading`] cannot carry
//! a level outside 1..=6; a setext heading cannot carry level > 2;
//! a [`code::FencedCodeBlock`]'s fence is always strictly longer than
//! the longest body run of its fence character (CM §4.5); a
//! [`thematic::ThematicBreak`] always knows which of `-`, `*`, `_`
//! it should emit.
//!
//! These values live on [`crate::tree::Node::typed`] alongside the
//! legacy [`crate::tree::NodeKind`] enum. The Phase-R printer
//! (prompt 27) dispatches every block through this module's
//! [`TypedBlock::pretty`] — each typed value owns its serialisation.

pub(crate) mod code;
pub(crate) mod footnote;
pub(crate) mod heading;
pub(crate) mod html;
pub(crate) mod list;
pub(crate) mod paragraph;
pub(crate) mod quote;
pub(crate) mod table;
pub(crate) mod thematic;

use code::{FencedCodeBlock, IndentedCodeBlock};
use footnote::FootnoteDef;
use heading::Heading;
use html::HtmlBlock;
use list::ListBlock;
use paragraph::Paragraph;
use quote::BlockQuote;
use table::TableBlock;
use thematic::ThematicBreak;

use crate::format::doc::Doc;
use crate::format::pretty::PrettyCtx;
use crate::tree::NodeId;

/// One typed block value attached to a [`crate::tree::Node`]. The
/// variants mirror the `CommonMark` §4 and GFM §4.10 / extension
/// block kinds whose well-formedness invariants Phase R has lifted
/// into types. Post-prompt-26b every printable block kind has a
/// variant here; [`TypedBlock::pretty`] is the printer entry point.
#[derive(Clone, Debug)]
pub(crate) enum TypedBlock {
    Paragraph(Paragraph),
    Heading(Heading),
    FencedCodeBlock(FencedCodeBlock),
    IndentedCodeBlock(IndentedCodeBlock),
    HtmlBlock(HtmlBlock),
    BlockQuote(BlockQuote),
    ThematicBreak(ThematicBreak),
    ListBlock(ListBlock),
    Table(TableBlock),
    FootnoteDef(FootnoteDef),
}

impl TypedBlock {
    /// Render this block. The exhaustive match makes adding a variant
    /// a compile error here — surfacing the missing renderer rather
    /// than silently falling through.
    pub(crate) fn pretty<'a>(&self, ctx: &PrettyCtx<'a>, id: NodeId) -> Doc<'a> {
        match self {
            Self::Paragraph(p) => (*p).pretty(ctx, id),
            Self::Heading(h) => (*h).pretty(ctx, id),
            Self::FencedCodeBlock(c) => c.pretty(ctx, id),
            Self::IndentedCodeBlock(c) => c.pretty(ctx, id),
            Self::HtmlBlock(h) => h.pretty(ctx, id),
            Self::BlockQuote(q) => (*q).pretty(ctx, id),
            Self::ThematicBreak(t) => (*t).pretty(ctx, id),
            Self::ListBlock(l) => l.pretty(ctx, id),
            Self::Table(t) => t.pretty(ctx, id),
            Self::FootnoteDef(f) => f.pretty(ctx, id),
        }
    }
}
