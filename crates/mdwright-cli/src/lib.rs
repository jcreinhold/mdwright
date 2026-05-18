#![forbid(unsafe_code)]

mod cli;
mod discover;

pub use cli::run_with_rules;
pub use discover::discover_markdown;
