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

use crate::FmtOptions;
use mdwright_document::ParseOptions;

pub(crate) fn apply_rewrites(source: &str, opts: &FmtOptions, parse_options: ParseOptions) -> String {
    engine::apply_rewrites(source, opts, parse_options)
}
