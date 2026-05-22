#![forbid(unsafe_code)]

mod config;
pub mod documentation;

pub use config::{Config, ConfigError, LintMathJaxOptions, LintRulePreset, LintRuleSelection, RuleSelectionError};
