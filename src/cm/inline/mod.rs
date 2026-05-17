//! Typed inline values: text runs, code spans, HTML spans.
//!
//! Each type is `pub(crate)` and constructed exclusively by the IR
//! builder. Existence of a value is evidence that its bytes round-trip
//! through the `CommonMark` tokenizer under the scope it was built for.

pub(crate) mod autolink;
pub(crate) mod code;
pub(crate) mod emphasis;
pub(crate) mod escape_policy;
pub(crate) mod footnote;
pub(crate) mod html;
pub(crate) mod link;
pub(crate) mod run;
pub(crate) mod strikethrough;
pub(crate) mod task;
