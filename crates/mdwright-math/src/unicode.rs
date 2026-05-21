//! Curated LaTeX and Unicode vocabulary shared by linting, preview,
//! and future source translation.

const LATEX_SYMBOLS: &[(&str, &str)] = &[
    ("cdot", "⋅"),
    ("circ", "∘"),
    ("times", "×"),
    ("to", "→"),
    ("mapsto", "↦"),
    ("otimes", "⊗"),
    ("oplus", "⊕"),
    ("hookrightarrow", "↪"),
    ("leftrightarrow", "↔"),
    ("Longrightarrow", "⟹"),
    ("Leftrightarrow", "⟺"),
    ("dashrightarrow", "⇢"),
    ("curvearrowright", "↷"),
    ("bullet", "•"),
    ("partial", "∂"),
    ("nabla", "∇"),
    ("prime", "′"),
    ("wedge", "∧"),
    ("vee", "∨"),
    ("cup", "∪"),
    ("cap", "∩"),
    ("bigcup", "⋃"),
    ("bigcap", "⋂"),
    ("emptyset", "∅"),
    ("infty", "∞"),
    ("cong", "≅"),
    ("sim", "∼"),
    ("leq", "≤"),
    ("geq", "≥"),
    ("neq", "≠"),
    ("subset", "⊂"),
    ("subseteq", "⊆"),
    ("in", "∈"),
    ("ni", "∋"),
    ("notin", "∉"),
    ("forall", "∀"),
    ("exists", "∃"),
    ("langle", "⟨"),
    ("rangle", "⟩"),
    ("setminus", "∖"),
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("pi", "π"),
    ("rho", "ρ"),
    ("sigma", "σ"),
    ("tau", "τ"),
    ("phi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
];

/// Return the Unicode symbol for a known LaTeX command name.
#[must_use]
pub fn latex_symbol(name: &str) -> Option<&'static str> {
    LATEX_SYMBOLS
        .iter()
        .find_map(|(latex, unicode)| (*latex == name).then_some(*unicode))
}

/// Return one preferred LaTeX command name for a known Unicode symbol.
#[must_use]
pub fn unicode_symbol_latex(symbol: &str) -> Option<&'static str> {
    LATEX_SYMBOLS
        .iter()
        .find_map(|(latex, unicode)| (*unicode == symbol).then_some(*latex))
}

/// Unicode superscript for a single ASCII character.
#[must_use]
pub fn unicode_super(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        '-' => '⁻',
        _ => return None,
    })
}

/// Unicode subscript for a single ASCII character.
#[must_use]
pub fn unicode_sub(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        'n' => 'ₙ',
        'i' => 'ᵢ',
        _ => return None,
    })
}

/// Render a whole script string as Unicode superscript.
#[must_use]
pub fn unicode_super_str(s: &str) -> Option<String> {
    s.chars().map(unicode_super).collect()
}

/// Render a whole script string as Unicode subscript.
#[must_use]
pub fn unicode_sub_str(s: &str) -> Option<String> {
    s.chars().map(unicode_sub).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_table_roundtrips_represented_symbols() {
        assert_eq!(latex_symbol("alpha"), Some("α"));
        assert_eq!(unicode_symbol_latex("α"), Some("alpha"));
        assert_eq!(latex_symbol("cdot"), Some("⋅"));
        assert_eq!(unicode_symbol_latex("⋅"), Some("cdot"));
    }

    #[test]
    fn script_maps_cover_lint_vocabulary() {
        assert_eq!(unicode_super_str("-1"), Some("⁻¹".to_owned()));
        assert_eq!(unicode_super_str("n"), Some("ⁿ".to_owned()));
        assert_eq!(unicode_sub_str("i"), Some("ᵢ".to_owned()));
        assert_eq!(unicode_sub_str("x"), None);
    }
}
