//! The single chokepoint for every `pulldown_cmark::Parser` construction
//! in production `src/` code.
//!
//! Two helpers, [`events`] and [`events_with_offsets`], take a
//! [`CanonicalSource`] (the type-level proof that input bytes went
//! through [`crate::source::Source`] canonicalisation) and hand back
//! pulldown iterators. Every other place in the crate that needs a
//! pulldown parser routes through here, so we have one site to reason
//! about when adding a new emit-decision invariant or chasing a
//! per-construct pulldown quirk.
//!
//! Cross-reference: pulldown's per-construct behaviour we depend on is
//! documented in `docs/architecture/pulldown-model.md`. The drift tests
//! in `tests/pulldown_model.rs` fail when pulldown's behaviour changes
//! underneath us, forcing a documentation update before code changes.

use pulldown_cmark::{OffsetIter, Options, Parser};

use crate::{ParseOptions, source::CanonicalSource};

/// Build the pulldown option set for a document parse.
///
/// The safety ladder, the canonical-event walker, and `Ir::parse` all
/// route through this function so extension recognition stays coherent.
///
/// `cm::refs` does its own pre-pass for `[label]: dest` definitions with
/// base `CommonMark` options only; that's the one exception and lives
/// at its own (test-only) call site.
pub fn options(opts: ParseOptions) -> Options {
    let mut pulldown = Options::ENABLE_STRIKETHROUGH
        .union(Options::ENABLE_FOOTNOTES)
        .union(Options::ENABLE_TABLES)
        .union(Options::ENABLE_TASKLISTS);
    let extensions = opts.extensions();
    if extensions.definition_lists {
        pulldown.insert(Options::ENABLE_DEFINITION_LIST);
    }
    if extensions.heading_attribute_lists {
        pulldown.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    }
    pulldown
}

/// Parser iterator over canonical bytes. Returns the pulldown
/// `Parser` directly so callers retain pulldown's lifetime parameter
/// — wrapping the iterator buys nothing.
#[must_use]
pub fn events(src: CanonicalSource<'_>, opts: Options) -> Parser<'_> {
    Parser::new_ext(src.as_str(), opts)
}

/// Same as [`events`] but produces the offset iterator. Callers that
/// need absolute byte ranges (the IR builder, the safety ladder) use
/// this; everyone else uses [`events`].
#[must_use]
pub fn events_with_offsets(src: CanonicalSource<'_>, opts: Options) -> OffsetIter<'_> {
    Parser::new_ext(src.as_str(), opts).into_offset_iter()
}
