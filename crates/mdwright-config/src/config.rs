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

use std::error::Error as StdError;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use mdwright_document::{ExtensionOptions, MystOptions, PandocOptions, ParseOptions};
use mdwright_format::{
    EndOfLine, FmtOptions, HeadingAttrsStyle, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions, MathRender,
    OrderedListStyle, Placement, StrongStyle, ThematicStyle, TrailingNewline, Wrap,
};
use serde::Deserialize;

// ============================================================
// Public surface
// ============================================================

/// Resolved project configuration. Construct with
/// [`Config::load_explicit`] (for `--config PATH`) or
/// [`Config::discover`] (for the ancestor walk from CWD).
#[derive(Debug, Clone)]
pub struct Config {
    rules_spec: String,
    exclude_globs: Vec<String>,
    extra_info_strings: Vec<String>,
    fmt_options: FmtOptions,
    parse_options: ParseOptions,
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

    /// The `--rules`-equivalent token list. `"default"` when no
    /// config file is found or the file does not set it.
    #[must_use]
    pub fn rules_spec(&self) -> &str {
        &self.rules_spec
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
        let Schema { lint, fmt, parse } = schema;
        Self {
            rules_spec: lint.rules,
            exclude_globs: lint.exclude,
            extra_info_strings: lint.info_strings.extra,
            fmt_options: fmt_options_from_schema(fmt),
            parse_options: parse_options_from_schema(parse),
            source,
        }
    }
}

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
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LintSchema {
    #[serde(default = "default_rules_spec")]
    rules: String,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default, rename = "info-strings")]
    info_strings: InfoStringsSchema,
}

impl Default for LintSchema {
    fn default() -> Self {
        Self {
            rules: default_rules_spec(),
            exclude: Vec::new(),
            info_strings: InfoStringsSchema::default(),
        }
    }
}

fn default_rules_spec() -> String {
    "default".to_owned()
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
    wrap: Option<WrapSchema>,
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
    frontmatter: Option<FrontmatterSchema>,
    #[serde(default)]
    math: Option<MathSchema>,
    #[serde(default, rename = "heading-attrs")]
    heading_attrs: Option<HeadingAttrsSchema>,
}

fn fmt_options_from_schema(schema: FmtSchema) -> FmtOptions {
    let refs = schema.refs.unwrap_or_default();
    let footnotes = schema.footnotes.unwrap_or_default();
    let frontmatter = schema.frontmatter.unwrap_or_default();
    let default = FmtOptions::default();
    let mut opts = FmtOptions::default()
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
    if let Some(wrap) = schema.wrap {
        opts = opts.with_wrap(Wrap::from(wrap));
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

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WrapSchema {
    Mode(WrapMode),
    Columns(u32),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum WrapMode {
    Keep,
    No,
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
    Consistent,
    Preserve,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ThematicSchema {
    Dash,
    Asterisk,
    Underscore,
    Preserve,
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
            ThematicSchema::Preserve => Self::Preserve,
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

    use super::{
        Config, EndOfLine, FmtOptions, ItalicStyle, ListMarkerStyle, OrderedListStyle, Schema, StrongStyle,
        ThematicStyle, TrailingNewline, Wrap,
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
rules = "default,+escaped-emphasis"
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
"#;
        let cfg = config_from_str(src)?;
        assert_eq!(cfg.rules_spec(), "default,+escaped-emphasis");
        assert_eq!(cfg.exclude_globs(), &["docs/vendored/**".to_owned()]);
        assert_eq!(cfg.extra_info_strings(), &["promql".to_owned()]);
        let fmt = cfg.fmt_options();
        assert_eq!(fmt.wrap(), Wrap::At(70));
        assert_eq!(fmt.italic(), ItalicStyle::Asterisk);
        assert_eq!(fmt.strong(), StrongStyle::Underscore);
        assert_eq!(fmt.list_marker(), ListMarkerStyle::Dash);
        assert_eq!(fmt.ordered_list(), OrderedListStyle::Consistent);
        assert_eq!(fmt.thematic_break_style(), ThematicStyle::Asterisk);
        assert_eq!(fmt.trailing_newline(), TrailingNewline::Ensure);
        assert_eq!(fmt.end_of_line(), EndOfLine::Lf);
        assert_eq!(fmt.exclude_globs(), &["docs/generated/**".to_owned()]);
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
            r"
[parse.extensions]
definition-lists = false
heading-attribute-lists = false

[parse.extensions.myst]
comments = false

[parse.extensions.pandoc]
inline-attribute-spans = false
",
        )?;
        let extensions = cfg.parse_options().extensions();
        assert!(!extensions.definition_lists);
        assert!(!extensions.heading_attribute_lists);
        assert!(!extensions.myst.comments);
        assert!(!extensions.pandoc.inline_attribute_spans);
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
