#![forbid(unsafe_code)]

mod lsp;

pub use lsp::serve;

#[cfg(test)]
mod lsp_tests;
