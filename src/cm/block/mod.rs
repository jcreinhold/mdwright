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
//! legacy [`crate::tree::NodeKind`] enum. The legacy block formatter
//! keeps consuming `NodeKind` until prompt 27 swaps it; emitter
//! bridges that read the typed value land per-kind as that prompt
//! progresses.

// Accessors on the typed-block values land before prompt 27's
// emitter swap, so the bridges that will consume them are not yet
// in place. Suppress dead-code warnings on the submodules until then.
#[allow(dead_code)]
pub(crate) mod code;
#[allow(dead_code)]
pub(crate) mod heading;
#[allow(dead_code)]
pub(crate) mod list;
#[allow(dead_code)]
pub(crate) mod quote;
#[allow(dead_code)]
pub(crate) mod table;
#[allow(dead_code)]
pub(crate) mod thematic;

use code::{FencedCodeBlock, IndentedCodeBlock};
use heading::Heading;
use list::ListBlock;
use quote::BlockQuote;
use table::TableBlock;
use thematic::ThematicBreak;

/// One typed block value attached to a [`crate::tree::Node`]. The
/// variants mirror the `CommonMark` §4 and GFM §4.10 block kinds
/// whose well-formedness invariants Phase R prompts 24–26 lifted
/// into types. Paragraphs, HTML blocks, footnote definitions, and
/// link-reference definitions remain typed only by their `NodeKind`
/// variant until later prompts.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) enum TypedBlock<'a> {
    Heading(Heading),
    FencedCodeBlock(FencedCodeBlock<'a>),
    IndentedCodeBlock(IndentedCodeBlock<'a>),
    BlockQuote(BlockQuote),
    ThematicBreak(ThematicBreak),
    ListBlock(ListBlock),
    Table(TableBlock<'a>),
}
