//! MathJax-style command registry and Unicode vocabulary.
//!
//! The registry is data, not parser code. It classifies commands by
//! category, support status, argument shape, and Unicode spelling so
//! parsing, rendering, linting, and source translation can ask narrow
//! questions without owning parallel tables.

/// MathJax-style command category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandCategory {
    /// Direct symbol such as `\infty`.
    Symbol,
    /// Greek letter or variant.
    Greek,
    /// Binary operator.
    BinaryOperator,
    /// Relation symbol.
    Relation,
    /// Arrow symbol.
    Arrow,
    /// Delimiter command.
    Delimiter,
    /// Large operator.
    LargeOperator,
    /// Accent command that owns one argument.
    Accent,
    /// Spacing command.
    Spacing,
    /// Function/operator name.
    Function,
    /// Font or style command.
    Font,
    /// Environment name.
    Environment,
    /// Structural TeX command parsed specially.
    Structural,
    /// Known `MathJax` command outside mdwright's Unicode subset.
    Unsupported,
}

/// Argument shape for a command or environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentShape {
    /// No arguments.
    None,
    /// One required argument.
    OneRequired,
    /// Two required arguments.
    TwoRequired,
    /// Optional argument followed by one required argument.
    OptionalThenRequired,
    /// Environment body with rows and cells.
    EnvironmentBody,
    /// Macro-like or otherwise variable argument shape.
    Variable,
}

/// Whether mdwright can currently interpret a known command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportStatus {
    /// Command maps directly to Unicode text.
    DirectUnicode,
    /// Parser has a typed construct for this command.
    ParsedConstruct,
    /// Command is recognised but intentionally produces no visible text.
    RecognisedNoOutput,
    /// Command is known from MathJax-style input but unsupported here.
    Unsupported,
}

/// Public, copyable view of one command registry entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandInfo {
    name: &'static str,
    category: CommandCategory,
    arguments: ArgumentShape,
    unicode: Option<&'static str>,
    preferred: &'static str,
    support: SupportStatus,
    package: &'static str,
}

impl CommandInfo {
    /// Command name without the leading backslash.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Registry category.
    #[must_use]
    pub const fn category(self) -> CommandCategory {
        self.category
    }

    /// Argument shape.
    #[must_use]
    pub const fn arguments(self) -> ArgumentShape {
        self.arguments
    }

    /// Direct Unicode output, when the command has one.
    #[must_use]
    pub const fn unicode(self) -> Option<&'static str> {
        self.unicode
    }

    /// Preferred LaTeX spelling for reverse translation.
    #[must_use]
    pub const fn preferred(self) -> &'static str {
        self.preferred
    }

    /// Current mdwright support status.
    #[must_use]
    pub const fn support(self) -> SupportStatus {
        self.support
    }

    /// `MathJax` package/source classification used as a coverage note.
    #[must_use]
    pub const fn package(self) -> &'static str {
        self.package
    }
}

#[derive(Clone, Copy)]
struct CommandEntry {
    name: &'static str,
    category: CommandCategory,
    arguments: ArgumentShape,
    unicode: Option<&'static str>,
    preferred: &'static str,
    support: SupportStatus,
    package: &'static str,
}

impl CommandEntry {
    const fn direct(
        name: &'static str,
        category: CommandCategory,
        unicode: &'static str,
        preferred: &'static str,
        package: &'static str,
    ) -> Self {
        Self {
            name,
            category,
            arguments: ArgumentShape::None,
            unicode: Some(unicode),
            preferred,
            support: SupportStatus::DirectUnicode,
            package,
        }
    }

    const fn parsed(
        name: &'static str,
        category: CommandCategory,
        arguments: ArgumentShape,
        package: &'static str,
    ) -> Self {
        Self {
            name,
            category,
            arguments,
            unicode: None,
            preferred: name,
            support: SupportStatus::ParsedConstruct,
            package,
        }
    }

    const fn no_output(name: &'static str, category: CommandCategory, package: &'static str) -> Self {
        Self {
            name,
            category,
            arguments: ArgumentShape::None,
            unicode: None,
            preferred: name,
            support: SupportStatus::RecognisedNoOutput,
            package,
        }
    }

    const fn unsupported(
        name: &'static str,
        category: CommandCategory,
        arguments: ArgumentShape,
        package: &'static str,
    ) -> Self {
        Self {
            name,
            category,
            arguments,
            unicode: None,
            preferred: name,
            support: SupportStatus::Unsupported,
            package,
        }
    }

    const fn info(self) -> CommandInfo {
        CommandInfo {
            name: self.name,
            category: self.category,
            arguments: self.arguments,
            unicode: self.unicode,
            preferred: self.preferred,
            support: self.support,
            package: self.package,
        }
    }
}

const BASE: &str = "base";
const AMS: &str = "ams";
const MATHTOOLS: &str = "mathtools";
const TEXT_BASE: &str = "text-base";

const COMMANDS: &[CommandEntry] = &[
    // Structural commands parsed by the recursive-descent parser.
    CommandEntry::parsed("frac", CommandCategory::Structural, ArgumentShape::TwoRequired, BASE),
    CommandEntry::parsed("dfrac", CommandCategory::Structural, ArgumentShape::TwoRequired, AMS),
    CommandEntry::parsed("tfrac", CommandCategory::Structural, ArgumentShape::TwoRequired, AMS),
    CommandEntry::parsed(
        "sqrt",
        CommandCategory::Structural,
        ArgumentShape::OptionalThenRequired,
        BASE,
    ),
    CommandEntry::parsed("left", CommandCategory::Structural, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("right", CommandCategory::Structural, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("begin", CommandCategory::Structural, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("end", CommandCategory::Structural, ArgumentShape::OneRequired, BASE),
    // Accents.
    CommandEntry::parsed("hat", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("widehat", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("bar", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("overline", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("tilde", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("widetilde", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("vec", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("dot", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("ddot", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("acute", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("grave", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("breve", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("check", CommandCategory::Accent, ArgumentShape::OneRequired, BASE),
    // Greek letters and variants.
    CommandEntry::direct("alpha", CommandCategory::Greek, "α", "alpha", BASE),
    CommandEntry::direct("beta", CommandCategory::Greek, "β", "beta", BASE),
    CommandEntry::direct("gamma", CommandCategory::Greek, "γ", "gamma", BASE),
    CommandEntry::direct("delta", CommandCategory::Greek, "δ", "delta", BASE),
    CommandEntry::direct("epsilon", CommandCategory::Greek, "ε", "epsilon", BASE),
    CommandEntry::direct("varepsilon", CommandCategory::Greek, "ϵ", "varepsilon", BASE),
    CommandEntry::direct("zeta", CommandCategory::Greek, "ζ", "zeta", BASE),
    CommandEntry::direct("eta", CommandCategory::Greek, "η", "eta", BASE),
    CommandEntry::direct("theta", CommandCategory::Greek, "θ", "theta", BASE),
    CommandEntry::direct("vartheta", CommandCategory::Greek, "ϑ", "vartheta", BASE),
    CommandEntry::direct("iota", CommandCategory::Greek, "ι", "iota", BASE),
    CommandEntry::direct("kappa", CommandCategory::Greek, "κ", "kappa", BASE),
    CommandEntry::direct("varkappa", CommandCategory::Greek, "ϰ", "varkappa", AMS),
    CommandEntry::direct("lambda", CommandCategory::Greek, "λ", "lambda", BASE),
    CommandEntry::direct("mu", CommandCategory::Greek, "μ", "mu", BASE),
    CommandEntry::direct("nu", CommandCategory::Greek, "ν", "nu", BASE),
    CommandEntry::direct("xi", CommandCategory::Greek, "ξ", "xi", BASE),
    CommandEntry::direct("pi", CommandCategory::Greek, "π", "pi", BASE),
    CommandEntry::direct("varpi", CommandCategory::Greek, "ϖ", "varpi", BASE),
    CommandEntry::direct("rho", CommandCategory::Greek, "ρ", "rho", BASE),
    CommandEntry::direct("varrho", CommandCategory::Greek, "ϱ", "varrho", BASE),
    CommandEntry::direct("sigma", CommandCategory::Greek, "σ", "sigma", BASE),
    CommandEntry::direct("varsigma", CommandCategory::Greek, "ς", "varsigma", BASE),
    CommandEntry::direct("tau", CommandCategory::Greek, "τ", "tau", BASE),
    CommandEntry::direct("upsilon", CommandCategory::Greek, "υ", "upsilon", BASE),
    CommandEntry::direct("phi", CommandCategory::Greek, "φ", "phi", BASE),
    CommandEntry::direct("varphi", CommandCategory::Greek, "ϕ", "varphi", BASE),
    CommandEntry::direct("chi", CommandCategory::Greek, "χ", "chi", BASE),
    CommandEntry::direct("psi", CommandCategory::Greek, "ψ", "psi", BASE),
    CommandEntry::direct("omega", CommandCategory::Greek, "ω", "omega", BASE),
    CommandEntry::direct("Gamma", CommandCategory::Greek, "Γ", "Gamma", BASE),
    CommandEntry::direct("Delta", CommandCategory::Greek, "Δ", "Delta", BASE),
    CommandEntry::direct("Theta", CommandCategory::Greek, "Θ", "Theta", BASE),
    CommandEntry::direct("Lambda", CommandCategory::Greek, "Λ", "Lambda", BASE),
    CommandEntry::direct("Xi", CommandCategory::Greek, "Ξ", "Xi", BASE),
    CommandEntry::direct("Pi", CommandCategory::Greek, "Π", "Pi", BASE),
    CommandEntry::direct("Sigma", CommandCategory::Greek, "Σ", "Sigma", BASE),
    CommandEntry::direct("Upsilon", CommandCategory::Greek, "Υ", "Upsilon", BASE),
    CommandEntry::direct("Phi", CommandCategory::Greek, "Φ", "Phi", BASE),
    CommandEntry::direct("Psi", CommandCategory::Greek, "Ψ", "Psi", BASE),
    CommandEntry::direct("Omega", CommandCategory::Greek, "Ω", "Omega", BASE),
    // Binary operators.
    CommandEntry::direct("pm", CommandCategory::BinaryOperator, "±", "pm", BASE),
    CommandEntry::direct("mp", CommandCategory::BinaryOperator, "∓", "mp", BASE),
    CommandEntry::direct("times", CommandCategory::BinaryOperator, "×", "times", BASE),
    CommandEntry::direct("div", CommandCategory::BinaryOperator, "÷", "div", BASE),
    CommandEntry::direct("cdot", CommandCategory::BinaryOperator, "⋅", "cdot", BASE),
    CommandEntry::direct("circ", CommandCategory::BinaryOperator, "∘", "circ", BASE),
    CommandEntry::direct("bullet", CommandCategory::BinaryOperator, "•", "bullet", BASE),
    CommandEntry::direct("ast", CommandCategory::BinaryOperator, "∗", "ast", BASE),
    CommandEntry::direct("star", CommandCategory::BinaryOperator, "⋆", "star", BASE),
    CommandEntry::direct("wedge", CommandCategory::BinaryOperator, "∧", "wedge", BASE),
    CommandEntry::direct("land", CommandCategory::BinaryOperator, "∧", "wedge", BASE),
    CommandEntry::direct("vee", CommandCategory::BinaryOperator, "∨", "vee", BASE),
    CommandEntry::direct("lor", CommandCategory::BinaryOperator, "∨", "vee", BASE),
    CommandEntry::direct("cap", CommandCategory::BinaryOperator, "∩", "cap", BASE),
    CommandEntry::direct("cup", CommandCategory::BinaryOperator, "∪", "cup", BASE),
    CommandEntry::direct("setminus", CommandCategory::BinaryOperator, "∖", "setminus", BASE),
    CommandEntry::direct("oplus", CommandCategory::BinaryOperator, "⊕", "oplus", BASE),
    CommandEntry::direct("otimes", CommandCategory::BinaryOperator, "⊗", "otimes", BASE),
    CommandEntry::direct("ominus", CommandCategory::BinaryOperator, "⊖", "ominus", BASE),
    CommandEntry::direct("oslash", CommandCategory::BinaryOperator, "⊘", "oslash", BASE),
    CommandEntry::direct("odot", CommandCategory::BinaryOperator, "⊙", "odot", BASE),
    CommandEntry::direct("amalg", CommandCategory::BinaryOperator, "⨿", "amalg", BASE),
    // Relations.
    CommandEntry::direct("leq", CommandCategory::Relation, "≤", "leq", BASE),
    CommandEntry::direct("le", CommandCategory::Relation, "≤", "leq", BASE),
    CommandEntry::direct("geq", CommandCategory::Relation, "≥", "geq", BASE),
    CommandEntry::direct("ge", CommandCategory::Relation, "≥", "geq", BASE),
    CommandEntry::direct("neq", CommandCategory::Relation, "≠", "neq", BASE),
    CommandEntry::direct("ne", CommandCategory::Relation, "≠", "neq", BASE),
    CommandEntry::direct("equiv", CommandCategory::Relation, "≡", "equiv", BASE),
    CommandEntry::direct("sim", CommandCategory::Relation, "∼", "sim", BASE),
    CommandEntry::direct("simeq", CommandCategory::Relation, "≃", "simeq", BASE),
    CommandEntry::direct("approx", CommandCategory::Relation, "≈", "approx", BASE),
    CommandEntry::direct("cong", CommandCategory::Relation, "≅", "cong", BASE),
    CommandEntry::direct("propto", CommandCategory::Relation, "∝", "propto", BASE),
    CommandEntry::direct("in", CommandCategory::Relation, "∈", "in", BASE),
    CommandEntry::direct("ni", CommandCategory::Relation, "∋", "ni", BASE),
    CommandEntry::direct("notin", CommandCategory::Relation, "∉", "notin", BASE),
    CommandEntry::direct("subset", CommandCategory::Relation, "⊂", "subset", BASE),
    CommandEntry::direct("supset", CommandCategory::Relation, "⊃", "supset", BASE),
    CommandEntry::direct("subseteq", CommandCategory::Relation, "⊆", "subseteq", BASE),
    CommandEntry::direct("supseteq", CommandCategory::Relation, "⊇", "supseteq", BASE),
    CommandEntry::direct("models", CommandCategory::Relation, "⊨", "models", AMS),
    CommandEntry::direct("vdash", CommandCategory::Relation, "⊢", "vdash", BASE),
    CommandEntry::direct("dashv", CommandCategory::Relation, "⊣", "dashv", BASE),
    CommandEntry::direct("perp", CommandCategory::Relation, "⊥", "perp", BASE),
    CommandEntry::direct("parallel", CommandCategory::Relation, "∥", "parallel", BASE),
    CommandEntry::direct("mid", CommandCategory::Relation, "∣", "mid", BASE),
    CommandEntry::direct("asymp", CommandCategory::Relation, "≍", "asymp", BASE),
    // Arrows.
    CommandEntry::direct("to", CommandCategory::Arrow, "→", "to", BASE),
    CommandEntry::direct("rightarrow", CommandCategory::Arrow, "→", "to", BASE),
    CommandEntry::direct("gets", CommandCategory::Arrow, "←", "leftarrow", BASE),
    CommandEntry::direct("leftarrow", CommandCategory::Arrow, "←", "leftarrow", BASE),
    CommandEntry::direct("mapsto", CommandCategory::Arrow, "↦", "mapsto", BASE),
    CommandEntry::direct("leftrightarrow", CommandCategory::Arrow, "↔", "leftrightarrow", BASE),
    CommandEntry::direct("Rightarrow", CommandCategory::Arrow, "⇒", "Rightarrow", BASE),
    CommandEntry::direct("Leftarrow", CommandCategory::Arrow, "⇐", "Leftarrow", BASE),
    CommandEntry::direct("Leftrightarrow", CommandCategory::Arrow, "⇔", "Leftrightarrow", BASE),
    CommandEntry::direct("longrightarrow", CommandCategory::Arrow, "⟶", "longrightarrow", BASE),
    CommandEntry::direct("longleftarrow", CommandCategory::Arrow, "⟵", "longleftarrow", BASE),
    CommandEntry::direct("Longrightarrow", CommandCategory::Arrow, "⟹", "Longrightarrow", BASE),
    CommandEntry::direct("Longleftarrow", CommandCategory::Arrow, "⟸", "Longleftarrow", BASE),
    CommandEntry::direct("hookrightarrow", CommandCategory::Arrow, "↪", "hookrightarrow", BASE),
    CommandEntry::direct("hookleftarrow", CommandCategory::Arrow, "↩", "hookleftarrow", BASE),
    CommandEntry::direct("uparrow", CommandCategory::Arrow, "↑", "uparrow", BASE),
    CommandEntry::direct("downarrow", CommandCategory::Arrow, "↓", "downarrow", BASE),
    CommandEntry::direct("updownarrow", CommandCategory::Arrow, "↕", "updownarrow", BASE),
    CommandEntry::direct("dashrightarrow", CommandCategory::Arrow, "⇢", "dashrightarrow", AMS),
    CommandEntry::direct("curvearrowright", CommandCategory::Arrow, "↷", "curvearrowright", AMS),
    // Delimiters and set symbols.
    CommandEntry::direct("langle", CommandCategory::Delimiter, "⟨", "langle", BASE),
    CommandEntry::direct("rangle", CommandCategory::Delimiter, "⟩", "rangle", BASE),
    CommandEntry::direct("lbrace", CommandCategory::Delimiter, "{", "lbrace", BASE),
    CommandEntry::direct("rbrace", CommandCategory::Delimiter, "}", "rbrace", BASE),
    CommandEntry::direct("lvert", CommandCategory::Delimiter, "|", "lvert", BASE),
    CommandEntry::direct("rvert", CommandCategory::Delimiter, "|", "rvert", BASE),
    CommandEntry::direct("Vert", CommandCategory::Delimiter, "‖", "Vert", BASE),
    CommandEntry::direct("lVert", CommandCategory::Delimiter, "‖", "Vert", BASE),
    CommandEntry::direct("rVert", CommandCategory::Delimiter, "‖", "Vert", BASE),
    CommandEntry::direct("backslash", CommandCategory::Delimiter, "\\", "backslash", BASE),
    CommandEntry::direct("emptyset", CommandCategory::Symbol, "∅", "emptyset", BASE),
    CommandEntry::direct("varnothing", CommandCategory::Symbol, "∅", "emptyset", AMS),
    // Large operators and miscellaneous symbols.
    CommandEntry::direct("sum", CommandCategory::LargeOperator, "∑", "sum", BASE),
    CommandEntry::direct("prod", CommandCategory::LargeOperator, "∏", "prod", BASE),
    CommandEntry::direct("coprod", CommandCategory::LargeOperator, "∐", "coprod", BASE),
    CommandEntry::direct("int", CommandCategory::LargeOperator, "∫", "int", BASE),
    CommandEntry::direct("iint", CommandCategory::LargeOperator, "∬", "iint", AMS),
    CommandEntry::direct("iiint", CommandCategory::LargeOperator, "∭", "iiint", AMS),
    CommandEntry::direct("oint", CommandCategory::LargeOperator, "∮", "oint", BASE),
    CommandEntry::direct("bigcup", CommandCategory::LargeOperator, "⋃", "bigcup", BASE),
    CommandEntry::direct("bigcap", CommandCategory::LargeOperator, "⋂", "bigcap", BASE),
    CommandEntry::direct("bigsqcup", CommandCategory::LargeOperator, "⨆", "bigsqcup", BASE),
    CommandEntry::direct("bigvee", CommandCategory::LargeOperator, "⋁", "bigvee", BASE),
    CommandEntry::direct("bigwedge", CommandCategory::LargeOperator, "⋀", "bigwedge", BASE),
    CommandEntry::direct("partial", CommandCategory::Symbol, "∂", "partial", BASE),
    CommandEntry::direct("nabla", CommandCategory::Symbol, "∇", "nabla", BASE),
    CommandEntry::direct("infty", CommandCategory::Symbol, "∞", "infty", BASE),
    CommandEntry::direct("prime", CommandCategory::Symbol, "′", "prime", BASE),
    CommandEntry::direct("forall", CommandCategory::Symbol, "∀", "forall", BASE),
    CommandEntry::direct("exists", CommandCategory::Symbol, "∃", "exists", BASE),
    CommandEntry::direct("neg", CommandCategory::Symbol, "¬", "neg", BASE),
    CommandEntry::direct("lnot", CommandCategory::Symbol, "¬", "neg", BASE),
    CommandEntry::direct("angle", CommandCategory::Symbol, "∠", "angle", BASE),
    CommandEntry::direct("aleph", CommandCategory::Symbol, "ℵ", "aleph", BASE),
    CommandEntry::direct("beth", CommandCategory::Symbol, "ℶ", "beth", AMS),
    CommandEntry::direct("ell", CommandCategory::Symbol, "ℓ", "ell", BASE),
    CommandEntry::direct("hbar", CommandCategory::Symbol, "ℏ", "hbar", BASE),
    CommandEntry::direct("imath", CommandCategory::Symbol, "ı", "imath", BASE),
    CommandEntry::direct("jmath", CommandCategory::Symbol, "ȷ", "jmath", BASE),
    CommandEntry::direct("Re", CommandCategory::Symbol, "ℜ", "Re", BASE),
    CommandEntry::direct("Im", CommandCategory::Symbol, "ℑ", "Im", BASE),
    CommandEntry::direct("wp", CommandCategory::Symbol, "℘", "wp", BASE),
    // Function names.
    CommandEntry::direct("sin", CommandCategory::Function, "sin", "sin", BASE),
    CommandEntry::direct("cos", CommandCategory::Function, "cos", "cos", BASE),
    CommandEntry::direct("tan", CommandCategory::Function, "tan", "tan", BASE),
    CommandEntry::direct("cot", CommandCategory::Function, "cot", "cot", BASE),
    CommandEntry::direct("sec", CommandCategory::Function, "sec", "sec", BASE),
    CommandEntry::direct("csc", CommandCategory::Function, "csc", "csc", BASE),
    CommandEntry::direct("arcsin", CommandCategory::Function, "arcsin", "arcsin", BASE),
    CommandEntry::direct("arccos", CommandCategory::Function, "arccos", "arccos", BASE),
    CommandEntry::direct("arctan", CommandCategory::Function, "arctan", "arctan", BASE),
    CommandEntry::direct("log", CommandCategory::Function, "log", "log", BASE),
    CommandEntry::direct("ln", CommandCategory::Function, "ln", "ln", BASE),
    CommandEntry::direct("lim", CommandCategory::Function, "lim", "lim", BASE),
    CommandEntry::direct("arg", CommandCategory::Function, "arg", "arg", BASE),
    CommandEntry::direct("det", CommandCategory::Function, "det", "det", BASE),
    CommandEntry::direct("dim", CommandCategory::Function, "dim", "dim", BASE),
    CommandEntry::direct("ker", CommandCategory::Function, "ker", "ker", BASE),
    CommandEntry::direct("hom", CommandCategory::Function, "hom", "hom", BASE),
    CommandEntry::direct("min", CommandCategory::Function, "min", "min", BASE),
    CommandEntry::direct("max", CommandCategory::Function, "max", "max", BASE),
    CommandEntry::direct("sup", CommandCategory::Function, "sup", "sup", BASE),
    CommandEntry::direct("inf", CommandCategory::Function, "inf", "inf", BASE),
    // Spacing and invisible commands.
    CommandEntry::no_output(",", CommandCategory::Spacing, BASE),
    CommandEntry::no_output(":", CommandCategory::Spacing, BASE),
    CommandEntry::no_output(";", CommandCategory::Spacing, BASE),
    CommandEntry::no_output("!", CommandCategory::Spacing, BASE),
    CommandEntry::no_output(" ", CommandCategory::Spacing, BASE),
    CommandEntry::no_output("quad", CommandCategory::Spacing, BASE),
    CommandEntry::no_output("qquad", CommandCategory::Spacing, BASE),
    // Font/style commands. Actual alphabet mapping is handled by a later pass.
    CommandEntry::parsed("mathbb", CommandCategory::Font, ArgumentShape::OneRequired, AMS),
    CommandEntry::parsed("mathcal", CommandCategory::Font, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("mathfrak", CommandCategory::Font, ArgumentShape::OneRequired, AMS),
    CommandEntry::parsed("mathrm", CommandCategory::Font, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("mathbf", CommandCategory::Font, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("mathit", CommandCategory::Font, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("mathsf", CommandCategory::Font, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed("mathtt", CommandCategory::Font, ArgumentShape::OneRequired, BASE),
    CommandEntry::parsed(
        "operatorname",
        CommandCategory::Function,
        ArgumentShape::OneRequired,
        BASE,
    ),
    // Environments used by the parser.
    CommandEntry::parsed(
        "matrix",
        CommandCategory::Environment,
        ArgumentShape::EnvironmentBody,
        BASE,
    ),
    CommandEntry::parsed(
        "pmatrix",
        CommandCategory::Environment,
        ArgumentShape::EnvironmentBody,
        AMS,
    ),
    CommandEntry::parsed(
        "bmatrix",
        CommandCategory::Environment,
        ArgumentShape::EnvironmentBody,
        AMS,
    ),
    CommandEntry::parsed(
        "Bmatrix",
        CommandCategory::Environment,
        ArgumentShape::EnvironmentBody,
        AMS,
    ),
    CommandEntry::parsed(
        "vmatrix",
        CommandCategory::Environment,
        ArgumentShape::EnvironmentBody,
        AMS,
    ),
    CommandEntry::parsed(
        "Vmatrix",
        CommandCategory::Environment,
        ArgumentShape::EnvironmentBody,
        AMS,
    ),
    CommandEntry::parsed(
        "array",
        CommandCategory::Environment,
        ArgumentShape::EnvironmentBody,
        BASE,
    ),
    CommandEntry::parsed(
        "cases",
        CommandCategory::Environment,
        ArgumentShape::EnvironmentBody,
        AMS,
    ),
    CommandEntry::parsed(
        "aligned",
        CommandCategory::Environment,
        ArgumentShape::EnvironmentBody,
        AMS,
    ),
    CommandEntry::parsed(
        "split",
        CommandCategory::Environment,
        ArgumentShape::EnvironmentBody,
        AMS,
    ),
    // Known MathJax commands outside this Unicode subset.
    CommandEntry::unsupported(
        "newcommand",
        CommandCategory::Unsupported,
        ArgumentShape::Variable,
        BASE,
    ),
    CommandEntry::unsupported(
        "renewcommand",
        CommandCategory::Unsupported,
        ArgumentShape::Variable,
        BASE,
    ),
    CommandEntry::unsupported("def", CommandCategory::Unsupported, ArgumentShape::Variable, BASE),
    CommandEntry::unsupported("let", CommandCategory::Unsupported, ArgumentShape::Variable, BASE),
    CommandEntry::unsupported(
        "require",
        CommandCategory::Unsupported,
        ArgumentShape::OneRequired,
        BASE,
    ),
    CommandEntry::unsupported("color", CommandCategory::Unsupported, ArgumentShape::Variable, BASE),
    CommandEntry::unsupported("href", CommandCategory::Unsupported, ArgumentShape::TwoRequired, BASE),
    CommandEntry::unsupported("class", CommandCategory::Unsupported, ArgumentShape::TwoRequired, BASE),
    CommandEntry::unsupported("style", CommandCategory::Unsupported, ArgumentShape::TwoRequired, BASE),
    CommandEntry::unsupported("text", CommandCategory::Unsupported, ArgumentShape::OneRequired, BASE),
    CommandEntry::unsupported("mbox", CommandCategory::Unsupported, ArgumentShape::OneRequired, BASE),
    CommandEntry::unsupported(
        "cancel",
        CommandCategory::Unsupported,
        ArgumentShape::OneRequired,
        "cancel",
    ),
    CommandEntry::unsupported(
        "bcancel",
        CommandCategory::Unsupported,
        ArgumentShape::OneRequired,
        "cancel",
    ),
    CommandEntry::unsupported(
        "xcancel",
        CommandCategory::Unsupported,
        ArgumentShape::OneRequired,
        "cancel",
    ),
    CommandEntry::unsupported(
        "enclose",
        CommandCategory::Unsupported,
        ArgumentShape::Variable,
        "enclose",
    ),
    CommandEntry::unsupported(
        "Aboxed",
        CommandCategory::Unsupported,
        ArgumentShape::OneRequired,
        MATHTOOLS,
    ),
    CommandEntry::unsupported("bbox", CommandCategory::Unsupported, ArgumentShape::Variable, "bbox"),
    CommandEntry::unsupported(
        "unicode",
        CommandCategory::Unsupported,
        ArgumentShape::OneRequired,
        TEXT_BASE,
    ),
];

const SUPERSCRIPTS: &[(char, char, &str)] = &[
    ('0', '⁰', "0"),
    ('1', '¹', "1"),
    ('2', '²', "2"),
    ('3', '³', "3"),
    ('4', '⁴', "4"),
    ('5', '⁵', "5"),
    ('6', '⁶', "6"),
    ('7', '⁷', "7"),
    ('8', '⁸', "8"),
    ('9', '⁹', "9"),
    ('n', 'ⁿ', "n"),
    ('i', 'ⁱ', "i"),
    ('-', '⁻', "-"),
];

const SUBSCRIPTS: &[(char, char, &str)] = &[
    ('0', '₀', "0"),
    ('1', '₁', "1"),
    ('2', '₂', "2"),
    ('3', '₃', "3"),
    ('4', '₄', "4"),
    ('5', '₅', "5"),
    ('6', '₆', "6"),
    ('7', '₇', "7"),
    ('8', '₈', "8"),
    ('9', '₉', "9"),
    ('n', 'ₙ', "n"),
    ('i', 'ᵢ', "i"),
];

/// Look up a MathJax-style command by name without a leading backslash.
#[must_use]
pub fn lookup_command(name: &str) -> Option<CommandInfo> {
    COMMANDS
        .iter()
        .find_map(|entry| (entry.name == name).then(|| entry.info()))
}

/// Return whether a command is known but outside mdwright's Unicode subset.
#[must_use]
pub fn is_known_unsupported_command(name: &str) -> bool {
    lookup_command(name).is_some_and(|info| info.support() == SupportStatus::Unsupported)
}

/// Return the Unicode symbol for a direct LaTeX command.
#[must_use]
pub fn latex_symbol(name: &str) -> Option<&'static str> {
    lookup_command(name).and_then(|info| {
        (info.support() == SupportStatus::DirectUnicode)
            .then(|| info.unicode())
            .flatten()
    })
}

/// Return one preferred LaTeX command name for a Unicode symbol.
#[must_use]
pub fn unicode_symbol_latex(symbol: &str) -> Option<&'static str> {
    COMMANDS
        .iter()
        .find_map(|entry| (entry.unicode == Some(symbol) && entry.preferred == entry.name).then_some(entry.preferred))
}

/// Unicode superscript for a single ASCII character.
#[must_use]
pub fn unicode_super(c: char) -> Option<char> {
    SUPERSCRIPTS
        .iter()
        .find_map(|(source, rendered, _latex)| (*source == c).then_some(*rendered))
}

/// Unicode subscript for a single ASCII character.
#[must_use]
pub fn unicode_sub(c: char) -> Option<char> {
    SUBSCRIPTS
        .iter()
        .find_map(|(source, rendered, _latex)| (*source == c).then_some(*rendered))
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

/// Preferred ASCII source for one Unicode superscript character.
#[must_use]
pub fn unicode_super_latex(c: char) -> Option<&'static str> {
    SUPERSCRIPTS
        .iter()
        .find_map(|(_source, rendered, latex)| (*rendered == c).then_some(*latex))
}

/// Preferred ASCII source for one Unicode subscript character.
#[must_use]
pub fn unicode_sub_latex(c: char) -> Option<&'static str> {
    SUBSCRIPTS
        .iter()
        .find_map(|(_source, rendered, latex)| (*rendered == c).then_some(*latex))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, reason = "registry tests fail with direct invariant context")]

    use super::*;

    fn command(name: &str) -> CommandInfo {
        lookup_command(name).unwrap_or_else(|| panic!("missing registry command {name}"))
    }

    #[test]
    fn direct_symbol_lookup_covers_mathjax_style_categories() {
        assert_eq!(latex_symbol("alpha"), Some("α"));
        assert_eq!(latex_symbol("otimes"), Some("⊗"));
        assert_eq!(latex_symbol("le"), Some("≤"));
        assert_eq!(latex_symbol("rightarrow"), Some("→"));
        assert_eq!(latex_symbol("sum"), Some("∑"));
        assert_eq!(latex_symbol("langle"), Some("⟨"));
    }

    #[test]
    fn alias_lookup_keeps_preferred_reverse_spelling() {
        let le = command("le");
        assert_eq!(le.unicode(), Some("≤"));
        assert_eq!(le.preferred(), "leq");
        assert_eq!(unicode_symbol_latex("≤"), Some("leq"));
        assert_eq!(unicode_symbol_latex("∅"), Some("emptyset"));
    }

    #[test]
    fn registry_distinguishes_parsed_and_unsupported_commands() {
        let frac = command("frac");
        assert_eq!(frac.support(), SupportStatus::ParsedConstruct);
        assert_eq!(frac.arguments(), ArgumentShape::TwoRequired);

        let color = command("color");
        assert_eq!(color.support(), SupportStatus::Unsupported);
        assert!(is_known_unsupported_command("color"));
    }

    #[test]
    fn environments_are_registry_entries_not_parser_strings_only() {
        let matrix = command("matrix");
        assert_eq!(matrix.category(), CommandCategory::Environment);
        assert_eq!(matrix.arguments(), ArgumentShape::EnvironmentBody);
    }

    #[test]
    fn script_maps_support_forward_and_reverse_lookup() {
        assert_eq!(unicode_super_str("-1"), Some("⁻¹".to_owned()));
        assert_eq!(unicode_super_str("n"), Some("ⁿ".to_owned()));
        assert_eq!(unicode_sub_str("i"), Some("ᵢ".to_owned()));
        assert_eq!(unicode_sub_str("x"), None);
        assert_eq!(unicode_super_latex('⁻'), Some("-"));
        assert_eq!(unicode_sub_latex('ᵢ'), Some("i"));
    }
}
