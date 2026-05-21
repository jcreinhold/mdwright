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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LatexSourceFragment {
    Command(&'static str),
    Raw(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperatorWordKind {
    BuiltInCommand,
    OperatorName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OperatorWordInfo {
    pub(crate) source: LatexSourceFragment,
    pub(crate) kind: OperatorWordKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MathAlphabetStyle {
    Bold,
    Italic,
    BoldItalic,
    Script,
    BoldScript,
    Fraktur,
    DoubleStruck,
    BoldFraktur,
    Sans,
    SansBold,
    SansItalic,
    SansBoldItalic,
    Monospace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MathAlphabetChar {
    pub style: MathAlphabetStyle,
    pub base: char,
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
    CommandEntry::direct("sqcup", CommandCategory::BinaryOperator, "⊔", "sqcup", AMS),
    CommandEntry::direct("setminus", CommandCategory::BinaryOperator, "∖", "setminus", BASE),
    CommandEntry::direct("oplus", CommandCategory::BinaryOperator, "⊕", "oplus", BASE),
    CommandEntry::direct("bigoplus", CommandCategory::LargeOperator, "⨁", "bigoplus", BASE),
    CommandEntry::direct("otimes", CommandCategory::BinaryOperator, "⊗", "otimes", BASE),
    CommandEntry::direct("bigotimes", CommandCategory::LargeOperator, "⨂", "bigotimes", BASE),
    CommandEntry::direct("boxtimes", CommandCategory::BinaryOperator, "⊠", "boxtimes", AMS),
    CommandEntry::direct("ominus", CommandCategory::BinaryOperator, "⊖", "ominus", BASE),
    CommandEntry::direct("oslash", CommandCategory::BinaryOperator, "⊘", "oslash", BASE),
    CommandEntry::direct("odot", CommandCategory::BinaryOperator, "⊙", "odot", BASE),
    CommandEntry::direct("amalg", CommandCategory::BinaryOperator, "⨿", "amalg", BASE),
    // Relations.
    CommandEntry::direct("leq", CommandCategory::Relation, "≤", "leq", BASE),
    CommandEntry::direct("le", CommandCategory::Relation, "≤", "leq", BASE),
    CommandEntry::direct("leqslant", CommandCategory::Relation, "⩽", "leqslant", AMS),
    CommandEntry::direct(
        "nleqslant",
        CommandCategory::Relation,
        "\u{2A7D}\u{0338}",
        "nleqslant",
        AMS,
    ),
    CommandEntry::direct("geq", CommandCategory::Relation, "≥", "geq", BASE),
    CommandEntry::direct("ge", CommandCategory::Relation, "≥", "geq", BASE),
    CommandEntry::direct("geqslant", CommandCategory::Relation, "⩾", "geqslant", AMS),
    CommandEntry::direct(
        "ngeqslant",
        CommandCategory::Relation,
        "\u{2A7E}\u{0338}",
        "ngeqslant",
        AMS,
    ),
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
    CommandEntry::direct("nsubset", CommandCategory::Relation, "⊄", "nsubset", AMS),
    CommandEntry::direct("nsupset", CommandCategory::Relation, "⊅", "nsupset", AMS),
    CommandEntry::direct("nsupseteq", CommandCategory::Relation, "⊉", "nsupseteq", AMS),
    CommandEntry::direct("subsetneq", CommandCategory::Relation, "⊊", "subsetneq", AMS),
    CommandEntry::direct("supsetneq", CommandCategory::Relation, "⊋", "supsetneq", AMS),
    CommandEntry::direct("subseteq", CommandCategory::Relation, "⊆", "subseteq", BASE),
    CommandEntry::direct("supseteq", CommandCategory::Relation, "⊇", "supseteq", BASE),
    CommandEntry::direct("models", CommandCategory::Relation, "⊨", "models", AMS),
    CommandEntry::direct("vdash", CommandCategory::Relation, "⊢", "vdash", BASE),
    CommandEntry::direct("dashv", CommandCategory::Relation, "⊣", "dashv", BASE),
    CommandEntry::direct("perp", CommandCategory::Relation, "⊥", "perp", BASE),
    CommandEntry::direct("parallel", CommandCategory::Relation, "∥", "parallel", BASE),
    CommandEntry::direct("mid", CommandCategory::Relation, "∣", "mid", BASE),
    CommandEntry::direct("asymp", CommandCategory::Relation, "≍", "asymp", BASE),
    CommandEntry::direct("prec", CommandCategory::Relation, "≺", "prec", AMS),
    CommandEntry::direct("nprec", CommandCategory::Relation, "⊀", "nprec", AMS),
    CommandEntry::direct("preceq", CommandCategory::Relation, "≼", "preceq", AMS),
    CommandEntry::direct("succeq", CommandCategory::Relation, "≽", "succeq", AMS),
    CommandEntry::direct("gg", CommandCategory::Relation, "≫", "gg", BASE),
    // Arrows.
    CommandEntry::direct("to", CommandCategory::Arrow, "→", "to", BASE),
    CommandEntry::direct("rightarrow", CommandCategory::Arrow, "→", "to", BASE),
    CommandEntry::direct("gets", CommandCategory::Arrow, "←", "leftarrow", BASE),
    CommandEntry::direct("leftarrow", CommandCategory::Arrow, "←", "leftarrow", BASE),
    CommandEntry::direct("mapsto", CommandCategory::Arrow, "↦", "mapsto", BASE),
    CommandEntry::direct("leftrightarrow", CommandCategory::Arrow, "↔", "leftrightarrow", BASE),
    CommandEntry::direct(
        "twoheadrightarrow",
        CommandCategory::Arrow,
        "↠",
        "twoheadrightarrow",
        AMS,
    ),
    CommandEntry::direct("Rightarrow", CommandCategory::Arrow, "⇒", "Rightarrow", BASE),
    CommandEntry::direct("Leftarrow", CommandCategory::Arrow, "⇐", "Leftarrow", BASE),
    CommandEntry::direct("Leftrightarrow", CommandCategory::Arrow, "⇔", "Leftrightarrow", BASE),
    CommandEntry::direct("longrightarrow", CommandCategory::Arrow, "⟶", "longrightarrow", BASE),
    CommandEntry::direct("longleftarrow", CommandCategory::Arrow, "⟵", "longleftarrow", BASE),
    CommandEntry::direct(
        "longleftrightarrow",
        CommandCategory::Arrow,
        "⟷",
        "longleftrightarrow",
        BASE,
    ),
    CommandEntry::direct("Longrightarrow", CommandCategory::Arrow, "⟹", "Longrightarrow", BASE),
    CommandEntry::direct("Longleftarrow", CommandCategory::Arrow, "⟸", "Longleftarrow", BASE),
    CommandEntry::direct(
        "Longleftrightarrow",
        CommandCategory::Arrow,
        "⟺",
        "Longleftrightarrow",
        BASE,
    ),
    CommandEntry::direct("hookrightarrow", CommandCategory::Arrow, "↪", "hookrightarrow", BASE),
    CommandEntry::direct("hookleftarrow", CommandCategory::Arrow, "↩", "hookleftarrow", BASE),
    CommandEntry::direct("uparrow", CommandCategory::Arrow, "↑", "uparrow", BASE),
    CommandEntry::direct("downarrow", CommandCategory::Arrow, "↓", "downarrow", BASE),
    CommandEntry::direct("updownarrow", CommandCategory::Arrow, "↕", "updownarrow", BASE),
    CommandEntry::direct("dashrightarrow", CommandCategory::Arrow, "⇢", "dashrightarrow", AMS),
    CommandEntry::direct("curvearrowright", CommandCategory::Arrow, "↷", "curvearrowright", AMS),
    CommandEntry::direct("rightsquigarrow", CommandCategory::Arrow, "↝", "rightsquigarrow", AMS),
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
    CommandEntry::direct("cdots", CommandCategory::Symbol, "⋯", "cdots", BASE),
    CommandEntry::direct("dots", CommandCategory::Symbol, "…", "cdots", BASE),
    CommandEntry::direct("square", CommandCategory::Symbol, "□", "square", AMS),
    CommandEntry::direct("complement", CommandCategory::Symbol, "∁", "complement", AMS),
    CommandEntry::direct("sharp", CommandCategory::Symbol, "♯", "sharp", BASE),
    CommandEntry::direct("flat", CommandCategory::Symbol, "♭", "flat", BASE),
    CommandEntry::direct("natural", CommandCategory::Symbol, "♮", "natural", BASE),
    CommandEntry::direct("wr", CommandCategory::BinaryOperator, "≀", "wr", BASE),
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
    CommandEntry::direct("exp", CommandCategory::Function, "exp", "exp", BASE),
    CommandEntry::direct("log", CommandCategory::Function, "log", "log", BASE),
    CommandEntry::direct("ln", CommandCategory::Function, "ln", "ln", BASE),
    CommandEntry::direct("lim", CommandCategory::Function, "lim", "lim", BASE),
    CommandEntry::direct("arg", CommandCategory::Function, "arg", "arg", BASE),
    CommandEntry::direct("det", CommandCategory::Function, "det", "det", BASE),
    CommandEntry::direct("dim", CommandCategory::Function, "dim", "dim", BASE),
    CommandEntry::direct("ker", CommandCategory::Function, "ker", "ker", BASE),
    CommandEntry::direct("im", CommandCategory::Function, "im", "im", BASE),
    CommandEntry::direct("coker", CommandCategory::Function, "coker", "coker", AMS),
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
    ('+', '⁺', "+"),
    ('=', '⁼', "="),
    ('(', '⁽', "("),
    (')', '⁾', ")"),
    ('a', 'ᵃ', "a"),
    ('b', 'ᵇ', "b"),
    ('c', 'ᶜ', "c"),
    ('d', 'ᵈ', "d"),
    ('e', 'ᵉ', "e"),
    ('f', 'ᶠ', "f"),
    ('g', 'ᵍ', "g"),
    ('h', 'ʰ', "h"),
    ('n', 'ⁿ', "n"),
    ('i', 'ⁱ', "i"),
    ('j', 'ʲ', "j"),
    ('k', 'ᵏ', "k"),
    ('l', 'ˡ', "l"),
    ('m', 'ᵐ', "m"),
    ('o', 'ᵒ', "o"),
    ('p', 'ᵖ', "p"),
    ('r', 'ʳ', "r"),
    ('s', 'ˢ', "s"),
    ('t', 'ᵗ', "t"),
    ('u', 'ᵘ', "u"),
    ('v', 'ᵛ', "v"),
    ('w', 'ʷ', "w"),
    ('x', 'ˣ', "x"),
    ('y', 'ʸ', "y"),
    ('z', 'ᶻ', "z"),
    ('A', 'ᴬ', "A"),
    ('B', 'ᴮ', "B"),
    ('D', 'ᴰ', "D"),
    ('E', 'ᴱ', "E"),
    ('G', 'ᴳ', "G"),
    ('H', 'ᴴ', "H"),
    ('I', 'ᴵ', "I"),
    ('J', 'ᴶ', "J"),
    ('K', 'ᴷ', "K"),
    ('L', 'ᴸ', "L"),
    ('M', 'ᴹ', "M"),
    ('N', 'ᴺ', "N"),
    ('O', 'ᴼ', "O"),
    ('P', 'ᴾ', "P"),
    ('R', 'ᴿ', "R"),
    ('T', 'ᵀ', "T"),
    ('U', 'ᵁ', "U"),
    ('V', 'ⱽ', "V"),
    ('W', 'ᵂ', "W"),
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
    ('+', '₊', "+"),
    ('-', '₋', "-"),
    ('=', '₌', "="),
    ('(', '₍', "("),
    (')', '₎', ")"),
    ('a', 'ₐ', "a"),
    ('e', 'ₑ', "e"),
    ('h', 'ₕ', "h"),
    ('j', 'ⱼ', "j"),
    ('k', 'ₖ', "k"),
    ('l', 'ₗ', "l"),
    ('m', 'ₘ', "m"),
    ('n', 'ₙ', "n"),
    ('o', 'ₒ', "o"),
    ('p', 'ₚ', "p"),
    ('r', 'ᵣ', "r"),
    ('s', 'ₛ', "s"),
    ('t', 'ₜ', "t"),
    ('u', 'ᵤ', "u"),
    ('v', 'ᵥ', "v"),
    ('x', 'ₓ', "x"),
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

pub(crate) fn unicode_symbol_latex_source(symbol: &str) -> Option<LatexSourceFragment> {
    match symbol {
        "−" => Some(LatexSourceFragment::Raw("-")),
        "·" | "⋅" => Some(LatexSourceFragment::Command("cdot")),
        "…" => Some(LatexSourceFragment::Command("cdots")),
        "ℎ" => Some(LatexSourceFragment::Raw("h")),
        "ℴ" => Some(LatexSourceFragment::Raw("o")),
        "\u{227A}\u{0338}" => Some(LatexSourceFragment::Command("nprec")),
        "\u{2A7D}\u{0338}" => Some(LatexSourceFragment::Command("nleqslant")),
        "⥲" => Some(LatexSourceFragment::Raw(r"\xrightarrow{\sim}")),
        "⤏" => Some(LatexSourceFragment::Command("dashrightarrow")),
        _ => unicode_symbol_latex(symbol).map(LatexSourceFragment::Command),
    }
}

pub(crate) fn unicode_math_alphabet_char(ch: char) -> Option<MathAlphabetChar> {
    legacy_math_alphabet_char(ch)
        .or_else(|| latin_math_alphabet_char(ch))
        .or_else(|| digit_math_alphabet_char(ch))
        .or_else(|| greek_math_alphabet_char(ch))
}

pub(crate) fn math_alphabet_latex_command(style: MathAlphabetStyle) -> Option<&'static str> {
    match style {
        MathAlphabetStyle::Bold => Some("mathbf"),
        MathAlphabetStyle::Italic | MathAlphabetStyle::BoldItalic => Some("mathit"),
        MathAlphabetStyle::Script | MathAlphabetStyle::BoldScript => Some("mathcal"),
        MathAlphabetStyle::Fraktur | MathAlphabetStyle::BoldFraktur => Some("mathfrak"),
        MathAlphabetStyle::DoubleStruck => Some("mathbb"),
        MathAlphabetStyle::Sans
        | MathAlphabetStyle::SansBold
        | MathAlphabetStyle::SansItalic
        | MathAlphabetStyle::SansBoldItalic => Some("mathsf"),
        MathAlphabetStyle::Monospace => Some("mathtt"),
    }
}

// Word normalization is registry data rather than xtask migration policy or
// parser branches. The parser owns source shape; this table owns the narrower
// vocabulary decision that a word is a mathematical operator.
const BUILT_IN_OPERATOR_WORDS: &[(&str, &str)] = &[
    ("log", "log"),
    ("sin", "sin"),
    ("cos", "cos"),
    ("tan", "tan"),
    ("exp", "exp"),
    ("dim", "dim"),
    ("ker", "ker"),
    ("im", "im"),
    ("coker", "coker"),
    ("lim", "lim"),
    ("sup", "sup"),
    ("inf", "inf"),
    ("max", "max"),
    ("min", "min"),
];

const NAMED_OPERATOR_WORDS: &[(&str, &str)] = &[
    ("Spec", r"\operatorname{Spec}"),
    ("Proj", r"\operatorname{Proj}"),
    ("Hom", r"\operatorname{Hom}"),
    ("End", r"\operatorname{End}"),
    ("Aut", r"\operatorname{Aut}"),
    ("Gal", r"\operatorname{Gal}"),
    ("Pic", r"\operatorname{Pic}"),
    ("Div", r"\operatorname{Div}"),
    ("Der", r"\operatorname{Der}"),
    ("Idem", r"\operatorname{Idem}"),
    ("Frob", r"\operatorname{Frob}"),
    ("LabCusp", r"\operatorname{LabCusp}"),
];

pub(crate) fn operator_word_latex_source(word: &str) -> Option<OperatorWordInfo> {
    if let Some((_word, command)) = BUILT_IN_OPERATOR_WORDS
        .iter()
        .find(|(candidate, _command)| *candidate == word)
    {
        return Some(OperatorWordInfo {
            source: LatexSourceFragment::Command(command),
            kind: OperatorWordKind::BuiltInCommand,
        });
    }
    NAMED_OPERATOR_WORDS
        .iter()
        .find(|(candidate, _source)| *candidate == word)
        .map(|(_word, source)| OperatorWordInfo {
            source: LatexSourceFragment::Raw(source),
            kind: OperatorWordKind::OperatorName,
        })
}

pub(crate) fn styled_operator_word_latex_source(style: MathAlphabetStyle, base: &str) -> Option<OperatorWordInfo> {
    match style {
        MathAlphabetStyle::Script | MathAlphabetStyle::BoldScript => operator_word_latex_source(base),
        MathAlphabetStyle::Bold
        | MathAlphabetStyle::Italic
        | MathAlphabetStyle::BoldItalic
        | MathAlphabetStyle::Fraktur
        | MathAlphabetStyle::DoubleStruck
        | MathAlphabetStyle::BoldFraktur
        | MathAlphabetStyle::Sans
        | MathAlphabetStyle::SansBold
        | MathAlphabetStyle::SansItalic
        | MathAlphabetStyle::SansBoldItalic
        | MathAlphabetStyle::Monospace => None,
    }
}

fn legacy_math_alphabet_char(ch: char) -> Option<MathAlphabetChar> {
    let (style, base) = match ch {
        'ℂ' => (MathAlphabetStyle::DoubleStruck, 'C'),
        'ℍ' => (MathAlphabetStyle::DoubleStruck, 'H'),
        'ℕ' => (MathAlphabetStyle::DoubleStruck, 'N'),
        'ℙ' => (MathAlphabetStyle::DoubleStruck, 'P'),
        'ℚ' => (MathAlphabetStyle::DoubleStruck, 'Q'),
        'ℝ' => (MathAlphabetStyle::DoubleStruck, 'R'),
        'ℤ' => (MathAlphabetStyle::DoubleStruck, 'Z'),
        'ℬ' => (MathAlphabetStyle::Script, 'B'),
        'ℰ' => (MathAlphabetStyle::Script, 'E'),
        'ℱ' => (MathAlphabetStyle::Script, 'F'),
        'ℋ' => (MathAlphabetStyle::Script, 'H'),
        'ℐ' => (MathAlphabetStyle::Script, 'I'),
        'ℒ' => (MathAlphabetStyle::Script, 'L'),
        'ℳ' => (MathAlphabetStyle::Script, 'M'),
        'ℛ' => (MathAlphabetStyle::Script, 'R'),
        'ℯ' => (MathAlphabetStyle::Script, 'e'),
        'ℭ' => (MathAlphabetStyle::Fraktur, 'C'),
        'ℌ' => (MathAlphabetStyle::Fraktur, 'H'),
        'ℑ' => (MathAlphabetStyle::Fraktur, 'I'),
        'ℜ' => (MathAlphabetStyle::Fraktur, 'R'),
        'ℨ' => (MathAlphabetStyle::Fraktur, 'Z'),
        _ => return None,
    };
    Some(MathAlphabetChar { style, base })
}

fn latin_math_alphabet_char(ch: char) -> Option<MathAlphabetChar> {
    const LATIN_RANGES: &[(u32, MathAlphabetStyle, char, usize)] = &[
        (0x1d400, MathAlphabetStyle::Bold, 'A', 26),
        (0x1d41a, MathAlphabetStyle::Bold, 'a', 26),
        (0x1d434, MathAlphabetStyle::Italic, 'A', 26),
        (0x1d44e, MathAlphabetStyle::Italic, 'a', 26),
        (0x1d468, MathAlphabetStyle::BoldItalic, 'A', 26),
        (0x1d482, MathAlphabetStyle::BoldItalic, 'a', 26),
        (0x1d49c, MathAlphabetStyle::Script, 'A', 26),
        (0x1d4b6, MathAlphabetStyle::Script, 'a', 26),
        (0x1d4d0, MathAlphabetStyle::BoldScript, 'A', 26),
        (0x1d4ea, MathAlphabetStyle::BoldScript, 'a', 26),
        (0x1d504, MathAlphabetStyle::Fraktur, 'A', 26),
        (0x1d51e, MathAlphabetStyle::Fraktur, 'a', 26),
        (0x1d538, MathAlphabetStyle::DoubleStruck, 'A', 26),
        (0x1d552, MathAlphabetStyle::DoubleStruck, 'a', 26),
        (0x1d56c, MathAlphabetStyle::BoldFraktur, 'A', 26),
        (0x1d586, MathAlphabetStyle::BoldFraktur, 'a', 26),
        (0x1d5a0, MathAlphabetStyle::Sans, 'A', 26),
        (0x1d5ba, MathAlphabetStyle::Sans, 'a', 26),
        (0x1d5d4, MathAlphabetStyle::SansBold, 'A', 26),
        (0x1d5ee, MathAlphabetStyle::SansBold, 'a', 26),
        (0x1d608, MathAlphabetStyle::SansItalic, 'A', 26),
        (0x1d622, MathAlphabetStyle::SansItalic, 'a', 26),
        (0x1d63c, MathAlphabetStyle::SansBoldItalic, 'A', 26),
        (0x1d656, MathAlphabetStyle::SansBoldItalic, 'a', 26),
        (0x1d670, MathAlphabetStyle::Monospace, 'A', 26),
        (0x1d68a, MathAlphabetStyle::Monospace, 'a', 26),
    ];
    for &(start, style, base, len) in LATIN_RANGES {
        if let Some(mapped) = offset_char(ch, start, base, len) {
            return Some(MathAlphabetChar { style, base: mapped });
        }
    }
    None
}

fn digit_math_alphabet_char(ch: char) -> Option<MathAlphabetChar> {
    const DIGIT_RANGES: &[(u32, MathAlphabetStyle)] = &[
        (0x1d7ce, MathAlphabetStyle::Bold),
        (0x1d7d8, MathAlphabetStyle::DoubleStruck),
        (0x1d7e2, MathAlphabetStyle::Sans),
        (0x1d7ec, MathAlphabetStyle::SansBold),
        (0x1d7f6, MathAlphabetStyle::Monospace),
    ];
    for &(start, style) in DIGIT_RANGES {
        if let Some(base) = offset_char(ch, start, '0', 10) {
            return Some(MathAlphabetChar { style, base });
        }
    }
    None
}

fn greek_math_alphabet_char(ch: char) -> Option<MathAlphabetChar> {
    const GREEK_BOLD_UPPER: &[char] = &[
        'Α', 'Β', 'Γ', 'Δ', 'Ε', 'Ζ', 'Η', 'Θ', 'Ι', 'Κ', 'Λ', 'Μ', 'Ν', 'Ξ', 'Ο', 'Π', 'Ρ', 'ϴ', 'Σ', 'Τ', 'Υ', 'Φ',
        'Χ', 'Ψ', 'Ω',
    ];
    const GREEK_BOLD_LOWER: &[char] = &[
        '∇', 'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'ι', 'κ', 'λ', 'μ', 'ν', 'ξ', 'ο', 'π', 'ρ', 'ς', 'σ', 'τ', 'υ',
        'φ', 'χ', 'ψ', 'ω',
    ];
    greek_range(ch, 0x1d6a8, MathAlphabetStyle::Bold, GREEK_BOLD_UPPER)
        .or_else(|| greek_range(ch, 0x1d6c2, MathAlphabetStyle::Bold, GREEK_BOLD_LOWER))
        .or_else(|| greek_range(ch, 0x1d6e2, MathAlphabetStyle::Italic, GREEK_BOLD_UPPER))
        .or_else(|| greek_range(ch, 0x1d6fc, MathAlphabetStyle::Italic, GREEK_BOLD_LOWER))
        .or_else(|| greek_range(ch, 0x1d71c, MathAlphabetStyle::BoldItalic, GREEK_BOLD_UPPER))
        .or_else(|| greek_range(ch, 0x1d736, MathAlphabetStyle::BoldItalic, GREEK_BOLD_LOWER))
        .or_else(|| greek_range(ch, 0x1d756, MathAlphabetStyle::SansBold, GREEK_BOLD_UPPER))
        .or_else(|| greek_range(ch, 0x1d770, MathAlphabetStyle::SansBold, GREEK_BOLD_LOWER))
        .or_else(|| greek_range(ch, 0x1d790, MathAlphabetStyle::SansBoldItalic, GREEK_BOLD_UPPER))
        .or_else(|| greek_range(ch, 0x1d7aa, MathAlphabetStyle::SansBoldItalic, GREEK_BOLD_LOWER))
}

fn greek_range(ch: char, start: u32, style: MathAlphabetStyle, bases: &'static [char]) -> Option<MathAlphabetChar> {
    let value = u32::from(ch);
    let offset = value.checked_sub(start)?;
    let index = usize::try_from(offset).ok()?;
    let base = bases.get(index).copied()?;
    Some(MathAlphabetChar { style, base })
}

fn offset_char(ch: char, start: u32, base: char, len: usize) -> Option<char> {
    let value = u32::from(ch);
    let offset = value.checked_sub(start)?;
    let index = usize::try_from(offset).ok()?;
    if index >= len {
        return None;
    }
    let base_value = u32::from(base);
    let mapped = base_value.checked_add(offset)?;
    char::from_u32(mapped)
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
    if c == '°' {
        return Some(r"\circ");
    }
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
        assert_eq!(
            unicode_symbol_latex_source("≤"),
            Some(LatexSourceFragment::Command("leq"))
        );
        assert_eq!(unicode_symbol_latex_source("−"), Some(LatexSourceFragment::Raw("-")));
        assert_eq!(
            unicode_symbol_latex_source("⥲"),
            Some(LatexSourceFragment::Raw(r"\xrightarrow{\sim}"))
        );
        assert_eq!(
            unicode_symbol_latex_source("\u{227A}\u{0338}"),
            Some(LatexSourceFragment::Command("nprec"))
        );
        assert_eq!(
            unicode_symbol_latex_source("\u{2A7D}\u{0338}"),
            Some(LatexSourceFragment::Command("nleqslant"))
        );
        assert_eq!(
            unicode_symbol_latex_source("⤏"),
            Some(LatexSourceFragment::Command("dashrightarrow"))
        );
    }

    #[test]
    fn operator_word_lookup_keeps_semantic_vocabulary_in_registry() {
        let log = operator_word_latex_source("log").unwrap_or_else(|| panic!("missing log operator entry"));
        assert_eq!(log.source, LatexSourceFragment::Command("log"));
        assert_eq!(log.kind, OperatorWordKind::BuiltInCommand);

        let spec = operator_word_latex_source("Spec").unwrap_or_else(|| panic!("missing Spec operator entry"));
        assert_eq!(spec.source, LatexSourceFragment::Raw(r"\operatorname{Spec}"));
        assert_eq!(spec.kind, OperatorWordKind::OperatorName);

        let styled_hom = styled_operator_word_latex_source(MathAlphabetStyle::Script, "Hom")
            .unwrap_or_else(|| panic!("missing script Hom semantic entry"));
        assert_eq!(styled_hom.source, LatexSourceFragment::Raw(r"\operatorname{Hom}"));

        assert_eq!(operator_word_latex_source("Thing"), None);
        assert_eq!(
            styled_operator_word_latex_source(MathAlphabetStyle::Fraktur, "Spec"),
            None
        );
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
        assert_eq!(unicode_sub_str("x"), Some("ₓ".to_owned()));
        assert_eq!(unicode_super_latex('⁻'), Some("-"));
        assert_eq!(unicode_sub_latex('ᵢ'), Some("i"));
    }
}
