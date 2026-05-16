//! Typed inline values: text runs, code spans, HTML spans.
//!
//! Each type is `pub(crate)` and constructed exclusively by the IR
//! builder. Existence of a value is evidence that its bytes round-trip
//! through the `CommonMark` tokenizer under the scope it was built for.

#[allow(dead_code)]
pub(crate) mod autolink;
#[allow(dead_code)]
pub(crate) mod code;
#[allow(dead_code)]
pub(crate) mod emphasis;
pub(crate) mod escape_policy;
#[allow(dead_code)]
pub(crate) mod footnote;
#[allow(dead_code)]
pub(crate) mod html;
#[allow(dead_code)]
pub(crate) mod link;
#[allow(dead_code)]
pub(crate) mod run;
// Strikethrough/TaskMarker accessors land before prompt 27's
// per-construct emitters — some helpers are wired only by the
// dispatcher that arrives there.
#[allow(dead_code)]
pub(crate) mod strikethrough;
#[allow(dead_code)]
pub(crate) mod task;
