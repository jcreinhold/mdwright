//! Transactional byte-rewrite engine for formatter passes.
//!
//! Structural formatting is identity emit. Every opt-in byte rewrite
//! enters through this module as a parsed-owner candidate and is
//! applied only after the engine verifies the resulting document.

mod candidate;
mod engine;
mod signature;
mod snapshot;

pub(crate) use candidate::{Candidate, Phase, Verification};
pub(crate) use snapshot::{OwnerId, OwnerKind, Snapshot};

use crate::{FmtOptions, FormatReport};
use mdwright_document::{Document, ParseError};

pub(crate) fn apply_rewrites(doc: &Document, opts: &FmtOptions) -> Result<(String, FormatReport), ParseError> {
    engine::apply_rewrites(doc, opts)
}
