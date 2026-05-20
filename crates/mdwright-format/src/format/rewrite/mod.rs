//! Rewrite-family pipeline for formatter passes.
//!
//! Structural formatting is identity emit. Every opt-in byte rewrite
//! enters through this module as a member of one rewrite family. A
//! family commits only after its local edits are non-overlapping and
//! verification accepts the resulting document.

mod candidate;
mod engine;
mod signature;
mod snapshot;

pub(crate) use candidate::{Candidate, RewriteFamily, Verification};
pub(crate) use snapshot::{OwnerId, OwnerKind, Snapshot};

use crate::{FmtOptions, FormatReport};
use mdwright_document::{Document, ParseError};

pub(crate) fn apply_rewrites(doc: &Document, opts: &FmtOptions) -> Result<(String, FormatReport), ParseError> {
    engine::apply_rewrites(doc, opts)
}
