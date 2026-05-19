//! The single chokepoint for every `pulldown_cmark::Parser` construction
//! in production `src/` code.
//!
//! [`collect_events_with_offsets`] takes a [`CanonicalSource`] (the
//! type-level proof that input bytes went through
//! [`crate::source::Source`] canonicalisation) and hands back pulldown
//! events with ranges. Every other place in the crate that needs a
//! pulldown parser routes through here, so we have one site to reason
//! about when adding a new emit-decision invariant or chasing a
//! per-construct pulldown quirk.
//!
//! Cross-reference: pulldown's per-construct behaviour we depend on is
//! documented in `docs/architecture/pulldown-model.md`. The drift tests
//! in `tests/pulldown_model.rs` fail when pulldown's behaviour changes
//! underneath us, forcing a documentation update before code changes.

use std::cell::Cell;
use std::ops::Range;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;

use pulldown_cmark::{Event, Options, Parser};

use crate::{ParseError, ParseOptions, source::CanonicalSource};

thread_local! {
    static SUPPRESS_PARSER_PANIC_HOOK: Cell<bool> = const { Cell::new(false) };
}

static INSTALL_PARSER_HOOK: Once = Once::new();

/// Build the pulldown option set for a document parse.
///
/// The safety ladder, the canonical-event walker, and `Ir::parse` all
/// route through this function so extension recognition stays coherent.
///
/// `cm::refs` does its own pre-pass for `[label]: dest` definitions with
/// base `CommonMark` options only; that's the one exception and lives
/// at its own (test-only) call site.
pub(crate) fn options(opts: ParseOptions) -> Options {
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

/// Collect parser events inside the document crate's panic boundary.
#[cfg(test)]
pub(crate) fn collect_events(src: CanonicalSource<'_>, opts: Options) -> Result<Vec<Event<'_>>, ParseError> {
    run_parser(src, || Parser::new_ext(src.as_str(), opts).collect())
}

/// Collect parser events with absolute byte ranges inside the document
/// crate's panic boundary.
pub(crate) fn collect_events_with_offsets(
    src: CanonicalSource<'_>,
    opts: Options,
) -> Result<Vec<(Event<'_>, Range<usize>)>, ParseError> {
    run_parser(src, || Parser::new_ext(src.as_str(), opts).into_offset_iter().collect())
}

fn run_parser<T>(src: CanonicalSource<'_>, f: impl FnOnce() -> T) -> Result<T, ParseError> {
    install_parser_panic_hook();
    let _guard = ParserPanicHookGuard::new();
    panic::catch_unwind(AssertUnwindSafe(f)).map_err(|_panic| ParseError::parser_panic(src.as_str().len()))
}

fn install_parser_panic_hook() {
    INSTALL_PARSER_HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if SUPPRESS_PARSER_PANIC_HOOK.with(Cell::get) {
                return;
            }
            previous(info);
        }));
    });
}

struct ParserPanicHookGuard;

impl ParserPanicHookGuard {
    fn new() -> Self {
        SUPPRESS_PARSER_PANIC_HOOK.with(|flag| flag.set(true));
        Self
    }
}

impl Drop for ParserPanicHookGuard {
    fn drop(&mut self) {
        SUPPRESS_PARSER_PANIC_HOOK.with(|flag| flag.set(false));
    }
}
