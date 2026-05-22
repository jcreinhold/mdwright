//! MathJax v3 overlay tables.
//!
//! `mdwright-latex`'s registry is the canonical TeX vocabulary; the tables
//! here are an *overlay* recording the things `mdwright-latex` cannot tell us:
//!
//! - commands MathJax ships that `mdwright-latex` does not know about
//!   (`\ce`, `\pu`, the `physics` package's bra/ket family, …);
//! - commands `mdwright-latex` records but MathJax requires a non-default
//!   package for (`\color`, `\cancel`, `\enclose`, …);
//! - the environment compatibility map (`mdwright-latex` only knows the
//!   environments it can render itself, but MathJax ships a wider set).
//!
//! Entries are sorted by `name` and binary-searched.

use crate::profile::PackageMask;

#[derive(Clone, Copy, Debug)]
pub(crate) struct OverlayEntry {
    pub(crate) name: &'static str,
    pub(crate) package: PackageMask,
}

/// Commands MathJax v3 ships. Overrides any classification `mdwright-latex`
/// would give. Sorted by `name`.
pub(crate) const COMMAND_OVERLAY: &[OverlayEntry] = &[
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
        name: "dd",
        package: PackageMask::PHYSICS,
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
];

/// Environments MathJax v3 ships, with the package each requires. Sorted by
/// `name`. Environments not listed here are reported as unsupported.
pub(crate) const ENVIRONMENT_TABLE: &[OverlayEntry] = &[
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

pub(crate) fn lookup_overlay(table: &[OverlayEntry], name: &str) -> Option<OverlayEntry> {
    table
        .binary_search_by_key(&name, |entry| entry.name)
        .ok()
        .and_then(|idx| table.get(idx).copied())
}

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
    fn command_overlay_is_sorted() {
        assert_sorted(COMMAND_OVERLAY);
    }

    #[test]
    fn environment_table_is_sorted() {
        assert_sorted(ENVIRONMENT_TABLE);
    }

    #[test]
    fn known_chemistry_command_resolves_to_mhchem() {
        let entry = lookup_overlay(COMMAND_OVERLAY, "ce").expect("ce in overlay");
        assert!(entry.package.contains(PackageMask::MHCHEM));
    }

    #[test]
    fn missing_command_returns_none() {
        assert!(lookup_overlay(COMMAND_OVERLAY, "totallyunknownmathjaxcmd").is_none());
    }

    #[test]
    fn align_star_environment_resolves_to_ams() {
        let entry = lookup_overlay(ENVIRONMENT_TABLE, "align*").expect("align* in overlay");
        assert!(entry.package.contains(PackageMask::AMS));
    }
}
