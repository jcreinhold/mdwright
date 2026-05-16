//! CommonMark-spec-aligned IR types.
//!
//! Each submodule owns one CM concept and exposes typed values whose
//! constructors enforce the round-trip invariant by construction. The
//! IR builder in [`crate::tree`] is the deep module that produces
//! these values; the format pipeline consumes them as final-form bytes.

pub(crate) mod inline;
pub(crate) mod refs;
