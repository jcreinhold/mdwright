//! Renderer profile.
//!
//! A profile records *which renderer* a math body is checked against, which
//! package set that renderer has loaded, and which user macros are in scope.
//! Compatibility tables live in `tables`; the profile is the *consumer* of
//! those tables.

use std::collections::HashMap;

/// Math renderer that consumes the TeX-shaped source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renderer {
    /// MathJax v3 (mjs3, the current stable line).
    MathJaxV3,
    /// KaTeX (any release in the 0.16+ feature window).
    Katex,
}

impl Renderer {
    /// User-facing name. Goes into lint diagnostic messages.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MathJaxV3 => "MathJax v3",
            Self::Katex => "KaTeX",
        }
    }

    /// User-facing noun for "package" when speaking about this renderer:
    /// MathJax says *package*, KaTeX says *extension*. Used by lint
    /// diagnostic text so the message reads naturally for the active
    /// renderer.
    #[must_use]
    pub const fn package_noun(self) -> &'static str {
        match self {
            Self::MathJaxV3 => "package",
            Self::Katex => "extension",
        }
    }
}

/// Package bitmask, shared across renderers. Each renderer's table maps its
/// commands to one of these bits; the same bit (e.g. `MHCHEM`) means
/// "MathJax's mhchem extension" or "KaTeX's mhchem extension" depending on
/// the active profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PackageMask(u32);

impl PackageMask {
    pub(crate) const BASE: Self = Self(1 << 0);
    pub(crate) const AMS: Self = Self(1 << 1);
    pub(crate) const NEWCOMMAND: Self = Self(1 << 2);
    pub(crate) const CONFIGMACROS: Self = Self(1 << 3);
    pub(crate) const BOLDSYMBOL: Self = Self(1 << 4);
    pub(crate) const REQUIRE: Self = Self(1 << 5);
    pub(crate) const NOUNDEFINED: Self = Self(1 << 6);
    pub(crate) const COLOR: Self = Self(1 << 7);
    pub(crate) const CANCEL: Self = Self(1 << 8);
    pub(crate) const ENCLOSE: Self = Self(1 << 9);
    pub(crate) const MHCHEM: Self = Self(1 << 10);
    pub(crate) const PHYSICS: Self = Self(1 << 11);
    pub(crate) const AMSCD: Self = Self(1 << 12);
    pub(crate) const BRACEMATCH: Self = Self(1 << 13);
    pub(crate) const TEXTMACROS: Self = Self(1 << 14);
    pub(crate) const MATHTOOLS: Self = Self(1 << 15);

    pub(crate) const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Resolve a package name (as users would write in config) to its mask bit.
/// Returns `None` for unknown names so callers can surface them. Names follow
/// the renderer's own conventions; both MathJax and KaTeX use the same names
/// (`mhchem`, `physics`, `color`, `cancel`, …) so one resolver works for both.
pub(crate) fn package_from_name(name: &str) -> Option<PackageMask> {
    match name {
        "base" => Some(PackageMask::BASE),
        "ams" => Some(PackageMask::AMS),
        "newcommand" => Some(PackageMask::NEWCOMMAND),
        "configmacros" => Some(PackageMask::CONFIGMACROS),
        "boldsymbol" => Some(PackageMask::BOLDSYMBOL),
        "require" => Some(PackageMask::REQUIRE),
        "noundefined" => Some(PackageMask::NOUNDEFINED),
        "color" => Some(PackageMask::COLOR),
        "cancel" => Some(PackageMask::CANCEL),
        "enclose" => Some(PackageMask::ENCLOSE),
        "mhchem" => Some(PackageMask::MHCHEM),
        "physics" => Some(PackageMask::PHYSICS),
        "amscd" => Some(PackageMask::AMSCD),
        "bracematch" => Some(PackageMask::BRACEMATCH),
        "textmacros" => Some(PackageMask::TEXTMACROS),
        "mathtools" => Some(PackageMask::MATHTOOLS),
        _ => None,
    }
}

/// Canonical user-facing name for a single package mask. Used in diagnostic text.
pub(crate) fn package_name(mask: PackageMask) -> &'static str {
    if mask.contains(PackageMask::BASE) {
        "base"
    } else if mask.contains(PackageMask::AMS) {
        "ams"
    } else if mask.contains(PackageMask::MHCHEM) {
        "mhchem"
    } else if mask.contains(PackageMask::PHYSICS) {
        "physics"
    } else if mask.contains(PackageMask::COLOR) {
        "color"
    } else if mask.contains(PackageMask::CANCEL) {
        "cancel"
    } else if mask.contains(PackageMask::ENCLOSE) {
        "enclose"
    } else if mask.contains(PackageMask::AMSCD) {
        "amscd"
    } else if mask.contains(PackageMask::BOLDSYMBOL) {
        "boldsymbol"
    } else if mask.contains(PackageMask::NEWCOMMAND) {
        "newcommand"
    } else if mask.contains(PackageMask::CONFIGMACROS) {
        "configmacros"
    } else if mask.contains(PackageMask::REQUIRE) {
        "require"
    } else if mask.contains(PackageMask::NOUNDEFINED) {
        "noundefined"
    } else if mask.contains(PackageMask::BRACEMATCH) {
        "bracematch"
    } else if mask.contains(PackageMask::TEXTMACROS) {
        "textmacros"
    } else if mask.contains(PackageMask::MATHTOOLS) {
        "mathtools"
    } else {
        "unknown"
    }
}

/// A configured renderer profile: which renderer, which packages it has
/// loaded, and which user-declared macros are in scope.
#[derive(Clone, Debug)]
pub struct RenderProfile {
    pub(crate) renderer: Renderer,
    pub(crate) packages: PackageMask,
    pub(crate) macros: HashMap<String, u8>,
}

impl Default for RenderProfile {
    fn default() -> Self {
        Self::mathjax_v3()
    }
}

impl RenderProfile {
    /// MathJax v3 with the default autoload set (`base`, `ams`, `newcommand`,
    /// `noundefined`, `require`, `configmacros`, `boldsymbol`).
    #[must_use]
    pub fn mathjax_v3() -> Self {
        let packages = PackageMask::BASE
            .union(PackageMask::AMS)
            .union(PackageMask::NEWCOMMAND)
            .union(PackageMask::NOUNDEFINED)
            .union(PackageMask::REQUIRE)
            .union(PackageMask::CONFIGMACROS)
            .union(PackageMask::BOLDSYMBOL);
        Self {
            renderer: Renderer::MathJaxV3,
            packages,
            macros: HashMap::new(),
        }
    }

    /// KaTeX with its always-available core (KaTeX core covers what MathJax
    /// splits between `base` and `ams`, so both bits are set). Extensions
    /// (`mhchem`, `physics`, `cancel`, `color`, `mathtools`) are opt-in via
    /// `with_package`.
    #[must_use]
    pub fn katex() -> Self {
        let packages = PackageMask::BASE
            .union(PackageMask::AMS)
            .union(PackageMask::NEWCOMMAND)
            .union(PackageMask::CONFIGMACROS);
        Self {
            renderer: Renderer::Katex,
            packages,
            macros: HashMap::new(),
        }
    }

    /// Which renderer this profile targets.
    #[must_use]
    pub const fn renderer(&self) -> Renderer {
        self.renderer
    }

    /// Load a package/extension by name (e.g. `"mhchem"`, `"physics"`).
    /// Unknown names are silently ignored; users learn about missing packages
    /// through check diagnostics, not the profile builder.
    #[must_use]
    pub fn with_package(mut self, package: &str) -> Self {
        if let Some(mask) = package_from_name(package) {
            self.packages = self.packages.union(mask);
        }
        self
    }

    /// Declare a user-defined macro known to be available at render time.
    /// The arity is informational only; the checker treats the name as defined
    /// and does not validate argument counts.
    #[must_use]
    pub fn with_macro(mut self, name: impl Into<String>, arity: u8) -> Self {
        self.macros.insert(name.into(), arity);
        self
    }

    pub(crate) fn has_package(&self, mask: PackageMask) -> bool {
        self.packages.contains(mask)
    }

    pub(crate) fn has_macro(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }
}
