//! Single-pass MathJax compatibility check.

use mdwright_latex::{CommandEvent, SourceSpan, inspect_math_body};

use crate::profile::{MathJaxProfile, PackageMask, package_from_name, package_name};
use crate::tables::{COMMAND_OVERLAY, ENVIRONMENT_TABLE, lookup_overlay};

/// One compatibility issue found in a math body. Spans are byte ranges into
/// the math-body source given to `check_math_body`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MathJaxIssue {
    /// A command MathJax does not ship in any package the profile knows about.
    UnsupportedCommand {
        /// Command name without the leading backslash.
        name: String,
        /// Byte range covering the command token.
        span: SourceSpan,
    },
    /// A command MathJax can render, but only with a package that this profile
    /// does not load. Suggest the package name in `package`.
    MissingPackage {
        /// Command name without the leading backslash.
        name: String,
        /// Canonical name of the package the user should load.
        package: &'static str,
        /// Byte range covering the command token.
        span: SourceSpan,
    },
    /// An environment MathJax does not ship in any package the profile knows.
    UnsupportedEnvironment {
        /// Environment name as written between the braces.
        name: String,
        /// Byte range covering `\begin{name}` through the closing brace.
        span: SourceSpan,
    },
    /// An environment that requires a package this profile does not load.
    MissingPackageEnv {
        /// Environment name as written between the braces.
        name: String,
        /// Canonical name of the package the user should load.
        package: &'static str,
        /// Byte range covering `\begin{name}` through the closing brace.
        span: SourceSpan,
    },
    /// A math-mode command used inside a `\text{...}` region, where MathJax
    /// will treat it as plain text rather than rendering it.
    MathCommandInTextMode {
        /// Command name without the leading backslash.
        name: String,
        /// Byte range covering the command token.
        span: SourceSpan,
    },
}

/// Check `source` (one math body, no enclosing delimiters) against `profile`.
///
/// The check is single-pass over the lexer event stream from `mdwright-latex`:
/// each command and environment is classified into "ok" / "needs package" /
/// "unsupported" by consulting the profile's package mask and the overlay
/// tables. Issues come back in source order; the result is empty when the
/// body is fully compatible.
#[must_use]
pub fn check_math_body(source: &str, profile: &MathJaxProfile) -> Vec<MathJaxIssue> {
    let events = inspect_math_body(source);
    let mut issues = Vec::new();
    let mut text_depth: usize = 0;

    for event in events {
        match event {
            CommandEvent::TextModeEnter { .. } => {
                text_depth = text_depth.saturating_add(1);
            }
            CommandEvent::TextModeExit { .. } => {
                text_depth = text_depth.saturating_sub(1);
            }
            CommandEvent::Command { name, span } => {
                if text_depth > 0 {
                    if is_math_only_command(name) {
                        issues.push(MathJaxIssue::MathCommandInTextMode {
                            name: name.to_owned(),
                            span,
                        });
                    }
                    continue;
                }
                if let Some(issue) = classify_command(name, span, profile) {
                    issues.push(issue);
                }
            }
            CommandEvent::EnvironmentEnter { name, span } => {
                if let Some(issue) = classify_environment(name, span, profile) {
                    issues.push(issue);
                }
            }
            CommandEvent::EnvironmentExit { .. } => {}
        }
    }

    issues
}

fn classify_command(name: &str, span: SourceSpan, profile: &MathJaxProfile) -> Option<MathJaxIssue> {
    if profile.has_macro(name) {
        return None;
    }
    if is_structural_macro(name) {
        return None;
    }
    if let Some(entry) = lookup_overlay(COMMAND_OVERLAY, name) {
        return resolve_package(name, span, entry.package, profile, false);
    }
    if let Some(info) = mdwright_latex::lookup_command(name) {
        if let Some(mask) = package_from_name(info.package()) {
            return resolve_package(name, span, mask, profile, false);
        }
        return Some(MathJaxIssue::UnsupportedCommand {
            name: name.to_owned(),
            span,
        });
    }
    Some(MathJaxIssue::UnsupportedCommand {
        name: name.to_owned(),
        span,
    })
}

fn classify_environment(name: &str, span: SourceSpan, profile: &MathJaxProfile) -> Option<MathJaxIssue> {
    if let Some(entry) = lookup_overlay(ENVIRONMENT_TABLE, name) {
        return resolve_package(name, span, entry.package, profile, true);
    }
    Some(MathJaxIssue::UnsupportedEnvironment {
        name: name.to_owned(),
        span,
    })
}

fn resolve_package(
    name: &str,
    span: SourceSpan,
    mask: PackageMask,
    profile: &MathJaxProfile,
    is_environment: bool,
) -> Option<MathJaxIssue> {
    if profile.has_package(mask) {
        return None;
    }
    let package = package_name(mask);
    Some(if is_environment {
        MathJaxIssue::MissingPackageEnv {
            name: name.to_owned(),
            package,
            span,
        }
    } else {
        MathJaxIssue::MissingPackage {
            name: name.to_owned(),
            package,
            span,
        }
    })
}

/// Structural commands `inspect_math_body` reports but which MathJax always
/// understands as part of the base grammar (not as user-visible commands).
fn is_structural_macro(name: &str) -> bool {
    matches!(
        name,
        "left"
            | "right"
            | "bigl"
            | "bigr"
            | "Bigl"
            | "Bigr"
            | "biggl"
            | "biggr"
            | "Biggl"
            | "Biggr"
            | "big"
            | "Big"
            | "bigg"
            | "Bigg"
            | "text"
            | "textbf"
            | "textit"
            | "textrm"
            | "textsf"
            | "texttt"
            | "textnormal"
            | "mbox"
            | "hbox"
    )
}

/// Whether `name` is a math-mode-only command. Used to decide whether a
/// command inside `\text{...}` is a likely rendering mistake. The list is
/// deliberately small: only commands that have a clear meaning in math mode
/// and would visibly fail in text mode.
fn is_math_only_command(name: &str) -> bool {
    if let Some(info) = mdwright_latex::lookup_command(name) {
        use mdwright_latex::CommandCategory;
        return matches!(
            info.category(),
            CommandCategory::Greek
                | CommandCategory::BinaryOperator
                | CommandCategory::Relation
                | CommandCategory::Arrow
                | CommandCategory::LargeOperator
                | CommandCategory::Accent
                | CommandCategory::Delimiter
        );
    }
    false
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::wildcard_enum_match_arm,
        reason = "tests assert diagnostic shape against fixed inputs"
    )]

    use super::*;

    fn issues(source: &str, profile: &MathJaxProfile) -> Vec<MathJaxIssue> {
        check_math_body(source, profile)
    }

    #[test]
    fn well_formed_math_produces_no_issues() {
        let profile = MathJaxProfile::v3_default();
        assert!(issues(r"\alpha + \beta = \gamma", &profile).is_empty());
        assert!(issues(r"\frac{a}{b} + \sqrt{x}", &profile).is_empty());
    }

    #[test]
    fn ams_commands_pass_under_default_profile() {
        let profile = MathJaxProfile::v3_default();
        assert!(issues(r"\dfrac{a}{b}", &profile).is_empty());
        assert!(issues(r"\mathbb{R}", &profile).is_empty());
    }

    #[test]
    fn chemistry_command_requires_mhchem() {
        let profile = MathJaxProfile::v3_default();
        let found = issues(r"\ce{H2O}", &profile);
        assert!(matches!(
            found.as_slice(),
            [MathJaxIssue::MissingPackage { name, package: "mhchem", .. }] if name == "ce"
        ));
    }

    #[test]
    fn loading_mhchem_clears_chemistry_diagnostic() {
        let profile = MathJaxProfile::v3_default().with_package("mhchem");
        assert!(issues(r"\ce{H2O}", &profile).is_empty());
    }

    #[test]
    fn physics_commands_require_physics_package() {
        let profile = MathJaxProfile::v3_default();
        let found = issues(r"\bra{\psi}\ket{\phi}", &profile);
        let names: Vec<&str> = found
            .iter()
            .filter_map(|issue| match issue {
                MathJaxIssue::MissingPackage {
                    name,
                    package: "physics",
                    ..
                } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["bra", "ket"]);
    }

    #[test]
    fn definitely_unknown_command_is_unsupported() {
        let profile = MathJaxProfile::v3_default();
        let found = issues(r"\nosuchmathjaxcommandever", &profile);
        assert!(matches!(
            found.as_slice(),
            [MathJaxIssue::UnsupportedCommand { name, .. }] if name == "nosuchmathjaxcommandever"
        ));
    }

    #[test]
    fn user_macro_silences_unsupported_command() {
        let profile = MathJaxProfile::v3_default().with_macro("RR", 0);
        assert!(issues(r"\RR", &profile).is_empty());
    }

    #[test]
    fn unknown_environment_is_unsupported() {
        let profile = MathJaxProfile::v3_default();
        let found = issues(r"\begin{tikzpicture}x\end{tikzpicture}", &profile);
        assert!(matches!(
            found.as_slice(),
            [MathJaxIssue::UnsupportedEnvironment { name, .. }] if name == "tikzpicture"
        ));
    }

    #[test]
    fn amscd_environment_needs_package() {
        let profile = MathJaxProfile::v3_default();
        let found = issues(r"\begin{CD}A @>>> B\end{CD}", &profile);
        assert!(matches!(
            found.first(),
            Some(MathJaxIssue::MissingPackageEnv {
                name,
                package: "amscd",
                ..
            }) if name == "CD"
        ));
    }

    #[test]
    fn math_command_inside_text_is_flagged() {
        let profile = MathJaxProfile::v3_default();
        let found = issues(r"\text{the symbol \alpha here}", &profile);
        assert!(matches!(
            found.as_slice(),
            [MathJaxIssue::MathCommandInTextMode { name, .. }] if name == "alpha"
        ));
    }

    #[test]
    fn math_command_outside_text_is_not_flagged() {
        let profile = MathJaxProfile::v3_default();
        assert!(issues(r"\alpha + \beta", &profile).is_empty());
    }

    #[test]
    fn color_needs_color_package() {
        let profile = MathJaxProfile::v3_default();
        let found = issues(r"\color{red} x", &profile);
        assert!(matches!(
            found.first(),
            Some(MathJaxIssue::MissingPackage {
                name,
                package: "color",
                ..
            }) if name == "color"
        ));
    }

    #[test]
    fn structural_left_right_are_silent() {
        let profile = MathJaxProfile::v3_default();
        assert!(issues(r"\left( x \right)", &profile).is_empty());
    }
}
