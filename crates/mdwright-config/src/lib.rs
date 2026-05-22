#![forbid(unsafe_code)]

mod config;
pub mod documentation;

pub use config::{Config, ConfigError, LintRenderOptions, LintRulePreset, LintRuleSelection, RuleSelectionError};
