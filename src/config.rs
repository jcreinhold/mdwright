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
        let Schema { lint, fmt } = schema;
        Self {
            rules_spec: lint.rules,
            exclude_globs: lint.exclude,
            extra_info_strings: lint.info_strings.extra,
            fmt_options: FmtOptions::from_schema(fmt),
            source,
        }
    }
}

/// Formatter knobs.
///
/// Style knobs (`italic`, `strong`, `list_marker`, `ordered_list`,
/// `link_def_style`, `thematic_break_style`) default to `Preserve`;
/// the structural-emit pipeline never consults them, so the defaults
/// round-trip source bytes verbatim. Style rewrites are applied by
/// the canonicalisation post-pass at
/// [`crate::format::canonicalise::canonicalise`], which reads the
/// per-knob `_target()` accessors below. Structural defaults:
/// `wrap = keep`, `trailing-newline = preserve`, `end-of-line = lf`,
/// empty exclude list.
#[derive(Debug, Clone)]
pub struct FmtOptions {
    wrap: Wrap,
    italic: ItalicStyle,
    strong: StrongStyle,
    list_marker: ListMarkerStyle,
    ordered_list: OrderedListStyle,
    trailing_newline: TrailingNewline,
    end_of_line: EndOfLine,
    exclude_globs: Vec<String>,
    link_def_placement: Placement,
    link_def_style: LinkDefStyle,
    footnote_placement: Placement,
    preserve_frontmatter: bool,
    thematic_break_style: ThematicStyle,
    math: MathOptions,
    heading_attrs: HeadingAttrsStyle,
    extensions: ExtensionOptions,
}

/// Heading-attribute trailer emission policy.
///
/// `# Heading {#id .class key=val}` parses (with
/// `Options::ENABLE_HEADING_ATTRIBUTES`) into a `Tag::Heading` carrying
/// the parsed `id`, `classes`, and `attrs`. This knob decides how the
/// formatter re-emits the trailer.
///
/// - [`Self::Preserve`] (default): emit the source trailer byte-verbatim
///   between the rendered inline body and the line terminator. Matches
///   the preserve-by-default ethos every other style knob defaults to.
/// - [`Self::Canonicalise`]: emit `{#id .class₁ .class₂ k=val}` in a
///   fixed order — id first, then classes in source order, then
///   key=value pairs in source order. Matches mdformat-mkdocs canonical.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum HeadingAttrsStyle {
    /// Emit the source trailer byte-verbatim.
    #[default]
    Preserve,
    /// Emit the trailer in fixed canonical order: `#id`, then classes
    /// (source order), then `key=value` pairs (source order).
    Canonicalise,
}

/// Per-extension recognition toggles.
///
/// Each field gates recognition of one mdformat-mkdocs extension. When
/// false, the corresponding scanner / typed-block construction is
/// skipped and the source bytes flow through the legacy verbatim path
/// instead. Defaults are **on**: these extensions recognise what the
/// source already says (definition-list shape, abbreviation declarations,
/// attribute trailers), not what the formatter prefers. The preserve-
/// by-default ethos applies to style knobs, not feature recognition.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one toggle per mdformat-mkdocs extension; the parallel naming with the TOML schema is intentional"
)]
pub struct ExtensionOptions {
    /// Recognise `Term\n: defn\n` definition lists (pulldown
    /// `Options::ENABLE_DEFINITION_LIST`).
    pub definition_lists: bool,
    /// Recognise `*[ABBR]: definition` abbreviation declarations
    /// (scan-and-preserve overlay; mdwright does not expand
    /// occurrences, only preserves the declarations).
    pub abbreviation_lists: bool,
    /// Recognise `{ #id .class key=val }` after an ATX heading
    /// (pulldown `Options::ENABLE_HEADING_ATTRIBUTES`). When false,
    /// the trailer is treated as plain text in the heading body.
    pub heading_attribute_lists: bool,
    /// Recognise `{ .class }` on a line by itself after a paragraph,
    /// image, or fenced block (scan-and-preserve overlay). Inline
    /// attribute lists (mid-paragraph) are explicitly out of scope.
    pub block_attribute_lists: bool,
    /// MyST-flavoured directive recognition (block directive
    /// containers, inline roles, substitutions, `%` comments).
    pub myst: MystOptions,
    /// Pandoc-flavoured directive recognition (fenced divs in their
    /// attr and short forms, inline attribute spans).
    pub pandoc: PandocOptions,
}

impl Default for ExtensionOptions {
    fn default() -> Self {
        Self {
            definition_lists: true,
            abbreviation_lists: true,
            heading_attribute_lists: true,
            block_attribute_lists: true,
            myst: MystOptions::default(),
            pandoc: PandocOptions::default(),
        }
    }
}

/// Recognition toggles for `MyST`-flavoured extensions.
///
/// `MyST` (Markedly Structured Text) is the substrate for jupyter-book
/// and Sphinx-`MyST`. All toggles default **on** because they recognise
/// what the source already says, not formatter opinion. See
/// [`ExtensionOptions`] for the preserve-by-default ethos.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one toggle per MyST construct; recognition gates are independent"
)]
pub struct MystOptions {
    /// Recognise `:::{name}` directive containers (with options) as a
    /// scan-and-preserve region.
    pub directive_containers: bool,
    /// Recognise `` {role}`payload` `` inline roles as a
    /// scan-and-preserve region.
    pub inline_roles: bool,
    /// Recognise `{{name}}` inline substitution references.
    pub substitution_references: bool,
    /// Recognise `%` line comments at line-start.
    pub comments: bool,
}

impl Default for MystOptions {
    fn default() -> Self {
        Self {
            directive_containers: true,
            inline_roles: true,
            substitution_references: true,
            comments: true,
        }
    }
}

/// Recognition toggles for `Pandoc`-flavoured extensions.
///
/// Defaults on. See [`ExtensionOptions`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one toggle per Pandoc construct; recognition gates are independent"
)]
pub struct PandocOptions {
    /// Recognise `::: {.cls}` fenced div openers (attr form).
    pub fenced_divs: bool,
    /// Recognise `:::name` fenced div openers (short form).
    pub short_form_divs: bool,
    /// Recognise `[content]{.cls}` inline attribute spans.
    pub inline_attribute_spans: bool,
}

impl Default for PandocOptions {
    fn default() -> Self {
        Self {
            fenced_divs: true,
            short_form_divs: true,
            inline_attribute_spans: true,
        }
    }
}

/// Math canonicalisation configuration.
///
/// All fields are off by default. Math regions are opaque to
/// `CommonMark`: pulldown-cmark parses their bytes as prose, so any
/// whitespace change inside shifts the byte-level HTML output and
/// trips [`crate::Document::format_validated`]. Authors who render
/// math downstream (`KaTeX`, `MathJax`) opt in.
#[derive(Copy, Clone, Debug, Default)]
pub struct MathOptions {
    /// Whether whole-block math regions (display `\[…\]` / `$$…$$`
    /// and environments standing alone) are normalised.
    pub normalise: bool,
    /// How math regions are emitted for downstream renderers. See
    /// [`MathRender`] for the modes; default is [`MathRender::None`]
    /// (verbatim emission, today's behaviour).
    pub render: MathRender,
}

/// Delimiter rewrite policy for math regions at emit time.
///
/// mdwright never typesets math itself; downstream renderers
/// (`KaTeX`, `MathJax`, `mkdocs-material`'s math plugin) do that. The
/// modes here determine the *shape* of the math regions in the
/// formatted output so the downstream renderer recognises them.
///
/// - [`Self::None`] (default): pass math regions through verbatim;
///   today's behaviour.
/// - [`Self::CommonmarkKatex`]: same emission as `None`, but signals
///   intent in build logs. The bracket/paren forms (`\[…\]`, `\(…\)`)
///   and dollar forms (`$$…$$`, `$…$`) are both recognised by `KaTeX`
///   and `MathJax` v3 auto-renderers without rewriting.
/// - [`Self::Dollar`]: rewrite `\[ … \]` to `$$ … $$` and `\( … \)`
///   to `$ … $` at emit time. LaTeX environments
///   (`\begin{align*}…\end{align*}`) are not rewritten — there is no
///   dollar form of an environment.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum MathRender {
    /// Pass math regions through verbatim.
    #[default]
    None,
    /// Pass through verbatim; greppable signal that downstream is
    /// `KaTeX` or `MathJax` over `CommonMark`-shaped math.
    CommonmarkKatex,
    /// Rewrite backslash-bracket / backslash-paren math to dollar
    /// form. Environments are left unchanged.
    Dollar,
}

impl FmtOptions {
    /// Wrap mode for prose paragraphs.
    #[must_use]
    pub fn wrap(&self) -> Wrap {
        self.wrap
    }

    /// Italic delimiter normalisation policy.
    #[must_use]
    pub fn italic(&self) -> ItalicStyle {
        self.italic
    }

    /// Strong-emphasis delimiter normalisation policy.
    #[must_use]
    pub fn strong(&self) -> StrongStyle {
        self.strong
    }

    /// Unordered-list bullet normalisation policy.
    #[must_use]
    pub fn list_marker(&self) -> ListMarkerStyle {
        self.list_marker
    }

    /// Ordered-list number normalisation policy.
    #[must_use]
    pub fn ordered_list(&self) -> OrderedListStyle {
        self.ordered_list
    }

    /// Trailing-newline policy at the document boundary.
    #[must_use]
    pub fn trailing_newline(&self) -> TrailingNewline {
        self.trailing_newline
    }

    /// Line-ending normalisation policy.
    #[must_use]
    pub fn end_of_line(&self) -> EndOfLine {
        self.end_of_line
    }

    /// Formatter-specific exclude globs (independent of `[lint]
    /// exclude`).
    #[must_use]
    pub fn exclude_globs(&self) -> &[String] {
        &self.exclude_globs
    }

    /// Where reference-link definitions are emitted: gathered + sorted
    /// at end of document, or kept in source positions.
    #[must_use]
    pub fn link_def_placement(&self) -> Placement {
        self.link_def_placement
    }

    /// Whether reference-link and inline-link destinations are emitted
    /// bare or angle-bracketed.
    #[must_use]
    pub fn link_def_style(&self) -> LinkDefStyle {
        self.link_def_style
    }

    /// Where footnote definitions are emitted. Populated for parity
    /// with [`Self::link_def_placement`]; session 11 reads this.
    #[must_use]
    pub fn footnote_placement(&self) -> Placement {
        self.footnote_placement
    }

    /// Whether to emit the document's frontmatter byte-verbatim in
    /// the formatted output. `true` (the default) preserves YAML or
    /// TOML frontmatter exactly; `false` strips it.
    #[must_use]
    pub fn preserve_frontmatter(&self) -> bool {
        self.preserve_frontmatter
    }

    /// Thematic-break canonicalisation policy. Defaults to
    /// [`ThematicStyle::Preserve`]. The structural-emit path does not
    /// consult this; a later canonicalisation post-pass will.
    #[must_use]
    pub fn thematic_break_style(&self) -> ThematicStyle {
        self.thematic_break_style
    }

    /// Math canonicalisation configuration. See [`MathOptions`] for the
    /// reason every field defaults off.
    #[must_use]
    pub fn math(&self) -> MathOptions {
        self.math
    }

    /// Override the math options. Returns the receiver for chaining.
    #[must_use]
    pub fn with_math(mut self, math: MathOptions) -> Self {
        self.math = math;
        self
    }

    /// Override only the math render policy. Composes with the
    /// existing [`MathOptions`]; preserves `normalise`.
    #[must_use]
    pub fn with_math_render(mut self, render: MathRender) -> Self {
        self.math.render = render;
        self
    }

    /// Heading-attribute trailer emission policy.
    #[must_use]
    pub fn heading_attrs(&self) -> HeadingAttrsStyle {
        self.heading_attrs
    }

    /// Override the heading-attribute style.
    #[must_use]
    pub fn with_heading_attrs(mut self, style: HeadingAttrsStyle) -> Self {
        self.heading_attrs = style;
        self
    }

    /// Per-extension recognition toggles. See [`ExtensionOptions`].
    #[must_use]
    pub fn extensions(&self) -> ExtensionOptions {
        self.extensions
    }

    /// Override the per-extension toggles.
    #[must_use]
    pub fn with_extensions(mut self, extensions: ExtensionOptions) -> Self {
        self.extensions = extensions;
        self
    }

    /// Resolve the italic delimiter to emit, given the byte the
    /// source originally used (`b'*'` or `b'_'`). `Preserve` returns
    /// `source_delim`; fixed styles return their own byte. Keeps the
    /// `match` out of every render site.
    #[must_use]
    pub fn resolve_italic(&self, source_delim: u8) -> u8 {
        match self.italic {
            ItalicStyle::Asterisk => b'*',
            ItalicStyle::Underscore => b'_',
            ItalicStyle::Preserve => source_delim,
        }
    }

    /// Override the wrap mode. Returns the receiver for chaining.
    /// Used by callers that build [`FmtOptions`] programmatically
    /// (benches, golden tests, `--wrap` overrides).
    #[must_use]
    pub fn with_wrap(mut self, wrap: Wrap) -> Self {
        self.wrap = wrap;
        self
    }

    /// Override the italic style. Used by callers that build options
    /// programmatically (property tests, CLI overrides).
    #[must_use]
    pub fn with_italic(mut self, italic: ItalicStyle) -> Self {
        self.italic = italic;
        self
    }

    /// Override the strong style.
    #[must_use]
    pub fn with_strong(mut self, strong: StrongStyle) -> Self {
        self.strong = strong;
        self
    }

    /// Override the unordered-list bullet style.
    #[must_use]
    pub fn with_list_marker(mut self, list_marker: ListMarkerStyle) -> Self {
        self.list_marker = list_marker;
        self
    }

    /// Override the ordered-list numbering policy.
    #[must_use]
    pub fn with_ordered_list(mut self, ordered_list: OrderedListStyle) -> Self {
        self.ordered_list = ordered_list;
        self
    }

    /// Override the thematic-break canonicalisation policy.
    #[must_use]
    pub fn with_thematic_break(mut self, thematic_break: ThematicStyle) -> Self {
        self.thematic_break_style = thematic_break;
        self
    }

    /// Override the link-destination style.
    #[must_use]
    pub fn with_link_def_style(mut self, link_def_style: LinkDefStyle) -> Self {
        self.link_def_style = link_def_style;
        self
    }

    /// Resolve the unordered-list bullet to emit, given the byte the
    /// source used (`b'-'`, `b'*'`, or `b'+'`).
    #[must_use]
    pub fn resolve_list_marker(&self, source_marker: u8) -> u8 {
        match self.list_marker {
            ListMarkerStyle::Dash => b'-',
            ListMarkerStyle::Asterisk => b'*',
            ListMarkerStyle::Plus => b'+',
            ListMarkerStyle::Preserve => source_marker,
        }
    }

    // ----- Canonicalisation post-pass targets ---------------------
    //
    // Each accessor below maps a style knob to `Some(target)` when the
    // user opted into canonicalisation and `None` when the knob is
    // `Preserve`. The canonicalisation pass at
    // `src/format/canonicalise.rs` is the only consumer; structural
    // emit never reads these.

    /// Italic-delimiter target byte. `None` keeps source bytes.
    #[must_use]
    pub(crate) fn italic_target_byte(&self) -> Option<u8> {
        match self.italic {
            ItalicStyle::Asterisk => Some(b'*'),
            ItalicStyle::Underscore => Some(b'_'),
            ItalicStyle::Preserve => None,
        }
    }

    /// Strong-delimiter target byte. `None` keeps source bytes.
    #[must_use]
    pub(crate) fn strong_target_byte(&self) -> Option<u8> {
        match self.strong {
            StrongStyle::Asterisk => Some(b'*'),
            StrongStyle::Underscore => Some(b'_'),
            StrongStyle::Preserve => None,
        }
    }

    /// Unordered-list bullet target byte. `None` keeps source bytes.
    #[must_use]
    pub(crate) fn list_marker_target_byte(&self) -> Option<u8> {
        match self.list_marker {
            ListMarkerStyle::Dash => Some(b'-'),
            ListMarkerStyle::Asterisk => Some(b'*'),
            ListMarkerStyle::Plus => Some(b'+'),
            ListMarkerStyle::Preserve => None,
        }
    }

    /// Thematic-break target byte. `None` keeps source bytes.
    #[must_use]
    pub(crate) fn thematic_target_byte(&self) -> Option<u8> {
        match self.thematic_break_style {
            ThematicStyle::Dash => Some(b'-'),
            ThematicStyle::Asterisk => Some(b'*'),
            ThematicStyle::Underscore => Some(b'_'),
            ThematicStyle::Preserve => None,
        }
    }

    /// `true` iff ordered lists should be renumbered.
    #[must_use]
    pub(crate) fn should_renumber_ordered_lists(&self) -> bool {
        matches!(self.ordered_list, OrderedListStyle::Consistent)
    }

    /// Link-destination angle-bracket rewrite target. `None` keeps
    /// source form per definition.
    #[must_use]
    pub(crate) fn link_def_target(&self) -> Option<LinkDefStyle> {
        match self.link_def_style {
            LinkDefStyle::Bare => Some(LinkDefStyle::Bare),
            LinkDefStyle::Angle => Some(LinkDefStyle::Angle),
            LinkDefStyle::Preserve => None,
        }
    }

    /// True iff any style knob is set to a non-`Preserve` value. When
    /// false, the canonicalisation pass is skipped entirely.
    #[must_use]
    pub(crate) fn has_any_canonicalisation(&self) -> bool {
        self.italic_target_byte().is_some()
            || self.strong_target_byte().is_some()
            || self.list_marker_target_byte().is_some()
            || self.thematic_target_byte().is_some()
            || self.should_renumber_ordered_lists()
            || self.link_def_target().is_some()
            || matches!(self.heading_attrs, HeadingAttrsStyle::Canonicalise)
            || matches!(self.math.render, MathRender::Dollar)
            || self.math.normalise
            || !self.preserve_frontmatter
    }

    fn from_schema(schema: FmtSchema) -> Self {
        let default = Self::default();
        let refs = schema.refs.unwrap_or_default();
        let footnotes = schema.footnotes.unwrap_or_default();
        let frontmatter = schema.frontmatter.unwrap_or_default();
        Self {
            wrap: schema.wrap.map_or(default.wrap, Wrap::from),
            italic: schema.italic.map_or(default.italic, ItalicStyle::from),
            strong: schema.strong.map_or(default.strong, StrongStyle::from),
            list_marker: schema.list_marker.map_or(default.list_marker, ListMarkerStyle::from),
            ordered_list: schema.ordered_list.map_or(default.ordered_list, OrderedListStyle::from),
            trailing_newline: schema
                .trailing_newline
                .map_or(default.trailing_newline, TrailingNewline::from),
            end_of_line: schema.end_of_line.map_or(default.end_of_line, EndOfLine::from),
            exclude_globs: schema.exclude,
            link_def_placement: refs.placement.map_or(default.link_def_placement, Placement::from),
            link_def_style: refs.style.map_or(default.link_def_style, LinkDefStyle::from),
            footnote_placement: footnotes.placement.map_or(default.footnote_placement, Placement::from),
            preserve_frontmatter: frontmatter.preserve.unwrap_or(default.preserve_frontmatter),
            thematic_break_style: schema
                .thematic_break
                .map_or(default.thematic_break_style, ThematicStyle::from),
            math: schema.math.map_or(default.math, MathOptions::from),
            heading_attrs: schema
                .heading_attrs
                .map_or(default.heading_attrs, HeadingAttrsStyle::from),
            extensions: schema.extensions.map_or(default.extensions, ExtensionOptions::from),
        }
    }
}

impl Default for FmtOptions {
    fn default() -> Self {
        Self {
            wrap: Wrap::Keep,
            // Style knobs default to Preserve so structural emit
            // round-trips source bytes. The canonicalisation post-pass
            // at `crate::format::canonicalise` reads these knobs to
            // opt in to rewrites.
            italic: ItalicStyle::Preserve,
            strong: StrongStyle::Preserve,
            list_marker: ListMarkerStyle::Preserve,
            ordered_list: OrderedListStyle::Preserve,
            trailing_newline: TrailingNewline::Preserve,
            end_of_line: EndOfLine::Lf,
            exclude_globs: Vec::new(),
            link_def_placement: Placement::End,
            link_def_style: LinkDefStyle::Preserve,
            // Footnotes stay at their source position. Pulldown's HTML
            // renderer emits the `<div class="footnote-definition">`
            // block at the point of parsing, so moving definitions to
            // the document tail under `Placement::End` would change
            // the rendered HTML byte stream and fail
            // [`crate::Document::format_validated`].
            footnote_placement: Placement::Preserve,
            preserve_frontmatter: true,
            thematic_break_style: ThematicStyle::Preserve,
            math: MathOptions::default(),
            heading_attrs: HeadingAttrsStyle::default(),
            extensions: ExtensionOptions::default(),
        }
    }
}

/// Trailing-newline policy applied at the document boundary.
///
/// `Preserve` (the default) matches the source's trailing-newline
/// shape: if the source ends with one or more `\n` bytes, the
/// formatted output ends with exactly one `\n`; if the source has no
/// trailing `\n`, the output has none either. This is what
/// `Document::format_validated` needs to hold: pulldown-cmark's HTML
/// render of `\t\x10` is `<pre><code>\x10</code></pre>` while its
/// render of `\t\x10\n` is `<pre><code>\x10\n</code></pre>` — the
/// trailing LF lives inside the code body for any document ending in
/// an indented code block. An unconditional "ensure trailing `\n`"
/// post-pass cannot avoid that class of HTML-divergence; `Preserve`
/// avoids it by construction.
///
/// `Strip` drops every trailing `\n`. `Ensure` forces exactly one
/// trailing `\n` (the pre-Preserve `trailing_newline = true`
/// behaviour); kept under an explicit name so callers opt in to the
/// foot-gun rather than getting it by default.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum TrailingNewline {
    /// Match the source: one trailing `\n` iff the source had any.
    #[default]
    Preserve,
    /// Drop every trailing `\n`.
    Strip,
    /// Force exactly one trailing `\n`, appending if absent.
    Ensure,
}

/// Emission position for collected items (link reference definitions,
/// footnote definitions). `End` (the default) gathers items and sorts
/// them at the end of the document; `Preserve` keeps source order.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    End,
    Preserve,
}

/// Destination style for link reference definitions and inline links.
///
/// `Bare` emits `[label]: url`; `Angle` emits `[label]: <url>`;
/// `Preserve` (the default) emits whichever form the source used for
/// each destination.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkDefStyle {
    Bare,
    Angle,
    Preserve,
}

/// Prose-wrap mode.
///
/// `Keep` (default) and `No` both leave existing breaks alone; the
/// distinction is whether the formatter is *allowed* to introduce new
/// breaks later (`Keep` means "do nothing if already within budget",
/// `No` means "never break"). `At(n)` wraps at column `n`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Wrap {
    Keep,
    No,
    At(u32),
}

impl Wrap {
    /// Effective column target. `Keep` and `No` both return
    /// `u32::MAX`, signalling the wrap pass "do not introduce breaks".
    #[must_use]
    pub fn columns(self) -> u32 {
        match self {
            Self::Keep | Self::No => u32::MAX,
            Self::At(n) => n,
        }
    }

    /// Reduce the target column count by `n` for nested contexts whose
    /// physical lines will be prefixed (blockquote `> `, list-item
    /// marker, footnote indent). `Keep` and `No` are unaffected.
    #[must_use]
    pub fn shrink(self, n: u32) -> Self {
        match self {
            Self::At(c) => Self::At(c.saturating_sub(n).max(1)),
            Self::Keep | Self::No => self,
        }
    }
}

/// Italic delimiter normalisation policy. Defaults to `Preserve`:
/// structural emit preserves each run's source delimiter. Fixed
/// variants are consumed only by the canonicalisation post-pass.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ItalicStyle {
    Asterisk,
    Underscore,
    Preserve,
}

/// Strong-emphasis delimiter normalisation policy. Defaults to
/// `Preserve`. Independent of [`ItalicStyle`] so an author can
/// canonicalise one without forcing the other.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StrongStyle {
    Asterisk,
    Underscore,
    Preserve,
}

/// Unordered-list bullet normalisation policy. Defaults to
/// `Preserve`: structural emit keeps each list's source bullet.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ListMarkerStyle {
    Dash,
    Asterisk,
    Plus,
    Preserve,
}

/// Ordered-list number normalisation policy. `Consistent` renumbers
/// from 1 (matches mdformat's default); `Preserve` (the default)
/// keeps source numbering verbatim.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OrderedListStyle {
    Consistent,
    Preserve,
}

/// Thematic-break canonicalisation policy. Defaults to `Preserve`:
/// structural emit echoes the source `---` / `***` / `___` line
/// verbatim. The fixed variants exist for the future canonicalisation
/// pass.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ThematicStyle {
    Dash,
    Asterisk,
    Underscore,
    Preserve,
}

impl ThematicStyle {
    /// The repeated byte the thematic-break line is built from, when
    /// the style names a single byte. `Preserve` returns `None`
    /// because the byte to emit comes from the source itself.
    #[must_use]
    pub fn as_byte(self) -> Option<u8> {
        match self {
            Self::Dash => Some(b'-'),
            Self::Asterisk => Some(b'*'),
            Self::Underscore => Some(b'_'),
            Self::Preserve => None,
        }
    }
}

/// Line-ending policy. `Keep` adopts the first newline in the source.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EndOfLine {
    Lf,
    Crlf,
    Keep,
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
    #[serde(default)]
    extensions: Option<ExtensionsSchema>,
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
