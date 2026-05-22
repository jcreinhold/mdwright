//! Renderer compatibility overlay tables.
//!
//! `mdwright-latex`'s registry is the canonical TeX vocabulary; the tables
//! here are an *overlay* recording renderer-specific facts:
//!
//! - commands a renderer ships that `mdwright-latex` does not know about
//!   (`\ce`, `\pu`, the `physics` package's bra/ket family, …);
//! - commands `mdwright-latex` records but the renderer requires a
//!   non-default package for (`\color`, `\cancel`, `\enclose`, …);
//! - the environment compatibility map per renderer.
//!
//! Each table is sorted by `name` and binary-searched.

use crate::profile::{PackageMask, Renderer};

#[derive(Clone, Copy, Debug)]
pub(crate) struct OverlayEntry {
    pub(crate) name: &'static str,
    pub(crate) package: PackageMask,
}

/// Pick the command overlay table for a given renderer.
pub(crate) const fn command_overlay(renderer: Renderer) -> &'static [OverlayEntry] {
    match renderer {
        Renderer::MathJaxV3 => MATHJAX_V3_COMMAND_OVERLAY,
        Renderer::Katex => KATEX_COMMAND_OVERLAY,
    }
}

/// Pick the environment overlay table for a given renderer.
pub(crate) const fn environment_overlay(renderer: Renderer) -> &'static [OverlayEntry] {
    match renderer {
        Renderer::MathJaxV3 => MATHJAX_V3_ENVIRONMENT_TABLE,
        Renderer::Katex => KATEX_ENVIRONMENT_TABLE,
    }
}

pub(crate) fn lookup_overlay(table: &[OverlayEntry], name: &str) -> Option<OverlayEntry> {
    table
        .binary_search_by_key(&name, |entry| entry.name)
        .ok()
        .and_then(|idx| table.get(idx).copied())
}

// ============================================================================
// MathJax v3
// ============================================================================

/// Commands MathJax v3 ships. Overrides any classification `mdwright-latex`
/// would give. Sorted by `name`. ASCII-betical: uppercase letters precede
/// lowercase ones.
const MATHJAX_V3_COMMAND_OVERLAY: &[OverlayEntry] = &[
    OverlayEntry {
        name: "Bra",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "Braket",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "Ket",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "abs",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "absolutevalue",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "acomm",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "acos",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "acot",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "acsc",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "anglevec",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "asec",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "asin",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "atan",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "bcancel",
        package: PackageMask::CANCEL,
    },
    OverlayEntry {
        name: "binom",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "bra",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "braket",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "cancel",
        package: PackageMask::CANCEL,
    },
    OverlayEntry {
        name: "ce",
        package: PackageMask::MHCHEM,
    },
    OverlayEntry {
        name: "color",
        package: PackageMask::COLOR,
    },
    OverlayEntry {
        name: "comm",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "commutator",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "cp",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "cross",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "crossproduct",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "curl",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "dbinom",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "dd",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "deg",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "derivative",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "differential",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "div",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "divergence",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "dotproduct",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "dv",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "enclose",
        package: PackageMask::ENCLOSE,
    },
    OverlayEntry {
        name: "eval",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "evaluated",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "expectationvalue",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "expval",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "fdv",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "functionalderivative",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "grad",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "gradient",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "innerproduct",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "ket",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "ketbra",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "laplacian",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "matrixel",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "matrixelement",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "mel",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "norm",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "not",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "order",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "partialderivative",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "pb",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "pdv",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "poissonbracket",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "principalvalue",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "pu",
        package: PackageMask::MHCHEM,
    },
    OverlayEntry {
        name: "qty",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "rank",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "tag",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "tbinom",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "textcolor",
        package: PackageMask::COLOR,
    },
    OverlayEntry {
        name: "tr",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "trace",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "va",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "var",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "variation",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "varinjlim",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "varliminf",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "varlimsup",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "varprojlim",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "vb",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "vdot",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "vectorbold",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "vectorunit",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "vert",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "vqty",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "vu",
        package: PackageMask::PHYSICS,
    },
    OverlayEntry {
        name: "xcancel",
        package: PackageMask::CANCEL,
    },
    OverlayEntry {
        name: "xhookleftarrow",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "xhookrightarrow",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "xleftarrow",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "xleftrightarrow",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "xmapsto",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "xrightarrow",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "xtwoheadleftarrow",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "xtwoheadrightarrow",
        package: PackageMask::AMS,
    },
];

/// Environments MathJax v3 ships, with the package each requires. Sorted by
/// `name`. Environments not listed here are reported as unsupported.
const MATHJAX_V3_ENVIRONMENT_TABLE: &[OverlayEntry] = &[
    OverlayEntry {
        name: "BVerbatim",
        package: PackageMask::BRACEMATCH,
    },
    OverlayEntry {
        name: "Bmatrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "CD",
        package: PackageMask::AMSCD,
    },
    OverlayEntry {
        name: "Vmatrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "align",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "align*",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "alignat",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "alignat*",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "aligned",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "alignedat",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "array",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "bmatrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "cases",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "darray",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "dcases",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "eqnarray",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "eqnarray*",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "equation",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "equation*",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "flalign",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "flalign*",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "gather",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "gather*",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "gathered",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "matrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "multline",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "multline*",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "pmatrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "rcases",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "smallmatrix",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "split",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "subarray",
        package: PackageMask::AMS,
    },
    OverlayEntry {
        name: "vmatrix",
        package: PackageMask::BASE,
    },
];

// ============================================================================
// KaTeX
// ============================================================================
//
// KaTeX core ships what MathJax splits between `base` and `ams`, so the
// vast majority of commands fall through to `mdwright-latex`'s registry and
// resolve via the BASE or AMS bit (both default-on for the KaTeX profile).
// The overlay only records commands that need an explicit extension or that
// KaTeX does not ship at all.

/// Commands KaTeX ships under explicit extensions. The `physics` family is
/// *not* part of KaTeX core — there is a community extension but it is not
/// uniformly available, so KaTeX users see `UnsupportedCommand` for `\bra`,
/// `\ket`, etc. unless they list those names under `[lint.render.macros]`.
/// Sorted by `name`.
const KATEX_COMMAND_OVERLAY: &[OverlayEntry] = &[
    OverlayEntry {
        name: "bcancel",
        package: PackageMask::CANCEL,
    },
    OverlayEntry {
        name: "binom",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "cancel",
        package: PackageMask::CANCEL,
    },
    OverlayEntry {
        name: "ce",
        package: PackageMask::MHCHEM,
    },
    OverlayEntry {
        name: "color",
        package: PackageMask::COLOR,
    },
    OverlayEntry {
        name: "dbinom",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "deg",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "enclose",
        package: PackageMask::ENCLOSE,
    },
    OverlayEntry {
        name: "not",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "pu",
        package: PackageMask::MHCHEM,
    },
    OverlayEntry {
        name: "tag",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "tbinom",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "textcolor",
        package: PackageMask::COLOR,
    },
    OverlayEntry {
        name: "varinjlim",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "varliminf",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "varlimsup",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "varprojlim",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "vert",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "xcancel",
        package: PackageMask::CANCEL,
    },
    OverlayEntry {
        name: "xhookleftarrow",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "xhookrightarrow",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "xleftarrow",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "xleftrightarrow",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "xmapsto",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "xrightarrow",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "xtwoheadleftarrow",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "xtwoheadrightarrow",
        package: PackageMask::BASE,
    },
];

/// Environments KaTeX ships. KaTeX core includes the `ams` environments, so
/// `align`, `gather`, etc. resolve to BASE rather than to AMS (the AMS bit
/// is still set in the KaTeX default profile, so either bit lets them pass —
/// BASE is what shows up in a diagnostic if a user explicitly drops it).
/// Sorted by `name`.
const KATEX_ENVIRONMENT_TABLE: &[OverlayEntry] = &[
    OverlayEntry {
        name: "Bmatrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "CD",
        package: PackageMask::AMSCD,
    },
    OverlayEntry {
        name: "Vmatrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "align",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "align*",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "alignat",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "alignat*",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "aligned",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "alignedat",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "array",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "bmatrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "cases",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "darray",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "dcases",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "equation",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "equation*",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "gather",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "gather*",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "gathered",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "matrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "pmatrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "rcases",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "smallmatrix",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "split",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "subarray",
        package: PackageMask::BASE,
    },
    OverlayEntry {
        name: "vmatrix",
        package: PackageMask::BASE,
    },
];

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::panic,
        reason = "tests assert invariants on the static overlay tables"
    )]

    use super::*;

    fn assert_sorted(table: &[OverlayEntry]) {
        for window in table.windows(2) {
            if let [a, b] = window {
                assert!(a.name < b.name, "overlay table not sorted: {} >= {}", a.name, b.name);
            }
        }
    }

    #[test]
    fn all_command_overlays_are_sorted() {
        assert_sorted(MATHJAX_V3_COMMAND_OVERLAY);
        assert_sorted(KATEX_COMMAND_OVERLAY);
    }

    #[test]
    fn all_environment_overlays_are_sorted() {
        assert_sorted(MATHJAX_V3_ENVIRONMENT_TABLE);
        assert_sorted(KATEX_ENVIRONMENT_TABLE);
    }

    #[test]
    fn mhchem_resolves_under_both_renderers() {
        let mj = lookup_overlay(MATHJAX_V3_COMMAND_OVERLAY, "ce").expect("ce in mathjax");
        let kx = lookup_overlay(KATEX_COMMAND_OVERLAY, "ce").expect("ce in katex");
        assert!(mj.package.contains(PackageMask::MHCHEM));
        assert!(kx.package.contains(PackageMask::MHCHEM));
    }

    #[test]
    fn katex_does_not_ship_physics_by_default() {
        assert!(lookup_overlay(KATEX_COMMAND_OVERLAY, "bra").is_none());
        assert!(lookup_overlay(KATEX_COMMAND_OVERLAY, "ket").is_none());
        assert!(lookup_overlay(KATEX_COMMAND_OVERLAY, "dv").is_none());
    }

    #[test]
    fn align_resolves_to_base_under_katex_and_ams_under_mathjax() {
        let mj = lookup_overlay(MATHJAX_V3_ENVIRONMENT_TABLE, "align*").expect("align* in mathjax");
        let kx = lookup_overlay(KATEX_ENVIRONMENT_TABLE, "align*").expect("align* in katex");
        assert!(mj.package.contains(PackageMask::AMS));
        assert!(kx.package.contains(PackageMask::BASE));
    }
}
