//! Project configuration loaded from `mdwright.toml`.
//!
//! The boundary [`Config::load_explicit`] / [`Config::discover`] hides
//! the discovery surfaces (explicit `--config` path; an ancestor walk
//! over `.mdwright.toml`, `mdwright.toml`, and `pyproject.toml`'s
//! `[tool.mdwright]` table, stopping at the first `.git/` boundary),
//! TOML parsing, schema validation, and the mapping from raw TOML
//! shapes into resolved values. Callers see opaque types with getters;
//! nothing outside this module imports `toml` or `serde`.
//!
//! ## Why two layers internally
//!
//! Misspellings in the TOML must produce immediate errors, so the
//! private [`Schema`] family deserialises with
//! `#[serde(deny_unknown_fields)]` and tracks per-key presence via
//! `Option<…>`. The public types ([`Config`], [`FmtOptions`], the
//! style enums) carry already-resolved values — no `Option`s leak —
//! and stay stable even if the on-disk format gains alternate
//! representations (e.g. CLI overrides for individual keys later on).

use std::collections::HashSet;
use std::error::Error as StdError;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use mdwright_document::{
    ExtensionOptions, GfmAutolinkPolicy, GfmOptions, MystOptions, PandocOptions, ParseOptions, RenderOptions,
    RenderProfile,
};
use mdwright_format::{
    EndOfLine, FmtOptions, HeadingAttrsStyle, ItalicStyle, LinkDefStyle, ListContinuationIndent, ListMarkerStyle,
    MathOptions, MathRender, OrderedListStyle, Placement, StrongStyle, TableStyle, ThematicStyle, TrailingNewline,
    Wrap, WrapStrategy,
};
use mdwright_lint::RuleSet;
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer};

// ============================================================
// Public surface
// ============================================================

/// Resolved project configuration. Construct with
/// [`Config::load_explicit`] (for `--config PATH`) or
/// [`Config::discover`] (for the ancestor walk from CWD).
#[derive(Debug, Clone)]
pub struct Config {
    lint_rule_selection: LintRuleSelection,
    exclude_globs: Vec<String>,
    extra_info_strings: Vec<String>,
    fmt_options: FmtOptions,
    parse_options: ParseOptions,
    render_options: RenderOptions,
    /// Path of the file this config was loaded from, if any. `None`
    /// for the defaults instance.
    source: Option<PathBuf>,
}

impl Config {
    /// Load configuration from exactly `path`. Used for `--config PATH`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if the file is missing, unreadable,
    /// malformed TOML, or fails schema validation (an unknown key or a
    /// malformed value is an error, not a silent default).
    pub fn load_explicit(path: &Path) -> Result<Self, ConfigError> {
        read_mdwright_toml(path)
    }

    /// Discover the nearest applicable config by walking upward from
    /// `cwd`. At each directory, candidates are tried in precedence
    /// order: `.mdwright.toml`, then `mdwright.toml`, then
    /// `pyproject.toml`'s `[tool.mdwright]` table (a `pyproject.toml`
    /// *without* that table does not stop the walk). The walk stops
    /// at the filesystem root or the first directory containing a
    /// `.git/` entry (the workspace boundary).
    ///
    /// Returns the all-defaults instance if no candidate is found.
    /// Absence of a config file is *not* an error.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if a candidate file is found but cannot
    /// be read, parsed as TOML, or matched against the schema.
    pub fn discover(cwd: &Path) -> Result<Self, ConfigError> {
        match discover_walk(cwd)? {
            Some(cfg) => Ok(cfg),
            None => Ok(Self::from_schema(Schema::default(), None)),
        }
    }

    /// Path of the configuration file this `Config` was loaded from,
    /// or `None` if no file was used (the defaults instance).
    #[must_use]
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    /// Directory containing the configuration file, useful as the
    /// base for resolving relative paths inside the config (e.g.
    /// `[lint] exclude` globs). `None` when no file was loaded; in
    /// that case callers typically use `$PWD` as the base.
    #[must_use]
    pub fn source_dir(&self) -> Option<&Path> {
        self.source.as_deref().and_then(Path::parent)
    }

    /// Resolved lint rule selection from `[lint]`.
    #[must_use]
    pub fn lint_rule_selection(&self) -> &LintRuleSelection {
        &self.lint_rule_selection
    }

    /// Gitignore-style patterns from `[lint] exclude`. Files matching
    /// any pattern are dropped from lint runs.
    #[must_use]
    pub fn exclude_globs(&self) -> &[String] {
        &self.exclude_globs
    }

    /// Project-specific allowlist extension for `info-string-typo`.
    /// The stdlib default still applies; these are *additions*.
    #[must_use]
    pub fn extra_info_strings(&self) -> &[String] {
        &self.extra_info_strings
    }

    /// Resolved formatter knobs from `[fmt]`. Formatter sessions are
    /// the first consumers; the lint side ignores these.
    #[must_use]
    pub fn fmt_options(&self) -> &FmtOptions {
        &self.fmt_options
    }

    /// Resolved Markdown recognition policy.
    #[must_use]
    pub fn parse_options(&self) -> ParseOptions {
        self.parse_options
    }

    /// Resolved HTML rendering policy.
    #[must_use]
    pub fn render_options(&self) -> RenderOptions {
        self.render_options
    }

    /// The all-defaults [`Config`] — what [`Self::discover`] returns
    /// when no `.mdwright.toml` / `mdwright.toml` / `pyproject.toml`
    /// is found on the upward walk. Exposed for long-lived processes
    /// (the LSP server) that need a synchronous fallback when
    /// `discover` encounters an unreadable config file mid-walk.
    #[must_use]
    pub fn defaults() -> Self {
        Self::from_schema(Schema::default(), None)
    }

    fn from_schema(schema: Schema, source: Option<PathBuf>) -> Self {
        let Schema {
            lint,
            fmt,
            parse,
            render,
        } = schema;
        Self {
            lint_rule_selection: LintRuleSelection {
                preset: LintRulePreset::from(lint.preset),
                select: lint.select,
                extend_select: lint.extend_select,
                ignore: lint.ignore,
            },
            exclude_globs: lint.exclude,
            extra_info_strings: lint.info_strings.extra,
            fmt_options: fmt_options_from_schema(fmt),
            parse_options: parse_options_from_schema(parse),
            render_options: render_options_from_schema(render),
            source,
        }
    }
}

/// Named baseline for lint rule selection.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum LintRulePreset {
    /// The curated default-on rule set.
    #[default]
    Default,
    /// Every registered rule.
    All,
    /// No baseline rules; use `select` for an exact explicit set.
    None,
}

/// Resolved `[lint]` rule-selection policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintRuleSelection {
    preset: LintRulePreset,
    select: Vec<String>,
    extend_select: Vec<String>,
    ignore: Vec<String>,
}

impl LintRuleSelection {
    #[must_use]
    pub fn preset(&self) -> LintRulePreset {
        self.preset
    }

    #[must_use]
    pub fn select(&self) -> &[String] {
        &self.select
    }

    #[must_use]
    pub fn extend_select(&self) -> &[String] {
        &self.extend_select
    }

    #[must_use]
    pub fn ignore(&self) -> &[String] {
        &self.ignore
    }

    /// Partition the available rule pool according to this config.
    ///
    /// The config schema owns selector policy, but callers provide the
    /// available pool so downstream binaries can include custom rules
    /// without teaching `mdwright-config` where they came from.
    ///
    /// # Errors
    ///
    /// Returns [`RuleSelectionError`] when a selected rule name is not
    /// present in `available`, or when a manually constructed selection
    /// violates the TOML schema invariants.
    pub fn resolve(&self, available: RuleSet) -> Result<RuleSet, RuleSelectionError> {
        if self.preset != LintRulePreset::None && !self.select.is_empty() {
            return Err(RuleSelectionError::new(
                "`lint.select` can only be used with `lint.preset = \"none\"`; use `extend-select` to add rules to a preset",
            ));
        }

        let inventory: Vec<(String, bool)> = available
            .iter()
            .map(|r| (r.name().to_owned(), r.is_default()))
            .collect();
        let all_names: HashSet<&str> = inventory.iter().map(|(name, _)| name.as_str()).collect();
        let default_names: HashSet<&str> = inventory
            .iter()
            .filter_map(|(name, is_default)| is_default.then_some(name.as_str()))
            .collect();

        let mut selected: HashSet<String> = match self.preset {
            LintRulePreset::Default => default_names.iter().map(|name| (*name).to_owned()).collect(),
            LintRulePreset::All => all_names.iter().map(|name| (*name).to_owned()).collect(),
            LintRulePreset::None => HashSet::new(),
        };

        for name in &self.select {
            ensure_known_rule(name, &all_names)?;
            selected.insert(name.clone());
        }
        for name in &self.extend_select {
            ensure_known_rule(name, &all_names)?;
            selected.insert(name.clone());
        }
        for name in &self.ignore {
            ensure_known_rule(name, &all_names)?;
            selected.remove(name);
        }

        let mut result = RuleSet::new();
        for rule in available {
            if selected.contains(rule.name()) {
                result
                    .add(rule)
                    .map_err(|err| RuleSelectionError::new(err.to_string()))?;
            }
        }
        Ok(result)
    }
}

fn ensure_known_rule(name: &str, known: &HashSet<&str>) -> Result<(), RuleSelectionError> {
    if known.contains(name) {
        Ok(())
    } else {
        Err(RuleSelectionError::new(format!(
            "unknown lint rule `{name}` (run `mdwright list-rules` to see what's registered)"
        )))
    }
}

/// Failure to resolve configured lint rule selection against the
/// available rule pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSelectionError {
    message: String,
}

impl RuleSelectionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RuleSelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl StdError for RuleSelectionError {}

/// Failure to load configuration: I/O, TOML syntax, or schema
/// validation. The `Display` impl renders the path and underlying
/// cause.
#[derive(Debug)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn io(path: &Path, err: &io::Error) -> Self {
        Self {
            message: format!("read {}: {err}", path.display()),
        }
    }

    fn parse(path: &Path, err: &toml::de::Error) -> Self {
        Self {
            message: format!("parse {}: {err}", path.display()),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl StdError for ConfigError {}

// ============================================================
// Internal schema (deserialisation target)
// ============================================================

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Schema {
    #[serde(default)]
    lint: LintSchema,
    #[serde(default)]
    fmt: FmtSchema,
    #[serde(default)]
    parse: ParseSchema,
    #[serde(default)]
    render: RenderSchema,
}

#[derive(Debug)]
struct LintSchema {
    preset: LintPresetSchema,
    select: Vec<String>,
    extend_select: Vec<String>,
    ignore: Vec<String>,
    exclude: Vec<String>,
    info_strings: InfoStringsSchema,
}

impl Default for LintSchema {
    fn default() -> Self {
        Self {
            preset: LintPresetSchema::Default,
            select: Vec::new(),
            extend_select: Vec::new(),
            ignore: Vec::new(),
            exclude: Vec::new(),
            info_strings: InfoStringsSchema::default(),
        }
    }
}

impl<'de> Deserialize<'de> for LintSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawLintSchema {
            #[serde(default, deserialize_with = "reject_legacy_rules")]
            rules: (),
            #[serde(default)]
            preset: LintPresetSchema,
            #[serde(default)]
            select: Vec<String>,
            #[serde(default, rename = "extend-select")]
            extend_select: Vec<String>,
            #[serde(default)]
            ignore: Vec<String>,
            #[serde(default)]
            exclude: Vec<String>,
            #[serde(default, rename = "info-strings")]
            info_strings: InfoStringsSchema,
        }

        let RawLintSchema {
            rules: _rules,
            preset,
            select,
            extend_select,
            ignore,
            exclude,
            info_strings,
        } = RawLintSchema::deserialize(deserializer)?;

        for (key, names) in [
            ("select", select.as_slice()),
            ("extend-select", extend_select.as_slice()),
            ("ignore", ignore.as_slice()),
        ] {
            for name in names {
                if matches!(name.as_str(), "default" | "all" | "none") {
                    return Err(D::Error::custom(format!(
                        "`lint.{key}` accepts rule names only; `{name}` is a preset, so use `lint.preset = \"{name}\"`"
                    )));
                }
            }
        }

        if preset != LintPresetSchema::None && !select.is_empty() {
            return Err(D::Error::custom(
                "`lint.select` can only be used with `lint.preset = \"none\"`; use `extend-select` to add rules to a preset",
            ));
        }

        Ok(Self {
            preset,
            select,
            extend_select,
            ignore,
            exclude,
            info_strings,
        })
    }
}

fn reject_legacy_rules<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: Deserializer<'de>,
{
    let _ignored = toml::Value::deserialize(deserializer)?;
    Err(D::Error::custom(
        "`lint.rules` has been replaced by `lint.preset`, `lint.select`, `lint.extend-select`, and `lint.ignore`",
    ))
}

#[derive(Copy, Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum LintPresetSchema {
    #[default]
    Default,
    All,
    None,
}

impl From<LintPresetSchema> for LintRulePreset {
    fn from(s: LintPresetSchema) -> Self {
        match s {
            LintPresetSchema::Default => Self::Default,
            LintPresetSchema::All => Self::All,
            LintPresetSchema::None => Self::None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct InfoStringsSchema {
    #[serde(default)]
    extra: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FmtSchema {
    #[serde(default)]
    profile: Option<FmtProfileSchema>,
    #[serde(default)]
    wrap: Option<WrapSchema>,
    #[serde(default, rename = "wrap-strategy")]
    wrap_strategy: Option<WrapStrategySchema>,
    #[serde(default)]
    italic: Option<ItalicSchema>,
    #[serde(default)]
    strong: Option<StrongSchema>,
    #[serde(default, rename = "list-marker")]
    list_marker: Option<ListMarkerSchema>,
    #[serde(default, rename = "ordered-list")]
    ordered_list: Option<OrderedListSchema>,
    #[serde(default, rename = "thematic-break")]
    thematic_break: Option<ThematicSchema>,
    #[serde(default, rename = "trailing-newline")]
    trailing_newline: Option<TrailingNewlineSchema>,
    #[serde(default, rename = "end-of-line")]
    end_of_line: Option<EndOfLineSchema>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    refs: Option<RefsSchema>,
    #[serde(default)]
    footnotes: Option<FootnotesSchema>,
    #[serde(default)]
    tables: Option<TablesSchema>,
    #[serde(default)]
    lists: Option<ListsSchema>,
    #[serde(default)]
    frontmatter: Option<FrontmatterSchema>,
    #[serde(default)]
    math: Option<MathSchema>,
    #[serde(default, rename = "heading-attrs")]
    heading_attrs: Option<HeadingAttrsSchema>,
}

fn fmt_options_from_schema(schema: FmtSchema) -> FmtOptions {
    let refs = schema.refs.unwrap_or_default();
    let footnotes = schema.footnotes.unwrap_or_default();
    let tables = schema.tables.unwrap_or_default();
    let lists = schema.lists.unwrap_or_default();
    let frontmatter = schema.frontmatter.unwrap_or_default();
    let default = match schema.profile.unwrap_or(FmtProfileSchema::Preserve) {
        FmtProfileSchema::Preserve => FmtOptions::default(),
        FmtProfileSchema::Mdformat => FmtOptions::mdformat(),
    };
    let mut opts = default
        .clone()
        .with_exclude_globs(schema.exclude)
        .with_link_def_placement(
            refs.placement
                .map_or_else(|| default.link_def_placement(), Placement::from),
        )
        .with_link_def_style(refs.style.map_or_else(|| default.link_def_style(), LinkDefStyle::from))
        .with_footnote_placement(
            footnotes
                .placement
                .map_or_else(|| default.footnote_placement(), Placement::from),
        );
    opts = opts.with_preserve_frontmatter(frontmatter.preserve.unwrap_or_else(|| default.preserve_frontmatter()));
    opts = opts.with_table(tables.style.map_or_else(|| default.table(), TableStyle::from));
    opts = opts.with_list_continuation_indent(
        lists
            .continuation_indent
            .map_or_else(|| default.list_continuation_indent(), ListContinuationIndent::from),
    );
    if let Some(wrap) = schema.wrap {
        opts = opts.with_wrap(Wrap::from(wrap));
    }
    if let Some(strategy) = schema.wrap_strategy {
        opts = opts.with_wrap_strategy(WrapStrategy::from(strategy));
    }
    if let Some(italic) = schema.italic {
        opts = opts.with_italic(ItalicStyle::from(italic));
    }
    if let Some(strong) = schema.strong {
        opts = opts.with_strong(StrongStyle::from(strong));
    }
    if let Some(list_marker) = schema.list_marker {
        opts = opts.with_list_marker(ListMarkerStyle::from(list_marker));
    }
    if let Some(ordered_list) = schema.ordered_list {
        opts = opts.with_ordered_list(OrderedListStyle::from(ordered_list));
    }
    if let Some(thematic_break) = schema.thematic_break {
        opts = opts.with_thematic_break(ThematicStyle::from(thematic_break));
    }
    if let Some(trailing_newline) = schema.trailing_newline {
        opts = opts.with_trailing_newline(TrailingNewline::from(trailing_newline));
    }
    if let Some(end_of_line) = schema.end_of_line {
        opts = opts.with_end_of_line(EndOfLine::from(end_of_line));
    }
    if let Some(math) = schema.math {
        opts = opts.with_math(MathOptions::from(math));
    }
    if let Some(heading_attrs) = schema.heading_attrs {
        opts = opts.with_heading_attrs(HeadingAttrsStyle::from(heading_attrs));
    }
    opts
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParseSchema {
    #[serde(default)]
    extensions: Option<ExtensionsSchema>,
}

fn parse_options_from_schema(schema: ParseSchema) -> ParseOptions {
    schema.extensions.map_or_else(ParseOptions::default, |extensions| {
        ParseOptions::default().with_extensions(ExtensionOptions::from(extensions))
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderSchema {
    #[serde(default)]
    profile: Option<RenderProfileSchema>,
}

fn render_options_from_schema(schema: RenderSchema) -> RenderOptions {
    let default = RenderOptions::default();
    RenderOptions::default().with_profile(schema.profile.map_or_else(|| default.profile(), RenderProfile::from))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RenderProfileSchema {
    Pulldown,
    CmarkGfm,
}

impl From<RenderProfileSchema> for RenderProfile {
    fn from(s: RenderProfileSchema) -> Self {
        match s {
            RenderProfileSchema::Pulldown => Self::Pulldown,
            RenderProfileSchema::CmarkGfm => Self::CmarkGfm,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum HeadingAttrsSchema {
    Preserve,
    Canonicalise,
}

impl From<HeadingAttrsSchema> for HeadingAttrsStyle {
    fn from(s: HeadingAttrsSchema) -> Self {
        match s {
            HeadingAttrsSchema::Preserve => Self::Preserve,
            HeadingAttrsSchema::Canonicalise => Self::Canonicalise,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_field_names,
    clippy::struct_excessive_bools,
    reason = "shape mirrors `ExtensionOptions`; the `_lists` postfix matches the TOML key convention"
)]
struct ExtensionsSchema {
    #[serde(default)]
    gfm: Option<GfmSchema>,
    #[serde(default, rename = "definition-lists")]
    definition_lists: Option<bool>,
    #[serde(default, rename = "abbreviation-lists")]
    abbreviation_lists: Option<bool>,
    #[serde(default, rename = "heading-attribute-lists")]
    heading_attribute_lists: Option<bool>,
    #[serde(default, rename = "block-attribute-lists")]
    block_attribute_lists: Option<bool>,
    #[serde(default)]
    myst: Option<MystSchema>,
    #[serde(default)]
    pandoc: Option<PandocSchema>,
}

impl From<ExtensionsSchema> for ExtensionOptions {
    fn from(s: ExtensionsSchema) -> Self {
        let default = Self::default();
        Self {
            gfm: s.gfm.map_or(default.gfm, GfmOptions::from),
            definition_lists: s.definition_lists.unwrap_or(default.definition_lists),
            abbreviation_lists: s.abbreviation_lists.unwrap_or(default.abbreviation_lists),
            heading_attribute_lists: s.heading_attribute_lists.unwrap_or(default.heading_attribute_lists),
            block_attribute_lists: s.block_attribute_lists.unwrap_or(default.block_attribute_lists),
            myst: s.myst.map_or(default.myst, MystOptions::from),
            pandoc: s.pandoc.map_or(default.pandoc, PandocOptions::from),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct GfmSchema {
    #[serde(default)]
    autolinks: Option<GfmAutolinkPolicySchema>,
    #[serde(default)]
    tagfilter: Option<bool>,
}

impl From<GfmSchema> for GfmOptions {
    fn from(s: GfmSchema) -> Self {
        let default = Self::default();
        Self {
            autolinks: s.autolinks.map_or(default.autolinks, GfmAutolinkPolicy::from),
            tagfilter: s.tagfilter.unwrap_or(default.tagfilter),
        }
    }
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum GfmAutolinkPolicySchema {
    Disabled,
    Urls,
    UrlsAndEmails,
}

impl From<GfmAutolinkPolicySchema> for GfmAutolinkPolicy {
    fn from(s: GfmAutolinkPolicySchema) -> Self {
        match s {
            GfmAutolinkPolicySchema::Disabled => Self::Disabled,
            GfmAutolinkPolicySchema::Urls => Self::Urls,
            GfmAutolinkPolicySchema::UrlsAndEmails => Self::UrlsAndEmails,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools, reason = "shape mirrors `MystOptions`")]
struct MystSchema {
    #[serde(default, rename = "directive-containers")]
    directive_containers: Option<bool>,
    #[serde(default, rename = "inline-roles")]
    inline_roles: Option<bool>,
    #[serde(default, rename = "substitution-references")]
    substitution_references: Option<bool>,
    #[serde(default)]
    comments: Option<bool>,
}

impl From<MystSchema> for MystOptions {
    fn from(s: MystSchema) -> Self {
        let default = Self::default();
        Self {
            directive_containers: s.directive_containers.unwrap_or(default.directive_containers),
            inline_roles: s.inline_roles.unwrap_or(default.inline_roles),
            substitution_references: s.substitution_references.unwrap_or(default.substitution_references),
            comments: s.comments.unwrap_or(default.comments),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PandocSchema {
    #[serde(default, rename = "fenced-divs")]
    fenced_divs: Option<bool>,
    #[serde(default, rename = "short-form-divs")]
    short_form_divs: Option<bool>,
    #[serde(default, rename = "inline-attribute-spans")]
    inline_attribute_spans: Option<bool>,
}

impl From<PandocSchema> for PandocOptions {
    fn from(s: PandocSchema) -> Self {
        let default = Self::default();
        Self {
            fenced_divs: s.fenced_divs.unwrap_or(default.fenced_divs),
            short_form_divs: s.short_form_divs.unwrap_or(default.short_form_divs),
            inline_attribute_spans: s.inline_attribute_spans.unwrap_or(default.inline_attribute_spans),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MathSchema {
    #[serde(default)]
    normalise: Option<bool>,
    #[serde(default)]
    render: Option<MathRenderSchema>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum MathRenderSchema {
    None,
    CommonmarkKatex,
    Dollar,
}

impl From<MathRenderSchema> for MathRender {
    fn from(s: MathRenderSchema) -> Self {
        match s {
            MathRenderSchema::None => Self::None,
            MathRenderSchema::CommonmarkKatex => Self::CommonmarkKatex,
            MathRenderSchema::Dollar => Self::Dollar,
        }
    }
}

impl From<MathSchema> for MathOptions {
    fn from(s: MathSchema) -> Self {
        let default = Self::default();
        Self {
            normalise: s.normalise.unwrap_or(default.normalise),
            render: s.render.map_or(default.render, MathRender::from),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrontmatterSchema {
    #[serde(default)]
    preserve: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RefsSchema {
    #[serde(default)]
    placement: Option<PlacementSchema>,
    #[serde(default)]
    style: Option<LinkDefStyleSchema>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FootnotesSchema {
    #[serde(default)]
    placement: Option<PlacementSchema>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TablesSchema {
    #[serde(default)]
    style: Option<TableStyleSchema>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListsSchema {
    #[serde(default, rename = "continuation-indent")]
    continuation_indent: Option<ListContinuationIndentSchema>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ListContinuationIndentSchema {
    MarkerWidth,
    FourSpace,
}

impl From<ListContinuationIndentSchema> for ListContinuationIndent {
    fn from(s: ListContinuationIndentSchema) -> Self {
        match s {
            ListContinuationIndentSchema::MarkerWidth => Self::MarkerWidth,
            ListContinuationIndentSchema::FourSpace => Self::FourSpace,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PlacementSchema {
    End,
    Preserve,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LinkDefStyleSchema {
    Bare,
    Angle,
    Preserve,
}

#[derive(Debug)]
enum WrapSchema {
    Mode(WrapMode),
    Columns(u32),
}

impl<'de> Deserialize<'de> for WrapSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct WrapVisitor;

        impl Visitor<'_> for WrapVisitor {
            type Value = WrapSchema;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(r#""keep", "no", or an integer column width"#)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                match value {
                    "keep" => Ok(WrapSchema::Mode(WrapMode::Keep)),
                    "no" => Ok(WrapSchema::Mode(WrapMode::No)),
                    _ => Err(E::custom(format!(
                        r#"invalid wrap value {value:?}; expected "keep", "no", or an integer column width"#
                    ))),
                }
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                let columns = u32::try_from(value).map_err(|_| {
                    E::custom(format!(
                        "wrap column width {value} is too large; expected an integer from 0 to {}",
                        u32::MAX
                    ))
                })?;
                Ok(WrapSchema::Columns(columns))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                let columns = u32::try_from(value).map_err(|_| {
                    E::custom(format!(
                        r#"invalid wrap value {value}; expected "keep", "no", or a non-negative integer column width"#
                    ))
                })?;
                Ok(WrapSchema::Columns(columns))
            }
        }

        deserializer.deserialize_any(WrapVisitor)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum WrapMode {
    Keep,
    No,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum WrapStrategySchema {
    Stable,
    Balanced,
}

impl From<WrapStrategySchema> for WrapStrategy {
    fn from(s: WrapStrategySchema) -> Self {
        match s {
            WrapStrategySchema::Stable => Self::Stable,
            WrapStrategySchema::Balanced => Self::Balanced,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ItalicSchema {
    Asterisk,
    Underscore,
    Preserve,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StrongSchema {
    Asterisk,
    Underscore,
    Preserve,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum FmtProfileSchema {
    Preserve,
    Mdformat,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ListMarkerSchema {
    Dash,
    Asterisk,
    Plus,
    Preserve,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OrderedListSchema {
    One,
    Consistent,
    Preserve,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ThematicSchema {
    Dash,
    Asterisk,
    Underscore,
    #[serde(rename = "underscore-70")]
    Underscore70,
    Preserve,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TableStyleSchema {
    Preserve,
    Pad,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TrailingNewlineSchema {
    Named(TrailingNewlineNamed),
    /// `trailing-newline = true` ⇒ `Ensure`; `false` ⇒ `Strip`. Kept
    /// for backward compatibility with config files written against
    /// the pre-Preserve schema.
    Bool(bool),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TrailingNewlineNamed {
    Preserve,
    Strip,
    Ensure,
}

impl From<TrailingNewlineSchema> for TrailingNewline {
    fn from(s: TrailingNewlineSchema) -> Self {
        match s {
            TrailingNewlineSchema::Named(TrailingNewlineNamed::Preserve) => Self::Preserve,
            TrailingNewlineSchema::Named(TrailingNewlineNamed::Strip) => Self::Strip,
            TrailingNewlineSchema::Named(TrailingNewlineNamed::Ensure) => Self::Ensure,
            TrailingNewlineSchema::Bool(true) => Self::Ensure,
            TrailingNewlineSchema::Bool(false) => Self::Strip,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum EndOfLineSchema {
    Lf,
    Crlf,
    Keep,
}

impl From<WrapSchema> for Wrap {
    fn from(s: WrapSchema) -> Self {
        match s {
            WrapSchema::Mode(WrapMode::Keep) => Self::Keep,
            WrapSchema::Mode(WrapMode::No) => Self::No,
            WrapSchema::Columns(n) => Self::At(n),
        }
    }
}

impl From<ItalicSchema> for ItalicStyle {
    fn from(s: ItalicSchema) -> Self {
        match s {
            ItalicSchema::Asterisk => Self::Asterisk,
            ItalicSchema::Underscore => Self::Underscore,
            ItalicSchema::Preserve => Self::Preserve,
        }
    }
}

impl From<StrongSchema> for StrongStyle {
    fn from(s: StrongSchema) -> Self {
        match s {
            StrongSchema::Asterisk => Self::Asterisk,
            StrongSchema::Underscore => Self::Underscore,
            StrongSchema::Preserve => Self::Preserve,
        }
    }
}

impl From<ThematicSchema> for ThematicStyle {
    fn from(s: ThematicSchema) -> Self {
        match s {
            ThematicSchema::Dash => Self::Dash,
            ThematicSchema::Asterisk => Self::Asterisk,
            ThematicSchema::Underscore => Self::Underscore,
            ThematicSchema::Underscore70 => Self::Underscore70,
            ThematicSchema::Preserve => Self::Preserve,
        }
    }
}

impl From<TableStyleSchema> for TableStyle {
    fn from(s: TableStyleSchema) -> Self {
        match s {
            TableStyleSchema::Preserve => Self::Preserve,
            TableStyleSchema::Pad => Self::Pad,
        }
    }
}

impl From<ListMarkerSchema> for ListMarkerStyle {
    fn from(s: ListMarkerSchema) -> Self {
        match s {
            ListMarkerSchema::Dash => Self::Dash,
            ListMarkerSchema::Asterisk => Self::Asterisk,
            ListMarkerSchema::Plus => Self::Plus,
            ListMarkerSchema::Preserve => Self::Preserve,
        }
    }
}

impl From<OrderedListSchema> for OrderedListStyle {
    fn from(s: OrderedListSchema) -> Self {
        match s {
            OrderedListSchema::One => Self::One,
            OrderedListSchema::Consistent => Self::Consistent,
            OrderedListSchema::Preserve => Self::Preserve,
        }
    }
}

impl From<PlacementSchema> for Placement {
    fn from(s: PlacementSchema) -> Self {
        match s {
            PlacementSchema::End => Self::End,
            PlacementSchema::Preserve => Self::Preserve,
        }
    }
}

impl From<LinkDefStyleSchema> for LinkDefStyle {
    fn from(s: LinkDefStyleSchema) -> Self {
        match s {
            LinkDefStyleSchema::Bare => Self::Bare,
            LinkDefStyleSchema::Angle => Self::Angle,
            LinkDefStyleSchema::Preserve => Self::Preserve,
        }
    }
}

impl From<EndOfLineSchema> for EndOfLine {
    fn from(s: EndOfLineSchema) -> Self {
        match s {
            EndOfLineSchema::Lf => Self::Lf,
            EndOfLineSchema::Crlf => Self::Crlf,
            EndOfLineSchema::Keep => Self::Keep,
        }
    }
}

// ============================================================
// File readers
// ============================================================

fn read_mdwright_toml(path: &Path) -> Result<Config, ConfigError> {
    let text = fs::read_to_string(path).map_err(|e| ConfigError::io(path, &e))?;
    let schema: Schema = toml::from_str(&text).map_err(|e| ConfigError::parse(path, &e))?;
    Ok(Config::from_schema(schema, Some(path.to_owned())))
}

/// Walk upward from `start`, returning the first config that matches.
/// Stops at the filesystem root or at the first directory containing a
/// `.git/` entry (the workspace boundary).
fn discover_walk(start: &Path) -> Result<Option<Config>, ConfigError> {
    for dir in start.ancestors() {
        if let Some(cfg) = try_load_dir(dir)? {
            return Ok(Some(cfg));
        }
        if dir.join(".git").exists() {
            return Ok(None);
        }
    }
    Ok(None)
}

/// Try the discovery candidates in one directory in precedence order:
/// `.mdwright.toml` > `mdwright.toml` > `pyproject.toml [tool.mdwright]`.
/// A `pyproject.toml` without the table returns `Ok(None)` so the
/// caller continues the ancestor walk.
fn try_load_dir(dir: &Path) -> Result<Option<Config>, ConfigError> {
    for name in [".mdwright.toml", "mdwright.toml"] {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(Some(read_mdwright_toml(&candidate)?));
        }
    }
    let pyproject = dir.join("pyproject.toml");
    if pyproject.is_file() {
        return read_pyproject(&pyproject);
    }
    Ok(None)
}

fn read_pyproject(path: &Path) -> Result<Option<Config>, ConfigError> {
    let text = fs::read_to_string(path).map_err(|e| ConfigError::io(path, &e))?;
    let value: toml::Value = toml::from_str(&text).map_err(|e| ConfigError::parse(path, &e))?;
    let Some(table) = value.as_table() else {
        return Ok(None);
    };
    let Some(tool) = table.get("tool").and_then(toml::Value::as_table) else {
        return Ok(None);
    };
    let Some(mdw) = tool.get("mdwright") else {
        return Ok(None);
    };
    let schema: Schema = mdw
        .clone()
        .try_into()
        .map_err(|e: toml::de::Error| ConfigError::parse(path, &e))?;
    Ok(Some(Config::from_schema(schema, Some(path.to_owned()))))
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow};

    use mdwright_lint::RuleSet;

    use crate::documentation;

    use super::{
        Config, EndOfLine, FmtOptions, GfmAutolinkPolicy, ItalicStyle, LintRulePreset, ListContinuationIndent,
        ListMarkerStyle, MathRender, OrderedListStyle, RenderProfile, Schema, StrongStyle, TableStyle, ThematicStyle,
        TrailingNewline, Wrap, WrapStrategy,
    };

    fn schema_from_str(src: &str) -> Result<Schema> {
        toml::from_str::<Schema>(src).map_err(|e| anyhow!("parse: {e}"))
    }

    fn config_from_str(src: &str) -> Result<Config> {
        Ok(Config::from_schema(schema_from_str(src)?, None))
    }

    #[test]
    fn parses_complete_toml() -> Result<()> {
        let src = r#"
[lint]
preset = "default"
extend-select = ["escaped-emphasis"]
ignore = ["bare-url"]
exclude = ["docs/vendored/**"]
[lint.info-strings]
extra = ["promql"]

[fmt]
wrap = 70
italic = "asterisk"
strong = "underscore"
list-marker = "dash"
ordered-list = "consistent"
thematic-break = "asterisk"
trailing-newline = true
end-of-line = "lf"
exclude = ["docs/generated/**"]

[fmt.tables]
style = "pad"
"#;
        let cfg = config_from_str(src)?;
        let lint = cfg.lint_rule_selection();
        assert_eq!(lint.preset(), LintRulePreset::Default);
        assert!(lint.select().is_empty());
        assert_eq!(lint.extend_select(), &["escaped-emphasis".to_owned()]);
        assert_eq!(lint.ignore(), &["bare-url".to_owned()]);
        assert_eq!(cfg.exclude_globs(), &["docs/vendored/**".to_owned()]);
        assert_eq!(cfg.extra_info_strings(), &["promql".to_owned()]);
        let fmt = cfg.fmt_options();
        assert_eq!(fmt.wrap(), Wrap::At(70));
        assert_eq!(fmt.wrap_strategy(), WrapStrategy::Stable);
        assert_eq!(fmt.italic(), ItalicStyle::Asterisk);
        assert_eq!(fmt.strong(), StrongStyle::Underscore);
        assert_eq!(fmt.list_marker(), ListMarkerStyle::Dash);
        assert_eq!(fmt.ordered_list(), OrderedListStyle::Consistent);
        assert_eq!(fmt.thematic_break_style(), ThematicStyle::Asterisk);
        assert_eq!(fmt.table(), TableStyle::Pad);
        assert_eq!(fmt.trailing_newline(), TrailingNewline::Ensure);
        assert_eq!(fmt.end_of_line(), EndOfLine::Lf);
        assert_eq!(fmt.exclude_globs(), &["docs/generated/**".to_owned()]);
        Ok(())
    }

    #[test]
    fn default_lint_selection_resolves_defaults() -> Result<()> {
        let cfg = config_from_str("")?;
        let rules = cfg
            .lint_rule_selection()
            .resolve(RuleSet::stdlib_all())
            .map_err(|err| anyhow!("{err}"))?;
        assert!(!rules.is_empty());
        assert!(rules.contains("bare-url"));
        assert!(!rules.contains("latex-command"));
        Ok(())
    }

    #[test]
    fn lint_selection_supports_all_preset() -> Result<()> {
        let cfg = config_from_str("[lint]\npreset = \"all\"\n")?;
        let rules = cfg
            .lint_rule_selection()
            .resolve(RuleSet::stdlib_all())
            .map_err(|err| anyhow!("{err}"))?;
        assert!(rules.contains("latex-command"));
        assert!(rules.contains("bare-url"));
        Ok(())
    }

    #[test]
    fn lint_selection_supports_explicit_select_with_none_preset() -> Result<()> {
        let cfg = config_from_str("[lint]\npreset = \"none\"\nselect = [\"heading-punctuation\", \"bare-url\"]\n")?;
        let rules = cfg
            .lint_rule_selection()
            .resolve(RuleSet::stdlib_all())
            .map_err(|err| anyhow!("{err}"))?;
        assert!(rules.contains("heading-punctuation"));
        assert!(rules.contains("bare-url"));
        assert_eq!(rules.len(), 2);
        Ok(())
    }

    #[test]
    fn lint_selection_supports_extend_select_and_ignore() -> Result<()> {
        let cfg = config_from_str(
            "[lint]\npreset = \"default\"\nextend-select = [\"latex-command\"]\nignore = [\"bare-url\"]\n",
        )?;
        let rules = cfg
            .lint_rule_selection()
            .resolve(RuleSet::stdlib_all())
            .map_err(|err| anyhow!("{err}"))?;
        assert!(rules.contains("latex-command"));
        assert!(!rules.contains("bare-url"));
        Ok(())
    }

    #[test]
    fn rejects_legacy_rules_key_with_migration_hint() -> Result<()> {
        let err = toml::from_str::<Schema>("[lint]\nrules = \"default,+latex-command\"\n")
            .err()
            .ok_or_else(|| anyhow!("expected error"))?;
        let rendered = err.to_string();
        assert!(
            rendered.contains("lint.rules"),
            "error should name legacy key: {rendered}"
        );
        assert!(
            rendered.contains("extend-select"),
            "error should suggest new keys: {rendered}"
        );
        Ok(())
    }

    #[test]
    fn rejects_presets_in_rule_name_lists() -> Result<()> {
        let err = toml::from_str::<Schema>("[lint]\npreset = \"none\"\nselect = [\"default\"]\n")
            .err()
            .ok_or_else(|| anyhow!("expected error"))?;
        let rendered = err.to_string();
        assert!(
            rendered.contains("preset") && rendered.contains("select"),
            "error should explain preset/rule split: {rendered}"
        );
        Ok(())
    }

    #[test]
    fn rejects_select_with_non_none_preset() -> Result<()> {
        let err = toml::from_str::<Schema>("[lint]\npreset = \"default\"\nselect = [\"bare-url\"]\n")
            .err()
            .ok_or_else(|| anyhow!("expected error"))?;
        let rendered = err.to_string();
        assert!(
            rendered.contains("extend-select") && rendered.contains("preset"),
            "error should explain valid shape: {rendered}"
        );
        Ok(())
    }

    #[test]
    fn resolve_rejects_unknown_rule_names() -> Result<()> {
        let cfg = config_from_str("[lint]\nextend-select = [\"no-such-rule\"]\n")?;
        let err = cfg
            .lint_rule_selection()
            .resolve(RuleSet::stdlib_all())
            .err()
            .ok_or_else(|| anyhow!("expected error"))?;
        assert!(err.to_string().contains("no-such-rule"));
        Ok(())
    }

    #[test]
    fn generated_default_toml_parses_as_defaults() -> Result<()> {
        let generated = documentation::render_default_toml();
        let cfg = config_from_str(&generated)?;
        let default = Config::defaults();

        assert_eq!(cfg.lint_rule_selection(), default.lint_rule_selection());
        assert_eq!(cfg.exclude_globs(), default.exclude_globs());
        assert_eq!(cfg.extra_info_strings(), default.extra_info_strings());
        assert_eq!(cfg.parse_options(), default.parse_options());
        assert_eq!(cfg.render_options(), default.render_options());

        let fmt = cfg.fmt_options();
        let default_fmt = default.fmt_options();
        assert_eq!(fmt.wrap(), default_fmt.wrap());
        assert_eq!(fmt.wrap_strategy(), default_fmt.wrap_strategy());
        assert_eq!(fmt.italic(), default_fmt.italic());
        assert_eq!(fmt.strong(), default_fmt.strong());
        assert_eq!(fmt.list_marker(), default_fmt.list_marker());
        assert_eq!(fmt.ordered_list(), default_fmt.ordered_list());
        assert_eq!(fmt.thematic_break_style(), default_fmt.thematic_break_style());
        assert_eq!(fmt.trailing_newline(), default_fmt.trailing_newline());
        assert_eq!(fmt.end_of_line(), default_fmt.end_of_line());
        assert_eq!(fmt.exclude_globs(), default_fmt.exclude_globs());
        assert_eq!(fmt.link_def_placement(), default_fmt.link_def_placement());
        assert_eq!(fmt.link_def_style(), default_fmt.link_def_style());
        assert_eq!(fmt.footnote_placement(), default_fmt.footnote_placement());
        assert_eq!(fmt.table(), default_fmt.table());
        assert_eq!(fmt.list_continuation_indent(), default_fmt.list_continuation_indent());
        assert_eq!(fmt.preserve_frontmatter(), default_fmt.preserve_frontmatter());
        assert_eq!(fmt.heading_attrs(), default_fmt.heading_attrs());
        assert!(!fmt.math().normalise);
        assert_eq!(fmt.math().render, MathRender::None);

        assert!(generated.contains("[lint.info-strings]"));
        assert!(generated.contains("extra = []"));
        assert!(generated.contains("[fmt.math]"));
        assert!(generated.contains("render = \"none\""));
        assert!(generated.contains("[parse.extensions.gfm]"));
        assert!(generated.contains("autolinks = \"urls-and-emails\""));
        Ok(())
    }

    #[test]
    fn rejects_unknown_top_level_key() -> Result<()> {
        let src = "[lnt]\nrules = \"default\"\n";
        let err = toml::from_str::<Schema>(src)
            .err()
            .ok_or_else(|| anyhow!("expected error"))?;
        let rendered = err.to_string();
        assert!(rendered.contains("lnt"), "error should name 'lnt': {rendered}");
        Ok(())
    }

    #[test]
    fn rejects_unknown_inner_key() -> Result<()> {
        let src = "[lint]\nrulez = \"default\"\n";
        let err = toml::from_str::<Schema>(src)
            .err()
            .ok_or_else(|| anyhow!("expected error"))?;
        let rendered = err.to_string();
        assert!(rendered.contains("rulez"), "error should name 'rulez': {rendered}");
        Ok(())
    }

    #[test]
    fn wrap_schema_accepts_string_or_int() -> Result<()> {
        let keep = config_from_str("[fmt]\nwrap = \"keep\"\n")?;
        assert_eq!(keep.fmt_options().wrap(), Wrap::Keep);
        assert_eq!(keep.fmt_options().wrap().columns(), u32::MAX);
        let no = config_from_str("[fmt]\nwrap = \"no\"\n")?;
        assert_eq!(no.fmt_options().wrap(), Wrap::No);
        assert_eq!(no.fmt_options().wrap().columns(), u32::MAX);
        let columns = config_from_str("[fmt]\nwrap = 70\n")?;
        assert_eq!(columns.fmt_options().wrap(), Wrap::At(70));
        assert_eq!(columns.fmt_options().wrap().columns(), 70);
        Ok(())
    }

    #[test]
    fn parse_extensions_are_parse_policy() -> Result<()> {
        let cfg = config_from_str(
            r#"
[parse.extensions]
definition-lists = false
heading-attribute-lists = false

[parse.extensions.gfm]
autolinks = "disabled"
tagfilter = false

[parse.extensions.myst]
comments = false

[parse.extensions.pandoc]
inline-attribute-spans = false
"#,
        )?;
        let extensions = cfg.parse_options().extensions();
        assert_eq!(extensions.gfm.autolinks, GfmAutolinkPolicy::Disabled);
        assert!(!extensions.gfm.tagfilter);
        assert!(!extensions.definition_lists);
        assert!(!extensions.heading_attribute_lists);
        assert!(!extensions.myst.comments);
        assert!(!extensions.pandoc.inline_attribute_spans);
        Ok(())
    }

    #[test]
    fn render_profile_is_render_policy() -> Result<()> {
        let default = config_from_str("")?;
        assert_eq!(default.render_options().profile(), RenderProfile::Pulldown);

        let cfg = config_from_str("[render]\nprofile = \"cmark-gfm\"\n")?;
        assert_eq!(cfg.render_options().profile(), RenderProfile::CmarkGfm);
        Ok(())
    }

    #[test]
    fn rejects_unknown_render_profile() -> Result<()> {
        let err = config_from_str("[render]\nprofile = \"github\"\n")
            .err()
            .ok_or_else(|| anyhow!("expected error"))?;
        assert!(
            err.to_string().contains("profile"),
            "error should name rejected render profile: {err}"
        );
        Ok(())
    }

    #[test]
    fn fmt_profile_mdformat_sets_compatible_defaults() -> Result<()> {
        let cfg = config_from_str("[fmt]\nprofile = \"mdformat\"\n")?;
        let fmt = cfg.fmt_options();
        assert_eq!(fmt.wrap(), Wrap::Keep);
        assert_eq!(fmt.wrap_strategy(), WrapStrategy::Stable);
        assert_eq!(fmt.italic(), ItalicStyle::Preserve);
        assert_eq!(fmt.strong(), StrongStyle::Preserve);
        assert_eq!(fmt.list_marker(), ListMarkerStyle::Dash);
        assert_eq!(fmt.list_continuation_indent(), ListContinuationIndent::FourSpace);
        assert_eq!(fmt.ordered_list(), OrderedListStyle::One);
        assert_eq!(fmt.thematic_break_style(), ThematicStyle::Underscore70);
        assert_eq!(fmt.table(), TableStyle::Pad);
        assert!(fmt.preserve_frontmatter());
        Ok(())
    }

    #[test]
    fn explicit_fmt_keys_override_mdformat_profile() -> Result<()> {
        let cfg = config_from_str(
            r#"
[fmt]
profile = "mdformat"
wrap = 120
wrap-strategy = "balanced"
list-marker = "plus"
ordered-list = "consistent"
thematic-break = "dash"

[fmt.lists]
continuation-indent = "marker-width"

[fmt.tables]
style = "preserve"
"#,
        )?;
        let fmt = cfg.fmt_options();
        assert_eq!(fmt.wrap(), Wrap::At(120));
        assert_eq!(fmt.wrap_strategy(), WrapStrategy::Balanced);
        assert_eq!(fmt.list_marker(), ListMarkerStyle::Plus);
        assert_eq!(fmt.ordered_list(), OrderedListStyle::Consistent);
        assert_eq!(fmt.list_continuation_indent(), ListContinuationIndent::MarkerWidth);
        assert_eq!(fmt.thematic_break_style(), ThematicStyle::Dash);
        assert_eq!(fmt.table(), TableStyle::Preserve);
        Ok(())
    }

    #[test]
    fn fmt_wrap_strategy_accepts_supported_styles() -> Result<()> {
        let stable = config_from_str("[fmt]\nwrap-strategy = \"stable\"\n")?;
        assert_eq!(stable.fmt_options().wrap_strategy(), WrapStrategy::Stable);

        let balanced = config_from_str("[fmt]\nwrap-strategy = \"balanced\"\n")?;
        assert_eq!(balanced.fmt_options().wrap_strategy(), WrapStrategy::Balanced);

        let err = config_from_str("[fmt]\nwrap-strategy = \"pretty\"\n")
            .err()
            .ok_or_else(|| anyhow!("expected wrap-strategy error"))?;
        assert!(
            err.to_string().contains("wrap-strategy"),
            "error should name wrap-strategy: {err}"
        );
        Ok(())
    }

    #[test]
    fn fmt_lists_continuation_indent_accepts_supported_styles() -> Result<()> {
        let marker_width = config_from_str("[fmt.lists]\ncontinuation-indent = \"marker-width\"\n")?;
        assert_eq!(
            marker_width.fmt_options().list_continuation_indent(),
            ListContinuationIndent::MarkerWidth
        );

        let four_space = config_from_str("[fmt.lists]\ncontinuation-indent = \"four-space\"\n")?;
        assert_eq!(
            four_space.fmt_options().list_continuation_indent(),
            ListContinuationIndent::FourSpace
        );

        let err = config_from_str("[fmt.lists]\ncontinuation-indent = \"tab\"\n")
            .err()
            .ok_or_else(|| anyhow!("expected continuation-indent error"))?;
        assert!(
            err.to_string().contains("continuation-indent"),
            "error should name rejected continuation-indent: {err}"
        );
        Ok(())
    }

    #[test]
    fn rejects_unknown_fmt_profile_and_table_style() -> Result<()> {
        let profile = config_from_str("[fmt]\nprofile = \"aggressive\"\n")
            .err()
            .ok_or_else(|| anyhow!("expected profile error"))?;
        assert!(
            profile.to_string().contains("profile"),
            "error should name profile: {profile}"
        );

        let table = config_from_str("[fmt.tables]\nstyle = \"compact\"\n")
            .err()
            .ok_or_else(|| anyhow!("expected table style error"))?;
        assert!(
            table.to_string().contains("style"),
            "error should name table style: {table}"
        );
        Ok(())
    }

    #[test]
    fn formatter_extension_table_is_not_a_schema_key() -> Result<()> {
        let src = concat!("[fmt", ".extensions]\ndefinition-lists = false\n");
        let err = toml::from_str::<Schema>(src)
            .err()
            .ok_or_else(|| anyhow!("expected error"))?;
        let rendered = err.to_string();
        assert!(
            rendered.contains("extensions"),
            "error should name rejected formatter extension table: {rendered}"
        );
        Ok(())
    }

    #[test]
    fn resolvers_honour_style() -> Result<()> {
        let preserve = config_from_str("[fmt]\nitalic = \"preserve\"\nlist-marker = \"preserve\"\n")?;
        let fmt = preserve.fmt_options();
        assert_eq!(fmt.resolve_italic(b'_'), b'_');
        assert_eq!(fmt.resolve_italic(b'*'), b'*');
        assert_eq!(fmt.resolve_list_marker(b'+'), b'+');

        let pin = config_from_str("[fmt]\nitalic = \"asterisk\"\nlist-marker = \"dash\"\n")?;
        let fmt = pin.fmt_options();
        assert_eq!(fmt.resolve_italic(b'_'), b'*');
        assert_eq!(fmt.resolve_list_marker(b'*'), b'-');

        // Default config (no [fmt] table): every style knob is Preserve,
        // so resolvers pass the source byte through unchanged.
        let defaults = FmtOptions::default();
        assert_eq!(defaults.resolve_italic(b'_'), b'_');
        assert_eq!(defaults.resolve_italic(b'*'), b'*');
        assert_eq!(defaults.resolve_list_marker(b'+'), b'+');
        assert_eq!(defaults.resolve_list_marker(b'-'), b'-');
        Ok(())
    }

    #[test]
    fn style_enums_round_trip() -> Result<()> {
        for (lit, expected) in [
            ("\"asterisk\"", ItalicStyle::Asterisk),
            ("\"underscore\"", ItalicStyle::Underscore),
            ("\"preserve\"", ItalicStyle::Preserve),
        ] {
            let cfg = config_from_str(&format!("[fmt]\nitalic = {lit}\n"))?;
            assert_eq!(cfg.fmt_options().italic(), expected);
        }
        for (lit, expected) in [
            ("\"asterisk\"", StrongStyle::Asterisk),
            ("\"underscore\"", StrongStyle::Underscore),
            ("\"preserve\"", StrongStyle::Preserve),
        ] {
            let cfg = config_from_str(&format!("[fmt]\nstrong = {lit}\n"))?;
            assert_eq!(cfg.fmt_options().strong(), expected);
        }
        for (lit, expected) in [
            ("\"dash\"", ThematicStyle::Dash),
            ("\"asterisk\"", ThematicStyle::Asterisk),
            ("\"underscore\"", ThematicStyle::Underscore),
            ("\"underscore-70\"", ThematicStyle::Underscore70),
            ("\"preserve\"", ThematicStyle::Preserve),
        ] {
            let cfg = config_from_str(&format!("[fmt]\nthematic-break = {lit}\n"))?;
            assert_eq!(cfg.fmt_options().thematic_break_style(), expected);
        }
        for (lit, expected) in [
            ("\"dash\"", ListMarkerStyle::Dash),
            ("\"asterisk\"", ListMarkerStyle::Asterisk),
            ("\"plus\"", ListMarkerStyle::Plus),
            ("\"preserve\"", ListMarkerStyle::Preserve),
        ] {
            let cfg = config_from_str(&format!("[fmt]\nlist-marker = {lit}\n"))?;
            assert_eq!(cfg.fmt_options().list_marker(), expected);
        }
        for (lit, expected) in [
            ("\"one\"", OrderedListStyle::One),
            ("\"consistent\"", OrderedListStyle::Consistent),
            ("\"preserve\"", OrderedListStyle::Preserve),
        ] {
            let cfg = config_from_str(&format!("[fmt]\nordered-list = {lit}\n"))?;
            assert_eq!(cfg.fmt_options().ordered_list(), expected);
        }
        for (lit, expected) in [
            ("\"lf\"", EndOfLine::Lf),
            ("\"crlf\"", EndOfLine::Crlf),
            ("\"keep\"", EndOfLine::Keep),
        ] {
            let cfg = config_from_str(&format!("[fmt]\nend-of-line = {lit}\n"))?;
            assert_eq!(cfg.fmt_options().end_of_line(), expected);
        }
        Ok(())
    }
}
