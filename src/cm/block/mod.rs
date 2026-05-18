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
//! [`crate::tree::NodeKind`] enum. The structural-emit formatter is
//! identity (see `crate::format::document::format_document`), so the
//! typed values exist for IR consumers (lint rules, the canonicalise
//! pass) rather than for per-construct serialisation.

pub(crate) mod code;
pub(crate) mod definition_list;
pub(crate) mod footnote;
pub(crate) mod heading;
pub(crate) mod html;
pub(crate) mod list;
pub(crate) mod paragraph;
pub(crate) mod paragraph_safety;
pub(crate) mod quote;
pub(crate) mod table;
pub(crate) mod thematic;

use code::{FencedCodeBlock, IndentedCodeBlock};
use definition_list::DefinitionList;
use footnote::FootnoteDef;
use heading::Heading;
use html::HtmlBlock;
use list::ListBlock;
use paragraph::Paragraph;
use quote::BlockQuote;
use table::TableBlock;
use thematic::ThematicBreak;

/// One typed block value attached to a [`crate::tree::Node`]. The
/// variants mirror the `CommonMark` §4 and GFM §4.10 / extension
/// block kinds. `ListBlock` is read by `Document::typed_list_blocks`
/// for lint rules; the other variants are scaffolding for future
/// canonicalise rewrites and are constructed but not yet inspected.
#[allow(dead_code)]
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
    DefinitionList(DefinitionList),
}
