# Public API Surface

mdwright is a virtual workspace, not a facade crate. Command users install the `mdwright` package. Rust library users
depend on the component crate that owns the capability they need.

The API is still pre-1.0. Import paths and operation shapes may change in minor releases under the
[pre-1.0 caveats](semver.md#pre-10-caveats).

## Common User Surfaces

| Capability | Public surface | Owning crate |
| --- | --- | --- |
| Parse Markdown into stable facts | `Document`, `ParseError`, `ParseOptions` | `mdwright-document` |
| Configure Markdown recognition | `ExtensionOptions`, `GfmOptions`, `GfmAutolinkPolicy`, `MystOptions`, `PandocOptions` | `mdwright-document` |
| Render Markdown to HTML | `RenderOptions`, `RenderProfile`, `render_html`, `render_html_with_options`, `render_html_with_render_options` | `mdwright-document` |
| Format parsed or source Markdown | `FmtOptions`, `WrapStrategy`, `FormatError`, `format_document`, `format_document_with_report`, `format_source`, `format_validated`, `format_validated_with_report` | `mdwright-format` |
| Format editor ranges | `CheckpointTable`, `format_range`, `format_range_with_checkpoints` | `mdwright-format` |
| Compare formatter semantics | `semantically_equivalent`, `first_divergence` | `mdwright-format` |
| Represent TeX math-body diagnostics, vocabulary, Unicode layout, source translation, and output | `LatexError`, `LatexErrorKind`, `SourceSpan`, `CommandInfo`, `CommandCategory`, `ArgumentShape`, `SupportStatus`, `lookup_command`, `latex_symbol`, `unicode_symbol_latex`, `unicode_super`, `unicode_sub`, `RenderedLatex`, `render_unicode_math`, `Translation`, `TranslationStatus`, `TranslationLoss`, `translate_latex_to_unicode`, `translate_unicode_to_latex`, `translate_latex_ranges_to_unicode`, `translate_unicode_ranges_to_latex` | `mdwright-latex` |
| Recognise Markdown math regions | `scan_math_regions`, `render::convert_for_dollar`, `MathBody::source_range` | `mdwright-math` |
| Run lint rules | `RuleSet`, `LintOptions` | `mdwright-lint` |
| Consume lint output | `Diagnostic`, `Fix`, `Severity`, `Snippet`, `DuplicateRuleName` | `mdwright-lint` |
| Apply safe lint fixes | `apply_safe_fixes` | `mdwright-lint` |
| Resolve configuration | `Config`, `ConfigError` | `mdwright-config` |
| Start the editor server | `serve` | `mdwright-lsp` |
| Build custom command binaries | `run_with_rules`, `discover_markdown` | `mdwright` |

`Document` is parse/query only. Linting, formatting, safe-fix application, config discovery, command delivery, and
editor delivery stay in their owning crates.

## Extension Surfaces

| Surface | Use |
| --- | --- |
| `LintRule` | Implement a downstream lint rule over `&Document`. |
| `RuleSet::{new, add, remove, by_name, contains, iter, names, check, check_with}` | Compose standard and downstream rules. |
| `mdwright_lint::stdlib::{defaults, all, by_name, names}` | Select standard rules for custom binaries or tests. |
| `Diagnostic`, `Fix`, `Severity`, `Snippet` | Report lint findings and optional safe fixes. |
| `rule_doc_url`, `docs_url`, `DOCS_URL_DEFAULT` | Attach stable documentation links to diagnostics. |
| `InfoStringTypo::{new, with_extra}` | Extend the standard info-string vocabulary without forking the rule. |
| `mdwright::run_with_rules` | Reuse the command package with a custom `RuleSet`. |
| `mdwright::discover_markdown` | Reuse command file-discovery policy in a custom command. |

The standard rule structs under `mdwright_lint::stdlib` are public so callers can build precise `RuleSet`s. Helper
functions inside those rules are not public extension points unless listed above.

## Advanced Document Facts

These facts are public because formatter, lint, audit, and custom-rule callers need stable source ranges without
learning pulldown event shapes.

| Fact family | Public surface |
| --- | --- |
| Text and blocks | `TextSlice`, `InlineCode`, `CodeBlock`, `HtmlBlock`, `InlineHtml`, `Heading` |
| Lists and references | `ListGroup`, `ListItem`, `LinkDef` |
| Frontmatter | `Frontmatter`, `FrontmatterDelimiter` |
| Autolinks | `AutolinkFact`, `AutolinkOrigin` |
| Suppressions | `Suppression`, `SuppressionKind`, `AllowScope` |
| Positions | `LineIndex`, `LineIndexError`, `BlockCheckpointFact` |
| Math | `MathRegion`, `MathSpan`, `MathError` |
| Formatter facts | `StructuralSpan`, `StructuralKind`, `InlineDelimiterSlot`, `InlineDelimiterKind`, `UnorderedListMarkerSite`, `OrderedListMarkerSite`, `HeadingAttrSite`, `InlineLinkDestinationSlot`, `ReferenceDefinitionSite`, `TableSite`, `TableRowSite`, `TableCellSite`, `TableAlign`, `WrappableParagraph`, `ParagraphHardBreak` |

Formatter-facing facts expose accessors instead of public fields where practical. That keeps invalid construction out of
downstream code while preserving stable ranges for rule authors and diagnostic tooling.

## Not Public Surface

- Root facade exports. There is no root package and no `mdwright::{Document, FmtOptions, RuleSet}` import path.
- Parser internals, pulldown events, source/canonical byte-map internals, and the private document tree.
- `Source`, `CanonicalSource`, `OffsetMap`, `ByteSpan`, `OriginalSpan`, `NormalisedLabel`, and heading trailer scanners.
- Top-level block checkpoint parser helpers. Use `mdwright_format::CheckpointTable`.
- Formatter rewrite candidates, rewrite snapshots, verification signatures, owner IDs, and byte-application internals.
- `mdwright-latex` lexer tokens, parser cursors, AST nodes, command-registry storage, and Unicode layout internals.
- Lint suppression maps, diagnostic sorting internals, and stdlib helper functions not listed as extension surfaces.
- TOML raw schema structs and config discovery internals.
- CLI and LSP state machines beyond the documented entry points.
