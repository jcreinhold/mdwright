#![forbid(unsafe_code)]

mod lsp;

pub use lsp::serve;

#[doc(hidden)]
pub use lsp::build_service_for_tests;
