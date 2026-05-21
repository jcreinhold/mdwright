#![forbid(unsafe_code)]

mod browser_open;
mod cli;
mod discover;
mod html_highlight;
mod preview;

pub use cli::run_with_rules;
pub use discover::discover_markdown;
