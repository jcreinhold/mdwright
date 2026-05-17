//! A set of rules to run against a document.
//!
//! `RuleSet` is a registry, not a bit-mask: it owns `Box<dyn
//! LintRule>` values keyed by name. The CLI builds one from
//! `RuleSet::stdlib_defaults()` and applies `+rule` / `-rule`
//! adjustments; library callers add their own rules in any
//! combination they like (see the crate-level extensibility
//! example).
//!
//! Names must be unique inside a set. `add` returns an error rather
//! than silently dropping or overriding — duplicate registration is
//! almost always a bug.

use std::fmt;

use crate::rule::LintRule;
use crate::stdlib;

/// An ordered, name-unique collection of [`LintRule`]s.
#[derive(Default)]
pub struct RuleSet {
    rules: Vec<Box<dyn LintRule>>,
}

impl RuleSet {
    /// An empty set; add rules with [`Self::add`].
    #[must_use]
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// The stdlib's curated default-on rules. Equivalent to
    /// [`crate::stdlib::defaults`].
    #[must_use]
    pub fn stdlib_defaults() -> Self {
        stdlib::defaults()
    }

    /// Every stdlib rule, including the default-off ones.
    /// Equivalent to [`crate::stdlib::all`].
    #[must_use]
    pub fn stdlib_all() -> Self {
        stdlib::all()
    }

    /// Insert a rule. Names must be unique within the set.
    ///
    /// # Errors
    ///
    /// Returns [`DuplicateRuleName`] if a rule with the same
    /// `name()` is already present.
    pub fn add(&mut self, rule: Box<dyn LintRule>) -> Result<&mut Self, DuplicateRuleName> {
        if self.contains(rule.name()) {
            return Err(DuplicateRuleName {
                name: rule.name().to_owned(),
            });
        }
        self.rules.push(rule);
        Ok(self)
    }

    /// Remove the rule with the given `name`. Returns `true` if a
    /// rule was removed, `false` if no rule had that name.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.rules.len();
        self.rules.retain(|r| r.name() != name);
        self.rules.len() != before
    }

    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.rules.iter().any(|r| r.name() == name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn LintRule> {
        self.rules.iter().map(|b| &**b)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

impl fmt::Debug for RuleSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuleSet")
            .field("rules", &self.rules.iter().map(|r| r.name()).collect::<Vec<_>>())
            .finish()
    }
}

/// Error returned by [`RuleSet::add`] when a name collides with an
/// already-registered rule.
#[derive(Debug, Clone)]
pub struct DuplicateRuleName {
    pub name: String,
}

impl fmt::Display for DuplicateRuleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rule already registered: {}", self.name)
    }
}

impl std::error::Error for DuplicateRuleName {}

#[cfg(test)]
mod tests {
    use super::{DuplicateRuleName, RuleSet};
    use crate::diagnostic::Diagnostic;
    use crate::document::Document;
    use crate::rule::LintRule;

    struct Noop(&'static str);
    impl LintRule for Noop {
        fn name(&self) -> &str {
            self.0
        }
        fn description(&self) -> &str {
            "noop"
        }
        fn check(&self, _doc: &Document, _out: &mut Vec<Diagnostic>) {}
    }

    #[test]
    fn add_and_contains() -> anyhow::Result<()> {
        let mut rs = RuleSet::new();
        rs.add(Box::new(Noop("a"))).map_err(|e| anyhow::anyhow!("{e}"))?;
        assert!(rs.contains("a"));
        assert!(!rs.contains("b"));
        Ok(())
    }

    #[test]
    fn duplicate_add_errors() -> anyhow::Result<()> {
        let mut rs = RuleSet::new();
        rs.add(Box::new(Noop("a"))).map_err(|e| anyhow::anyhow!("{e}"))?;
        let err = rs.add(Box::new(Noop("a")));
        assert!(matches!(err, Err(DuplicateRuleName { ref name }) if name == "a"));
        Ok(())
    }

    #[test]
    fn remove_works() -> anyhow::Result<()> {
        let mut rs = RuleSet::new();
        rs.add(Box::new(Noop("a"))).map_err(|e| anyhow::anyhow!("{e}"))?;
        assert!(rs.remove("a"));
        assert!(!rs.remove("a"));
        assert!(!rs.contains("a"));
        Ok(())
    }
}
