/// Formatter knobs.
///
/// Style knobs default to `Preserve` except GFM tables, which default
/// to compact normal form. The structural-emit pipeline never consults
/// style knobs; rewrites are applied by the formatter's rewrite-family
/// pipeline. Structural defaults: `wrap = keep`, `trailing-newline =
/// preserve`, `end-of-line = lf`, empty exclude list.
#[derive(Debug, Clone)]
pub struct FmtOptions {
    wrap: Wrap,
    wrap_strategy: WrapStrategy,
    italic: ItalicStyle,
    strong: StrongStyle,
    list_marker: ListMarkerStyle,
    list_continuation_indent: ListContinuationIndent,
    ordered_list: OrderedListStyle,
    table: TableStyle,
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
    blank_line_before_heading: BlankLine,
    blank_line_after_heading: BlankLine,
}

/// Blank-line policy for one side of a top-level heading.
///
/// The value set is deliberately just these two. The gap between `# A`
/// and `## B` is at once the *after* gap of one heading and the *before*
/// gap of the next, so the two knobs can address the same bytes; with
/// only `Preserve` and `One`, [`Self::One`] simply wins and no conflict
/// exists. A "remove the blank line" value would reintroduce one.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BlankLine {
    /// Echo the source's blank lines byte-verbatim.
    #[default]
    Preserve,
    /// Emit exactly one blank line, inserting or collapsing as needed.
    One,
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
///   fixed order: id first, then classes in source order, then
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

/// Math canonicalisation configuration.
///
/// All fields are off by default. Math regions are opaque to
/// `CommonMark`: pulldown-cmark parses their bytes as prose, so any
/// whitespace change inside shifts the byte-level event stream. Authors
/// who render math downstream (`KaTeX`, `MathJax`) opt in.
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
///   (`\begin{align*}…\end{align*}`) are not rewritten; there is no
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

    /// Paragraph wrap strategy.
    #[must_use]
    pub fn wrap_strategy(&self) -> WrapStrategy {
        self.wrap_strategy
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

    /// List continuation indentation used when wrapping list-item paragraphs.
    #[must_use]
    pub fn list_continuation_indent(&self) -> ListContinuationIndent {
        self.list_continuation_indent
    }

    /// Ordered-list number normalisation policy.
    #[must_use]
    pub fn ordered_list(&self) -> OrderedListStyle {
        self.ordered_list
    }

    /// GFM table canonicalisation policy.
    #[must_use]
    pub fn table(&self) -> TableStyle {
        self.table
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

    /// Override the paragraph wrap strategy.
    #[must_use]
    pub fn with_wrap_strategy(mut self, strategy: WrapStrategy) -> Self {
        self.wrap_strategy = strategy;
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

    /// Override list continuation indentation for wrapped list items.
    #[must_use]
    pub fn with_list_continuation_indent(mut self, indent: ListContinuationIndent) -> Self {
        self.list_continuation_indent = indent;
        self
    }

    /// Override the ordered-list numbering policy.
    #[must_use]
    pub fn with_ordered_list(mut self, ordered_list: OrderedListStyle) -> Self {
        self.ordered_list = ordered_list;
        self
    }

    /// Override the GFM table canonicalisation policy.
    #[must_use]
    pub fn with_table(mut self, table: TableStyle) -> Self {
        self.table = table;
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
    // `Preserve`. Rewrite-family producers are the only
    // consumers; identity emit never reads these.

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

    /// Ordered-list renumbering policy for the canonicalisation pass.
    #[must_use]
    pub(crate) fn ordered_list_target(&self) -> Option<OrderedListStyle> {
        match self.ordered_list {
            OrderedListStyle::Consistent | OrderedListStyle::One => Some(self.ordered_list),
            OrderedListStyle::Preserve => None,
        }
    }

    /// Thematic-break shape for the canonicalisation pass.
    #[must_use]
    pub(crate) fn thematic_target(&self) -> Option<ThematicStyle> {
        match self.thematic_break_style {
            ThematicStyle::Dash | ThematicStyle::Asterisk | ThematicStyle::Underscore | ThematicStyle::Underscore70 => {
                Some(self.thematic_break_style)
            }
            ThematicStyle::Preserve => None,
        }
    }

    /// `true` iff GFM tables should be normalised.
    #[must_use]
    pub(crate) fn should_normalise_tables(&self) -> bool {
        !matches!(self.table, TableStyle::Preserve)
    }

    /// True iff a canonicalisation family other than tables is active.
    ///
    /// Tables default to compact normalisation, so callers use this to
    /// preserve the identity fast path for table-free documents.
    #[must_use]
    pub(crate) fn has_non_table_canonicalisation(&self) -> bool {
        self.italic_target_byte().is_some()
            || self.strong_target_byte().is_some()
            || self.list_marker_target_byte().is_some()
            || self.thematic_target().is_some()
            || self.ordered_list_target().is_some()
            || self.link_def_target().is_some()
            || matches!(self.heading_attrs, HeadingAttrsStyle::Canonicalise)
            || matches!(self.math.render, MathRender::Dollar)
            || self.math.normalise
            || !self.preserve_frontmatter
            || self.normalises_heading_blank_lines()
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
        self.should_normalise_tables() || self.has_non_table_canonicalisation()
    }

    /// Override the blank-line policy before a top-level heading.
    #[must_use]
    pub fn with_blank_line_before_heading(mut self, blank_line: BlankLine) -> Self {
        self.blank_line_before_heading = blank_line;
        self
    }

    /// Override the blank-line policy after a top-level heading.
    #[must_use]
    pub fn with_blank_line_after_heading(mut self, blank_line: BlankLine) -> Self {
        self.blank_line_after_heading = blank_line;
        self
    }

    /// Blank-line policy before a top-level heading.
    #[must_use]
    pub fn blank_line_before_heading(&self) -> BlankLine {
        self.blank_line_before_heading
    }

    /// Blank-line policy after a top-level heading.
    #[must_use]
    pub fn blank_line_after_heading(&self) -> BlankLine {
        self.blank_line_after_heading
    }

    /// True iff either heading blank-line knob is active.
    #[must_use]
    pub(crate) fn normalises_heading_blank_lines(&self) -> bool {
        matches!(self.blank_line_before_heading, BlankLine::One)
            || matches!(self.blank_line_after_heading, BlankLine::One)
    }

    /// Override the trailing-newline policy.
    #[must_use]
    pub fn with_trailing_newline(mut self, trailing_newline: TrailingNewline) -> Self {
        self.trailing_newline = trailing_newline;
        self
    }

    /// Override the end-of-line policy.
    #[must_use]
    pub fn with_end_of_line(mut self, end_of_line: EndOfLine) -> Self {
        self.end_of_line = end_of_line;
        self
    }

    /// Override formatter exclude globs.
    #[must_use]
    pub fn with_exclude_globs(mut self, exclude_globs: Vec<String>) -> Self {
        self.exclude_globs = exclude_globs;
        self
    }

    /// Override reference-definition placement.
    #[must_use]
    pub fn with_link_def_placement(mut self, placement: Placement) -> Self {
        self.link_def_placement = placement;
        self
    }

    /// Override footnote-definition placement.
    #[must_use]
    pub fn with_footnote_placement(mut self, placement: Placement) -> Self {
        self.footnote_placement = placement;
        self
    }

    /// Override frontmatter preservation.
    #[must_use]
    pub fn with_preserve_frontmatter(mut self, preserve_frontmatter: bool) -> Self {
        self.preserve_frontmatter = preserve_frontmatter;
        self
    }
}

impl Default for FmtOptions {
    fn default() -> Self {
        Self {
            wrap: Wrap::Keep,
            wrap_strategy: WrapStrategy::Stable,
            // Most style knobs default to Preserve so structural emit
            // round-trips source bytes. GFM tables default to compact
            // normal form because redundant cell padding is not useful
            // source information.
            italic: ItalicStyle::Preserve,
            strong: StrongStyle::Preserve,
            list_marker: ListMarkerStyle::Preserve,
            list_continuation_indent: ListContinuationIndent::MarkerWidth,
            ordered_list: OrderedListStyle::Preserve,
            table: TableStyle::Compact,
            trailing_newline: TrailingNewline::Preserve,
            end_of_line: EndOfLine::Lf,
            exclude_globs: Vec::new(),
            link_def_placement: Placement::End,
            link_def_style: LinkDefStyle::Preserve,
            // Footnotes stay at their source position. Pulldown's HTML
            // renderer emits the `<div class="footnote-definition">`
            // block at the point of parsing, so moving definitions to
            // the document tail under `Placement::End` would change the
            // event stream.
            footnote_placement: Placement::Preserve,
            preserve_frontmatter: true,
            thematic_break_style: ThematicStyle::Preserve,
            math: MathOptions::default(),
            heading_attrs: HeadingAttrsStyle::default(),
            blank_line_before_heading: BlankLine::Preserve,
            blank_line_after_heading: BlankLine::Preserve,
        }
    }
}

impl FmtOptions {
    /// mdformat-compatible formatting profile where mdwright can
    /// reproduce mdformat's spelling without weakening verification.
    ///
    /// mdformat 1.0 defaults to `wrap = keep`; callers that want a
    /// column limit should still set [`Self::with_wrap`] explicitly.
    ///
    /// The heading blank-line knobs stay at `Preserve` here. mdformat
    /// normalises *every* block gap to one blank line, not just the
    /// heading-adjacent ones, so enabling them would move this profile
    /// closer to mdformat while still diverging on every other gap —
    /// implying a parity that does not hold.
    #[must_use]
    pub fn mdformat() -> Self {
        Self::default()
            .with_wrap_strategy(WrapStrategy::Stable)
            .with_list_marker(ListMarkerStyle::Dash)
            .with_list_continuation_indent(ListContinuationIndent::FourSpace)
            .with_ordered_list(OrderedListStyle::One)
            .with_thematic_break(ThematicStyle::Underscore70)
            .with_table(TableStyle::Align)
    }
}

/// Trailing-newline policy applied at the document boundary.
///
/// `Preserve` (the default) matches the source's trailing-newline
/// shape: if the source ends with one or more `\n` bytes, the
/// formatted output ends with exactly one `\n`; if the source has no
/// trailing `\n`, the output has none either. This is what
/// formatter validation needs to hold: pulldown-cmark's HTML render of
/// `\t\x10` is `<pre><code>\x10</code></pre>` while its
/// render of `\t\x10\n` is `<pre><code>\x10\n</code></pre>`; the
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

/// Paragraph-wrap planner.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WrapStrategy {
    /// Reflow each hard-break-bounded run with mdformat-compatible soft breaks.
    Stable,
    /// Rebalance the whole paragraph with a squared-slack planner.
    Balanced,
}

impl WrapStrategy {
    /// TOML spelling for this strategy.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Balanced => "balanced",
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

/// Continuation indentation for wrapped list-item paragraphs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ListContinuationIndent {
    /// Continue at the marker width: `- text` wraps to `  text`.
    MarkerWidth,
    /// Continue with four spaces after the containing block prefix.
    FourSpace,
}

/// Ordered-list number normalisation policy.
///
/// `One` rewrites markers to `1.` where verification preserves the
/// list start. `Consistent` renumbers from the source first marker.
/// `Preserve` keeps source numbering verbatim.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OrderedListStyle {
    /// Rewrite every item marker in the list to `1.`.
    One,
    /// Renumber items from the source list's first marker.
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
    /// Rewrite the whole break line to mdformat's 70 underscores.
    Underscore70,
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
            Self::Underscore | Self::Underscore70 => Some(b'_'),
            Self::Preserve => None,
        }
    }
}

/// GFM table canonicalisation policy.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TableStyle {
    /// Trim source cell padding and emit one conventional space on
    /// each side of each cell.
    Compact,
    /// Pad cells and delimiter rows to display-width-aligned columns.
    Align,
    /// Keep source table spacing.
    Preserve,
}

/// Line-ending policy. `Keep` adopts the first newline in the source.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EndOfLine {
    Lf,
    Crlf,
    Keep,
}
