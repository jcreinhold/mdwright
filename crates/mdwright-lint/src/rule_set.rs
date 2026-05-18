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

use mdwright_document::Document;

use crate::LintOptions;
use crate::diagnostic::Diagnostic;
use crate::rule::LintRule;
use crate::stdlib;
use crate::suppression::SuppressionMap;

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

    /// Look up a rule by its `name`.
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<&dyn LintRule> {
        self.rules.iter().find(|r| r.name() == name).map(|b| &**b)
    }

    /// Iterate over the names of every rule in the set.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.rules.iter().map(|r| r.name())
    }

    /// Run every rule in the set over `doc`.
    #[must_use]
    pub fn check(&self, doc: &Document) -> Vec<Diagnostic> {
        self.check_with(doc, LintOptions::default())
    }

    /// Run every rule in the set over `doc` under `opts`.
    #[must_use]
    pub fn check_with(&self, doc: &Document, opts: LintOptions) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for rule in self.iter() {
            let before = out.len();
            rule.check(doc, &mut out);
            let name_owned = rule.name().to_owned();
            let advisory = rule.is_advisory();
            for d in out.get_mut(before..).into_iter().flatten() {
                d.rule = std::borrow::Cow::Owned(name_owned.clone());
                d.advisory = advisory;
            }
        }

        if opts.respect_suppressions {
            let user_names: Vec<String> = self.iter().map(|r| r.name().to_owned()).collect();
            let mut known: Vec<&str> = stdlib::names().collect();
            for n in &user_names {
                let s: &str = n.as_str();
                if !known.contains(&s) {
                    known.push(s);
                }
            }
            let (map, unknown) = SuppressionMap::build(doc.source(), doc.line_index(), doc.suppressions(), &known);
            out.retain(|d| !map.suppresses(&d.rule, &d.span));
            out.extend(unknown);
        }

        out.sort_by(|a, b| {
            a.line
                .cmp(&b.line)
                .then(a.column.cmp(&b.column))
                .then_with(|| a.rule.cmp(&b.rule))
        });
        out
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

/// Consumes the set and yields its rules in insertion order.
///
/// Required by the CLI's `--rules` selector, which partitions the
/// available pool of rules into the user-requested subset without
/// cloning trait objects (`LintRule` is not `Clone`).
impl IntoIterator for RuleSet {
    type Item = Box<dyn LintRule>;
    type IntoIter = std::vec::IntoIter<Box<dyn LintRule>>;

    fn into_iter(self) -> Self::IntoIter {
        self.rules.into_iter()
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
    use crate::rule::LintRule;
    use mdwright_document::Document;

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

    #[test]
    fn by_name_finds_or_returns_none() -> anyhow::Result<()> {
        let mut rs = RuleSet::new();
        rs.add(Box::new(Noop("a"))).map_err(|e| anyhow::anyhow!("{e}"))?;
        rs.add(Box::new(Noop("b"))).map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_eq!(rs.by_name("a").map(LintRule::name), Some("a"));
        assert_eq!(rs.by_name("b").map(LintRule::name), Some("b"));
        assert!(rs.by_name("c").is_none());
        Ok(())
    }

    #[test]
    fn names_iterates_in_insertion_order() -> anyhow::Result<()> {
        let mut rs = RuleSet::new();
        rs.add(Box::new(Noop("alpha"))).map_err(|e| anyhow::anyhow!("{e}"))?;
        rs.add(Box::new(Noop("beta"))).map_err(|e| anyhow::anyhow!("{e}"))?;
        rs.add(Box::new(Noop("gamma"))).map_err(|e| anyhow::anyhow!("{e}"))?;
        let collected: Vec<&str> = rs.names().collect();
        assert_eq!(collected, vec!["alpha", "beta", "gamma"]);
        Ok(())
    }

    #[test]
    fn into_iter_yields_owned_boxes_in_insertion_order() -> anyhow::Result<()> {
        let mut rs = RuleSet::new();
        rs.add(Box::new(Noop("first"))).map_err(|e| anyhow::anyhow!("{e}"))?;
        rs.add(Box::new(Noop("second"))).map_err(|e| anyhow::anyhow!("{e}"))?;
        let names: Vec<String> = rs.into_iter().map(|r| r.name().to_owned()).collect();
        assert_eq!(names, vec!["first".to_owned(), "second".to_owned()]);
        Ok(())
    }
}
