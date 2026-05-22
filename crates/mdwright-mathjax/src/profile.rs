//! MathJax renderer profile.
//!
//! A profile records which package set a particular MathJax configuration has
//! loaded plus any user-declared macros. Compatibility tables live in
//! `tables`; the profile is the *consumer* of those tables.

use std::collections::HashMap;

/// MathJax package bitmask. One bit per package.
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

    pub(crate) const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Resolve a MathJax package name (as users would write in config) to its mask
/// bit. Returns `None` for unknown names so callers can surface them.
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
        _ => None,
    }
}

/// Canonical user-facing name for a package mask. Used in diagnostic text.
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
    } else {
        "unknown"
    }
}

/// A configured MathJax renderer profile: which packages are loaded and which
/// user macros are in scope.
#[derive(Clone, Debug, Default)]
pub struct MathJaxProfile {
    pub(crate) packages: PackageMask,
    pub(crate) macros: HashMap<String, u8>,
}

impl MathJaxProfile {
    /// MathJax v3 with the default autoload set: `base`, `ams`, `newcommand`,
    /// `noundefined`, `require`, `configmacros`, `boldsymbol`.
    ///
    /// Add optional packages (e.g. `mhchem`, `physics`) with `with_package`.
    #[must_use]
    pub fn v3_default() -> Self {
        let packages = PackageMask::BASE
            .union(PackageMask::AMS)
            .union(PackageMask::NEWCOMMAND)
            .union(PackageMask::NOUNDEFINED)
            .union(PackageMask::REQUIRE)
            .union(PackageMask::CONFIGMACROS)
            .union(PackageMask::BOLDSYMBOL);
        Self {
            packages,
            macros: HashMap::new(),
        }
    }

    /// Load a MathJax package by name (e.g. `"mhchem"`, `"physics"`).
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
