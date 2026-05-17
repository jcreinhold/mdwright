//! The `LintRule` trait — `mdwright`'s open extension point.
//!
//! A rule is any `Send + Sync` value that names itself, describes
//! itself, and inspects a [`Document`] to produce [`Diagnostic`]s.
//! The standard library in [`crate::stdlib`] ships fifteen
//! implementors; user code adds more by implementing this trait on
//! a struct and dropping it into a [`RuleSet`](crate::RuleSet).
//!
//! ## Identity
//!
//! Each rule carries a stable kebab-case name. The name is the
//! identifier used in CLI flags (`--rules orphan-reference-link`),
//! configuration files, suppression comments, and diagnostic output.
//! Names must be unique within any `RuleSet` — duplicate insertion
//! fails (see [`RuleSet::add`](crate::RuleSet::add)).
//!
//! ## Emit pattern
//!
//! Rule implementations append [`Diagnostic`]s to the supplied
//! `Vec`. They should leave the diagnostic's `rule` field empty —
//! the dispatcher stamps it from `self.name()` after the call
//! returns, so rule code does not repeat its own name on every emit.

use crate::diagnostic::Diagnostic;
use crate::document::Document;

/// A lint check. Implementors may be unit structs (stdlib rules) or
/// carry configuration (regex patterns, allowlists, …).
pub trait LintRule: Send + Sync {
    /// Stable kebab-case identifier. Must be unique within any
    /// [`RuleSet`](crate::RuleSet).
    fn name(&self) -> &str;

    /// One-line summary for `mdwright list-rules`.
    fn description(&self) -> &str;

    /// Whether this rule is enabled in
    /// [`RuleSet::stdlib_defaults`](crate::RuleSet::stdlib_defaults).
    /// Most rules are on by default; the few opinionated or
    /// repair-focused checks return `false`.
    fn is_default(&self) -> bool {
        true
    }

    /// Advisory rules emit informational diagnostics that do not
    /// fail `mdwright check --check`. Their output still prints.
    fn is_advisory(&self) -> bool {
        false
    }

    /// Run the check against a parsed document. Append diagnostics
    /// to `out`. The dispatcher fills in each diagnostic's `rule`
    /// and `advisory` fields from `self.name()` and
    /// `self.is_advisory()` after the call returns.
    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>);
}
