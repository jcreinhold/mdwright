//! Typed inline values: text runs, code spans, HTML spans.
//!
//! Each type is `pub(crate)` and constructed exclusively by the IR
//! builder. Existence of a value is evidence that its bytes round-trip
//! through the `CommonMark` tokenizer under the scope it was built for.

pub(crate) mod code;
pub(crate) mod escape_policy;
pub(crate) mod html;
pub(crate) mod run;
