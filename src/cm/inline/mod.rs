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
// Strikethrough/TaskMarker accessors land before prompt 27's
// per-construct emitters — some helpers are wired only by the
// dispatcher that arrives there.
#[allow(dead_code)]
pub(crate) mod strikethrough;
#[allow(dead_code)]
pub(crate) mod task;
