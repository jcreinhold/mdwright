//! `since:` versions for every stdlib rule. The xtask doc-rules
//! generator emits this into per-rule frontmatter so external readers
//! can tell when a rule shipped.
//!
//! Hand-maintained: when a new stdlib rule lands, add `(name, "x.y.z")`
//! here in the same change. The `rule_docs_in_sync` test fails if a
//! rule is missing from this table.

pub const SINCE: &[(&str, &str)] = &[
    ("unbalanced-backtick", "0.1.0"),
    ("math/unbalanced-delim", "0.1.0"),
    ("math/unbalanced-env", "0.1.0"),
    ("math/unbalanced-braces", "0.1.0"),
    ("adjacent-code-no-space", "0.1.0"),
    ("heading-punctuation", "0.1.0"),
    ("orphan-reference-link", "0.1.0"),
    ("duplicate-link-label", "0.1.0"),
    ("bare-url", "0.1.0"),
    ("trailing-whitespace", "0.1.0"),
    ("inconsistent-list-marker", "0.1.0"),
    ("list-tightness-flipped", "0.2.0"),
    ("duplicate-heading", "0.2.0"),
    ("unicodeable-subscript", "0.2.0"),
    ("info-string-typo", "0.2.0"),
    ("stray-dollar", "0.1.0"),
    ("latex-command", "0.1.0"),
    ("escaped-emphasis", "0.1.0"),
    ("subscript-damage", "0.2.0"),
];

#[must_use]
pub fn version_for(name: &str) -> Option<&'static str> {
    SINCE.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}
