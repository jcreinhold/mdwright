//! The standard library of lint rules.
//!
//! Every rule here implements [`crate::LintRule`]. The constructors
//! [`defaults`] and [`all`] return ready-made [`RuleSet`]s; [`by_name`]
//! looks up a fresh instance of one stdlib rule by its kebab-case
//! name, used by the CLI's `--rules` parser.
//!
//! ## Rules
//!
//! | Name | Default | Advisory |
//! | --- | --- | --- |
//! | `unbalanced-backtick`      | yes | no  |
//! | `math/unbalanced-delim`    | yes | no  |
//! | `math/unbalanced-env`      | yes | no  |
//! | `math/unbalanced-braces`   | yes | no  |
//! | `math/render-compat`       | no  | no  |
//! | `adjacent-code-no-space`   | yes | no  |
//! | `heading-punctuation`      | yes | no  |
//! | `orphan-reference-link`    | yes | no  |
//! | `duplicate-link-label`     | yes | no  |
//! | `bare-url`                 | yes | no  |
//! | `trailing-whitespace`      | yes | no  |
//! | `inconsistent-list-marker` | yes | no  |
//! | `list-tightness-flipped`   | no  | yes |
//! | `duplicate-heading`        | yes | no  |
//! | `unicodeable-subscript`    | yes | yes |
//! | `info-string-typo`         | yes | yes |
//! | `stray-dollar`             | no  | no  |
//! | `latex-command`            | no  | no  |
//! | `escaped-emphasis`         | no  | no  |
//! | `subscript-damage`         | no  | no  |

mod adjacent_code;
mod bare_url;
mod duplicate_heading;
mod duplicate_link_label;
mod escaped_emphasis;
mod heading_punctuation;
mod inconsistent_list_marker;
mod info_string_typo;
mod latex_command;
mod list_tightness_flipped;
mod math_render;
mod math_unbalanced_braces;
mod math_unbalanced_delim;
mod math_unbalanced_env;
mod orphan_reference_link;
mod stray_dollar;
mod subscript_damage;
mod trailing_whitespace;
mod unbalanced_backtick;
mod unicodeable_subscript;

use crate::rule::LintRule;
use crate::rule_set::RuleSet;

pub use adjacent_code::AdjacentCodeNoSpace;
pub use bare_url::BareUrl;
pub use duplicate_heading::DuplicateHeading;
pub use duplicate_link_label::DuplicateLinkLabel;
pub use escaped_emphasis::EscapedEmphasis;
pub use heading_punctuation::HeadingPunctuation;
pub use inconsistent_list_marker::InconsistentListMarker;
pub use info_string_typo::InfoStringTypo;
pub use latex_command::LatexCommand;
pub use list_tightness_flipped::ListTightnessFlipped;
pub use math_render::RenderCompat;
pub use math_unbalanced_braces::MathUnbalancedBraces;
pub use math_unbalanced_delim::MathUnbalancedDelim;
pub use math_unbalanced_env::MathUnbalancedEnv;
pub use orphan_reference_link::OrphanReferenceLink;
pub use stray_dollar::StrayDollar;
pub use subscript_damage::SubscriptDamage;
pub use trailing_whitespace::TrailingWhitespace;
pub use unbalanced_backtick::UnbalancedBacktick;
pub use unicodeable_subscript::UnicodeableSubscript;

/// Every stdlib rule's kebab-case name, in registration order.
///
/// Parallel to the boxed-rule registry: the [`LintRule::name`] trait signature
/// returns `&str` (not `&'static str`) so user rules can borrow from
/// `self`, which means stdlib names can't be lifted off the rule
/// instances at compile time. The test
/// `names_match_all_boxed` catches drift between this array and the
/// rules themselves.
pub const NAMES: &[&str] = &[
    "unbalanced-backtick",
    "math/unbalanced-delim",
    "math/unbalanced-env",
    "math/unbalanced-braces",
    "math/render-compat",
    "adjacent-code-no-space",
    "heading-punctuation",
    "orphan-reference-link",
    "duplicate-link-label",
    "bare-url",
    "trailing-whitespace",
    "inconsistent-list-marker",
    "list-tightness-flipped",
    "duplicate-heading",
    "unicodeable-subscript",
    "info-string-typo",
    "stray-dollar",
    "latex-command",
    "escaped-emphasis",
    "subscript-damage",
];

/// Iterator over every stdlib rule's kebab-case name. Used by the
/// suppression-map builder to validate names in `<!-- mdwright: ... -->`
/// comments without instantiating every rule.
pub fn names() -> impl Iterator<Item = &'static str> {
    NAMES.iter().copied()
}

/// Construct every stdlib rule once. Used as the source-of-truth for
/// [`all`], [`defaults`], and [`by_name`].
fn all_boxed() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(UnbalancedBacktick),
        Box::new(MathUnbalancedDelim),
        Box::new(MathUnbalancedEnv),
        Box::new(MathUnbalancedBraces),
        Box::new(RenderCompat::new()),
        Box::new(AdjacentCodeNoSpace),
        Box::new(HeadingPunctuation),
        Box::new(OrphanReferenceLink),
        Box::new(DuplicateLinkLabel),
        Box::new(BareUrl),
        Box::new(TrailingWhitespace),
        Box::new(InconsistentListMarker),
        Box::new(ListTightnessFlipped),
        Box::new(DuplicateHeading),
        Box::new(UnicodeableSubscript),
        Box::new(InfoStringTypo::new()),
        Box::new(StrayDollar),
        Box::new(LatexCommand),
        Box::new(EscapedEmphasis),
        Box::new(SubscriptDamage),
    ]
}

/// Every stdlib rule, including the default-off ones.
#[must_use]
pub fn all() -> RuleSet {
    let mut rs = RuleSet::new();
    for rule in all_boxed() {
        // Stdlib rules have stable, unique names. Duplicate
        // registration here would be a programming error in this
        // crate, not a user mistake.
        // Stdlib rules have unique kebab-case names by construction;
        // a duplicate here is a programming error in this crate.
        let _unused = rs.add(rule);
    }
    rs
}

/// The curated default-on subset.
#[must_use]
pub fn defaults() -> RuleSet {
    let mut rs = RuleSet::new();
    for rule in all_boxed() {
        if rule.is_default() {
            // Stdlib rules have unique kebab-case names by construction;
            // a duplicate here is a programming error in this crate.
            let _unused = rs.add(rule);
        }
    }
    rs
}

/// Construct a fresh instance of one stdlib rule by kebab-case name.
/// Returns `None` if `name` is not a stdlib rule. Used by the CLI's
/// `--rules` parser to look up `+rule-name` modifiers.
#[must_use]
pub fn by_name(name: &str) -> Option<Box<dyn LintRule>> {
    all_boxed().into_iter().find(|r| r.name() == name)
}

#[cfg(test)]
mod tests {
    use super::{all, by_name, defaults};

    #[test]
    fn defaults_excludes_opt_in() {
        let rs = defaults();
        assert!(rs.contains("unbalanced-backtick"));
        assert!(rs.contains("heading-punctuation"));
        assert!(!rs.contains("stray-dollar"));
        assert!(!rs.contains("latex-command"));
        assert!(!rs.contains("escaped-emphasis"));
        assert!(!rs.contains("subscript-damage"));
    }

    #[test]
    fn all_includes_everything() {
        let rs = all();
        assert!(rs.contains("stray-dollar"));
        assert!(rs.contains("subscript-damage"));
        assert!(rs.contains("math/unbalanced-delim"));
        assert!(rs.contains("math/unbalanced-env"));
        assert!(rs.contains("math/unbalanced-braces"));
        assert!(rs.contains("math/render-compat"));
        assert!(rs.len() == 20);
    }

    #[test]
    fn by_name_known() {
        assert!(by_name("unbalanced-backtick").is_some());
        assert!(by_name("escaped-emphasis").is_some());
        assert!(by_name("does-not-exist").is_none());
    }

    #[test]
    fn names_match_all_boxed() {
        let from_rules: std::collections::BTreeSet<String> =
            super::all_boxed().iter().map(|r| r.name().to_owned()).collect();
        let from_const: std::collections::BTreeSet<String> = super::NAMES.iter().map(|s| (*s).to_owned()).collect();
        assert_eq!(from_rules, from_const, "stdlib::NAMES drift");
    }
}
